use r2r_jev::gateway::{config::GatewayConfig, service::GatewayServer};
use rmcp::{transport::stdio, ServiceExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("R2R_GATEWAY_CONFIG").ok())
        .unwrap_or_else(|| "gateway.json".to_string());
    let config = GatewayConfig::load(&path).map_err(anyhow::Error::msg)?;
    let server = GatewayServer::start(&config)
        .await
        .map_err(anyhow::Error::msg)?;
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
