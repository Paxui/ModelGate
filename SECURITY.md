# 安全策略

## 报告安全问题

请不要在公开 Issue 中提交 API Key、完整 Prompt、模型回答、配置文件或可直接利用的漏洞细节。维护者建立公开安全联系渠道前，请使用代码托管平台的私密安全报告功能。

报告中建议包含：

- 受影响版本和操作系统
- 最小复现步骤
- 预期影响
- 已进行的测试，注意删除所有真实凭据

## 默认安全边界

- ModelGate 固定监听 `127.0.0.1`，当前版本不提供公网监听选项。
- 默认不记录 Prompt、回答、Reasoning 或 HTTP 正文。
- API Key 使用 `SecretString` 包装，Debug 和 Management API 均不会回显密钥。
- 路由具有最大 hop 限制，避免规则循环持续消耗上游额度。
- 规则导入拒绝未知字段、未知版本、无效 Regex 和过深 AST。

## v0.1 已知限制

API Key 当前仍以明文写入操作系统标准配置目录。`SecretString` 只能降低进程内误打印和内存残留风险，不等同于磁盘加密。后续版本计划接入：

- macOS Keychain
- Windows Credential Manager
- Linux Secret Service

在此之前，请使用操作系统账户权限保护配置目录，不要共享配置文件，也不要把它加入 Git。

Management API 没有身份认证，因为服务仅面向本机回环地址。不要通过反向代理、端口转发或隧道将其暴露到其他设备。
