pub mod api;
pub mod config;
pub mod error;
pub mod protocol;
pub mod provider;
pub mod router;
pub mod rules;
pub mod supervisor;

use std::sync::Arc;

use config::AppConfig;
use tokio::sync::RwLock;

/// Axum、Router 与管理 API 共享的应用状态。
///
/// 配置读取远多于写入，因此使用 `RwLock` 允许请求并发读取；保存配置时整体替换，
/// 避免多个字段分步更新后被正在路由的请求看到不完整状态。
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<AppConfig>>,
    pub gateway_enabled: Arc<std::sync::atomic::AtomicBool>,
    pub client: reqwest::Client,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            gateway_enabled: Arc::new(std::sync::atomic::AtomicBool::new(config.gateway_enabled)),
            config: Arc::new(RwLock::new(config)),
            client: reqwest::Client::new(),
        }
    }
}
