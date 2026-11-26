use anyhow::{Context, Result};

// El malloc de musl es lento y fragmenta mucho con cargas grandes (importaciones): mimalloc.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use gordurasfm::{config::Config, crypto::Crypto, db, state};
use tracing_subscriber::EnvFilter;

const HELP: &str = "GordurasFM — estadísticas de escucha autoalojadas para Navidrome

Uso:
  gordurasfm [serve] [--config RUTA]   arranca el servidor (por defecto)
  gordurasfm backup [--config RUTA]    copia la BD en <data_dir>/backups (sirve con el servidor en marcha)
  gordurasfm healthcheck [--config RUTA]  sale con 0 si el servidor responde (para Docker)
  gordurasfm version                   muestra la versión

Variables de entorno: GFM_LISTEN, GFM_DATA_DIR, GFM_MASTER_KEY_FILE, GFM_PUBLIC_URL,
GFM_NAVIDROME_URL, GFM_REGISTRATION, GFM_DEFAULT_TIMEZONE, GFM_COOKIE_SECURE,
GFM_LISTENBRAINZ_API_URL, GFM_LASTFM_API_KEY, GFM_LASTFM_API_SECRET, GFM_DNS, GFM_CONFIG, RUST_LOG.";

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut config_path: Option<String> = std::env::var("GFM_CONFIG").ok();
    let mut cmd = "serve".to_string();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--config" | "-c" => config_path = Some(it.next().context("falta la ruta tras --config")?.clone()),
            "-h" | "--help" | "help" => {
                println!("{HELP}");
                return Ok(());
            }
            "-V" | "--version" | "version" => cmd = "version".into(),
            "serve" | "backup" | "healthcheck" => cmd = a.clone(),
            other => anyhow::bail!("argumento desconocido: {other}\n\n{HELP}"),
        }
    }
    if cmd == "version" {
        println!("gordurasfm {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,tower_http=warn,sqlx=warn")),
        )
        .init();

    let cfg = Config::load(config_path.as_deref())?;
    match cmd.as_str() {
        "healthcheck" => return healthcheck(cfg.listen).await,
        "backup" => {
            let db = db::open_read_only(&cfg.db_path()).await?;
            let p = gordurasfm::backup::backup(&db, &cfg.backups_dir(), "manual", chrono::Utc::now()).await?;
            println!("{}", p.display());
            return Ok(());
        }
        _ => {}
    }
    std::fs::create_dir_all(&cfg.data_dir).with_context(|| format!("creando {}", cfg.data_dir.display()))?;
    let crypto = Crypto::load_or_create(&cfg.key_path())?;
    let pool = db::open(&cfg.db_path()).await?;
    let listen = cfg.listen;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), %listen, navidrome = %cfg.navidrome.url, bd = %cfg.db_path().display(), "arrancando GordurasFM");

    let st = state::new(cfg, pool.clone(), crypto);
    gordurasfm::spawn_background(&st);
    let app = gordurasfm::app(st);
    let listener =
        tokio::net::TcpListener::bind(listen).await.with_context(|| format!("no se puede escuchar en {listen}"))?;
    axum::serve(listener, app).with_graceful_shutdown(shutdown()).await?;
    pool.close().await;
    tracing::info!("parado");
    Ok(())
}

/// Pide `/api/v1/salud` a la propia instancia (sin curl: la imagen de Docker es `scratch`).
async fn healthcheck(listen: std::net::SocketAddr) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut addr = listen;
    if addr.ip().is_unspecified() {
        addr.set_ip(if addr.is_ipv4() {
            std::net::Ipv4Addr::LOCALHOST.into()
        } else {
            std::net::Ipv6Addr::LOCALHOST.into()
        });
    }
    let fut = async {
        let mut s = tokio::net::TcpStream::connect(addr).await?;
        s.write_all(b"GET /api/v1/salud HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").await?;
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).await?;
        anyhow::Ok(String::from_utf8_lossy(&buf).into_owned())
    };
    let resp = tokio::time::timeout(std::time::Duration::from_secs(5), fut)
        .await
        .context("sin respuesta en 5 s")?
        .with_context(|| format!("no se puede conectar a {addr}"))?;
    anyhow::ensure!(
        resp.starts_with("HTTP/1.1 200") && resp.contains("\"status\":\"ok\""),
        "respuesta inesperada: {}",
        resp.lines().next().unwrap_or("")
    );
    println!("ok");
    Ok(())
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}
