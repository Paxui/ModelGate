use regex::Regex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ModelGateError;

pub const RULE_FILE_FORMAT: &str = "modelgate-visual-rules";
pub const RULE_FILE_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleFile {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub exported_at: Option<String>,
    pub rules: Vec<RuleProgram>,
}

impl RuleFile {
    pub fn validate(&self) -> Result<(), ModelGateError> {
        if self.format != RULE_FILE_FORMAT {
            return Err(ModelGateError::Rule("不是 ModelGate 可视化规则文件".into()));
        }
        if self.version != RULE_FILE_VERSION {
            return Err(ModelGateError::Rule(format!(
                "不支持规则文件版本 {}",
                self.version
            )));
        }
        if self.rules.len() > 256 {
            return Err(ModelGateError::Rule("单个文件最多包含 256 条规则".into()));
        }
        for rule in &self.rules {
            rule.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleProgram {
    pub id: Uuid,
    pub name: String,
    pub event: RuleEvent,
    pub condition: ConditionExpr,
    pub actions: Vec<RuleAction>,
}

impl RuleProgram {
    pub fn validate(&self) -> Result<(), ModelGateError> {
        if self.name.trim().is_empty() {
            return Err(ModelGateError::Rule("规则名称不能为空".into()));
        }
        if self.name.len() > 200 {
            return Err(ModelGateError::Rule(format!("规则“{}”名称过长", self.name)));
        }
        if self.actions.is_empty() {
            return Err(ModelGateError::Rule(format!("规则“{}”缺少动作", self.name)));
        }
        if self.actions.len() > 16 {
            return Err(ModelGateError::Rule(format!("规则“{}”动作过多", self.name)));
        }
        validate_condition(&self.condition, 0, &self.name)
    }
}

fn validate_condition(
    condition: &ConditionExpr,
    depth: usize,
    rule_name: &str,
) -> Result<(), ModelGateError> {
    if depth > 32 {
        return Err(ModelGateError::Rule(format!(
            "规则“{rule_name}”嵌套超过 32 层"
        )));
    }
    match condition {
        ConditionExpr::RegexMatches { pattern, .. } => {
            let value = pattern.value();
            if value.is_empty() {
                return Err(ModelGateError::Rule(format!("规则“{rule_name}”的正则为空")));
            }
            if value.len() > 4096 {
                return Err(ModelGateError::Rule(format!("规则“{rule_name}”的正则过长")));
            }
            Regex::new(value).map_err(|error| {
                ModelGateError::Rule(format!("规则“{rule_name}”包含无效正则：{error}"))
            })?;
        }
        ConditionExpr::And { left, right } | ConditionExpr::Or { left, right } => {
            validate_condition(left, depth + 1, rule_name)?;
            validate_condition(right, depth + 1, rule_name)?;
        }
        ConditionExpr::Not { value } => validate_condition(value, depth + 1, rule_name)?,
        _ => {}
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelRole {
    Cheap,
    Advanced,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleEvent {
    AnswerCompleted { role: ModelRole },
    RequestFinished { role: ModelRole },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextExpr {
    Reasoning,
    FinalAnswer,
    UserMessage,
    SystemPrompt,
    Literal { value: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum NumberExpr {
    ReasoningTokens,
    OutputTokens,
    ResponseTimeMs,
    HttpStatus,
    Literal { value: f64 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PatternExpr {
    Literal { value: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConditionExpr {
    Contains {
        text: TextExpr,
        value: TextExpr,
    },
    NotContains {
        text: TextExpr,
        value: TextExpr,
    },
    TextEquals {
        left: TextExpr,
        right: TextExpr,
    },
    RegexMatches {
        text: TextExpr,
        pattern: PatternExpr,
    },
    GreaterThan {
        left: NumberExpr,
        right: NumberExpr,
    },
    LessThan {
        left: NumberExpr,
        right: NumberExpr,
    },
    GreaterOrEqual {
        left: NumberExpr,
        right: NumberExpr,
    },
    LessOrEqual {
        left: NumberExpr,
        right: NumberExpr,
    },
    And {
        left: Box<ConditionExpr>,
        right: Box<ConditionExpr>,
    },
    Or {
        left: Box<ConditionExpr>,
        right: Box<ConditionExpr>,
    },
    Not {
        value: Box<ConditionExpr>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleAction {
    UseCheapModel,
    UseAdvancedModel,
    RetryCurrentModel,
    LogOnly,
}

#[derive(Clone, Debug, Default)]
pub struct EvaluationContext {
    pub reasoning: String,
    pub final_answer: String,
    pub user_message: String,
    pub system_prompt: String,
    pub reasoning_tokens: f64,
    pub output_tokens: f64,
    pub response_time_ms: f64,
    pub http_status: f64,
}

/// Rule Engine 只解释 AST，不了解 GUI 积木或具体 Provider。
/// 这样导入的规则、管理 API 创建的规则和未来其他编辑器创建的规则共用同一执行语义。
pub struct RuleEngine;

impl RuleEngine {
    pub fn evaluate(
        condition: &ConditionExpr,
        context: &EvaluationContext,
    ) -> Result<bool, ModelGateError> {
        let result = match condition {
            ConditionExpr::Contains { text, value } => {
                text.eval(context).contains(&value.eval(context))
            }
            ConditionExpr::NotContains { text, value } => {
                !text.eval(context).contains(&value.eval(context))
            }
            ConditionExpr::TextEquals { left, right } => left.eval(context) == right.eval(context),
            ConditionExpr::RegexMatches { text, pattern } => Regex::new(pattern.value())
                .map_err(|error| ModelGateError::Rule(format!("无效正则表达式：{error}")))?
                .is_match(&text.eval(context)),
            ConditionExpr::GreaterThan { left, right } => left.eval(context) > right.eval(context),
            ConditionExpr::LessThan { left, right } => left.eval(context) < right.eval(context),
            ConditionExpr::GreaterOrEqual { left, right } => {
                left.eval(context) >= right.eval(context)
            }
            ConditionExpr::LessOrEqual { left, right } => left.eval(context) <= right.eval(context),
            ConditionExpr::And { left, right } => {
                Self::evaluate(left, context)? && Self::evaluate(right, context)?
            }
            ConditionExpr::Or { left, right } => {
                Self::evaluate(left, context)? || Self::evaluate(right, context)?
            }
            ConditionExpr::Not { value } => !Self::evaluate(value, context)?,
        };
        Ok(result)
    }
}

impl TextExpr {
    fn eval(&self, context: &EvaluationContext) -> String {
        match self {
            Self::Reasoning => context.reasoning.clone(),
            Self::FinalAnswer => context.final_answer.clone(),
            Self::UserMessage => context.user_message.clone(),
            Self::SystemPrompt => context.system_prompt.clone(),
            Self::Literal { value } => value.clone(),
        }
    }
}

impl NumberExpr {
    fn eval(&self, context: &EvaluationContext) -> f64 {
        match self {
            Self::ReasoningTokens => context.reasoning_tokens,
            Self::OutputTokens => context.output_tokens,
            Self::ResponseTimeMs => context.response_time_ms,
            Self::HttpStatus => context.http_status,
            Self::Literal { value } => *value,
        }
    }
}

impl PatternExpr {
    fn value(&self) -> &str {
        match self {
            Self::Literal { value } => value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_nested_condition() {
        let condition = ConditionExpr::And {
            left: Box::new(ConditionExpr::Contains {
                text: TextExpr::FinalAnswer,
                value: TextExpr::Literal {
                    value: "error".into(),
                },
            }),
            right: Box::new(ConditionExpr::GreaterThan {
                left: NumberExpr::OutputTokens,
                right: NumberExpr::Literal { value: 10.0 },
            }),
        };
        let context = EvaluationContext {
            final_answer: "an error occurred".into(),
            output_tokens: 11.0,
            ..Default::default()
        };
        assert!(RuleEngine::evaluate(&condition, &context).unwrap());
    }

    #[test]
    fn rejects_invalid_regex() {
        let condition = ConditionExpr::RegexMatches {
            text: TextExpr::FinalAnswer,
            pattern: PatternExpr::Literal { value: "(".into() },
        };
        assert!(RuleEngine::evaluate(&condition, &EvaluationContext::default()).is_err());
    }

    #[test]
    fn rejects_unknown_rule_file_fields_and_versions() {
        let unknown =
            r#"{"format":"modelgate-visual-rules","version":1,"rules":[],"dangerous":true}"#;
        assert!(serde_json::from_str::<RuleFile>(unknown).is_err());
        let unsupported = RuleFile {
            format: RULE_FILE_FORMAT.into(),
            version: 999,
            exported_at: None,
            rules: Vec::new(),
        };
        assert!(unsupported.validate().is_err());
    }

    #[test]
    fn supervisor_mode_is_a_single_enum_value() {
        let visual: crate::supervisor::SupervisorMode =
            serde_json::from_str(r#""visual_rules""#).unwrap();
        let local: crate::supervisor::SupervisorMode =
            serde_json::from_str(r#""local_ai""#).unwrap();
        assert_ne!(visual, local);
        assert!(serde_json::from_str::<crate::supervisor::SupervisorMode>("true").is_err());
    }
}
