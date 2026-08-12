use axum::{Json, http::StatusCode, response::IntoResponse};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModelGateError {
    #[error("配置错误：{0}")]
    Config(String),
    #[error("规则错误：{0}")]
    Rule(String),
    #[error("Provider 请求失败：{0}")]
    Provider(String),
    #[error("Gateway 当前未启用")]
    GatewayDisabled,
}

impl IntoResponse for ModelGateError {
    fn into_response(self) -> axum::response::Response {
        let status = match self {
            Self::GatewayDisabled => StatusCode::SERVICE_UNAVAILABLE,
            Self::Config(_) | Self::Rule(_) => StatusCode::BAD_REQUEST,
            Self::Provider(_) => StatusCode::BAD_GATEWAY,
        };
        // 错误响应只返回整理后的消息，绝不附带 reqwest Debug 内容，以免泄露 API Key。
        (
            status,
            Json(json!({"error": {"message": self.to_string(), "type": "modelgate_error"}})),
        )
            .into_response()
    }
}
