use anyhow::Context;
use notedock_server::{auth, config::Config};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("NOTEDOCK_LOG")
                .unwrap_or_else(|_| EnvFilter::new("notedock_server=info,tower_http=info")),
        )
        .init();

    // `notedock-server hash-password <password>` prints a PHC string to paste
    // into NOTEDOCK_PASSWORD_HASH, so the plaintext never has to be deployed.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("hash-password") {
        let password = args
            .get(2)
            .context("usage: notedock-server hash-password <password>")?;
        println!("{}", auth::hash_password(password)?);
        return Ok(());
    }

    let config = Config::from_env()?;
    let bind = config.bind;
    tracing::info!(%bind, db = %config.db_path.display(), "starting NoteDock server");

    let app = notedock_server::build(config).await?;

    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("failed to bind {bind}"))?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    if let Err(err) = tokio::signal::ctrl_c().await {
        tracing::error!(%err, "failed to listen for shutdown signal");
    }
    tracing::info!("shutting down");
}
