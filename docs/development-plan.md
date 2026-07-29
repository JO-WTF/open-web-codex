# open-web-codex 开发计划

## 当前状态

| 字段 | 内容 |
| --- | --- |
| 更新日期 | 2026-07-29 |
| 当前分支 | `codex/agent-architecture-features` |
| Codex 基线 | `openai/codex` `6e5a2d6b8d148a5554fdceb6f399ca45bd1c78d9` |
| 上游状态快照 | 2026-07-28 观测到 official main `95637f7056835fea66bdd0044414af480fc0fd74`，当时待同步 142；本轮未刷新且暂不同步 |
| 当前工作 | 已落地受治理的 Agent/Supervisor 发布纵向切片，继续补真实 Runtime 验证、失败恢复、Artifact 生命周期与可信门禁 |
| 中期顺序 | `docs/roadmap.md` |
| M2 详细实施 | `docs/enterprise-supervisor-copilot-plan.md` |
| 能力事实 | `docs/capability-baseline.md` |

当前 Codex 基线上的定制仍按 patch map 分类；上表只保留最近一次已验证的上游状态
快照，不把未联网刷新的提交差距描述为当前事实。本轮不执行上游同步；未来恢复同步时
仍必须通过专用 `codex/sync-upstream-*` 分支。1421 WebApp 的 CSS、页面布局
和交互保持既有产品形态；当前单用户入口不显示登录或注册，浏览器自动取得本地
Session；差异集中在该入口、`src/services/webClient.ts` Server
适配层，以及三个由完整文件哈希锁定的非视觉 Thread 上下文接线文件。平台具备原生 Profile Host、Provider 服务、
加密 Secret、持久审批、独立授权 Workspace 与租约式 Run 编排。桌面运行时、sidecar、
4732/4733 daemon Gateway、原始浏览器 RPC/SSE 和桌面发布链已经移除。认证后的
根入口与 `/web` 都只加载同一个 WebApp；旧根 App/Bridge 源码不进入生产构建，
清理工作按当前范围暂缓。

本文只记录当前里程碑和下一个里程碑的有效状态、任务与验收。完成项必须有代码、
测试或可重现运行证据；更长期的阶段只在路线图中维护。
企业多 Agent 平台是当前工作台之上的演进方向：
`docs/enterprise-agent-platform-architecture.md` 负责解释目标、取舍和收敛过程，
本文只汇总已经进入交付顺序的工作包，并继续用能力基线区分实现与验证。当前 M2
逐切片任务、真实企业案例和活动可信风险统一维护在
`docs/enterprise-supervisor-copilot-plan.md`，不在本文复制第二份详细看板。

## 执行规则

- `[x]` 已完成并有证据；`[-]` 正在执行；`[ ]` 尚未完成；`[!]` 外部阻塞。
- Codex 源码变化前运行 `scripts/codex-upstream-status.sh` 和
  `scripts/codex-customization-status.sh`，新增差异必须先进入 patch map。
- 接受上游结构后，固定按 Chat transport、Provider metadata/cache、app-server
  Provider API、TUI Provider、legacy history、Capability Manifest、生成物顺序重放。
- 平台不得复制 Thread/Turn、Memory、multi-agent、Skills、Plugins 或 MCP Runtime。
- 浏览器不得接收 raw JSON-RPC、app-server request ID、凭据、Profile/Workspace
  路径或不受限 Runtime payload。
- 每个功能实现前必须写明 owning layer、输入输出、Capability gate 和验证方式；
  若实现需要跨 WebApp、Platform app-server、Profile Host 和 Codex Runtime，必须拆成
  独立 owner 的小变更，禁止用 WebApp 拦截或启动脚本写配置绕过 Runtime 发现链路。
- 数据库、授权、协议或恢复变化必须覆盖拒绝、重试、并发或重启路径。
- 文档必须遵守 `docs/README.md` 的时间视角和权威边界；被替代的实现过程留在
  Git，仍会约束未来的决定进入 ADR。

## 当前功能主线：M2 Enterprise Supervisor Copilot

当前优先跑通一个单 Profile 真实企业案例：根 Supervisor 使用 Codex 原生多 Agent
能力创建 Data Agent 与 Network Planning Agent，通过只读数据 MCP、有界规划 MCP
和持久 Artifact 完成协作，最终报告引用关键证据。详细实施顺序、功能完成标准和
可信风险台账以
[Enterprise Supervisor Copilot 短期实施计划](enterprise-supervisor-copilot-plan.md)
为准。

M2 功能切片可以与 Gate 0 并行，不等待全部可信矩阵完成；但不得绕过 Runtime、
Workspace、授权、Artifact 和 Tool 边界。未完成的可信项限制能力声明，并在短期计划
风险台账中保留触发条件。

当前切片顺序：

1. 固定“华东新增仓”案例、数据与 Artifact Schema；
2. 打通真实多 Agent Runtime 轨迹；
3. 建立持久 Artifact 和跨子 Thread 交接；
4. 绑定版本化 Supervisor Policy 与两个 Agent Definition；
5. 接入只读数据 MCP 和有界规划 MCP；
6. 完成浏览器体验与真实企业案例 E2E。

2026-07-28 已完成首个 Supervisor 发布纵向切片：内置 Supervisor、Agent 指令与
Artifact 交付合同收敛到根目录 `capabilities/`；新增独立
`supervisor-catalog` owner、Definition/Revision/Release PostgreSQL 模型和类型化
草稿、校验、发布 API；Web Settings 提供 Supervisor Studio；发布版本通过 release
ID、完整 spec 和内容哈希进入现有 Run Policy catalog，并在 Thread 创建前重新校验。
随后完成了用户 Agent Definition 发布纵向切片：Agent Catalog 提供组织作用域的
Definition、草稿 Revision、校验和不可变 Release；用户 Agent 必须选择一个代码评审
的 capability template，只能收窄其 Artifact 合同。浏览器不提交 Runtime Role、
MCP inventory、Tool allowlist、capability roots 或文件路径；服务端派生并锁定这些
事实。Supervisor 可选择精确 Agent Release UUID，发布时持久化版本与内容哈希依赖，
Run 预检重新解析并拒绝缺失或漂移。平台现允许只有一个 Agent 的 Supervisor，支持
类似 Hello Agent 的最小发布。临时 PostgreSQL 验证覆盖空库迁移、发布解析、
跨 Organization 拒绝和精确依赖。尚未完成任意 Tool/Plugin 创作或原生 Runtime
Agent CRUD，也尚未用真实 Runtime 重跑用户发布的 Release。

代码 Package 与 Web 草稿现已收敛为同一个 Agent/Supervisor authoring contract 和
服务端语义编译器。Agent Runtime Role 只由 `definitionId + version` 稳定派生；
Supervisor 的 Role、Runtime requirement、Artifact handoff 与并发限制只由精确
Agent 发布和草稿合同派生。两种来源必须产生逐字段相等的规范化执行语义和
execution-semantics SHA-256，代码 Package 中任何派生字段漂移都会显式失败。
Supervisor 指令进一步拆为平台 Owner 发布的不可变行为合同、作者可编辑的
`customInstructions` 和服务端生成的结构化执行合同；编译器把三者确定性合成为官方
`thread/start.developerInstructions`。Web 可发布和选择精确平台合同版本，普通
Supervisor 草稿不能提交或覆盖平台指令正文，`codex/` 无新增修改。

Agent Studio 已将资源目录、不可变详情和草稿创建/编辑拆成独立页面状态。
Capability 页面从受版本控制的 Plugin/MCP 清单生成浏览器安全目录；MCP 页面把
“平台已审查的声明”与“当前 Thread 实际启用状态”并列展示，因此 `map_utils`
可以作为可用声明被查看，同时在未授权给当前 Thread 时明确显示为未启用。任意
Capability/MCP 创作仍保持显式不可用，直到具备类型化校验、隔离运行与发布合同。

Agent、Skill、MCP 的长期整改顺序统一维护在
[Agent、Skill 与 MCP 原生生命周期整改计划](agent-capability-lifecycle-plan.md)。
当前 Phase 0 删除 V1 产品兼容、伪 V2 capability、Profile 全局企业 Role 注册和
`profile_runtime_role_projections` 第二状态机。企业 Thread 只通过正式 request config
启用 V2 并引用启动前校验的 Role 文件；后续阶段再以类型化 app-server V2 CRUD
替换这一临时文件边界。

受限 happy path 已经越过“平台骨架”阶段。真实 PostgreSQL 与 Profile 上的最新
Codex Runtime 运行证明：绑定 `enterprise-supervisor-copilot@1.8.0` 的根 Thread
按顺序创建了 `data_agent` 与 `network_planning_agent` 两个真实子 Thread；前者
通过只读数据 MCP 产生并验证 `planning-dataset.v1`，后者读取同一 Resource 后再
调用有界规划 MCP；以“分析现有网络并给出建议。”为完整输入的最新重跑形成十个
ready Task Artifact 和完整决策报告。运行轨迹严格只有 Root、Data、Network 三个
Thread，33 次 MCP 调用分别归属于 Data/Network 的授权服务，Root 无业务 MCP 或命令
调用，Run 与 Task 最终均为 completed 且无活动 Turn。精确调用与 Artifact 数量取决于
有效调查步骤，门禁固定的是必需 Schema 的最小集合以及所有已注册 Artifact 必须 ready。

这不等于 M2 已全部完成。当前主线转向 happy path 没有覆盖的行为：Completed Agent
follow-up、interrupt、部分失败和审批拒绝后的综合；更深层 Agent 树导航；Artifact
替代、失效、删除与保留；更完整的重启/乱序、共享 Workspace、multi-`cwd`
和多用户隔离矩阵。它们继续限制“可恢复企业能力”和生产发布声明。

仓库现提供 `scripts/probe-enterprise-runtime-lifecycle.sh`，在既有真实企业 E2E
之后继续验证同一已完成子 Agent 的下一 ordinal、根 Turn 中断终态和中断后的恢复
Turn。探针必须在可绑定本机临时端口并可访问已配置 Provider 的环境中通过后，才能把
follow-up 或 interrupt 标为真实验证完成；脚本存在本身不构成通过证据。

浏览器已将授权 Agent 树中的子 Thread 审批提升为任务级响应队列：根对话和
Agent Activity 都渲染同一平台审批 ID 的批准/拒绝卡片，刷新后从持久审批事件重放，
提交中阻止重复决定。真实审批拒绝后的 Agent 恢复、Supervisor 综合和最终 Run
收敛仍属于上述未完成门禁。

供应链插件的两个 MCP Server 现按风险合同声明默认预批准：它们的完整 Tool 集合
仅包含只读数据访问、有界确定性计算和内部不可变 Resource 发布，并继续受精确
Agent Tool allowlist 限制。命令、文件、权限、凭据、外部副作用和未来混合风险
Server 不继承该设置。右侧 Agent/Files 面板同时增加外部点击关闭，面板内部交互、
显式关闭按钮和 Escape 行为保持不变。

## 并行可信工作：Gate 0 平台证据恢复

Gate 0 让独立 Workspace、单 Profile Runtime、PostgreSQL 迁移、安全拒绝和真实
Provider/MCP 链路获得同一组可复现证据。它现在是 M2 的并行可信与发布门禁，而非
全部功能编码的前置阶段。组件所有权以 [系统架构](architecture.md) 为准，安全门禁
以 [安全模型](security-model.md) 为准。

## 当前边界债务 TODO

这些是当前实现中仍需按边界复审或迁移的项；在完成前不得把它们宣传为完整能力：

1. [ ] `apps/web/src/features/threads/hooks/useThreadMessaging.ts` 中 `/apps`、`/status`、
   `/fast` 等本地命令需要逐项边界复审：纯 UI 状态命令可保留；凡是查询 Runtime
   capability、工具、MCP、Skills、Plugins 或模型上下文的命令必须改为 Runtime/typed
   app-server 合同，不能由 WebApp 生成模型式回答。
2. [ ] `apps/web/src/services/tauri.ts` 仍是浏览器适配兼容层命名，需在不改变 1421 UI
   行为的前提下拆名或迁移，避免继续暗示桌面/Tauri 边界存在。
3. [ ] Capability Manifest 仍有手工 Alpha 子集；必须继续收敛到由 Codex 生成事实驱动，
   Web feature policy 只能消费这些事实，不能自行声明 Runtime 支持。
4. [ ] 旧根 App/Bridge 未引用源码和 browser shims 仍待裁剪，避免未来功能误接回旧桥。

## 单 Profile 收口目标

近期目标是先让一个真实用户使用一个持久 Profile 可靠跑通，再扩展多 Profile。
该目标是部署范围收窄，不改变所有权边界：WebApp 不发现、不启动、不模拟
MCP/Skills/Plugins；Server/Profile Host 只负责单 Profile 生命周期、Workspace
授权和安全诊断；Codex Runtime 继续拥有 Thread/Turn、当前 `cwd`、Provider、
Skills、Plugins 和 MCP。

单 Profile 运行合同：

1. [ ] 启动期必须显式确定唯一 `profile_id`、`CODEX_HOME`、允许使用的
   Workspace roots、Runner root 和 source root；Real mode 缺少 `CODEX_HOME`、
   Codex `cwd` 不在授权 root 内或 root 不一致时失败并给出可诊断错误。
2. [-] Server health/profile status 返回安全摘要，能确认当前 Profile Home
   identity、Profile Host state、Codex build/protocol/capability digest、Provider
   登录/模型目录状态和 MCP startup diagnostics；浏览器仍不得接收本地路径、凭据或
   raw JSON-RPC。当前已新增 Profile runtime status 安全摘要，包含 Profile Home
   fingerprint、Runtime health、capability 计数和 MCP server status 投影；Provider
   模型目录诊断已接入安全摘要，仍需用官方 OpenAI smoke 覆盖 file-backed auth 与远端刷新错误。
3. [-] MCP startup failure 归类并投影为安全诊断：capability root 未选择、`.mcp.json`
   缺失、`cwd` 解析错误、command 不存在、权限不足、Python/venv/pip 失败、
   package import 失败、MCP initialize 失败或 timeout。当前 runtime status 已投影
   Runtime 的 MCP server status；`map_utils` 依赖准备已移到平台启动期的共享
   maps MCP venv，launcher 在对话期只做快速 import 校验并失败快返；即使 MCP 子进程未继承平台环境变量，
   也会回退到仓库级共享 venv，并将 repo root、cwd、args、venv、Python 版本、import check 失败摘要
   和 server stderr 写入 launcher log；launcher smoke
   覆盖 initialize、tools/list、`create_map_card` 的 `map.v3` 输入 schema
   （平台管理的 GeoJSON sources、官方 Mapbox layer JSON、标准 camera 字段和
   可选 hover/legend extensions）以及官方 Style Spec warning/error 行为；
   下一步要把 Runtime
   failureReason 归一到上述分类。
4. [-] 第三方 Provider smoke 使用真实 Codex Runtime 工具调用链验证：模型可见
   `map_utils` tool schema，Provider 返回标准 tool call，Runtime 执行 MCP tool，
   Server 从 Tool `structuredContent` 注册类型化 Artifact，Assistant 只复制
   Tool 生成的短代码决定展示位置。真实 Web/Profile Host 的 DeepSeek 新 Thread
   与独立 CLI smoke 已通过迁移前合同；严格类 Mapbox authoring schema 已通过
   单元测试和真实 launcher MCP smoke，CLI prompt 已更新，仍需在可用第三方
   Provider 凭据下重跑模型调用；
   `scripts/smoke-third-party-map-card-mcp.sh` 覆盖 Codex Runtime + Chat provider +
   `map_utils.create_map_card`，浏览器渲染由 `scripts/smoke-map-card-rendering.sh`
   覆盖。
   Profile Host 现将“已分配官方 Thread id、但首个 rollout 尚未物化”的持久 Thread
   计入 Runtime replacement blocker；Provider Secret 更新、模型目录刷新或上下文
   修改不得重启并丢弃该 Thread。聚焦单元测试和真实 app-server 回归分别覆盖
   首 Turn 后解除门禁、平台显式放弃未物化 Thread 后解除门禁，以及重启失败关闭；
   该路径不伪造 rollout，也不解析 Runtime 错误文本。
5. [ ] 补齐 Responses Provider 的真实浏览器矩阵，验证 Inline Visualization 的
   “不引用不显示”、消息内顺序、Thread 切换、刷新恢复和 live/history 一致性；
   不为不同 Provider 增加第二套 Artifact 或 renderer 路径。
6. [-] 官方 OpenAI Provider smoke 验证 `codex login` 与 Web 使用同一个
   `CODEX_HOME`，模型列表按当前 Profile/Provider 刷新且错误状态可诊断。当前新增
   `scripts/smoke-openai-provider-models.sh` 验证 file-backed auth 和 `model/list` 非空。单
   Profile 过渡期允许在 Profile 缺少 `auth.json` 时，从
   `OPEN_WEB_CODEX_IMPORT_CODEX_AUTH_FROM` 或默认 `~/.codex` 导入 file-backed
   登录态；多用户阶段必须替换为 Profile-scoped auth 设计。
短期 smoke 命令：

- `scripts/smoke-enterprise-supervisor-copilot.sh`：构建当前 Server，在一次性
  PostgreSQL、Profile 与 managed Workspace 上运行真实“华东新增仓”九项 E2E，
  并输出不含凭据和宿主机路径的证据文件。
- `scripts/smoke-maps-mcp-launcher.sh`：验证 maps MCP launcher 可启动、声明
  `outputSchema`、声明 GeoJSON Resource template，并生成带 `map.v3` renderer 和
  embed code 的 Inline Visualization Artifact。
- `scripts/smoke-third-party-map-card-mcp.sh`：使用 `THIRD_PARTY_PROVIDER_*`/`DEEPSEEK_API_KEY`
  等环境变量临时创建 `CODEX_HOME`，验证第三方 Chat provider 通过 Codex Runtime 调用
  `map_utils.create_map_card`，断言 Tool 返回 Artifact/envelope，后续 Assistant
  embed code 才决定地图位置。
- `scripts/smoke-openai-provider-models.sh`：导入 file-backed `auth.json` 到临时 Profile，
  通过 app-server `modelProvider/list` 和 `model/list` 验证官方 Provider 模型目录非空。
- `scripts/smoke-map-card-rendering.sh`：运行官方指令流式解析、消息内分段、
  未引用不显示、live/history 一致性、MapReplyCard 和授权 GeoJSON 相关前端测试，
  并验证 camera/fit viewport 与点线面样式。

以上 smoke 和空白 PostgreSQL migration/security/Workspace 证据与 M2 功能切片
并行推进。它们不阻塞早期 Copilot 编码和受限案例演示，但在完成前不能把 M2 声明为
可信发布能力。

## Codex 上游同步与定制收敛

- [ ] 通过新的 `codex/sync-upstream-*` 分支集成已观测到的后续 142 个官方提交。
- [ ] 按 Chat/Responses 转译规范修正输出语义：标准 Chat 文本不再根据 Tool
  call 推断 phase，流累计器保持 Message/Tool 首次出现顺序，Provider-specific
  phase/reasoning 扩展默认关闭并按 Provider 隔离；真实第三方地图卡片 Turn 的
  live/history 对照必须通过。

同步门禁：`scripts/codex-upstream-status.sh`、
`scripts/codex-customization-status.sh`、patch map、生成物 drift、Web contract、
真实 app-server smoke 必须同时通过。

## 平台纵向闭环

- [x] 资源查询带 Organization/User/Profile 归属；双组织越权、Artifact ID 猜测和
  Profile 访问拒绝已在当前空白 PostgreSQL schema 上通过。
- [x] 数据库模型为独立 Workspace/grant/lifecycle，Runner 只租赁 Run 并使用其
  选择的授权根；Run 取消、失败、恢复和 lease 过期不触发 Workspace 清理，浏览器
  文件/Git API 也直接按 Workspace 授权。当前 fresh-schema 测试已证明同一
  Workspace 可跨 Run 生命周期复用。
- [ ] 增加登记现有执行根与真实 worktree lifecycle，并完成共享 Workspace 并发、
  越权、恢复和删除阻断矩阵。Thread 和 Run 始终只引用 Workspace，不拥有
  checkout。

## Gate 0 验证矩阵

- [x] `bash -n scripts/*.sh` 和本地启动脚本 help/status 路径。
- [-] 1,243 个浏览器测试、typecheck、build、no-desktop、Codex contracts 和真实
  Codex app-server 的 19 项 Capability Manifest smoke 通过；Enterprise Supervisor
  在内置 OpenAI Provider 上的真实案例 9/9 通过。main-ui-parity 仍会报告尚未
  并入参考基线的有意浏览器 UI 扩展。
- [x] `cargo fmt --all --check`、`cargo test --workspace --locked`。
- [x] 当前空白 PostgreSQL schema 上的迁移幂等/Secret 加密、两组织安全、
  Artifact 拒绝、子 Agent/根 Run 生命周期隔离和独立 Workspace 跨 Run 复用
  ignored integration tests 通过；Git Runtime 的 17 项文件与 Workspace 边界测试
  同时通过。
- [x] `npm run check:codex-generated`、`npm run check:codex-contracts`、fixtures、
  Feature Policy 和真实 `--require-manifest` smoke。
- [x] 状态脚本已复核；当前集成基线为 `6e5a2d6b8d14`，观测到的 official
  main 已前进到 `95637f705683`，142 个待同步提交留给下一专用同步分支处理。
- [x] Fake Server HTTP/static/WebSocket 端到端启动验证。
- [x] Git status/diff 审查，确认没有未分类 Codex 差异或意外用户文件。

## 当前发布边界

本分支完成的是可持续同步的 Codex 定制、浏览器纵向平台边界和桌面运行时
淘汰，不等于 V1 GA。以下是当前仍真实存在的产品门禁：

### 浏览器等价语义复审清单

以下入口保留了原页面和调用行为，但受浏览器/服务端边界限制，适配完成后再决定
是否调整前端表达：

1. `Open in app` 对 HTTP/GitHub remote 可打开网页；服务器本地路径不能启动用户
   桌面应用，Reveal 当前复制服务器路径。
2. Codex 自更新与 Tailscale daemon 生命周期由部署管理，页面调用返回明确的
   deployment-managed 状态。
3. 任意 Workspace 的 Codex CLI args 可以持久化，但共享 Profile Host 不会按单
   Workspace respawn；需先定义 Profile/Workspace 级策略，并通过 Codex 官方
   Thread/Turn 合同应用 `cwd` 和环境设置，不建立 Thread 专属进程或 checkout。
4. local usage 可按 Run/Project 汇总 token 与 Turn 数；官方事件尚不提供可靠的
   model share 和 agent time，因此对应值不伪造。
5. 目录选择输入服务器路径；图片选择、拖放和导出使用浏览器 blob/download；
   浏览器不获得任意服务器文件系统访问权。

M2 功能完成与可信发布采用两级判断：短期计划的真实案例完成定义决定功能闭环是否
成立；Gate 0 和能力基线决定能否扩大试用或声称恢复、安全与兼容性已经可信。M2
之后的多用户治理、Capability-gated Studio、生产 GA，以及有条件的 Task Knowledge
Ledger 不在本文展开，统一由 [产品与工程路线图](roadmap.md) 管理。
