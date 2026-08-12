use std::time::Instant;

use serde_json::Value;

use crate::{
    config::{AppConfig, ModelProfile},
    error::ModelGateError,
    protocol::ChatCompletionRequest,
    provider::{Provider, ProviderResponse},
    rules::{EvaluationContext, ModelRole, RuleAction, RuleEngine, RuleEvent},
    supervisor::{SupervisorMode, evaluate_local_ai},
};

#[derive(Clone, Debug, Default)]
pub struct RouteTrace {
    pub hop_count: u8,
    pub visited_roles: Vec<ModelRole>,
    pub triggered_rules: Vec<String>,
}

impl RouteTrace {
    /// 重试和模型切换都会消耗 hop；两者都可能再次触发规则，若只计算模型切换仍可能无限重试。
    pub fn next_hop(&mut self, role: ModelRole, max_hops: u8) -> Result<(), ModelGateError> {
        if self.hop_count >= max_hops {
            tracing::warn!(hop_count = self.hop_count, "route_loop_prevented");
            return Err(ModelGateError::Rule(format!(
                "路由已达到最大跳数 {max_hops}"
            )));
        }
        self.hop_count += 1;
        self.visited_roles.push(role);
        Ok(())
    }
}

/// 完成一次请求的完整路由生命周期。每一跳都会重新触发该角色对应的规则，
/// 因而高级模型也能拥有独立规则；`RouteTrace` 负责阻止规则在角色间无限往返。
pub async fn route_chat<P: Provider>(
    provider: &P,
    config: &AppConfig,
    request: &ChatCompletionRequest,
) -> Result<ProviderResponse, ModelGateError> {
    let mut role = ModelRole::Cheap;
    let mut trace = RouteTrace::default();

    loop {
        trace.next_hop(role, config.max_route_hops)?;
        let profile = profile_for_role(config, role)?;
        let started = Instant::now();
        let response = provider
            .chat_completion(profile, request, config.server.timeout())
            .await?;
        let context =
            context_from_response(request, &response, started.elapsed().as_millis() as f64);

        let mut next_role = None;
        // 两种监管模式由枚举保证互斥：Local AI 启用时保留可视化规则数据，但不执行它们。
        // 监管调用发生在低价模型成功回答之后，它只产生布尔决策，不能替换候选回答内容。
        if config.supervisor_mode == SupervisorMode::LocalAi
            && role == ModelRole::Cheap
            && (200..300).contains(&response.status)
        {
            let judge_id = config
                .roles
                .local_supervisor_model_id
                .ok_or_else(|| ModelGateError::Config("尚未选择本地监管模型".into()))?;
            let judge = config
                .profiles
                .iter()
                .find(|profile| profile.id == judge_id)
                .ok_or_else(|| ModelGateError::Config("本地监管模型配置不存在".into()))?;
            if evaluate_local_ai(
                provider,
                judge,
                &config.local_ai_supervisor.markdown,
                &context.final_answer,
                config.server.timeout(),
            )
            .await?
            {
                next_role = Some(ModelRole::Advanced);
            }
        }
        let events = if (200..300).contains(&response.status) {
            vec![
                RuleEvent::RequestFinished { role },
                RuleEvent::AnswerCompleted { role },
            ]
        } else {
            vec![RuleEvent::RequestFinished { role }]
        };

        for event in events {
            if config.supervisor_mode != SupervisorMode::VisualRules {
                break;
            }
            for rule in config.rules.iter().filter(|rule| rule.event == event) {
                if !RuleEngine::evaluate(&rule.condition, &context)? {
                    continue;
                }
                trace.triggered_rules.push(rule.id.to_string());
                tracing::info!(rule_id=%rule.id, rule_name=%rule.name, ?role, "规则命中");
                for action in &rule.actions {
                    match action {
                        RuleAction::LogOnly => tracing::info!(rule_id=%rule.id, "规则记录日志"),
                        action => {
                            // 同一规则内最后一个路由动作生效；规则顺序保持稳定，便于用户预测结果。
                            next_role = action_role(*action, role);
                        }
                    }
                }
            }
        }

        match next_role {
            Some(target) => role = target,
            None => return Ok(response),
        }
    }
}

fn profile_for_role(config: &AppConfig, role: ModelRole) -> Result<&ModelProfile, ModelGateError> {
    let id = match role {
        ModelRole::Cheap => config.roles.cheap_model_id,
        ModelRole::Advanced => config.roles.advanced_model_id,
    }
    .ok_or_else(|| ModelGateError::Config(format!("尚未选择{role:?}模型")))?;
    config
        .profiles
        .iter()
        .find(|profile| profile.id == id)
        .ok_or_else(|| ModelGateError::Config(format!("{role:?}模型配置不存在")))
}

fn context_from_response(
    request: &ChatCompletionRequest,
    response: &ProviderResponse,
    elapsed_ms: f64,
) -> EvaluationContext {
    let message = response.body.pointer("/choices/0/message");
    let final_answer = message
        .and_then(|value| value.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let reasoning = message
        .and_then(|value| value.get("reasoning_content"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let user_message = request
        .messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .and_then(|message| message.content.as_str())
        .unwrap_or_default()
        .to_owned();
    let system_prompt = request
        .messages
        .iter()
        .find(|message| message.role == "system")
        .and_then(|message| message.content.as_str())
        .unwrap_or_default()
        .to_owned();
    EvaluationContext {
        reasoning,
        final_answer,
        user_message,
        system_prompt,
        reasoning_tokens: response
            .body
            .pointer("/usage/reasoning_tokens")
            .and_then(Value::as_f64)
            .unwrap_or_default(),
        output_tokens: response
            .body
            .pointer("/usage/completion_tokens")
            .and_then(Value::as_f64)
            .unwrap_or_default(),
        response_time_ms: elapsed_ms,
        http_status: f64::from(response.status),
    }
}

pub fn action_role(action: RuleAction, current: ModelRole) -> Option<ModelRole> {
    match action {
        RuleAction::UseCheapModel => Some(ModelRole::Cheap),
        RuleAction::UseAdvancedModel => Some(ModelRole::Advanced),
        RuleAction::RetryCurrentModel => Some(current),
        RuleAction::LogOnly => None,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use serde_json::json;
    use uuid::Uuid;

    use crate::{
        config::{ModelRoles, ServerConfig},
        protocol::ChatMessage,
        rules::{ConditionExpr, NumberExpr, RuleProgram, TextExpr},
        supervisor::{LocalAiSupervisorConfig, SupervisorMode},
    };

    use super::*;

    struct MockProvider {
        responses: Arc<Mutex<VecDeque<ProviderResponse>>>,
    }

    #[async_trait]
    impl Provider for MockProvider {
        async fn chat_completion(
            &self,
            _: &ModelProfile,
            _: &ChatCompletionRequest,
            _: std::time::Duration,
        ) -> Result<ProviderResponse, ModelGateError> {
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| ModelGateError::Provider("测试响应耗尽".into()))
        }
    }

    fn profile(id: Uuid, name: &str) -> ModelProfile {
        ModelProfile {
            id,
            display_name: name.into(),
            model_name: name.into(),
            base_url: "http://localhost/v1".into(),
            api_key: Default::default(),
        }
    }

    fn request() -> ChatCompletionRequest {
        ChatCompletionRequest {
            model: "modelgate-auto".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: json!("hello"),
                extra: Default::default(),
            }],
            stream: false,
            options: Default::default(),
        }
    }
    #[test]
    fn prevents_route_loop() {
        let mut trace = RouteTrace::default();
        trace.next_hop(ModelRole::Cheap, 2).unwrap();
        trace.next_hop(ModelRole::Advanced, 2).unwrap();
        assert!(trace.next_hop(ModelRole::Cheap, 2).is_err());
    }

    #[tokio::test]
    async fn routes_from_cheap_to_advanced_when_rule_matches() {
        let cheap_id = Uuid::new_v4();
        let advanced_id = Uuid::new_v4();
        let rule = RuleProgram {
            id: Uuid::new_v4(),
            name: "升级".into(),
            event: RuleEvent::AnswerCompleted {
                role: ModelRole::Cheap,
            },
            condition: ConditionExpr::Contains {
                text: TextExpr::FinalAnswer,
                value: TextExpr::Literal {
                    value: "error".into(),
                },
            },
            actions: vec![RuleAction::UseAdvancedModel],
        };
        let config = AppConfig {
            version: 1,
            gateway_enabled: true,
            server: ServerConfig::default(),
            profiles: vec![profile(cheap_id, "cheap"), profile(advanced_id, "advanced")],
            roles: ModelRoles {
                cheap_model_id: Some(cheap_id),
                advanced_model_id: Some(advanced_id),
                local_supervisor_model_id: None,
            },
            supervisor_mode: SupervisorMode::VisualRules,
            local_ai_supervisor: Default::default(),
            rules: vec![rule],
            max_route_hops: 3,
            debug_content_logging: false,
        };
        let provider = MockProvider {
            responses: Arc::new(Mutex::new(VecDeque::from([
                ProviderResponse {
                    status: 200,
                    body: json!({"choices":[{"message":{"content":"error"}}]}),
                },
                ProviderResponse {
                    status: 200,
                    body: json!({"choices":[{"message":{"content":"advanced answer"}}]}),
                },
            ]))),
        };
        let response = route_chat(&provider, &config, &request()).await.unwrap();
        assert_eq!(
            response.body.pointer("/choices/0/message/content").unwrap(),
            "advanced answer"
        );
    }

    #[tokio::test]
    async fn request_finished_rule_falls_back_after_http_500() {
        let cheap_id = Uuid::new_v4();
        let advanced_id = Uuid::new_v4();
        let config = AppConfig {
            version: 1,
            gateway_enabled: true,
            server: ServerConfig::default(),
            profiles: vec![profile(cheap_id, "cheap"), profile(advanced_id, "advanced")],
            roles: ModelRoles {
                cheap_model_id: Some(cheap_id),
                advanced_model_id: Some(advanced_id),
                local_supervisor_model_id: None,
            },
            supervisor_mode: SupervisorMode::VisualRules,
            local_ai_supervisor: Default::default(),
            rules: vec![RuleProgram {
                id: Uuid::new_v4(),
                name: "服务错误升级".into(),
                event: RuleEvent::RequestFinished {
                    role: ModelRole::Cheap,
                },
                condition: ConditionExpr::GreaterOrEqual {
                    left: NumberExpr::HttpStatus,
                    right: NumberExpr::Literal { value: 500.0 },
                },
                actions: vec![RuleAction::UseAdvancedModel],
            }],
            max_route_hops: 3,
            debug_content_logging: false,
        };
        let provider = MockProvider {
            responses: Arc::new(Mutex::new(VecDeque::from([
                ProviderResponse {
                    status: 500,
                    body: json!({"error":{"message":"upstream failed"}}),
                },
                ProviderResponse {
                    status: 200,
                    body: json!({"choices":[{"message":{"content":"fallback answer"}}]}),
                },
            ]))),
        };
        let response = route_chat(&provider, &config, &request()).await.unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body.pointer("/choices/0/message/content").unwrap(),
            "fallback answer"
        );
    }

    #[tokio::test]
    async fn local_ai_supervisor_escalates_without_running_visual_rules() {
        let cheap_id = Uuid::new_v4();
        let advanced_id = Uuid::new_v4();
        let judge_id = Uuid::new_v4();
        let config = AppConfig {
            version: 1,
            gateway_enabled: true,
            server: ServerConfig::default(),
            profiles: vec![
                profile(cheap_id, "cheap"),
                profile(advanced_id, "advanced"),
                profile(judge_id, "judge"),
            ],
            roles: ModelRoles {
                cheap_model_id: Some(cheap_id),
                advanced_model_id: Some(advanced_id),
                local_supervisor_model_id: Some(judge_id),
            },
            supervisor_mode: SupervisorMode::LocalAi,
            local_ai_supervisor: LocalAiSupervisorConfig {
                markdown: "发现错误时升级".into(),
            },
            rules: Vec::new(),
            max_route_hops: 3,
            debug_content_logging: false,
        };
        let provider = MockProvider {
            responses: Arc::new(Mutex::new(VecDeque::from([
                ProviderResponse {
                    status: 200,
                    body: json!({"choices":[{"message":{"content":"candidate"}}]}),
                },
                ProviderResponse {
                    status: 200,
                    body: json!({"choices":[{"message":{"content":"{\"escalate\":true}"}}]}),
                },
                ProviderResponse {
                    status: 200,
                    body: json!({"choices":[{"message":{"content":"advanced answer"}}]}),
                },
            ]))),
        };
        let response = route_chat(&provider, &config, &request()).await.unwrap();
        assert_eq!(
            response.body.pointer("/choices/0/message/content").unwrap(),
            "advanced answer"
        );
    }
}
