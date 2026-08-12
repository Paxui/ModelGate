use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{
    config::ModelProfile,
    error::ModelGateError,
    protocol::{ChatCompletionRequest, ChatMessage},
    provider::Provider,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorMode {
    #[default]
    VisualRules,
    LocalAi,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LocalAiSupervisorConfig {
    pub markdown: String,
}

/// 本地监管模型只判断现有回答是否需要升级，不参与回答用户问题。
/// 提示词要求返回最小 JSON，避免把自然语言判断误当成路由指令。
pub async fn evaluate_local_ai<P: Provider>(
    provider: &P,
    profile: &ModelProfile,
    markdown: &str,
    answer: &str,
    timeout: std::time::Duration,
) -> Result<bool, ModelGateError> {
    if markdown.trim().is_empty() {
        return Err(ModelGateError::Config("本地 AI 监管规则为空".into()));
    }
    let prompt = format!(
        "你是 ModelGate 监管器，只判断候选回答是否需要高级模型重做。\n规则：\n{markdown}\n\n候选回答：\n{answer}\n\n只返回 JSON：{{\"escalate\":true或false}}"
    );
    let request = ChatCompletionRequest {
        model: profile.model_name.clone(),
        messages: vec![ChatMessage {
            role: "user".into(),
            content: Value::String(prompt),
            extra: Map::new(),
        }],
        stream: false,
        options: Map::new(),
    };
    let response = provider.chat_completion(profile, &request, timeout).await?;
    if !(200..300).contains(&response.status) {
        return Err(ModelGateError::Provider(format!(
            "本地监管模型返回 HTTP {}",
            response.status
        )));
    }
    let content = response
        .body
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| ModelGateError::Provider("本地监管模型未返回文本判断".into()))?;
    let decision = parse_decision(content)?;
    decision
        .get("escalate")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            ModelGateError::Provider(format!("本地监管结果缺少布尔字段：{}", json!(decision)))
        })
}

fn parse_decision(content: &str) -> Result<Value, ModelGateError> {
    let trimmed = content.trim();
    // 部分兼容服务即使被要求只返回 JSON，仍会包一层 Markdown 代码围栏。
    // 只剥离完整围栏，不从任意自然语言中猜测 JSON，避免误触发高级模型。
    let json_text = if trimmed.starts_with("```") && trimmed.ends_with("```") {
        trimmed
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
    } else {
        trimmed
    };
    serde_json::from_str(json_text)
        .map_err(|_| ModelGateError::Provider("本地监管模型没有返回有效 JSON".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_and_fenced_decisions() {
        assert_eq!(
            parse_decision(r#"{"escalate":true}"#).unwrap()["escalate"],
            true
        );
        assert_eq!(
            parse_decision("```json\n{\"escalate\":false}\n```").unwrap()["escalate"],
            false
        );
    }

    #[test]
    fn rejects_explanatory_text() {
        assert!(parse_decision("建议升级 {\"escalate\":true}").is_err());
    }
}
