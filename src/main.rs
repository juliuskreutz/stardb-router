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

        // Svelte doesn't like cookies being in separate headers
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

        let path = req_header.uri.path();

        let port = if path.starts_with("/api") {
            // api port
            8000
        } else if path.starts_with("/cms") {
            // ghost port
            2368
        } else {
            // stardb port
            3000
        };

        let peer = HttpPeer::new(("127.0.0.1", port), false, String::new());

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
