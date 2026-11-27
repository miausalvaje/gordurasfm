//! Resolución DNS propia para las peticiones salientes (Last.fm, ListenBrainz, Navidrome por nombre).
//!
//! El binario es estático (musl), así que no puede usar el resolutor del sistema de Android. Orden:
//! 1. `OWNFM_DNS` (lista de IP separadas por comas), si está puesta.
//! 2. El primer `resolv.conf` con servidores: `/etc/resolv.conf` (Linux, Docker) o el de Termux
//!    (`$PREFIX/etc/resolv.conf`).
//! 3. Cloudflare (1.1.1.1 y 1.0.0.1).
//!
//! `localhost` se resuelve siempre a la propia máquina, sin preguntar a ningún servidor (en Termux
//! no hay `/etc/hosts` donde mirarlo).

use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
};

use hickory_resolver::{
    TokioResolver,
    config::{CLOUDFLARE, LookupIpStrategy, NameServerConfig, ResolverConfig, ResolverOpts},
    net::runtime::TokioRuntimeProvider,
};
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use tokio::sync::OnceCell;

/// De dónde ha salido la configuración (para el registro).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Env,
    File(PathBuf),
    Fallback,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Env => write!(f, "OWNFM_DNS"),
            Source::File(p) => write!(f, "{}", p.display()),
            Source::Fallback => write!(f, "Cloudflare (1.1.1.1), no hay resolv.conf"),
        }
    }
}

/// `resolv.conf` que se prueban, en orden.
pub fn candidate_files() -> Vec<PathBuf> {
    let mut v = vec![PathBuf::from("/etc/resolv.conf")];
    if let Ok(prefix) = std::env::var("PREFIX") {
        v.push(Path::new(&prefix).join("etc/resolv.conf"));
    }
    v.push(PathBuf::from("/data/data/com.termux/files/usr/etc/resolv.conf"));
    v
}

/// Elige la configuración del resolutor.
pub fn choose(env_dns: Option<&str>, files: &[PathBuf]) -> (ResolverConfig, ResolverOpts, Source) {
    if let Some(list) = env_dns {
        let servers: Vec<NameServerConfig> = list
            .split([',', ' '])
            .filter_map(|s| s.trim().parse::<IpAddr>().ok())
            .map(NameServerConfig::udp_and_tcp)
            .collect();
        if !servers.is_empty() {
            return (ResolverConfig::from_parts(None, vec![], servers), ResolverOpts::default(), Source::Env);
        }
        tracing::warn!(valor = list, "OWNFM_DNS no tiene ninguna IP válida; se ignora");
    }
    for f in files {
        let Ok(data) = std::fs::read(f) else { continue };
        match hickory_resolver::system_conf::parse_resolv_conf(&data) {
            Ok((cfg, opts)) if !cfg.name_servers().is_empty() => return (cfg, opts, Source::File(f.clone())),
            Ok(_) => tracing::warn!(archivo = %f.display(), "resolv.conf sin servidores"),
            Err(e) => tracing::warn!(archivo = %f.display(), error = %e, "resolv.conf no válido"),
        }
    }
    (ResolverConfig::udp_and_tcp(&CLOUDFLARE), ResolverOpts::default(), Source::Fallback)
}

fn is_localhost(host: &str) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    h == "localhost" || h.ends_with(".localhost")
}

/// Resolutor para `reqwest`. Se construye en la primera petición (necesita el runtime de tokio).
#[derive(Clone, Default)]
pub struct Resolver {
    inner: Arc<OnceCell<TokioResolver>>,
}

impl Resolver {
    async fn get(&self) -> Result<&TokioResolver, hickory_resolver::net::NetError> {
        self.inner
            .get_or_try_init(|| async {
                let env = std::env::var("OWNFM_DNS").ok().or_else(|| std::env::var("GFM_DNS").ok());
                let (cfg, opts, source) = choose(env.as_deref(), &candidate_files());
                tracing::info!(origen = %source, "DNS");
                let mut b = TokioResolver::builder_with_config(cfg, TokioRuntimeProvider::default()).with_options(opts);
                b.options_mut().ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
                b.build()
            })
            .await
    }
}

impl Resolve for Resolver {
    fn resolve(&self, name: Name) -> Resolving {
        let me = self.clone();
        Box::pin(async move {
            let host = name.as_str();
            if is_localhost(host) {
                let v = vec![
                    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
                    SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 0),
                ];
                return Ok(Box::new(v.into_iter()) as Addrs);
            }
            let lookup = me.get().await?.lookup_ip(host).await?;
            let v: Vec<SocketAddr> = lookup.iter().map(|ip| SocketAddr::new(ip, 0)).collect();
            Ok(Box::new(v.into_iter()) as Addrs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ips(cfg: &ResolverConfig) -> Vec<IpAddr> {
        let mut v: Vec<IpAddr> = cfg.name_servers().iter().map(|s| s.ip).collect();
        v.dedup();
        v
    }

    #[test]
    fn variable_de_entorno_primero() {
        let (cfg, _, src) = choose(Some("9.9.9.9, 149.112.112.112"), &[]);
        assert_eq!(src, Source::Env);
        assert_eq!(ips(&cfg), vec!["9.9.9.9".parse::<IpAddr>().unwrap(), "149.112.112.112".parse().unwrap()]);
    }

    #[test]
    fn primer_resolv_conf_con_servidores_y_si_no_cloudflare() {
        let dir = tempfile::tempdir().unwrap();
        let vacio = dir.path().join("vacio.conf");
        std::fs::write(&vacio, "# sin servidores\nsearch lan\n").unwrap();
        let termux = dir.path().join("termux.conf");
        std::fs::write(&termux, "nameserver 192.168.1.1\n").unwrap();
        let no_existe = dir.path().join("no-existe.conf");

        let (cfg, _, src) = choose(None, &[no_existe.clone(), vacio.clone(), termux.clone()]);
        assert_eq!(src, Source::File(termux));
        assert_eq!(ips(&cfg), vec!["192.168.1.1".parse::<IpAddr>().unwrap()]);

        // OWNFM_DNS sin IP válidas se ignora.
        let (cfg, _, src) = choose(Some("nada"), &[no_existe, vacio]);
        assert_eq!(src, Source::Fallback);
        assert!(ips(&cfg).contains(&"1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn localhost_sin_dns() {
        assert!(is_localhost("localhost"));
        assert!(is_localhost("LOCALHOST."));
        assert!(is_localhost("navidrome.localhost"));
        assert!(!is_localhost("localhost.example.com"));
    }

    #[tokio::test]
    async fn resuelve_localhost() {
        let r = Resolver::default();
        let addrs: Vec<SocketAddr> = r.resolve("localhost".parse().unwrap()).await.unwrap().collect();
        assert!(addrs.iter().any(|a| a.ip().is_loopback()));
    }
}
