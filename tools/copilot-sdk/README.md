# Copilot SDK

Copilot SDK 提供一个面向开发者的最小源码入口：创建和静态验证 Copilot 源码组合，并在
隔离的 Codex Profile 中执行一次无模型调用的 Runtime discovery probe。

## 开始使用

需要 Python 3.11 或更高版本。在仓库根目录安装当前 checkout：

```bash
python3 -m pip install -e tools/copilot-sdk
```

创建一个新的 Copilot 源码目录：

```bash
copilot init ./scratch/example-copilot --name example-copilot
```

验证该目录：

```bash
copilot validate ./scratch/example-copilot
```

`init` 生成可以立即通过静态验证的最小组合；这不表示它已经安装到 Profile，或已经具备
生产安装或模型验收。

## 隔离开发探针

`dev` 先执行同一份 manifest 静态验证，再把完整 Skill 目录树和 Role TOML 复制到隔离
Profile。Tool 不复制进 Profile，而是通过官方 `thread/start.selectedCapabilityRoots` 从源码根
选择。每个 Tool 自己拥有可执行的 `bin/setup-env` 和依赖描述；`dev` 在启动 Runtime 前调用
该入口，并只提供 Profile 外的临时 `OPEN_WEB_CODEX_DATA_DIR`。入口缺失或失败会返回 typed
`EnvironmentUnavailable`，不会继续并伪装 ready。Workspace 必须是已存在的绝对目录：

```bash
copilot dev ./scratch/example-copilot --workspace "$PWD"
```

探针依次调用官方 `initialize`/`initialized`、`skills/list`、`thread/start` 和线程范围的
`mcpServerStatus/list`。只有全部已声明 Skill 与 MCP server 都被 Runtime 观察到时才输出
`discovery_ready`；额外的用户或系统能力不会导致失败。它不启动 Turn、不调用模型，也不 spawn
Role，因此 `roleSpawn` 和 `modelAcceptance` 明确保持 `not_run`。

默认 Profile 是临时目录，成功或失败后删除。`--keep-profile` 保留这个临时 Profile；
`--profile DIR` 使用并保留显式目录，但只接受空目录或由同一 source identity 创建的目录。
app-server 的 HOME 和进程 cwd 使用 Profile 外的另一临时目录，并始终删除。可用
`--manifest REL`、`--codex-bin PATH` 和 `--json` 覆盖默认值或取得有界机器输出。

`init` 生成的 Python Tool 自带有版本边界的 `requirements.txt`、`bin/setup-env` 与仅消费已准备环境的
launcher，因此 fresh `init → dev` 不依赖宿主预装 `mcp`。自定义或非 Python Tool 也必须用自己
的 `bin/setup-env` 实现准备合同；生成的 setup 只从 `requirements.txt` 安装依赖，不安装 Tool
源码包，SDK 也不在 launcher 内隐式安装。

## 显式 source root

`validate` 的第一个参数始终是显式 source root。`copilot.toml` 中的 `path`、`role` 和
`root` 都相对于这个 source root 解析，而不是相对于 manifest 所在目录解析。默认 manifest
是 source root 下的 `copilot.toml`；monorepo 可以通过 `--manifest` 指定 source root 内的
其他位置。

仓库内置仓网 Copilot 是 monorepo reference。请从仓库根目录运行：

```bash
copilot validate . \
  --manifest apps/web/builtin/warehouse-network-copilot/copilot.toml
```

该 reference 声明真实的三项 Skill、两项 Runtime Role，以及 Role TOML 使用的
`supply_chain` 和 `map_utils` Tool ID。

## 当前边界

当前交付没有 `copilot test`、生产 Profile 安装、Role spawn、模型验收、readiness 持久化/聚合或
Web 创作体验。`dev` 是本地、一次性、无模型的 discovery gate，不是安装链。已有的
`copilot tool ...` 命令仍是高级 Tool 组件入口。

Settings 中的 Agents 管理 Codex Runtime Role；它不是 Copilot 创建、安装或 readiness 页面。

完整的新手流程见[开发者快速开始](../../docs/tutorials/copilot-developer-quickstart.md)。
