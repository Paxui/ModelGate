use std::{path::PathBuf, time::Duration};

use directories::ProjectDirs;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use tokio::fs;
use uuid::Uuid;

use crate::{
    error::ModelGateError,
    rules::RuleProgram,
    supervisor::{LocalAiSupervisorConfig, SupervisorMode},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub version: u32,
    pub gateway_enabled: bool,
    pub server: ServerConfig,
    pub profiles: Vec<ModelProfile>,
    pub roles: ModelRoles,
    pub supervisor_mode: SupervisorMode,
    pub local_ai_supervisor: LocalAiSupervisorConfig,
    pub rules: Vec<RuleProgram>,
    pub max_route_hops: u8,
    pub debug_content_logging: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            gateway_enabled: false,
            server: ServerConfig::default(),
            profiles: Vec::new(),
            roles: ModelRoles::default(),
            supervisor_mode: SupervisorMode::VisualRules,
            local_ai_supervisor: LocalAiSupervisorConfig::default(),
            rules: Vec::new(),
            max_route_hops: 3,
            debug_content_logging: false,
        }
    }
}

impl AppConfig {
    pub fn validate(&self) -> Result<(), ModelGateError> {
        if self.server.port == 0 {
            return Err(ModelGateError::Config("监听端口不能为 0".into()));
        }
        if self.server.virtual_model.trim().is_empty() {
            return Err(ModelGateError::Config("虚拟模型名称不能为空".into()));
        }
        if !(1..=16).contains(&self.max_route_hops) {
            return Err(ModelGateError::Config(
                "最大路由跳数必须在 1 到 16 之间".into(),
            ));
        }
        for profile in &self.profiles {
            profile.validate()?;
        }
        let mut profile_ids = std::collections::HashSet::new();
        if self
            .profiles
            .iter()
            .any(|profile| !profile_ids.insert(profile.id))
        {
            return Err(ModelGateError::Config("模型配置 ID 不能重复".into()));
        }
        for (label, role_id) in [
            ("低价模型", self.roles.cheap_model_id),
            ("高级模型", self.roles.advanced_model_id),
            ("本地监管模型", self.roles.local_supervisor_model_id),
        ] {
            if let Some(id) = role_id
                && !profile_ids.contains(&id)
            {
                return Err(ModelGateError::Config(format!(
                    "{label}引用了不存在的模型配置"
                )));
            }
        }
        if self.local_ai_supervisor.markdown.len() > 256 * 1024 {
            return Err(ModelGateError::Config(
                "本地 AI 监管 Markdown 不能超过 256 KiB".into(),
            ));
        }
        for rule in &self.rules {
            rule.validate()?;
        }
        Ok(())
    }

    /// 管理 API 返回配置时移除密钥，防止同机浏览器页面或错误报告读取凭据。
    /// 前端用空字符串表示“保留原密钥”，真正保存时由管理 API 合并。
    pub fn redacted(&self) -> Self {
        let mut config = self.clone();
        for profile in &mut config.profiles {
            profile.api_key = SecretString::default();
        }
        config
    }

    /// 浏览器无法读取既有密钥，因此提交空值表示“保持不变”。
    /// 合并必须发生在后端持有写锁之前，避免前端用脱敏结果意外清空所有凭据。
    pub fn merge_retained_secrets(&mut self, previous: &Self) {
        for profile in &mut self.profiles {
            if profile.api_key_is_empty()
                && let Some(old) = previous.profiles.iter().find(|old| old.id == profile.id)
            {
                profile.api_key.clone_from(&old.api_key);
            }
        }
    }

    pub fn config_path() -> Result<PathBuf, ModelGateError> {
        // 仅在测试构建中生效：允许 API 测试把配置写入临时目录，避免污染真实配置。
        #[cfg(test)]
        if let Some(path) = test_config_path() {
            return Ok(path);
        }
        ProjectDirs::from("dev", "ModelGate", "ModelGate")
            .map(|dirs| dirs.config_dir().join("config.json"))
            .ok_or_else(|| ModelGateError::Config("无法确定系统配置目录".into()))
    }

    pub async fn load_or_default() -> Result<Self, ModelGateError> {
        let path = Self::config_path()?;
        Self::load_from(&path).await
    }

    async fn load_from(path: &std::path::Path) -> Result<Self, ModelGateError> {
        match fs::read(&path).await {
            Ok(bytes) => {
                let config: Self = serde_json::from_slice(&bytes).map_err(|error| {
                    ModelGateError::Config(format!("配置文件解析失败：{error}"))
                })?;
                config.validate()?;
                Ok(config)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(ModelGateError::Config(format!("配置文件读取失败：{error}"))),
        }
    }

    /// 先写同目录临时文件再重命名，避免程序退出或磁盘短暂断开时留下半个 JSON。
    /// 临时文件与目标位于同一目录，也保证跨平台重命名不跨文件系统。
    pub async fn save(&self) -> Result<(), ModelGateError> {
        let path = Self::config_path()?;
        self.save_to(&path).await
    }

    async fn save_to(&self, path: &std::path::Path) -> Result<(), ModelGateError> {
        let parent = path
            .parent()
            .ok_or_else(|| ModelGateError::Config("配置路径无父目录".into()))?;
        fs::create_dir_all(parent)
            .await
            .map_err(|error| ModelGateError::Config(error.to_string()))?;
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| ModelGateError::Config(error.to_string()))?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, bytes)
            .await
            .map_err(|error| ModelGateError::Config(error.to_string()))?;
        replace_file(&temporary, path).await
    }
}

#[cfg(test)]
static TEST_CONFIG_PATH: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// 让测试把 `config.save()` 写入临时目录。调用方必须串行化使用，
/// 见 `src/api/mod.rs` 中持有写锁的 API 测试。
#[cfg(test)]
pub(crate) fn set_test_config_path(path: PathBuf) {
    *TEST_CONFIG_PATH.lock().unwrap() = Some(path);
}

#[cfg(test)]
fn test_config_path() -> Option<PathBuf> {
    TEST_CONFIG_PATH.lock().unwrap().clone()
}

#[cfg(not(windows))]
async fn replace_file(
    temporary: &std::path::Path,
    target: &std::path::Path,
) -> Result<(), ModelGateError> {
    fs::rename(temporary, target)
        .await
        .map_err(|error| ModelGateError::Config(error.to_string()))
}

#[cfg(windows)]
async fn replace_file(
    temporary: &std::path::Path,
    target: &std::path::Path,
) -> Result<(), ModelGateError> {
    // Windows 的 rename 不会覆盖现有目标；先删除旧文件才能保持统一保存路径。
    // 临时文件已完整写入，因此即使替换失败也仍保留可恢复的新配置。
    match fs::remove_file(target).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(ModelGateError::Config(error.to_string())),
    }
    fs::rename(temporary, target)
        .await
        .map_err(|error| ModelGateError::Config(error.to_string()))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub port: u16,
    pub virtual_model: String,
    pub request_timeout_seconds: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 8181,
            virtual_model: "modelgate-auto".into(),
            request_timeout_seconds: 120,
        }
    }
}

impl ServerConfig {
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_seconds)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// 模型连接信息与 cheap/advanced/judge 职责刻意分离。
/// 同一个 Profile 可以被重新分配职责，而不需要复制 API Key 或 Provider 参数。
pub struct ModelProfile {
    pub id: Uuid,
    pub display_name: String,
    pub model_name: String,
    pub base_url: String,
    #[serde(default, with = "secret_string_serde")]
    pub api_key: SecretString,
}

impl ModelProfile {
    pub fn api_key_is_empty(&self) -> bool {
        self.api_key.expose_secret().is_empty()
    }

    /// 只有构建 Authorization 请求头时才应调用该方法；返回值不得进入日志或错误文本。
    pub fn exposed_api_key(&self) -> &str {
        self.api_key.expose_secret()
    }

    pub fn validate(&self) -> Result<(), ModelGateError> {
        if self.display_name.trim().is_empty()
            || self.model_name.trim().is_empty()
            || self.base_url.trim().is_empty()
        {
            return Err(ModelGateError::Config(
                "显示名称、Model Name 和 Base URL 均不能为空".into(),
            ));
        }
        let url = url::Url::parse(&self.base_url)
            .map_err(|error| ModelGateError::Config(format!("Base URL 无效：{error}")))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(ModelGateError::Config(
                "Base URL 只允许 http 或 https".into(),
            ));
        }
        Ok(())
    }
}

/// `SecretString` 默认拒绝序列化，防止密钥被无意写出。
/// 配置持久化是此项目唯一明确允许的序列化边界，因此在字段上单独启用受控适配器。
mod secret_string_serde {
    use secrecy::{ExposeSecret, SecretString};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(secret: &SecretString, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(secret.expose_secret())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SecretString, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(SecretString::from)
    }
}

impl std::fmt::Debug for ModelProfile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 手写 Debug 是为了让 tracing、测试失败和 panic 链路都无法自动打印密钥。
        formatter
            .debug_struct("ModelProfile")
            .field("id", &self.id)
            .field("display_name", &self.display_name)
            .field("model_name", &self.model_name)
            .field("base_url", &self.base_url)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelRoles {
    pub cheap_model_id: Option<Uuid>,
    pub advanced_model_id: Option<Uuid>,
    pub local_supervisor_model_id: Option<Uuid>,
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn profile(id: Uuid, key: &str) -> ModelProfile {
        ModelProfile {
            id,
            display_name: "测试模型".into(),
            model_name: "test-model".into(),
            base_url: "http://127.0.0.1:1234/v1".into(),
            api_key: key.into(),
        }
    }

    #[test]
    fn redaction_and_secret_merge_never_expose_or_clear_key() {
        let id = Uuid::new_v4();
        let mut previous = AppConfig::default();
        previous.profiles.push(profile(id, "sk-sensitive"));
        let mut submitted = previous.redacted();
        assert!(submitted.profiles[0].api_key_is_empty());
        submitted.merge_retained_secrets(&previous);
        assert_eq!(submitted.profiles[0].exposed_api_key(), "sk-sensitive");
        assert!(!format!("{:?}", submitted.profiles[0]).contains("sk-sensitive"));
    }

    #[test]
    fn rejects_role_that_references_missing_profile() {
        let mut config = AppConfig::default();
        config.roles.cheap_model_id = Some(Uuid::new_v4());
        assert!(config.validate().is_err());
    }

    #[tokio::test]
    async fn saves_replaces_and_loads_config_without_partial_json() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.json");
        let mut config = AppConfig::default();
        config.server.virtual_model = "first".into();
        config.save_to(&path).await.unwrap();
        config.server.virtual_model = "second".into();
        config.save_to(&path).await.unwrap();
        let loaded = AppConfig::load_from(&path).await.unwrap();
        assert_eq!(loaded.server.virtual_model, "second");
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn validates_server_and_routing_bounds() {
        let zero_port = AppConfig {
            server: ServerConfig {
                port: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(zero_port.validate().is_err());

        let empty_model = AppConfig {
            server: ServerConfig {
                virtual_model: "   ".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(empty_model.validate().is_err());

        let zero_hops = AppConfig {
            max_route_hops: 0,
            ..Default::default()
        };
        assert!(zero_hops.validate().is_err());

        let too_many_hops = AppConfig {
            max_route_hops: 17,
            ..Default::default()
        };
        assert!(too_many_hops.validate().is_err());

        let max_hops = AppConfig {
            max_route_hops: 16,
            ..Default::default()
        };
        assert!(max_hops.validate().is_ok());
    }

    #[test]
    fn rejects_duplicate_profile_ids() {
        let id = Uuid::new_v4();
        let mut config = AppConfig::default();
        config.profiles.push(profile(id, "a"));
        config.profiles.push(profile(id, "b"));
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_oversized_supervisor_markdown() {
        let mut config = AppConfig::default();
        config.local_ai_supervisor.markdown = "x".repeat(256 * 1024 + 1);
        assert!(config.validate().is_err());
    }

    #[test]
    fn profile_validation_rejects_empty_or_invalid_urls() {
        let mut candidate = profile(Uuid::new_v4(), "key");
        candidate.display_name = "".into();
        assert!(candidate.validate().is_err());
        candidate.display_name = "正常".into();
        candidate.base_url = "not a url".into();
        assert!(candidate.validate().is_err());
        candidate.base_url = "ftp://example.com/v1".into();
        assert!(candidate.validate().is_err());
        candidate.base_url = "https://example.com/v1".into();
        assert!(candidate.validate().is_ok());
    }

    #[tokio::test]
    async fn loading_missing_broken_or_invalid_config() {
        let directory = tempdir().unwrap();

        let missing = directory.path().join("missing.json");
        let loaded = AppConfig::load_from(&missing).await.unwrap();
        assert_eq!(loaded.version, 1);

        let broken = directory.path().join("broken.json");
        std::fs::write(&broken, b"{not json").unwrap();
        assert!(AppConfig::load_from(&broken).await.is_err());

        let invalid = directory.path().join("invalid.json");
        std::fs::write(&invalid, br#"{"version":1,"server":{"port":0}}"#).unwrap();
        assert!(AppConfig::load_from(&invalid).await.is_err());
    }

    #[test]
    fn redacted_serialization_never_contains_key() {
        let mut config = AppConfig::default();
        config
            .profiles
            .push(profile(Uuid::new_v4(), "sk-super-secret"));
        let redacted = config.redacted();
        let text = serde_json::to_string(&redacted).unwrap();
        assert!(!text.contains("sk-super-secret"));
        assert!(text.contains("\"api_key\":\"\""));
    }

    #[test]
    fn merge_retained_secrets_only_matches_same_id() {
        let id = Uuid::new_v4();
        let mut previous = AppConfig::default();
        previous.profiles.push(profile(id, "sk-old"));
        let mut submitted = AppConfig::default();
        submitted.profiles.push(profile(Uuid::new_v4(), "sk-new"));
        submitted.merge_retained_secrets(&previous);
        assert_eq!(submitted.profiles[0].exposed_api_key(), "sk-new");
    }

    #[test]
    fn profile_debug_redacts_key_even_with_empty_value() {
        let candidate = profile(Uuid::new_v4(), "");
        let debug = format!("{:?}", candidate);
        assert!(debug.contains("[REDACTED]"));
    }
}
