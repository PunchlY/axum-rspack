use axum::Router;
use std::{env, ffi::OsStr, net::SocketAddr};
use tokio::{net::TcpListener, signal};
use tracing::{error, info};
use tracing_subscriber::{filter::EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

mod dev_routes;
mod watcher;

pub fn env(key: impl AsRef<OsStr>, default: &str) -> Result<String, env::VarError> {
    match env::var(key) {
        Ok(value) => Ok(value),
        Err(env::VarError::NotPresent) => Ok(default.to_string()),
        Err(error) => Err(error),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let address: SocketAddr = env("SOCKET", "127.0.0.1:3000")?.parse()?;

    let app = Router::new();

    let app = app.fallback_service(dev_routes::routes());

    let listener: TcpListener = TcpListener::bind(&address).await.unwrap();
    info!(target: "app.server", address = %listener.local_addr()?, "listening");
    let server = axum::serve(listener, app);

    let graceful = server.with_graceful_shutdown(async move {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        info!(target: "app.server", "shutting down...");
    });
    if let Err(error) = graceful.await {
        error!(target: "app.server", %error);
    }

    Ok(())
}
