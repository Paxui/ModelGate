use async_trait::async_trait;
use serde_json::Value;
use url::Url;

use crate::{config::ModelProfile, error::ModelGateError, protocol::ChatCompletionRequest};

#[derive(Debug)]
pub struct ProviderResponse {
    pub status: u16,
    pub body: Value,
}

/// Provider 是 Router 与上游模型之间的唯一边界。
/// Router 依赖这层抽象后，无需知道上游是云服务还是 Ollama、LM Studio 等本地服务。
#[async_trait]
pub trait Provider: Send + Sync {
    async fn chat_completion(
        &self,
        profile: &ModelProfile,
        request: &ChatCompletionRequest,
        timeout: std::time::Duration,
    ) -> Result<ProviderResponse, ModelGateError>;
}

#[derive(Clone)]
pub struct OpenAiCompatibleProvider {
    client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    pub fn chat_url(base_url: &str) -> Result<Url, ModelGateError> {
        let normalized = format!("{}/", base_url.trim_end_matches('/'));
        Url::parse(&normalized)
            .and_then(|url| url.join("chat/completions"))
            .map_err(|error| ModelGateError::Config(format!("Base URL 无效：{error}")))
    }

    pub fn models_url(base_url: &str) -> Result<Url, ModelGateError> {
        let normalized = format!("{}/", base_url.trim_end_matches('/'));
        Url::parse(&normalized)
            .and_then(|url| url.join("models"))
            .map_err(|error| ModelGateError::Config(format!("Base URL 无效：{error}")))
    }

    pub async fn test_connection(
        &self,
        profile: &ModelProfile,
        timeout: std::time::Duration,
    ) -> Result<(), ModelGateError> {
        // 使用轻量 `/models` 探测，不生成内容，也不会在连接测试中消耗模型 Token。
        let mut request = self
            .client
            .get(Self::models_url(&profile.base_url)?)
            .timeout(timeout);
        if !profile.api_key_is_empty() {
            request = request.bearer_auth(profile.exposed_api_key());
        }
        let response = request
            .send()
            .await
            .map_err(|error| ModelGateError::Provider(safe_reqwest_error(&error)))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(ModelGateError::Provider(format!(
                "上游返回 HTTP {}",
                response.status().as_u16()
            )))
        }
    }
}

#[async_trait]
impl Provider for OpenAiCompatibleProvider {
    async fn chat_completion(
        &self,
        profile: &ModelProfile,
        request: &ChatCompletionRequest,
        timeout: std::time::Duration,
    ) -> Result<ProviderResponse, ModelGateError> {
        let mut upstream = serde_json::to_value(request)
            .map_err(|error| ModelGateError::Provider(error.to_string()))?;
        upstream["model"] = Value::String(profile.model_name.clone());
        let mut builder = self
            .client
            .post(Self::chat_url(&profile.base_url)?)
            .timeout(timeout)
            .json(&upstream);
        if !profile.api_key_is_empty() {
            builder = builder.bearer_auth(profile.exposed_api_key());
        }
        let response = builder
            .send()
            .await
            .map_err(|error| ModelGateError::Provider(safe_reqwest_error(&error)))?;
        let status = response.status().as_u16();
        let body = response
            .json()
            .await
            .map_err(|error| ModelGateError::Provider(safe_reqwest_error(&error)))?;
        Ok(ProviderResponse { status, body })
    }
}

fn safe_reqwest_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "上游模型请求超时".into()
    } else if error.is_connect() {
        "无法连接上游模型服务".into()
    } else {
        "上游模型返回了无法解析的响应".into()
    }
}

#[cfg(test)]
mod tests {
    use httpmock::{Method::POST, MockServer};
    use serde_json::json;
    use uuid::Uuid;

    use crate::protocol::ChatMessage;

    use super::*;
    #[test]
    fn joins_base_url_with_or_without_slash() {
        assert_eq!(
            OpenAiCompatibleProvider::chat_url("https://example.com/v1")
                .unwrap()
                .as_str(),
            "https://example.com/v1/chat/completions"
        );
        assert_eq!(
            OpenAiCompatibleProvider::chat_url("https://example.com/v1/")
                .unwrap()
                .as_str(),
            "https://example.com/v1/chat/completions"
        );
    }

    #[tokio::test]
    async fn sends_auth_and_replaces_virtual_model_without_paid_api() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/v1/chat/completions")
                    .header("authorization", "Bearer sk-test-secret")
                    .json_body_includes(r#"{"model":"real-upstream-model"}"#);
                then.status(200).json_body(json!({
                    "id":"chatcmpl-test",
                    "model":"real-upstream-model",
                    "choices":[{"message":{"role":"assistant","content":"mock answer"}}]
                }));
            })
            .await;
        let profile = ModelProfile {
            id: Uuid::new_v4(),
            display_name: "Mock".into(),
            model_name: "real-upstream-model".into(),
            base_url: server.url("/v1"),
            api_key: "sk-test-secret".into(),
        };
        let request = ChatCompletionRequest {
            model: "modelgate-auto".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: json!("hello"),
                extra: Default::default(),
            }],
            stream: false,
            options: Default::default(),
        };
        let provider = OpenAiCompatibleProvider::new(reqwest::Client::new());
        let response = provider
            .chat_completion(&profile, &request, std::time::Duration::from_secs(2))
            .await
            .unwrap();
        mock.assert_async().await;
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body.pointer("/choices/0/message/content").unwrap(),
            "mock answer"
        );
    }
}
