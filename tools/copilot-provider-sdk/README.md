# Copilot Provider SDK

`open-web-codex-provider-sdk` 是供任意 Python MCP provider 复用的领域无关基础库。它拥有：

- strict `ResourceRef` envelope、canonical JSON codec、schema 与 payload bounds；
- provider-scoped、Workspace-isolated 的 typed Resource load/publish store；
- 从 Codex sandbox metadata 取得当前 Workspace scope，以及 canonical/no-follow、atomic
  create-new 的 Workspace 文件写入；
- 把这些 primitive 绑定为 MCP Resource read/publish 的小型 runtime adapter。

业务 Tool 仍拥有领域 schema、数据解释、算法、Tool contract 与实际 Resource 内容和生命周期；
Platform/Runner 仍拥有 Workspace 授权与 Artifact 物化。这个库不是 Resource Broker、数据库或
Platform API，也不保存跨 provider 状态。

在当前仓库开发 SDK 时同时安装两个 distribution：

```bash
python3 -m pip install -e tools/copilot-provider-sdk -e tools/copilot-sdk
```

使用它的 Tool 必须在 `pyproject.toml` 声明
`open-web-codex-provider-sdk>=0.1,<0.2`，并在 `runtime.toml` 的 Python dependency 中声明：

```toml
platform_packages = ["open-web-codex-provider-sdk"]
```

Copilot SDK 的单一 platform-package registry 验证两处声明，从当前 SDK 环境已安装的
distribution 构建 wheel，并注入外置 Tool 环境。Tool 不声明平台源码路径，不依赖
`PYTHONPATH`，Runtime 启动与用户对话期间也不安装依赖。
