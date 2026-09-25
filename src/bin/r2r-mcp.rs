use rmcp::{transport::stdio, ServiceExt};
use r2r_jev::mcp::R2rMcpServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = R2rMcpServer::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
