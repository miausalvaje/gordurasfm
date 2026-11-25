use anyhow::{Context, Result};
use gordurasfm::{config::Config, crypto::Crypto, db, state};
use tracing_subscriber::EnvFilter;

const HELP: &str = "GordurasFM — estadísticas de escucha autoalojadas para Navidrome

Uso:
  gordurasfm [serve] [--config RUTA]   arranca el servidor (por defecto)
  gordurasfm version                   muestra la versión

Variables de entorno: GFM_LISTEN, GFM_DATA_DIR, GFM_MASTER_KEY_FILE, GFM_PUBLIC_URL,
GFM_NAVIDROME_URL, GFM_REGISTRATION, GFM_DEFAULT_TIMEZONE, GFM_COOKIE_SECURE,
GFM_LISTENBRAINZ_API_URL, RUST_LOG.";

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
            "serve" => cmd = "serve".into(),
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
