use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::Context;
use modelgate::{AppState, api, config::AppConfig};
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "modelgate=info,tower_http=info".into()),
        )
        .init();

    let no_browser = std::env::args().any(|arg| arg == "--no-browser");
    let config = AppConfig::load_or_default().await?;
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), config.server.port);
    let state = AppState::new(config);
    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("无法监听 {address}"))?;
    let url = format!("http://{address}/");

    info!(%address, "ModelGate 已启动");
    if !no_browser {
        let url = url.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(error) = webbrowser::open(&url) {
                tracing::warn!(%error, "无法自动打开浏览器");
            }
        });
    }

    axum::serve(listener, api::router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("服务器运行失败")
}

async fn shutdown_signal() {
    // Axum 收到该 future 后停止接收新连接，并等待正在处理的请求自然结束。
    // 这里不直接 exit，避免配置写入或上游响应处理中途被截断。
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "无法注册关闭信号");
    }
    info!("正在安全关闭 ModelGate");
}
