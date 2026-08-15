use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
/// 只显式建模路由需要读取的字段，其余 OpenAI-Compatible 参数原样转发。
/// 这样新增 temperature 等参数时不必频繁修改 Gateway 协议结构。
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(flatten)]
    pub options: serde_json::Map<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: Value,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    #[serde(flatten)]
    pub body: serde_json::Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_request_roundtrips_and_keeps_extra_options() {
        let json = r#"{
            "model": "modelgate-auto",
            "messages": [{"role": "user", "content": "你好", "name": "alice"}],
            "stream": false,
            "temperature": 0.7,
            "max_tokens": 100
        }"#;
        let request: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.model, "modelgate-auto");
        assert!(!request.stream);
        assert_eq!(request.options["temperature"], 0.7);
        assert_eq!(request.options["max_tokens"], 100);
        assert_eq!(request.messages[0].extra["name"], "alice");

        let encoded = serde_json::to_value(&request).unwrap();
        assert_eq!(encoded["model"], "modelgate-auto");
        assert_eq!(encoded["max_tokens"], 100);
        assert_eq!(encoded["messages"][0]["name"], "alice");
    }

    #[test]
    fn stream_defaults_to_false_when_omitted() {
        let request: ChatCompletionRequest =
            serde_json::from_str(r#"{"model":"m","messages":[]}"#).unwrap();
        assert!(!request.stream);
    }

    #[test]
    fn message_content_accepts_array_content() {
        let request: ChatCompletionRequest = serde_json::from_str(
            r#"{"model":"m","messages":[{"role":"user","content":[{"type":"text","text":"hi"}]}]}"#,
        )
        .unwrap();
        assert!(request.messages[0].content.is_array());
    }
}
