//! Cliente HTTP saliente: rustls (ring) con certificados raíz incluidos en el binario.
//! No depende de OpenSSL ni de los certificados del sistema (sirve en `scratch` y en Termux).
//! El DNS lo resuelve `dns::Resolver` (también funciona en Termux, donde no hay `/etc/resolv.conf`).

use std::{sync::Arc, time::Duration};

pub fn build() -> reqwest::Client {
    let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("versiones TLS")
        .with_root_certificates(roots)
        .with_no_client_auth();
    reqwest::Client::builder()
        .tls_backend_preconfigured(tls)
        .dns_resolver(Arc::new(crate::dns::Resolver::default()))
        .user_agent(concat!("GordurasFM/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .expect("cliente HTTP")
}
