# Tool `runtime.toml` 参考

> **适合谁**：正在新增 Python MCP server 或修复 Tool 环境准备的开发者
> **预计时间**：10 分钟
> **前置条件**：Tool 已有 `pyproject.toml` 与带 SHA-256 的 `requirements.lock`
> **完成结果**：Tool 能在源码、Workspace 和运行态之外被确定性准备并通过 stdio discovery

`runtime.toml` 是 Tool 唯一运行声明，不是安装脚本。最小 Python 形状：

```toml
schema_version = 1

[[dependencies]]
id = "python"
kind = "python-project"
manifest = "pyproject.toml"
lock = "requirements.lock"

[[servers]]
id = "record_review"
entry = { kind = "python-module", dependency = "python", module = "record_review.server" }
args = []
startup_timeout_sec = 20
tool_timeout_sec = 60
```

## 依赖

`kind` 当前接受 `python-project` 或 `node-project`。Python lock 必须是精确版本和 SHA-256 hash；
不得包含 editable、源码路径或 include。Tool 本身由 SDK staged build 后安装，不写进 lock。

需要 Provider SDK 时，Python 项目依赖与 runtime 必须同时声明：

```toml
[[dependencies]]
id = "python"
kind = "python-project"
manifest = "pyproject.toml"
lock = "requirements.lock"
platform_packages = ["open-web-codex-provider-sdk"]
```

何时不需要它、怎样写可运行 provider，见
[Provider SDK cookbook](../../../../packages/copilot-provider-sdk/examples/record-review/README.md)。

## Server 与环境绑定

server ID 是 Role policy 和 manifest test 引用的正式 ID。`entry.module` 是可执行 Python 模块；
server stdout 只承载 MCP，普通诊断写 stderr。

可声明的 typed 环境来源：

| source | 用途 |
| --- | --- |
| `profile_home` | provider 私有持久状态根 |
| `tool_state_root` | 当前 Tool 私有状态根 |
| `dependency_root` | 已准备依赖根；必须同时指定 dependency |
| `host` | 仅允许标准 proxy 环境变量 |

不要在 runtime 中写 Secret 值、Workspace 绝对路径或启动时安装命令。Runtime 会使用当前授权
Workspace 作为 server cwd。

## 验证

```bash
./scripts/copilot.sh check ./copilots/record-review --workspace "$PWD"
```

成功信号：`prepare` 与 `dev` 均 passed，Tool 的 stdio server 被真实 app-server 观察到。失败转到
[Tool 准备或 stdio 失败](../testing-and-troubleshooting.md#tool-准备或-stdio-失败)。
