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

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use http_body_util::BodyExt;

    use super::*;

    #[test]
    fn maps_each_error_to_expected_status() {
        let cases = [
            (ModelGateError::Config("x".into()), StatusCode::BAD_REQUEST),
            (ModelGateError::Rule("x".into()), StatusCode::BAD_REQUEST),
            (
                ModelGateError::Provider("x".into()),
                StatusCode::BAD_GATEWAY,
            ),
            (
                ModelGateError::GatewayDisabled,
                StatusCode::SERVICE_UNAVAILABLE,
            ),
        ];
        for (error, expected) in cases {
            let response = error.into_response();
            assert_eq!(response.status(), expected);
        }
    }

    #[tokio::test]
    async fn error_body_is_openai_shaped() {
        let response = ModelGateError::Provider("上游失败".into()).into_response();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"]["type"], "modelgate_error");
        assert_eq!(json["error"]["message"], "Provider 请求失败：上游失败");
    }
}
