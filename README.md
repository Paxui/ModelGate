# ModelGate

ModelGate 是一个使用 Rust 编写的本地 LLM Gateway，提供 OpenAI-Compatible API、可视化多规则路由和 Local AI Supervisor。客户端只需连接 `modelgate-auto`，实际使用低价模型、高级模型或重新请求由用户配置的规则决定。

> [!IMPORTANT]
> v0.1.0 仅在 macOS Apple Silicon 上进行过实际运行测试。Windows、Linux 和 Intel Mac 版本由 GitHub Actions 完成编译与自动测试，尚未经过真实设备验证，不保证所有功能均可正常使用。

## 功能

- `GET /v1/models`
- 非流式 `POST /v1/chat/completions`
- 低价模型、高级模型和本地监管模型配置
- 多条独立可视化规则与强类型 AST
- HTTP 错误 fallback 和最大路由 hop 防护
- Visual Rules 与 Local AI 两种互斥监管模式
- `.mgrule` 和 Markdown 导入、导出
- 内嵌 Web 管理界面
- 默认仅监听 `127.0.0.1:8181`

## 从源码运行

从源码运行需要安装：

- [Git](https://git-scm.com/)
- [Rust stable](https://www.rust-lang.org/tools/install)（包含 Cargo）
- 对应平台的基础编译工具

Cargo 只负责下载依赖、编译和启动程序。编译完成后，也可以直接运行 `target/release/` 中的二进制文件。

### macOS

<details>
<summary><strong>展开 macOS 源码运行指南</strong></summary>


#### 1. 安装基础工具

打开“终端”，安装 Apple Command Line Tools：

```shell
xcode-select --install
```

如果系统提示已经安装，可以直接继续。

#### 2. 安装 Rust

推荐使用 rustup：

```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

按照提示选择默认安装。完成后关闭并重新打开终端，或者执行：

```shell
source "$HOME/.cargo/env"
```

确认安装：

```shell
rustc --version
cargo --version
```

#### 3. 下载并运行 ModelGate

```shell
git clone https://github.com/Paxui/ModelGate.git
cd ModelGate
cargo run --release
```

首次构建需要下载和编译依赖，时间可能较长。后续源码没有变化时启动会更快。

如果不希望程序自动打开浏览器：

```shell
cargo run --release -- --no-browser
```

编译一次后可以直接运行：

```shell
./target/release/modelgate
```

</details>

### Windows

<details>
<summary><strong>展开 Windows 源码运行指南</strong></summary>


> Windows 版本尚未经过真实设备测试。

#### 1. 安装 Git

从 [Git for Windows](https://git-scm.com/download/win) 下载并安装 Git。安装完成后打开 PowerShell。

#### 2. 安装 Rust 与编译工具

从 [Rust 官方安装页面](https://www.rust-lang.org/tools/install) 下载并运行 `rustup-init.exe`，选择默认的 stable MSVC 工具链。

如果安装程序提示缺少 Microsoft C++ Build Tools，请安装 Visual Studio Build Tools，并选择：

```text
Desktop development with C++（使用 C++ 的桌面开发）
```

安装完成后重新打开 PowerShell，确认：

```powershell
rustc --version
cargo --version
```

#### 3. 下载并运行 ModelGate

```powershell
git clone https://github.com/Paxui/ModelGate.git
Set-Location ModelGate
cargo run --release
```

不自动打开浏览器：

```powershell
cargo run --release -- --no-browser
```

编译一次后可以直接运行：

```powershell
.\target\release\modelgate.exe
```

Windows SmartScreen 可能提示程序来源未知，这是因为 v0.1.0 尚未进行代码签名。

</details>

### Linux

<details>
<summary><strong>展开 Linux（Ubuntu/Debian）源码运行指南</strong></summary>


> Linux 版本尚未经过真实设备测试。以下命令以 Ubuntu/Debian 为例，其他发行版请安装等价软件包。

#### 1. 安装基础工具

```shell
sudo apt update
sudo apt install -y build-essential curl git
```

#### 2. 安装 Rust

```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

确认安装：

```shell
rustc --version
cargo --version
```

#### 3. 下载并运行 ModelGate

```shell
git clone https://github.com/Paxui/ModelGate.git
cd ModelGate
cargo run --release
```

服务器、SSH 或无桌面环境建议禁用自动打开浏览器：

```shell
cargo run --release -- --no-browser
```

编译一次后可以直接运行：

```shell
./target/release/modelgate
```

然后在能够访问该机器桌面的浏览器中打开 `http://127.0.0.1:8181/`。ModelGate v0.1.0 固定监听回环地址，不适合直接部署为远程服务。

</details>

## 配置与使用

程序启动后会提供：

```text
管理界面：http://127.0.0.1:8181/
API Base URL：http://127.0.0.1:8181/v1
虚拟模型：modelgate-auto
```

基本步骤：

1. 在管理页面配置低价模型和高级模型。
2. Base URL 填写到 `/v1`，不要附加 `/chat/completions`。
3. 点击“测试连接”，确认上游 `/models` 可访问。
4. 选择可视化规则或 Local AI 监管模式。
5. 保存规则后，在主页启用 Gateway。
6. 在 OpenAI-Compatible 客户端中填写上述 Base URL 和虚拟模型名。

当前 ModelGate API 不验证客户端 API Key。如果客户端强制要求填写，可以使用任意非空占位值；真正的上游 API Key 应在 ModelGate 管理页面配置。

### curl 测试

macOS 或 Linux：

```shell
curl http://127.0.0.1:8181/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "modelgate-auto",
    "messages": [{"role": "user", "content": "你好"}],
    "stream": false
  }'
```

Windows PowerShell：

```powershell
$body = @{
  model = "modelgate-auto"
  messages = @(@{ role = "user"; content = "你好" })
  stream = $false
} | ConvertTo-Json -Depth 4

Invoke-RestMethod `
  -Uri "http://127.0.0.1:8181/v1/chat/completions" `
  -Method Post `
  -ContentType "application/json" `
  -Body $body
```

## 停止程序

网页中的停止按钮只会停用 LLM Gateway，管理页面仍会继续运行。要完全退出程序，请回到启动 ModelGate 的终端并按：

```text
Control + C
```

## 常见问题

### `cargo: command not found`

Rust 未安装，或者 Cargo 没有加入 PATH。使用 rustup 安装后重新打开终端；macOS/Linux 也可执行：

```shell
source "$HOME/.cargo/env"
```

### 无法访问 `127.0.0.1:8181`

- 确认启动 ModelGate 的终端仍在运行。
- 检查终端是否显示端口被占用或配置解析错误。
- 确认浏览器地址使用 `http`，不是 `https`。

### 修改端口后没有立即变化

端口设置在下次启动时生效。按 `Control + C` 完全退出，再重新启动 ModelGate。

### 如何清理编译文件

```shell
cargo clean
```

编译产物位于 `target/`，已被 Git 忽略，不会上传到 GitHub。

## 开发检查

```shell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

CI 会在 Ubuntu、Windows 和 macOS 上执行检查。Provider 网络测试使用本地 Mock Server，不会调用真实付费模型。

## 安全说明

- 日志和 Debug 输出不会显示 API Key。
- Management API 不回显已保存的 API Key。
- 默认不记录完整 Prompt、回答、Reasoning 或 HTTP 正文。
- API Key 尚未接入 Keychain、Credential Manager 或 Secret Service，仍以明文保存在本地配置文件中。
- Management API 没有身份认证，不要通过代理、隧道或端口转发将其暴露到其他设备。

详细安全边界参见 [SECURITY.md](SECURITY.md)，版本变更参见 [CHANGELOG.md](CHANGELOG.md)。

## v0.1 暂不支持

- `stream=true`
- Reasoning Stream
- 请求中途 Abort
- 系统安全凭据库
- 远程管理、多用户认证和公网监听
- 规则撤销、重做与导入覆盖预览

## 许可证

ModelGate 采用双许可证发布，你可以选择以下任一许可证使用：

- [Apache License 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)
