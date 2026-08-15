use std::sync::atomic::Ordering;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tower_http::trace::TraceLayer;

use crate::{
    AppState,
    config::ModelProfile,
    error::ModelGateError,
    protocol::ChatCompletionRequest,
    provider::OpenAiCompatibleProvider,
    router::route_chat,
    rules::{RuleFile, RuleProgram},
    supervisor::evaluate_local_ai,
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/assets/styles.css", get(styles))
        .route("/assets/app.js", get(script))
        .route("/api/status", get(status))
        .route("/api/config", get(get_config).put(put_config))
        .route("/api/rules", get(get_rules).put(put_rules))
        .route("/api/rules/import", post(import_rules))
        .route("/api/models/test", post(test_model))
        .route("/api/supervisor/test", post(test_supervisor))
        .route("/api/gateway/start", post(start_gateway))
        .route("/api/gateway/stop", post(stop_gateway))
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat_completions))
        // 管理接口会接收规则与 Markdown；统一限制请求体，避免本地恶意页面耗尽内存。
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        // TraceLayer 只记录方法、路径、状态和耗时；请求正文不进入 tracing，避免泄露 Prompt。
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SupervisorTestRequest {
    answer: String,
}

async fn test_supervisor(
    State(state): State<AppState>,
    Json(request): Json<SupervisorTestRequest>,
) -> Result<Json<Value>, ModelGateError> {
    if request.answer.trim().is_empty() {
        return Err(ModelGateError::Config("监管测试回答不能为空".into()));
    }
    let config = state.config.read().await.clone();
    let profile_id = config
        .roles
        .local_supervisor_model_id
        .ok_or_else(|| ModelGateError::Config("尚未选择本地监管模型".into()))?;
    let profile = config
        .profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| ModelGateError::Config("本地监管模型配置不存在".into()))?;
    let provider = OpenAiCompatibleProvider::new(state.client.clone());
    let escalate = evaluate_local_ai(
        &provider,
        profile,
        &config.local_ai_supervisor.markdown,
        &request.answer,
        config.server.timeout(),
    )
    .await?;
    Ok(Json(json!({"ok": true, "escalate": escalate})))
}

async fn test_model(
    State(state): State<AppState>,
    Json(mut profile): Json<ModelProfile>,
) -> Result<Json<Value>, ModelGateError> {
    let config = state.config.read().await;
    if profile.api_key_is_empty()
        && let Some(saved) = config.profiles.iter().find(|saved| saved.id == profile.id)
    {
        profile.api_key.clone_from(&saved.api_key);
    }
    profile.validate()?;
    let provider = OpenAiCompatibleProvider::new(state.client.clone());
    provider
        .test_connection(&profile, config.server.timeout())
        .await?;
    Ok(Json(json!({"ok": true})))
}

async fn get_rules(State(state): State<AppState>) -> Json<Vec<RuleProgram>> {
    Json(state.config.read().await.rules.clone())
}

async fn put_rules(
    State(state): State<AppState>,
    Json(rules): Json<Vec<RuleProgram>>,
) -> Result<StatusCode, ModelGateError> {
    if rules.len() > 256 {
        return Err(ModelGateError::Rule("最多保存 256 条规则".into()));
    }
    for rule in &rules {
        rule.validate()?;
    }
    let mut config = state.config.write().await;
    config.rules = rules;
    config.save().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn import_rules(
    State(state): State<AppState>,
    Json(file): Json<RuleFile>,
) -> Result<Json<Value>, ModelGateError> {
    file.validate()?;
    let mut config = state.config.write().await;
    if config.rules.len() + file.rules.len() > 256 {
        return Err(ModelGateError::Rule("导入后规则总数超过 256".into()));
    }
    let imported = file.rules.len();
    // 默认追加而非覆盖；这是导入接口的安全默认值，覆盖将在单独确认流程中实现。
    config.rules.extend(file.rules);
    config.save().await?;
    Ok(Json(
        json!({"imported": imported, "total": config.rules.len()}),
    ))
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../../web/index.html"))
}

async fn styles() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../../web/styles.css"),
    )
}

async fn script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../../web/app.js"),
    )
}

#[derive(Serialize)]
struct Status {
    gateway_enabled: bool,
    version: &'static str,
}

async fn status(State(state): State<AppState>) -> Json<Status> {
    Json(Status {
        gateway_enabled: state.gateway_enabled.load(Ordering::Relaxed),
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn get_config(State(state): State<AppState>) -> Json<crate::config::AppConfig> {
    Json(state.config.read().await.redacted())
}

async fn put_config(
    State(state): State<AppState>,
    Json(mut config): Json<crate::config::AppConfig>,
) -> Result<StatusCode, ModelGateError> {
    {
        let previous = state.config.read().await;
        config.merge_retained_secrets(&previous);
    }
    config.validate()?;
    config.save().await?;
    state
        .gateway_enabled
        .store(config.gateway_enabled, Ordering::Relaxed);
    *state.config.write().await = config;
    Ok(StatusCode::NO_CONTENT)
}

async fn start_gateway(State(state): State<AppState>) -> Result<Json<Value>, ModelGateError> {
    state.gateway_enabled.store(true, Ordering::Relaxed);
    let mut config = state.config.write().await;
    config.gateway_enabled = true;
    config.save().await?;
    Ok(Json(json!({"gateway_enabled": true})))
}

async fn stop_gateway(State(state): State<AppState>) -> Result<Json<Value>, ModelGateError> {
    state.gateway_enabled.store(false, Ordering::Relaxed);
    let mut config = state.config.write().await;
    config.gateway_enabled = false;
    config.save().await?;
    Ok(Json(json!({"gateway_enabled": false})))
}

async fn models(State(state): State<AppState>) -> Json<Value> {
    let config = state.config.read().await;
    Json(
        json!({"object":"list","data":[{"id":config.server.virtual_model,"object":"model","owned_by":"modelgate"}]}),
    )
}

async fn chat_completions(
    State(state): State<AppState>,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<impl IntoResponse, ModelGateError> {
    if !state.gateway_enabled.load(Ordering::Relaxed) {
        return Err(ModelGateError::GatewayDisabled);
    }
    if request.stream {
        return Err(ModelGateError::Config("v0.1 暂不支持 stream=true".into()));
    }
    let config = state.config.read().await.clone();
    let provider = OpenAiCompatibleProvider::new(state.client.clone());
    let mut response = route_chat(&provider, &config, &request).await?;
    // 上游返回的真实模型名属于内部路由细节；客户端始终只看到 ModelGate 虚拟模型。
    // 这也避免一次请求发生模型切换后，响应模型名随最终 Provider 改变。
    if let Some(body) = response.body.as_object_mut() {
        body.insert(
            "model".into(),
            Value::String(config.server.virtual_model.clone()),
        );
    }
    tracing::info!(status = response.status, "路由请求完成");
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::BAD_GATEWAY);
    Ok((
        status,
        [(header::CONTENT_TYPE, "application/json")],
        Json(response.body),
    ))
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use uuid::Uuid;

    use super::*;

    #[tokio::test]
    async fn serves_split_web_assets() {
        let app = router(AppState::new(crate::config::AppConfig::default()));
        for (path, content_type, marker) in [
            ("/", "text/html; charset=utf-8", "ModelGate"),
            ("/assets/styles.css", "text/css; charset=utf-8", ":root"),
            (
                "/assets/app.js",
                "text/javascript; charset=utf-8",
                "loadAppConfig",
            ),
        ] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()[header::CONTENT_TYPE], content_type);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            assert!(String::from_utf8_lossy(&body).contains(marker));
        }
    }

    #[tokio::test]
    async fn disabled_gateway_returns_openai_shaped_error() {
        let app = router(AppState::new(crate::config::AppConfig::default()));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"model":"modelgate-auto","messages":[],"stream":false}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["type"], "modelgate_error");
    }

    #[tokio::test]
    async fn models_exposes_only_virtual_model() {
        let app = router(AppState::new(crate::config::AppConfig::default()));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["data"][0]["id"], "modelgate-auto");
        assert_eq!(json["data"].as_array().unwrap().len(), 1);
    }

    /// 读取响应并把 JSON 正文解析为 `Value`；空正文返回 `Value::Null`。
    async fn body_json(response: axum::response::Response) -> (StatusCode, Value) {
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, value)
    }

    fn json_request(method: &str, uri: &str, body: Value) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    fn rule_json(name: &str) -> Value {
        json!({
            "id": Uuid::new_v4(),
            "name": name,
            "event": {"type": "answer_completed", "role": "cheap"},
            "condition": {
                "type": "contains",
                "text": {"type": "final_answer"},
                "value": {"type": "literal", "value": "error"}
            },
            "actions": [{"type": "log_only"}]
        })
    }

    /// 持有写锁的 API 测试共用一把全局锁：`config.save()` 使用进程级测试路径覆盖，
    /// 并行测试若交错设置该路径会互相污染，因此这类测试必须串行执行。
    static SAVE_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// 把 `config.save()` 指向临时目录并返回该目录，测试结束自动清理。
    fn temp_config_dir() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        crate::config::set_test_config_path(directory.path().join("config.json"));
        directory
    }

    #[tokio::test]
    async fn status_reports_version_and_disabled_gateway() {
        let app = router(AppState::new(crate::config::AppConfig::default()));
        let (status, body) = body_json(
            app.oneshot(
                Request::builder()
                    .uri("/api/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["gateway_enabled"], false);
        assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn config_endpoint_redacts_api_keys() {
        let mut config = crate::config::AppConfig::default();
        config.profiles.push(ModelProfile {
            id: Uuid::new_v4(),
            display_name: "上游".into(),
            model_name: "upstream".into(),
            base_url: "https://example.com/v1".into(),
            api_key: "sk-top-secret".into(),
        });
        let app = router(AppState::new(config));
        let (status, body) = body_json(
            app.oneshot(
                Request::builder()
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(!body.to_string().contains("sk-top-secret"));
    }

    #[tokio::test]
    async fn rules_endpoint_returns_saved_rules() {
        let mut config = crate::config::AppConfig::default();
        config.rules.push(RuleProgram {
            id: Uuid::new_v4(),
            name: "现有规则".into(),
            event: crate::rules::RuleEvent::AnswerCompleted {
                role: crate::rules::ModelRole::Cheap,
            },
            condition: crate::rules::ConditionExpr::Contains {
                text: crate::rules::TextExpr::FinalAnswer,
                value: crate::rules::TextExpr::Literal {
                    value: "error".into(),
                },
            },
            actions: vec![crate::rules::RuleAction::LogOnly],
        });
        let app = router(AppState::new(config));
        let (status, body) = body_json(
            app.oneshot(
                Request::builder()
                    .uri("/api/rules")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let rules = body.as_array().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["name"], "现有规则");
    }

    #[tokio::test]
    async fn put_rules_rejects_oversized_or_invalid_before_saving() {
        let app = router(AppState::new(crate::config::AppConfig::default()));

        let too_many: Vec<Value> = (0..257).map(|_| rule_json("规则")).collect();
        let response = app
            .clone()
            .oneshot(json_request("PUT", "/api/rules", json!(too_many)))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let mut invalid = rule_json("空名称");
        invalid["name"] = Value::String("".into());
        let response = app
            .oneshot(json_request("PUT", "/api/rules", json!([invalid])))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn import_rules_rejects_invalid_or_oversized_files() {
        let app = router(AppState::new(crate::config::AppConfig::default()));

        let wrong_format = json!({"format": "other", "version": 1, "rules": []});
        let response = app
            .clone()
            .oneshot(json_request("POST", "/api/rules/import", wrong_format))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let oversized: Vec<Value> = (0..257).map(|_| rule_json("规则")).collect();
        let response = app
            .oneshot(json_request(
                "POST",
                "/api/rules/import",
                json!({"format": "modelgate-visual-rules", "version": 1, "rules": oversized}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn import_rules_appends_to_existing() {
        let _guard = SAVE_TEST_LOCK.lock().await;
        let _directory = temp_config_dir();
        let mut config = crate::config::AppConfig::default();
        config.rules.push(RuleProgram {
            id: Uuid::new_v4(),
            name: "原有规则".into(),
            event: crate::rules::RuleEvent::RequestFinished {
                role: crate::rules::ModelRole::Cheap,
            },
            condition: crate::rules::ConditionExpr::TextEquals {
                left: crate::rules::TextExpr::UserMessage,
                right: crate::rules::TextExpr::Literal { value: "hi".into() },
            },
            actions: vec![crate::rules::RuleAction::UseAdvancedModel],
        });
        let app = router(AppState::new(config));
        let file = json!({
            "format": "modelgate-visual-rules",
            "version": 1,
            "exported_at": null,
            "rules": [rule_json("导入规则")]
        });
        let (status, body) = body_json(
            app.oneshot(json_request("POST", "/api/rules/import", file))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["imported"], 1);
        assert_eq!(body["total"], 2);
    }

    #[tokio::test]
    async fn put_config_rejects_invalid_before_saving() {
        let app = router(AppState::new(crate::config::AppConfig::default()));
        let mut config = crate::config::AppConfig::default();
        config.server.port = 0;
        let response = app
            .oneshot(json_request(
                "PUT",
                "/api/config",
                serde_json::to_value(&config).unwrap(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn config_rejects_unknown_fields() {
        let app = router(AppState::new(crate::config::AppConfig::default()));
        let response = app
            .oneshot(json_request(
                "PUT",
                "/api/config",
                json!({"version": 1, "unknown": true}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn put_config_validates_merges_and_persists() {
        let _guard = SAVE_TEST_LOCK.lock().await;
        let directory = temp_config_dir();
        let state = AppState::new(crate::config::AppConfig::default());
        let app = router(state.clone());
        let config = crate::config::AppConfig {
            gateway_enabled: true,
            server: crate::config::ServerConfig {
                port: 9999,
                ..Default::default()
            },
            ..Default::default()
        };
        let response = app
            .oneshot(json_request(
                "PUT",
                "/api/config",
                serde_json::to_value(&config).unwrap(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(state.gateway_enabled.load(Ordering::Relaxed));
        let bytes = std::fs::read(directory.path().join("config.json")).unwrap();
        let saved: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(saved["server"]["port"], 9999);
        assert_eq!(saved["gateway_enabled"], true);
    }

    #[tokio::test]
    async fn start_and_stop_gateway_persist_state() {
        let _guard = SAVE_TEST_LOCK.lock().await;
        let directory = temp_config_dir();
        let state = AppState::new(crate::config::AppConfig::default());
        let app = router(state.clone());

        let (status, body) = body_json(
            app.clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/gateway/start")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["gateway_enabled"], true);
        assert!(state.gateway_enabled.load(Ordering::Relaxed));

        let (status, _) = body_json(
            app.oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/gateway/stop")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(!state.gateway_enabled.load(Ordering::Relaxed));

        let bytes = std::fs::read(directory.path().join("config.json")).unwrap();
        let saved: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(saved["gateway_enabled"], false);
    }

    #[tokio::test]
    async fn chat_completions_rejects_stream_before_routing() {
        let config = crate::config::AppConfig {
            gateway_enabled: true,
            ..Default::default()
        };
        let app = router(AppState::new(config));
        let response = app
            .oneshot(json_request(
                "POST",
                "/v1/chat/completions",
                json!({"model": "modelgate-auto", "messages": [], "stream": true}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn model_test_rejects_invalid_profile_without_network() {
        let app = router(AppState::new(crate::config::AppConfig::default()));
        let response = app
            .oneshot(json_request(
                "POST",
                "/api/models/test",
                json!({
                    "id": "00000000-0000-0000-0000-000000000000",
                    "display_name": "",
                    "model_name": "",
                    "base_url": ""
                }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
