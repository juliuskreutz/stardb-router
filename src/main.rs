use std::sync::Arc;

use http::{header, Response, StatusCode, Uri};
use pingora::{
    apps::http_app::ServeHttp,
    listeners::TlsSettings,
    protocols::http::ServerSession,
    server::{configuration::Opt, Server},
    services::listening::Service,
    upstreams::peer::{HttpPeer, Peer, PeerOptions},
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

        log::info!("HttpsRedirect: {}", req_header.uri.path());

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

        let mut peer_options = PeerOptions::new();
        peer_options.idle_timeout = Some(std::time::Duration::from_secs(1));

        Ok(if req_header.uri.path().starts_with("/api") {
            let mut peer = HttpPeer::new(("0.0.0.0", 8000), false, String::new());
            *peer.get_mut_peer_options().unwrap() = peer_options;

            Box::new(peer)
        } else if req_header.uri.path().starts_with("/wuwa/map/") {
            let mut parts = req_header.uri.clone().into_parts();
            parts.path_and_query = Some(
                parts
                    .path_and_query
                    .unwrap()
                    .as_str()
                    .strip_prefix("/wuwa/map")
                    .unwrap()
                    .parse()
                    .unwrap(),
            );
            session
                .req_header_mut()
                .set_uri(Uri::from_parts(parts).unwrap());

            let mut peer = HttpPeer::new(("0.0.0.0", 8001), false, String::new());
            *peer.get_mut_peer_options().unwrap() = peer_options;

            Box::new(peer)
        } else {
            let mut peer = HttpPeer::new(("0.0.0.0", 3000), false, String::new());
            *peer.get_mut_peer_options().unwrap() = peer_options;

            Box::new(peer)
        })
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

    let mut my_redirect = Service::new("Https Redirect".to_string(), Arc::new(HttpsRedirect {}));
    my_redirect.add_tcp("0.0.0.0:80");

    my_server.add_service(my_proxy);
    my_server.add_service(my_redirect);

    my_server.run_forever();
}
