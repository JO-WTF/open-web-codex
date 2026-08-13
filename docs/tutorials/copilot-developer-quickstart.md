# Copilot 开发者快速开始

这个快速开始使用 Copilot SDK 创建、静态验证并在隔离 Profile 中发现一个最小 Copilot 源码
目录。完成后，你会得到一个声明 Supervisor Skill、child Skill、Runtime Role 和 Tool 组件的
`copilot.toml`，以及一份来自真实 Codex app-server 的 `discovery_ready` 结果。

## 准备 SDK

需要 Python 3.11 或更高版本。在仓库根目录安装当前 checkout：

```bash
python3 -m pip install -e tools/copilot-provider-sdk -e tools/copilot-sdk
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

第一个参数是显式 source root。manifest 中所有 `path`、`role`、`root` 和 `runtime` 都相对于这个
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
Profile。每个 `[[tools]]` 显式声明 `runtime`；对应 `runtime.toml` 只描述 Python/Node 项目、
直接 manifest、hash lock、server entry、参数和 typed 环境绑定。SDK 在 Profile、Tool source 与
Workspace 之外准备依赖：Python 使用 `pip --require-hashes` 和 staged 非 editable wheel，Node
使用 `npm ci --ignore-scripts`。然后 SDK 生成临时 Plugin 投影，由官方
`thread/start.selectedCapabilityRoots` 选择。Tool source 不复制进 Profile/Runtime projection，也
不被写入；Python source 只在 SDK-owned 临时 build root 做 staged wheel build。Tool 不再自带 setup、
launcher 或 transport 文件。准备失败会明确返回 `EnvironmentUnavailable`；Runtime 启动时不再
安装。随后 `dev` 完成 app-server 握手，核对 `skills/list` 与线程范围的 `mcpServerStatus/list`。全部声明项
都被发现才返回 `discovery_ready`；额外系统或用户能力被安全忽略。

需要平台 provider primitives 的 Python Tool，必须在 `pyproject.toml` 写正式 distribution
依赖，并在对应 `runtime.toml` dependency 中写
`platform_packages = ["open-web-codex-provider-sdk"]`。SDK 只接受单一 registry 中的包，并从
当前 SDK 环境的已安装 distribution 注入 Tool 环境；这里没有源码路径、`PYTHONPATH` 或隐式安装。

默认 Profile 会在成功或失败后删除。调试时可用 `--keep-profile` 保留临时 Profile，或用
`--profile /absolute/profile-dir` 选择并保留显式目录。显式目录必须为空，或已由同一源码身份
创建；工具不会覆盖任意既有 Profile。`--json` 返回有界机器输出，`--codex-bin PATH` 可选择
Codex executable。app-server 的 HOME 与 cwd 位于 Profile 外的独立临时目录并始终删除。
Tool 依赖环境默认复用 SDK 的本机持久缓存；只改 Skill、Role 或提示词不会重新安装
未变化的依赖。需要与部署准备共用一个显式目录时，给 `dev`/`test` 传入
`--tool-environment-root /absolute/cache-dir`。

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

仓网参考 Copilot 同样采用这一通用合同。平台本地启动只调用一次 `copilot prepare`，把其内部
`prepared-tools.v1.json` 交给 Server；Server 为具体 Profile 解析 `profile_home`、
`tool_state_root` 和声明的 host 环境绑定并叠加 Role policy。Browser 不读取该描述符，Runtime
启动和用户对话期间也不安装依赖。

## 验证仓网参考 Copilot

仓网 Copilot 的所有开发者源码位于同一个目录，包括 Skill、Agent Role 和 Tool。仍在仓库根目录运行：

```bash
copilot validate copilots/warehouse-network
```

这里的 capability-root ID 是 `supply_chain` 与 `map_utils`；server ID 分别来自两项 Tool 的
`runtime.toml`，并由 Runtime Role TOML 的 `plugins.<tool>.mcp_servers.<server>` policy 精确引用。

## 当前边界

`copilot init`、`copilot validate`、通用环境 `copilot prepare`、无模型的 `copilot dev` 和
本地确定性正常链 `copilot test` 是当前 Copilot 开发者入口。尚无生产
Profile 安装、真实生产模型质量验收或 Web 创作链。

Web Settings 中的 Agents 是 Codex Runtime Role 配置，不是 Copilot Builder。当前没有从这里
创建、安装或运行上述源码目录的产品流程。
