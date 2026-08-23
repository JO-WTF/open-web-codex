# Provider SDK cookbook：Workspace 文件与 MCP Resource

> **适合谁**：Tool 需要安全写入 Workspace，或要发布/读取 provider-owned typed Resource 的 Python 开发者
> **预计时间**：20 分钟
> **前置条件**：Python 3.11+、`uv`，位于仓库根目录
> **完成结果**：真实 stdio 下完成 create-new 文件、Resource publish/read/load，并验证重复写入的 bounded typed error

## 先判断：你可能不需要 Provider SDK

以下情况直接使用 `copilot init` / `tool init` 生成的 FastMCP Tool 即可：

- 只做内存中的确定性计算并返回小型结构化结果；
- 不写 Workspace 文件；
- 不发布需要跨调用精确读取的 MCP Resource；
- 不需要从 Codex sandbox metadata 校验当前 Workspace。

第一个 Copilot 教程就是这种情况。不要为了“以后可能用到”增加依赖。

只有需要以下 primitive 时才用 `open-web-codex-provider-sdk`：canonical/no-follow Workspace
scope、原子 create-new 文件、provider 私有 ResourceStore、严格 `ResourceRef`、有界 canonical JSON
以及 FastMCP ResourceLink/publish/load adapter。

## 运行完整 cookbook

```bash
./scripts/smoke-copilot-provider-example.sh
```

脚本使用隔离环境安装当前 Provider SDK，不设置 `PYTHONPATH`，依次运行 SDK 全量测试、cookbook
pytest 和真实 stdio smoke。期望最后看到：

```text
Provider SDK record-review stdio smoke passed
```

成功同时证明：

1. `publish_review` 从请求 metadata 校验当前 Workspace；
2. 创建 `outputs/record-review/sample.json`，同名时不覆盖；
3. 相同 typed payload 发布为私有、内容寻址 Resource；
4. MCP `resources/read` 和 `load_review` 都能按 exact ref 读取；
5. 重复写入返回 `record_review_error.v1`，同时保留 typed `structuredContent` 与同一 envelope 的
   canonical JSON `TextContent`，不泄露绝对路径或异常文本。

失败时保留第一条 pytest 或 stdio 错误，转到
[Tool 准备或 stdio 失败](../../../../docs/developers/copilot/testing-and-troubleshooting.md#tool-准备或-stdio-失败)。

## 文件导览

| 文件 | owner |
| --- | --- |
| `record_review_provider/models.py` | 领域 payload、成功结果与公开错误码 |
| `record_review_provider/service.py` | 业务校验、create-new、publish/load 组合 |
| `record_review_provider/server.py` | FastMCP Tool 与 Resource template |
| `tests/test_record_review_provider.py` | create-new、load、越界与错误 envelope |
| `tests/stdio_smoke.py` | 初始化、Tool list/call、Resource read 的真实 transport |

## 关键模式

### 1. Workspace scope 来自当前请求

Server 启动 cwd 只是启动 Workspace；每次写入仍调用 `runtime.require_workspace(ctx)`，核对
`codex/sandbox-state-meta.sandboxCwd` 与启动 scope 相同。工具参数不接受 server absolute path。

### 2. 默认 create-new

示例先创建受控目录，再用 `create_workspace_model` 原子写入相对路径。第二次写同名结果失败，不会
覆盖第一次结果。需要允许覆盖时，应先设计显式用户确认合同，不能改成静默 replace。

### 3. Resource 必须按 exact ref load

`runtime.publish` 返回包含 `{server, uri, resource_schema}` 的 `ResourceRef` 和 ResourceLink；
`load_review` 用同一个 ref 验证 server、URI prefix、schema 和 payload。示例不扫描“最新 Resource”，
也不把 Resource 内容复制进 Platform。

### 4. 错误有界且类型化

模型只看到 allowlist code、`status=error` 和 `retryable=false`。内部异常文本、绝对路径、Profile
位置和 store 布局都不跨 MCP。未知异常收敛为 `provider_failure`，但仍在 owner 层保留失败，绝不
返回伪成功。

## 接入生产 Tool 包

先用统一入口创建根级包：

```bash
./scripts/copilot.sh tool init ./tools/record-review --name record-review
```

把经过测试的领域代码移入生成包后，在 `pyproject.toml` 声明：

```toml
dependencies = [
  "mcp==1.27.1",
  "open-web-codex-provider-sdk>=0.1,<0.2",
]
```

并在生成的 `runtime.toml` Python dependency 中补：

```toml
platform_packages = ["open-web-codex-provider-sdk"]
```

随后重新生成带 hash 的 `requirements.lock`，在目标 Copilot 的 `copilot.toml` 用
`package = "record-review"` 引用，并在 Role 中精确启用其 server。不要写 SDK 源码路径、
`PYTHONPATH`、安装脚本或第二份 MCP transport。

## 成功信号与失败跳转

最终成功信号是 pytest 全过、stdio smoke 通过、重复写入保持第一份文件、exact Resource 可读、
错误 envelope 不含绝对路径。接线后的下一门是
[`./scripts/copilot.sh check`](../../../../docs/developers/copilot/reference/cli.md#check)；失败按其首个
phase 排查。
