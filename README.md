# ModelGate

ModelGate 是一个以 Rust 编写、跨平台且兼容 OpenAI API 的本地 LLM Gateway。它把模型配置与模型职责分离，并使用强类型规则 AST 执行用户定义的可视化路由策略。

## 当前状态

项目处于 v0.1 开发阶段，已经具备：

- Axum 本地管理界面与 Management API
- `GET /v1/models`
- 非流式 `POST /v1/chat/completions`
- OpenAI-Compatible Provider
- 多规则强类型 AST、后端校验与 Rule Engine
- 低价/高级模型独立事件、多动作执行与真实多跳路由
- `.mgrule` 严格导入、默认追加和规则持久化 API
- Gateway 启停和最大路由跳数防护
- 拆分后的原生 HTML、CSS、JavaScript 管理界面，由 Axum 内嵌提供
- 系统标准配置目录和原子式配置写入

GUI 拖拽树会转换成精简执行 AST，经过 Rust 后端校验和保存后才参与路由。

## 项目结构

```text
src/
├── api/          # Management API、OpenAI-Compatible API 与 Web 资源
├── config/       # 跨平台配置路径、校验与原子保存
├── protocol/     # OpenAI-Compatible 请求协议
├── provider/     # 上游模型抽象和 OpenAI-Compatible 实现
├── router/       # 多跳模型路由与循环防护
├── rules/        # 强类型规则 AST、导入校验与执行器
└── supervisor/   # Visual Rules / Local AI 互斥监管
web/
├── index.html
├── styles.css
└── app.js
```

## 运行

### 下载预编译版本

从 GitHub Releases 下载与你的系统和 CPU 匹配的压缩包，解压后直接运行 `modelgate`（Windows 为 `modelgate.exe`），无需安装 Rust 或 Cargo。每个 Release 同时提供 `SHA256SUMS` 用于校验下载文件。

当前发布二进制尚未进行代码签名，macOS Gatekeeper 或 Windows SmartScreen 可能要求你手动确认运行。

### 从源码运行

```shell
cargo run
```

默认打开 `http://127.0.0.1:8181/`。服务器或 SSH 环境使用：

```shell
cargo run -- --no-browser
```

ModelGate 默认只监听 `127.0.0.1`。

## 开发检查

```shell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

CI 会在 Ubuntu、Windows 和 macOS 上执行相同检查。所有 Provider 网络测试使用本地 Mock Server，不会调用真实付费模型。

## 安全说明

- 日志和 Debug 输出不会显示 API Key。
- Management API 返回的配置会清除 API Key。
- 默认不记录完整 Prompt、回答或 Reasoning。
- v0.1 的 API Key 仍保存在操作系统配置目录的本地配置文件中，尚未接入 Keychain、Credential Manager 或 Secret Service。这是已知技术债，不能视为加密存储。

更完整的边界、报告方式和已知限制参见 [SECURITY.md](SECURITY.md)。

版本变更记录参见 [CHANGELOG.md](CHANGELOG.md)。

## v0.1 暂不支持

- `stream=true`
- Reasoning Stream
- 中途 Abort
- 系统安全凭据库
- 远程管理、多用户认证和公网监听
- 规则撤销/重做与导入覆盖预览

这些限制均保留了清晰的扩展边界，但不会在 v0.1 中以不完整实现冒充支持。

## 许可证

ModelGate 采用双许可证发布，你可以选择以下任一许可证使用：

- [Apache License 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)
