# Changelog

ModelGate 的重要变更会记录在此文件中。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循[语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### Added

- 补充 rules、config、protocol、error、supervisor 与 API 模块测试，覆盖规则验证边界、全部条件类型求值、配置校验与密钥脱敏、错误响应映射、监管决策解析与 API 端点（共 57 个单元与集成测试）
- CI 新增 RustSec 供应链漏洞扫描（`cargo audit`）
- CI 使用 Cargo 构建缓存，缩短重复构建时间
- Dependabot 每周自动检查 Cargo 与 GitHub Actions 依赖更新

### Changed

- CI 的 `cargo clippy` 与 `cargo test` 使用 `--locked`，保证构建依赖提交的 `Cargo.lock`
- 测试构建可通过测试专用配置路径覆盖，API 保存类测试写入临时目录，不再触碰真实配置文件

### Planned

- OpenAI-Compatible Streaming
- Reasoning Stream 与请求中止
- macOS Keychain、Windows Credential Manager 和 Linux Secret Service
- 规则撤销、重做与导入覆盖预览

## [0.1.0] - 2026-08-12

### Added

- 基于 Rust、Tokio 与 Axum 的跨平台本地 LLM Gateway
- 内嵌原生 HTML、CSS 和 JavaScript 管理界面
- `GET /v1/models` OpenAI-Compatible 接口
- 非流式 `POST /v1/chat/completions` 接口
- 统一 `ModelProfile` 与低价、高级、本地监管模型职责分配
- OpenAI-Compatible Provider 与真实 `/models` 连接测试
- 多条独立可视化规则和强类型 AST
- Text、Number、Pattern、Boolean、Event 与 Action 类型约束
- 文本、数值、Regex、AND、OR、NOT 等规则条件
- 使用低价模型、使用高级模型、重试当前模型和仅记录日志动作
- 低价与高级模型独立完成/请求结束事件
- HTTP 5xx 状态规则与高级模型 fallback
- Local AI Supervisor 与 Markdown 监管规则
- Visual Rules 和 Local AI 两种互斥监管模式
- `.mgrule` JSON 导入、导出及默认追加策略
- 整条规则三级删除确认交互
- 最大路由 hop 限制与循环路由防护
- Management API、Gateway 独立启停和 `--no-browser`
- 跨平台标准配置目录和临时文件替换保存
- API Key `SecretString` 包装、管理接口脱敏和 Debug 防泄漏
- 默认仅监听 `127.0.0.1:8181`
- Ubuntu、Windows 与 macOS GitHub Actions CI
- 本地 Mock Server Provider 测试，不调用真实付费模型
- MIT OR Apache-2.0 双许可证

### Security

- 默认不记录完整 Prompt、回答、Reasoning 或 HTTP 正文
- Management API 不回显已保存的 API Key
- 规则导入拒绝未知字段、未知版本、无效 Regex 和过深 AST
- 管理请求体限制为 2 MiB
- 路由循环达到最大 hop 后记录 `route_loop_prevented` 并停止继续调用

### Known limitations

- 暂不支持 `stream=true`
- 暂不支持 Reasoning Stream 和请求中止
- API Key 尚未接入操作系统安全凭据库，仍以明文写入本地配置文件
- Management API 没有身份认证，只允许通过本机回环地址使用
- macOS 和 Windows 发布二进制尚未进行代码签名
- 暂不支持远程管理和多用户访问

[Unreleased]: https://github.com/Paxui/ModelGate/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Paxui/ModelGate/releases/tag/v0.1.0
