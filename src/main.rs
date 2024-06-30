use std::sync::Arc;

use http::{header, Response, StatusCode};
use pingora::{
    apps::http_app::ServeHttp,
    listeners::TlsSettings,
    protocols::http::ServerSession,
    server::{configuration::Opt, Server},
    services::listening::Service,
    upstreams::peer::{HttpPeer, Peer},
};
use pingora_core::Result;
use pingora_proxy::{ProxyHttp, Session};

struct HttpsRedirect;

#[async_trait::async_trait]
impl ServeHttp for HttpsRedirect {
    async fn response(&self, session: &mut ServerSession) -> Response<Vec<u8>> {
        let path_and_query = session.req_header().uri.path_and_query().unwrap();

        log::info!("HttpsRedirect: {}", path_and_query);

        Response::builder()
            .status(StatusCode::MOVED_PERMANENTLY)
            .header(
                header::LOCATION,
                format!("https://stardb.gg{}", path_and_query),
            )
            .body(Vec::new())
            .unwrap()
    }
}

struct StardbRouter;

#[async_trait::async_trait]
impl ProxyHttp for StardbRouter {
    type CTX = ();

    fn new_ctx(&self) -> Self::CTX {}

    async fn upstream_peer(
        &self,
        session: &mut Session,
        _: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let req_header = session.req_header().clone();

        log::info!("Proxy: {}", req_header.uri.path());

        session
            .req_header_mut()
            .insert_header("X-Forwarded-Proto", "https")
            .unwrap();

        let cookies = req_header
            .headers
            .get_all("cookie")
            .iter()
            .map(|c| c.to_str().unwrap())
            .collect::<Vec<_>>();

        let cookies = cookies.join("; ");

        session
            .req_header_mut()
            .insert_header("cookie", &cookies)
            .unwrap();

        session.downstream_compression.adjust_decompression(true);
        session.downstream_compression.adjust_level(6);
        session.downstream_compression.request_filter(&req_header);

        let (adress, tls, sni) = if req_header.uri.path().starts_with("/api") {
            (("127.0.0.1", 8000), false, String::new())
        } else if req_header.uri.path().starts_with("/wuwa/map") {
            (("127.0.0.1", 8001), false, String::new())
        } else if req_header.uri.path().starts_with("/cms") {
            (("127.0.0.1", 2368), false, String::new())
        } else {
            (("127.0.0.1", 3000), false, String::new())
        };

        let mut peer = HttpPeer::new(adress, tls, sni);
        peer.get_mut_peer_options().unwrap().h2_ping_interval =
            Some(std::time::Duration::from_secs(1));

        Ok(Box::new(peer))
    }
}

fn main() {
    let mut my_server = Server::new(Some(Opt::default())).unwrap();
    my_server.bootstrap();

    env_logger::init();

    let mut tls_settings = TlsSettings::intermediate(
        "/etc/letsencrypt/live/stardb.gg/fullchain.pem",
        "/etc/letsencrypt/live/stardb.gg/privkey.pem",
    )
    .unwrap();
    tls_settings.enable_h2();

    let mut my_proxy = pingora_proxy::http_proxy_service(&my_server.configuration, StardbRouter);
    my_proxy.add_tls_with_settings("0.0.0.0:443", None, tls_settings);

    let mut my_redirect = Service::new("Https Redirect".to_string(), Arc::new(HttpsRedirect));
    my_redirect.add_tcp("0.0.0.0:80");

    my_server.add_service(my_proxy);
    my_server.add_service(my_redirect);

    my_server.run_forever();
}
