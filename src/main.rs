use tomi_ai_core::{AppState, api, config::Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env().map_err(std::io::Error::other)?;
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(&config.log_filter)
        .init();
    let state = AppState::new(config)?;
    let listener = tokio::net::TcpListener::bind(state.config.http_addr).await?;
    tracing::info!(address = %state.config.http_addr, "AI gateway listening");
    let grpc = api::grpc::serve(state.clone());
    let http = axum::serve(listener, api::router(state)).with_graceful_shutdown(shutdown());
    tokio::select! {
        result = http => result?,
        result = grpc => result?,
    }
    Ok(())
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}
