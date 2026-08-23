# CLI 参考

> **适合谁**：已经理解主线、需要查命令和参数的 Copilot 开发者
> **预计时间**：5 分钟
> **前置条件**：位于仓库根目录
> **完成结果**：只用 `./scripts/copilot.sh` 选择正确的开发门

统一入口：

```bash
./scripts/copilot.sh --help
```

包装器固定本仓库的共享 Tool registry、当前 checkout Runtime 和本地 build store。不要改用裸
`copilot` 命令，也不要传 `--tool-registry-root`、`--codex-bin` 或 `--build-store-root` 绕过 owner。

## `init`

```bash
./scripts/copilot.sh init PATH --name ID \
  [--template single-agent|multi-agent] [--json]
```

默认模板是 `single-agent`。目标必须不存在或为空；ID 使用小写字母、数字和连字符。

## `tool init`

```bash
./scripts/copilot.sh tool init PATH --name ID [--json]
```

创建一个未接入任何 Copilot 的根级共享 Tool 包，包含 `tool.toml`、`runtime.toml`、精确 lock、
FastMCP 示例和实现测试。接线要由每个 Copilot manifest 与 Role 显式完成。

## `validate`

```bash
./scripts/copilot.sh validate SOURCE [--manifest RELATIVE] [--json]
```

只做静态合同检查。`SOURCE` 是明确源码根；manifest 内所有路径都相对它解析。

## `dev`

```bash
./scripts/copilot.sh dev SOURCE --workspace ABSOLUTE \
  [--manifest RELATIVE] [--profile DIR] [--keep-profile] \
  [--tool-environment-root DIR] [--timeout-seconds SEC] [--json]
```

启动真实 app-server，验证 Skill 和 MCP server discovery；不启动模型 Turn，也不证明 Role spawn。
普通开发不需要保留临时环境。

## `test`

```bash
./scripts/copilot.sh test SOURCE --workspace ABSOLUTE \
  [--manifest RELATIVE] [--tool-environment-root DIR] \
  [--case TEST_ID] [--timeout-seconds SEC] [--json]
```

运行 manifest 中全部或一条本地确定性原生用例。每条用例相互隔离，并从 Runtime canonical history
核对 Tool call 和结构化结果。

## `check`

```bash
./scripts/copilot.sh check SOURCE [--workspace ABSOLUTE] \
  [--manifest RELATIVE] [--tool-env DIR] [--case TEST_ID] \
  [--timeout-seconds SEC] [--json]
```

按 `validate → prepare → dev → test` 运行；首个失败阶段停止。未提供 Workspace 和 Tool output 时
使用会自动清理的临时目录。`--tool-env` 不得指向 Platform prepared root。

## `sync`

```bash
./scripts/copilot.sh sync copilots/PACKAGE \
  [--workspace ABSOLUTE] [--timeout-seconds SEC] [--build] [--json]
```

只接受受信任 `copilots/` 根的非 symlink 一级子目录。它先跑全部 `check` case，再让本地 launcher
冷重启；默认 `--no-build`，只有确实修改平台编译产物时才传 `--build`。`sync` 不接受 `--case`。

## 成功信号与失败跳转

静态开发以 `validate` 摘要为成功；可运行开发以 `check` 四阶段为成功；装入本地服务以
`sync` 的 `healthy=true` 为成功。参数被拒绝或 phase 失败时，转到
[按阶段排错](../testing-and-troubleshooting.md#按阶段排错)。
