use pingora::{
    listeners::TlsSettings,
    modules::http::{compression::ResponseCompressionBuilder, HttpModules},
    server::{configuration::Opt, Server},
    upstreams::peer::HttpPeer,
};
use pingora_core::Result;
use pingora_proxy::{ProxyHttp, Session};

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

        let (adress, tls, sni) = if req_header.uri.path().starts_with("/api") {
            (("127.0.0.1", 8000), false, String::new())
        } else if req_header.uri.path().starts_with("/wuwa/map") {
            (("127.0.0.1", 8001), false, String::new())
        } else if req_header.uri.path().starts_with("/cms") {
            (("127.0.0.1", 2368), false, String::new())
        } else if req_header.uri.path().starts_with("/challenge") {
            (("127.0.0.1", 3001), false, String::new())
        } else {
            (("127.0.0.1", 3000), false, String::new())
        };

        let peer = HttpPeer::new(adress, tls, sni);

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
    let mut downstream_modules = HttpModules::new();
    downstream_modules.add_module(ResponseCompressionBuilder::enable(6));
    my_proxy.app_logic_mut().unwrap().downstream_modules = downstream_modules;

    my_server.add_service(my_proxy);

    my_server.run_forever();
}
