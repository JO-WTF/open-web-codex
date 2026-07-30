# open-web-codex 开发计划

## 当前状态

| 字段 | 内容 |
| --- | --- |
| 更新日期 | 2026-07-30 |
| 当前分支 | `codex/agent-architecture-features` |
| Codex 基线 | `openai/codex` `6e5a2d6b8d148a5554fdceb6f399ca45bd1c78d9` |
| 上游状态快照 | 2026-07-28 观测到 official main `95637f7056835fea66bdd0044414af480fc0fd74`，当时待同步 142；本轮未刷新且暂不同步 |
| 当前工作 | 补齐 M2 的生产 Web 数据入口、类型化 readiness、统一启动器与 Tutorial Blueprint 新手闭环，再继续失败恢复和 Artifact 生命周期门禁 |
| 中期顺序 | `docs/roadmap.md` |
| M2 详细实施 | `docs/enterprise-supervisor-copilot-plan.md` |
| 能力事实 | `docs/capability-baseline.md` |

当前 Codex 基线上的定制仍按 patch map 分类；上表只保留最近一次已验证的上游状态
快照，不把未联网刷新的提交差距描述为当前事实。本轮不执行上游同步；未来恢复同步时
仍必须通过专用 `codex/sync-upstream-*` 分支。WebApp 的 CSS、页面布局
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
能力，根据印尼仓网问题的证据缺口从 Data、Network 和 Visualization 三个有界角色
中动态选择必要能力，通过只读数据访问、确定性网络计算、地图 MCP 和持久 Artifact
完成协作，最终报告引用关键证据。详细实施顺序、功能完成标准和可信风险台账以
[Enterprise Supervisor Copilot 短期实施计划](enterprise-supervisor-copilot-plan.md)
为准。

M2 功能切片可以与 Gate 0 并行，不等待全部可信矩阵完成；但不得绕过 Runtime、
Workspace、授权、Artifact 和 Tool 边界。未完成的可信项限制能力声明，并在短期计划
风险台账中保留触发条件。

当前最高优先级是
[M2 短期计划的 Web 可达性与新手闭环工作包](enterprise-supervisor-copilot-plan.md#5-web-可达性与新手闭环工作包)：
先把 Dataset Release 接入唯一生产 Files，再由同一 Server evaluator 在创建 Thread
前返回 readiness，随后接入版本化 Tutorial Blueprint、Learn 和渐进式 Agent Studio。
这不是新的 M2 看板；逐项状态、合同和验收只在短期计划维护。当前能力基线必须继续
区分“路由/组件存在”和“真实 `/web` 可达并通过 E2E”。

### 当前合同

代码 Package 与 Web 草稿使用同一个 Agent/Supervisor 语义编译器。Agent Runtime
Role 只由 `definitionId + version` 稳定派生；Supervisor 的 Role、Runtime
requirement、Artifact handoff 与并发限制只由精确 Agent Release 和草稿合同派生。
代码声明与 Web 发布必须产生逐字段相等的规范化执行语义和 SHA-256，任何漂移都显式
失败。平台行为合同、作者指令和生成执行合同确定性合成为官方
`thread/start.developerInstructions`，没有新增 `codex/` 修改。

当前内置 Release 是：

- `enterprise-supervisor-copilot@3.14.0`，选择
  `platform-supervisor-behavior@1.1.0`；
- Data `3.1.0`，只调用原子校验并发布一个精确授权 Workspace Dataset Release 的
  inspection Tool；
- Network `3.6.0`，按问题只运行服务基线、现网成本、指定候选、有限候选优化、地图
  准备或确定性决策报告中必要的最小分析；报告 Tool 发布权威
  `indonesia_decision_report.v1` Resource 和类型化 `report.v1` 交付；
- Visualization `1.4.0`，只把已验证地图 manifest 与 GeoJSON 交给
  `map_utils.create_map_card`；其 `MAP_HANDOFF` 只保留输入 Resource provenance 与
  map Artifact ID，Tool 的 `structuredContent.embed.code` 只作为独立指令出现；
- 十类条件性 Artifact handoff：八个 Resource 加 `report.v1`、`map.v3` 两种类型化
  回复交付，不构成固定 Workflow。

Root 没有业务 MCP。每个子 Agent 只得到 Definition 声明的精确 MCP Server、Tool
allowlist、Resource 读取范围和 capability root；受治理预检会拒绝缺失或漂移的
Agent、package、Dataset、Runtime capability 以及不该可见的兄弟 MCP。Task 级
Artifact 保留 Run/Thread/Turn/Item producer provenance，根 Thread 只能解析同一 Run
中来源已验证且引用唯一的子 Agent Resource。

### 当前 Web 纵向链路

当前 source 已经把分散能力收敛到生产 Web；真实 Runtime、Artifact 与纯 UI Learn
启动路径已通过，修复后的卡片显示和恢复仍有一个真实浏览器门禁：

1. 生产 Files 已接入 Dataset 发布、Release history、详情、新版本和失败重试；Agent
   Studio 缺少数据时复用同一 **Add data**。
2. 在 Agent Studio 中编写受限标准库 Python Tool、JSON Schema 和 Skill 指令；
   平台固定 launcher 与 package 形状，清空继承环境，完成 MCP initialize、
   Tool discovery，并可使用精确 Dataset Release 测试一个 Tool。
3. 发布不可变 capability-package Release，把精确 package 与 Dataset Release
   依赖绑定到 Agent，校验并发布 Agent Release。
4. 直接以该 Agent 作为根执行一个 Thread，或把多个精确 Agent Release 关联到
   Supervisor，再由 Runtime 自主协调；统一 launcher 与 Server readiness source 已
   接入。当前真实 enterprise E2E 已验证 Runtime 执行和类型化 Artifact。
5. Blueprint list/read/reconcile 与生产 **Learn** 入口已落地；全新空 Workspace 已只
   通过 `/web` 完成创建、打开 Learn、安装示例、readiness、启动和发送。首次完成后
   Artifact ready 但卡片 DOM 为空的问题已在 Web owning layer 修复，最后仍需真实浏览器
   复核修复后的显示和刷新恢复。

浏览器不提交 Runtime Role、MCP inventory、Tool allowlist、capability root、启动
命令、环境变量或服务器文件路径。该 authoring 切片不是任意 Plugin/MCP CRUD，也
不支持 Secret、任意 transport、Profile 全局安装或隐藏配置修改。

### 当前验证状态

- 印尼 Dataset、240,000 个合成客户、现有仓网、报价、候选点和 38 个省级边界具有
  版本化 manifest、摘要、来源说明和生成器回归；
- Python 分析测试、真实 stdio MCP smoke、Agent/Supervisor 编译与持久化聚焦测试
  已通过；
- Dataset Release、Python Tool Test、精确依赖、直接 Agent Run、三角色
  Supervisor、跨子 Thread Artifact 和地图嵌入都有自动化覆盖；
- `Learn UI E2E 20260730 0728` 已从全新空 Workspace 只通过生产 `/web` 完成 Create
  Workspace → Open Learn → Set up example → readiness（仅 maps presentation 为
  `degraded`）→ Start task → Send；Blueprint 精确安装，真实 DeepSeek Run completed，
  8 类 Resource 与 `report.v1`/`map.v3` 均 ready；
- 配送审计单 Agent 已在真实 Runtime 中通过一次精确 MCP 审批、一次 Tool 调用、
  ready Artifact 和刷新恢复；
- 当前 `3.14.0` 印尼 Supervisor 的全新 exact-hash 真实 DeepSeek Runtime/Artifact E2E
  已通过全部 13 项验收：一个 Root 与三个 child Threads、8 个持久 Agent tasks、14 次
  MCP 调用、8 个 ready Resource，以及恰好一个 `report.v1` 和一个 `map.v3`。报告
  验收覆盖 Artifact schema/version、非空 payload 与重算 digest、精确 Tool 来源、
  类型化 checks、Dataset Release 身份、producer provenance 和交付引用，不校验报告
  措辞或模型正文；
- 纯 UI Run 首次完成后卡片 DOM 为 0。权威状态表明两个 typed `inlineArtifacts` 位于
  Runtime `commentary` message，而 `final_answer` 没有 embeds；Web 折叠 commentary
  导致卡片未显示，Platform、Blueprint 和 Artifact 均正常。Web 修复保持 commentary
  prose 折叠，只按 Turn 项目顺序独立展示已授权 typed cards，并按稳定 ref 去重，不改变
  phase、不提升 prose。live/restored DOM 回归、58 项 focused tests、typecheck、lint、
  parity 和 build 通过，服务已重启；浏览器控制器随后因自身 URL policy 拒绝刷新，所以
  post-fix 真实浏览器显示/恢复仍是最终证据门禁。

主线现转向部分失败和审批拒绝后的综合；更深层 Agent 树；Artifact 替代、失效、
删除与保留；重启/乱序、共享
Workspace、multi-`cwd` 和多用户隔离矩阵。这些继续限制“可恢复企业能力”和生产
发布声明。

仓库现提供 `scripts/probe-enterprise-runtime-lifecycle.sh`。当前真实探针已验证
同一已完成 Network Agent 的后续任务、根 Turn 的 `interrupted` 终态和中断后的恢复
Turn，过程中 Agent 数量保持不变且未重复调用业务 MCP。部分失败、审批拒绝和
Server/Profile Host 重启的组合恢复仍未验证。

浏览器已将授权 Agent 树中的子 Thread 审批提升为任务级响应队列：根对话和
Agent Activity 都渲染同一平台审批 ID 的批准/拒绝卡片，刷新后从持久审批事件重放，
提交中阻止重复决定。真实审批拒绝后的 Agent 恢复、Supervisor 综合和最终 Run
收敛仍属于上述未完成门禁。

供应链插件中被当前 Agent 使用的 MCP Tool 只执行只读数据访问、有界确定性计算和
内部不可变 Resource 发布，并继续受精确 Agent Tool allowlist 限制。命令、文件、
权限、凭据、外部副作用和未来混合风险 Server 不继承该设置。右侧 Agent/Files
面板只在窄屏浮层状态下支持点击外部关闭；常规宽屏侧栏不因页面其他位置的点击而
收起，显式关闭按钮和 Escape 行为保持不变。

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
2. [ ] `apps/web/src/services/tauri.ts` 仍是浏览器适配兼容层命名，需在不改变 WebApp
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
   maps MCP venv，启动准备会显式拒绝 Plugin root 内的 `.venv`，从而保留 capability
   root 的严格整树 symlink containment；launcher 在对话期只做快速 import 校验并失败快返；即使 MCP 子进程未继承平台环境变量，
   也会回退到仓库级共享 venv，并将 repo root、cwd、args、venv、Python 版本、import check 失败摘要
   和 server stderr 写入 launcher log；launcher smoke
   覆盖 initialize、tools/list、`create_map_card` 的 `map.v3` 输入 schema
   （平台管理的 GeoJSON sources、官方 Mapbox layer JSON、标准 camera 字段和
   可选 hover/legend extensions）以及官方 Style Spec warning/error 行为；
   Run 在 Thread identity 建立前终止时，API 会投影有界的持久化失败分类，未知值
   显式归一为 `unknown_failure`；下一步仍要把 Runtime failureReason 归一到上述细分类。
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
  PostgreSQL、Profile 与 managed Workspace 上运行印尼供应链动态 Supervisor
  语义 E2E，并输出不含凭据和宿主机路径的证据文件。
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
- [-] typecheck、lint、main-ui-parity、build、58 项 Web owning-layer focused tests、
  Codex contracts 和真实 Codex app-server 的 18 项 Capability Manifest smoke 通过；
  配送审计单 Agent 和 `3.14.0` Supervisor 的真实 Runtime 验收通过；全新空 Workspace
  的 Learn UI 安装、readiness、启动和发送已通过。服务重启后的 post-fix 报告/地图卡片
  真实浏览器显示与刷新恢复仍待复核，因此生产 `/web` 新手闭环仍不得声明 available。
- [x] `cargo fmt --all --check`、`cargo test --workspace --locked`。
- [x] 当前空白 PostgreSQL schema 上的迁移幂等/Secret 加密、两组织安全、
  Artifact 拒绝、子 Agent/根 Run 生命周期隔离和独立 Workspace 跨 Run 复用
  ignored integration tests 通过；Git Runtime 的 22 项文件与 Workspace 边界测试
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
