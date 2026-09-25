use std::net::TcpListener as StdTcpListener;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout, Duration};

fn temp_store_path() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "r2r-remote-http-{}-{nanos}.json",
        std::process::id()
    ))
}

fn reserve_port() -> u16 {
    let listener = StdTcpListener::bind("127.0.0.1:0").expect("reserve port");
    listener.local_addr().expect("local addr").port()
}

async fn wait_until_listening(port: u16, child: &mut Child) -> anyhow::Result<()> {
    for _ in 0..100 {
        if let Some(status) = child.try_wait()? {
            anyhow::bail!("remote server exited before listening: {status}");
        }
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return Ok(());
        }
        sleep(Duration::from_millis(25)).await;
    }
    anyhow::bail!("remote server did not start listening")
}

async fn raw_http(port: u16, request: String) -> anyhow::Result<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await?;
    stream.write_all(request.as_bytes()).await?;
    stream.flush().await?;
    let mut response = Vec::new();
    timeout(Duration::from_secs(3), stream.read_to_end(&mut response)).await??;
    Ok(String::from_utf8_lossy(&response).into_owned())
}

fn status_line(response: &str) -> &str {
    response.lines().next().unwrap_or("")
}

#[tokio::test]
async fn remote_http_requires_bearer_and_accepts_mcp_initialize() -> anyhow::Result<()> {
    let port = reserve_port();
    let store_path = temp_store_path();
    let token = "r2r-test-token-0123456789-abcdef-XYZ";

    let mut child = Command::new(env!("CARGO_BIN_EXE_r2r-mcp-remote"))
        .env("R2R_STORE_PATH", &store_path)
        .env("R2R_REMOTE_PRINCIPAL", "agent:remote-test")
        .env("R2R_REMOTE_SCOPES", "repo:alpha")
        .env("R2R_REMOTE_BEARER_TOKEN", token)
        .env("R2R_REMOTE_BIND", format!("127.0.0.1:{port}"))
        .env("R2R_REMOTE_RATE_LIMIT_PER_MINUTE", "20")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let result = async {
        wait_until_listening(port, &mut child).await?;

        let unauthorized = raw_http(
            port,
            format!(
                "GET /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
            ),
        )
        .await?;
        assert!(
            status_line(&unauthorized).contains("401"),
            "expected 401, got {}\n{}",
            status_line(&unauthorized),
            unauthorized
        );
        assert!(unauthorized.to_ascii_lowercase().contains("www-authenticate: bearer"));

        let wrong_token = raw_http(
            port,
            format!(
                "GET /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer wrong-token\r\nConnection: close\r\n\r\n"
            ),
        )
        .await?;
        assert!(
            status_line(&wrong_token).contains("401"),
            "expected wrong token to return 401, got {}\n{}",
            status_line(&wrong_token),
            wrong_token
        );

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2026-07-28",
                "capabilities": {},
                "clientInfo": {"name": "r2r-remote-http-test", "version": "0.1.0"}
            }
        })
        .to_string();
        let initialized = raw_http(
            port,
            format!(
                "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            ),
        )
        .await?;
        assert!(
            status_line(&initialized).contains("200"),
            "expected successful MCP initialize, got {}\n{}",
            status_line(&initialized),
            initialized
        );
        assert!(initialized.contains("protocolVersion"));

        Ok::<(), anyhow::Error>(())
    }
    .await;

    let _ = child.kill().await;
    let _ = child.wait().await;
    let _ = std::fs::remove_file(store_path);
    result
}
