# Copilot 开发者快速开始

这个快速开始使用 Copilot SDK 创建、静态验证并在隔离 Profile 中发现一个最小 Copilot 源码
目录。完成后，你会得到一个声明 Supervisor Skill、child Skill、Runtime Role 和 Tool 组件的
`copilot.toml`，以及一份来自真实 Codex app-server 的 `discovery_ready` 结果。

## 准备 SDK

需要 Python 3.11 或更高版本。在仓库根目录安装当前 checkout：

```bash
python3 -m pip install -e tools/copilot-sdk
```

## 创建源码目录

```bash
copilot init ./scratch/my-copilot --name my-copilot
```

目标目录必须为空或尚不存在。生成的源码骨架可以立即执行静态验证，但其中的示例组件并不
代表已具备真实业务实现或 Runtime readiness。

## 验证组合

```bash
copilot validate ./scratch/my-copilot
```

第一个参数是显式 source root。manifest 中所有 `path`、`role` 和 `root` 都相对于这个
source root，而不是相对于 `copilot.toml` 所在目录。默认读取 source root 下的
`copilot.toml`。

验证会检查 manifest schema、组件路径、Skill 名称，以及 Role identity 和它对已声明 Skill、
Tool ID 的引用，并返回组合摘要与确定性组合描述 hash；失败会保留具体错误码和相对路径。
这是静态引用/描述合同，不证明完整 Role 配置可被 Runtime 加载；该 Runtime 门属于 Atom 2。

## 运行隔离 discovery probe

准备一个已存在的绝对 Workspace 路径，然后运行：

```bash
copilot dev ./scratch/my-copilot --workspace "$PWD"
```

`dev` 会重新验证当前 `copilot.toml`，将完整 Skill 目录树和 Role TOML 放入一次性隔离
Profile；Tool 源码不复制进 Profile，而由官方 `thread/start.selectedCapabilityRoots` 选择。
每个 Tool 必须自行提供可执行 `bin/setup-env`；`dev` 在 Runtime 前调用它，并提供隔离的
`OPEN_WEB_CODEX_DATA_DIR`。`init` 生成的示例 Tool 已通过自己的 `requirements.txt` 与 setup 入口
准备 Python/MCP 环境，launcher 不会隐式安装。入口缺失或失败会明确返回
`EnvironmentUnavailable`。随后 `dev` 完成 app-server 握手，核对 `skills/list` 与线程范围的 `mcpServerStatus/list`。全部声明项
都被发现才返回 `discovery_ready`；额外系统或用户能力被安全忽略。

默认 Profile 会在成功或失败后删除。调试时可用 `--keep-profile` 保留临时 Profile，或用
`--profile /absolute/profile-dir` 选择并保留显式目录。显式目录必须为空，或已由同一源码身份
创建；工具不会覆盖任意既有 Profile。`--json` 返回有界机器输出，`--codex-bin PATH` 可选择
Codex executable。app-server 的 HOME 与 cwd 位于 Profile 外的独立临时目录并始终删除。

这个探针不启动 Turn、不调用模型、不 spawn Role，所以只证明 Runtime discovery，明确报告
`roleSpawn=not_run` 与 `modelAcceptance=not_run`。它不是生产安装或完整运行 readiness。

## 验证原生 Supervisor 正常链

生成的 `copilot.toml` 已声明一个 `[[tests]]` 正常用例。运行：

```bash
copilot test ./scratch/my-copilot --workspace "$PWD"
```

该命令使用隔离 Profile 和本地确定性 Responses fixture，通过真实 Codex app-server 执行
Supervisor Skill 注入、声明 child Role、Role-local MCP Tool 的精确参数与结构化结果，以及
child/Root terminal。通过判定只依赖 canonical Runtime 事件，不依赖最终回答文字。JSON 成功
结果包含组合 descriptor hash 和 fixture Provider 标识，但不包含 Thread/Turn ID、绝对路径或
原始模型请求。它证明生成组合的单条正常执行链，不代表生产模型质量、完整失败矩阵或生产安装。

内置仓网 manifest 仍是 Atom 1 静态 monorepo reference；它使用 built-in 专属资产准备与 Role
适配，不提供通用 `bin/setup-env`，因此不是 `copilot dev` 的通用示例。

## 验证仓网 monorepo reference

内置仓网 Copilot 的 manifest 位于仓库子目录，但其显式 source root 是仓库根，因为它引用
仓库内其他位置的真实 Tool package。仍在仓库根目录运行：

```bash
copilot validate . \
  --manifest apps/web/builtin/warehouse-network-copilot/copilot.toml
```

这里的 Tool ID 是 `supply_chain` 与 `map_utils`，与两项 Runtime Role TOML 中的
`mcp_servers` key 一致。

## 当前边界

`copilot init`、`copilot validate`、无模型的 `copilot dev` 和本地确定性正常链 `copilot test`
是当前 Copilot 开发者入口。尚无生产 Profile 安装、真实生产模型质量验收或 Web 创作链。已有的
`copilot tool ...` 命令服务于高级 Tool 组件开发，也不构成生产运行就绪证据。

Web Settings 中的 Agents 是 Codex Runtime Role 配置，不是 Copilot Builder。当前没有从这里
创建、安装或运行上述源码目录的产品流程。
