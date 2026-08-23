# Copilot Provider SDK

> **适合谁**：维护 Python MCP provider，且需要 Workspace/Resource primitive 的开发者
> **预计时间**：5 分钟定位，20 分钟运行 cookbook
> **前置条件**：Python 3.11+、`uv`
> **完成结果**：知道何时不应引入 SDK，并能运行完整 publish/load 示例

## 定位

`open-web-codex-provider-sdk` 提供领域无关 primitive：严格 `ResourceRef`、有界 canonical JSON、
provider-owned ResourceStore、从 Codex metadata 校验 Workspace、canonical/no-follow 目录与原子
create-new 文件，以及 FastMCP Resource publish/load adapter。

它不是 Resource Broker、Platform API、领域数据库或业务算法库。

## 何时不需要

只做内存确定性计算、返回小型 structured result、既不写 Workspace 也不发布 Resource 的 Tool，
直接使用 `./scripts/copilot.sh tool init` 生成的 FastMCP 即可。未来可能使用不是新增依赖的理由。

## 一条命令运行 cookbook

```bash
./scripts/smoke-copilot-provider-example.sh
```

它在隔离环境中运行 Provider SDK 全量测试、cookbook pytest 和真实 stdio smoke，不要求
`PYTHONPATH`。完整代码与逐段解释见
[Workspace 文件与 MCP Resource cookbook](examples/record-review/README.md)。

## 接入边界

生产 Python Tool 必须同时：

1. 在 `pyproject.toml` 声明 `open-web-codex-provider-sdk>=0.1,<0.2`；
2. 在 `runtime.toml` 的 Python dependency 声明
   `platform_packages = ["open-web-codex-provider-sdk"]`；
3. 重新生成带 SHA-256 的精确 lock；
4. 不写 SDK 源码路径、`PYTHONPATH`、安装脚本或第二份 MCP transport。

业务 Tool 仍拥有领域 schema、校验、算法、公开错误码和 Resource 生命周期；Platform/Runner 仍拥有
Workspace 授权、Secret 与最终 Artifact 投影。

成功信号：SDK 与示例 pytest 全过，stdio 能完成 Tool call、Resource read 和 bounded typed error。
失败转到[Tool 准备或 stdio 排错](../../docs/developers/copilot/testing-and-troubleshooting.md#tool-准备或-stdio-失败)。
