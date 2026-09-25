use rmcp::{transport::stdio, ServiceExt};
use r2r_jev::mcp::R2rMcpServer;
use r2r_jev::store::json_file::JsonFileEventStore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server = match std::env::var("R2R_STORE_PATH") {
        Ok(path) if !path.trim().is_empty() => {
            let store = JsonFileEventStore::open(path).map_err(anyhow::Error::msg)?;
            R2rMcpServer::try_with_store(Box::new(store)).map_err(anyhow::Error::msg)?
        }
        _ => R2rMcpServer::new(),
    };

    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
