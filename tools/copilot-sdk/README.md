# Copilot SDK

Copilot SDK 提供一个面向开发者的最小源码入口：创建和静态验证 Copilot 源码组合，在
隔离的 Codex Profile 中执行 Runtime discovery probe，并用本地确定性 Responses fixture
验证一条原生 Supervisor、child Role 与 MCP Tool 正常链。

## 开始使用

需要 Python 3.11 或更高版本。在仓库根目录安装当前 checkout：

```bash
python3 -m pip install -e tools/copilot-provider-sdk -e tools/copilot-sdk
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

`dev` 先执行同一份 manifest 静态验证，再由 SDK 读取每个 `[[tools]].runtime` 指向的
`runtime.toml`。Tool 只声明 Python/Node 项目、直接依赖清单、hash lock、server entry、参数与
typed 环境绑定；SDK 在 Profile、Tool source 和 Workspace 之外准备依赖、构建非 editable
Python wheel 或执行 `npm ci --ignore-scripts`，并生成一次性的 selected capability Plugin
投影。Tool source 不复制进 Profile/Runtime projection，也不被写入；Python source 只在
SDK-owned 临时 build root 做 staged wheel build。Tool 不提供安装脚本、launcher、`.mcp.json` 或
`.codex-plugin` transport。环境准备失败会返回 typed `EnvironmentUnavailable`，不会启动
Runtime 并伪装 ready。Workspace 必须是已存在的绝对目录：

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
`--manifest REL`、`--codex-bin PATH` 和 `--json` 覆盖默认值或取得有界机器输出。Tool
依赖环境默认位于 SDK 的本机持久缓存；`--build-store-root DIR` 指定稳定的共享 build store，
`--tool-environment-root DIR`（`dev`/`test`）或 `--output-root DIR`（`prepare`）只指定当前 package 的 descriptor 输出根。相同 Tool fingerprint
的不同 Copilot 组合共享一套 immutable build；pip/npm cache 位于 build store 的稳定 `cache/`
根，不进入 fingerprint build 目录。Profile 与 Runtime 状态仍保持临时隔离。

## 原生正常链验收

`init` 生成的 `[[tests]]` 声明一个有界正常用例。运行：

```bash
copilot test ./scratch/example-copilot --workspace "$PWD"
```

`test` 使用隔离 Profile 和本地确定性 Responses fixture，但执行真实 Codex app-server 协议：
官方 Skill discovery、Thread/Turn、原生 Agent spawn/wait、Role-local MCP 调用和终态事件都必须
完成。通过条件来自 canonical Runtime 事件和精确 Tool 参数/结构化结果，不读取最终回答文本。
有界成功结果只公开组合摘要、fixture Provider、声明组件和终态证据，不公开 Runtime Thread/Turn
ID、绝对路径或原始请求。

`init` 生成的 Python Tool 自带 `pyproject.toml`、带 SHA-256 的 `requirements.lock` 和
`runtime.toml`。Python 依赖由 SDK 用 `pip --require-hashes` 安装，Tool source 经 staged wheel
构建后以 `--no-deps` 非 editable 安装；声明的 Node 项目由 SDK 在外置环境执行
`npm ci --prefer-offline --no-audit --no-fund --ignore-scripts`。`dev` 和 `test` 复用同一套通用 provisioner；Runtime 启动和 MCP
握手阶段不安装依赖。组合中只修改 Skill、Agent 或提示词时，缓存会更新组合描述，但不会
重新安装未变化的 Tool 依赖。

Tool 若需要平台提供的领域无关 provider primitives，必须同时在 Python 项目依赖和
`runtime.toml` 的 `platform_packages` 中声明 `open-web-codex-provider-sdk`。Copilot SDK 只从
单一 registry 解析当前 SDK 环境里已安装的 distribution，再把它作为 wheel 注入外置 Tool
环境；Tool manifest 不接受平台源码路径，也不依赖 `PYTHONPATH` 或运行时安装。

平台本地启动使用同一编译入口：

```bash
copilot prepare copilots/warehouse-network \
  --output-root /absolute/platform-data/copilot-environment \
  --build-store-root /absolute/platform-data/tool-builds
```

`prepare` 只返回有界的 Copilot、capability-root 与 server 标识，并在给定的 SDK-owned output
root 写入内部 `copilot-sdk/prepared-tools.v1.json`；实际依赖环境写入统一的 build store。该描述符包含已准备的 stdio transport、参数
和 typed 环境绑定，以及 `copilot.toml` 已验证的 delivery 声明，供 Platform Server 在具体 Profile
下解析；它不固定 MCP server cwd，Runtime
会使用 Thread 已授权的 Workspace cwd。该描述符不是 Browser DTO，也不包含
安装命令、安装日志或 Secret。

所有 package prepare 成功后，launcher 才调用 `copilot gc-builds`，传入本次成功 prepare 的全部
`--active-package-id` 和可信 `--packages-root`；GC 只把 active package 的 exact descriptor 当作引用真相，
只保留命令/依赖实际引用的 fingerprint build，并清理被当前 manifest ID 明确取代的旧 package
环境；未知目录保留并计数。任一 prepare 失败都会停止并跳过 GC。GC 会拒绝
越界、symlink、marker 不匹配或 malformed descriptor。Windows 没有 `fcntl` 时使用 bounded
mkdir lock；遇到 stale lock 会返回 typed `EnvironmentUnavailable`，不会默默删除或绕过锁。

`[[deliveries]]` 用精确 MCP `server`/`tool` 声明显式交付。目前只有固定 envelope 的
`workspace_artifact`（JSON Schema 或 Markdown marker 验证）与 `inline_geojson_map_card`；
Platform 不从模型文字推断交付，也不会把普通 MCP Resource 自动提升为 Artifact。

## 显式 source root

`validate` 的第一个参数始终是显式 source root。`copilot.toml` 中的 `path`、`role`、
`root` 和 `runtime` 都相对于这个 source root 解析，而不是相对于 manifest 所在目录解析。默认 manifest
是 source root 下的 `copilot.toml`；monorepo 可以通过 `--manifest` 指定 source root 内的
其他位置。

仓网 Copilot 是完整的开发者参考工程。请从仓库根目录运行：

```bash
copilot validate copilots/warehouse-network
```

该 reference 声明真实的三项 Skill、两项 Runtime Role，以及 Role TOML 使用的
`supply_chain` 和 `map_utils` Tool ID。

## 当前边界

当前交付没有生产 Profile 安装、真实生产模型质量验收、readiness 持久化/聚合或 Web 创作体验。
`dev` 是无模型 discovery gate；`test` 是使用本地确定性 Provider 的原生正常链 gate。两者都
不是安装链。CLI 只提供 `init`、`validate`、`prepare`、`dev` 和 `test`；Tool source 的唯一
运行声明是 `runtime.toml`，不提供独立 Tool package、source Plugin/MCP transport 或 launcher 入口。

Settings 中的 Agents 管理 Codex Runtime Role；它不是 Copilot 创建、安装或 readiness 页面。

完整的新手流程见[开发者快速开始](../../docs/tutorials/copilot-developer-quickstart.md)。
