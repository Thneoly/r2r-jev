use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderValue, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use r2r_jev::mcp::{AnnotatedR2rMcpServer, RemotePrincipalServer, R2rMcpServer};
use r2r_jev::store::json_file::JsonFileEventStore;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
struct RateWindow {
    started_at: Instant,
    count: u64,
}

#[derive(Debug)]
struct RemoteHttpSecurity {
    bearer_token: Arc<str>,
    requests_per_minute: u64,
    window: Mutex<RateWindow>,
}

impl RemoteHttpSecurity {
    fn new(bearer_token: String, requests_per_minute: u64) -> Result<Self, String> {
        if bearer_token.as_bytes().len() < 32 {
            return Err("R2R_REMOTE_BEARER_TOKEN must contain at least 32 bytes".to_string());
        }
        if requests_per_minute == 0 {
            return Err("R2R_REMOTE_RATE_LIMIT_PER_MINUTE must be greater than zero".to_string());
        }
        Ok(Self {
            bearer_token: Arc::from(bearer_token),
            requests_per_minute,
            window: Mutex::new(RateWindow {
                started_at: Instant::now(),
                count: 0,
            }),
        })
    }

    fn authenticate(&self, header_value: Option<&HeaderValue>) -> bool {
        let Some(value) = header_value.and_then(|value| value.to_str().ok()) else {
            return false;
        };
        let Some(token) = value.strip_prefix("Bearer ") else {
            return false;
        };
        constant_time_eq(token.as_bytes(), self.bearer_token.as_bytes())
    }

    fn consume_rate_limit(&self) -> bool {
        let Ok(mut window) = self.window.lock() else {
            return false;
        };
        let now = Instant::now();
        if now.duration_since(window.started_at) >= Duration::from_secs(60) {
            window.started_at = now;
            window.count = 0;
        }
        if window.count >= self.requests_per_minute {
            return false;
        }
        window.count += 1;
        true
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (left, right) in left.iter().zip(right.iter()) {
        difference |= left ^ right;
    }
    difference == 0
}

async fn security_middleware(
    State(security): State<Arc<RemoteHttpSecurity>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !security.authenticate(request.headers().get(header::AUTHORIZATION)) {
        let mut response = (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Bearer"),
        );
        return response;
    }
    if !security.consume_rate_limit() {
        let mut response = (StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded").into_response();
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
        return response;
    }
    next.run(request).await
}

fn required_env(name: &str) -> anyhow::Result<String> {
    let value = std::env::var(name).map_err(|_| anyhow::anyhow!("{name} is required"))?;
    if value.trim().is_empty() {
        anyhow::bail!("{name} must not be empty");
    }
    Ok(value)
}

fn parse_scopes(raw: &str) -> anyhow::Result<HashSet<String>> {
    let scopes: HashSet<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(ToString::to_string)
        .collect();
    if scopes.is_empty() {
        anyhow::bail!("R2R_REMOTE_SCOPES must contain at least one exact scope");
    }
    Ok(scopes)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let store_path = PathBuf::from(required_env("R2R_STORE_PATH")?);
    let principal = required_env("R2R_REMOTE_PRINCIPAL")?;
    let scopes = parse_scopes(&required_env("R2R_REMOTE_SCOPES")?)?;
    let bearer_token = required_env("R2R_REMOTE_BEARER_TOKEN")?;
    let requests_per_minute = std::env::var("R2R_REMOTE_RATE_LIMIT_PER_MINUTE")
        .ok()
        .map(|value| value.parse::<u64>())
        .transpose()?
        .unwrap_or(60);
    let bind: SocketAddr = std::env::var("R2R_REMOTE_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8000".to_string())
        .parse()?;

    let insecure_non_loopback = std::env::var("R2R_REMOTE_ALLOW_INSECURE_HTTP")
        .map(|value| value == "1")
        .unwrap_or(false);
    if !bind.ip().is_loopback() && !insecure_non_loopback {
        anyhow::bail!(
            "refusing plaintext HTTP on non-loopback address {bind}; terminate TLS on a loopback proxy or explicitly set R2R_REMOTE_ALLOW_INSECURE_HTTP=1 for controlled development"
        );
    }

    let security = Arc::new(
        RemoteHttpSecurity::new(bearer_token, requests_per_minute).map_err(anyhow::Error::msg)?,
    );

    let store = JsonFileEventStore::open(&store_path).map_err(anyhow::Error::msg)?;
    let base = R2rMcpServer::try_with_store(Box::new(store)).map_err(anyhow::Error::msg)?;
    let annotated = AnnotatedR2rMcpServer::new(base);
    let remote = RemotePrincipalServer::new(annotated, principal, scopes, &store_path)
        .map_err(anyhow::Error::msg)?;

    let cancellation = CancellationToken::new();
    let service = StreamableHttpService::new(
        move || Ok(remote.clone()),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_cancellation_token(cancellation.child_token()),
    );

    let router = Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn_with_state(
            security.clone(),
            security_middleware,
        ));

    let listener = tokio::net::TcpListener::bind(bind).await?;
    eprintln!("r2r-mcp-remote listening on http://{bind}/mcp");
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            cancellation.cancel();
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_comparison_and_rate_limit_fail_closed() {
        let security = RemoteHttpSecurity::new("x".repeat(32), 2).expect("security");
        let good = HeaderValue::from_str(&format!("Bearer {}", "x".repeat(32))).expect("header");
        let bad = HeaderValue::from_static("Bearer wrong");
        assert!(security.authenticate(Some(&good)));
        assert!(!security.authenticate(Some(&bad)));
        assert!(!security.authenticate(None));
        assert!(security.consume_rate_limit());
        assert!(security.consume_rate_limit());
        assert!(!security.consume_rate_limit());
    }

    #[test]
    fn scope_parser_is_exact_and_non_empty() {
        let scopes = parse_scopes("repo:alpha, repo:beta").expect("scopes");
        assert!(scopes.contains("repo:alpha"));
        assert!(scopes.contains("repo:beta"));
        assert!(!scopes.contains("repo:*"));
        assert!(parse_scopes(" , ").is_err());
    }
}
