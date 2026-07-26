# open-web-codex 开发计划

## 当前状态

| 字段 | 内容 |
| --- | --- |
| 更新日期 | 2026-07-26 |
| 当前分支 | `codex/agent-architecture-features` |
| Codex 基线 | `openai/codex` `6e5a2d6b8d148a5554fdceb6f399ca45bd1c78d9` |
| 上游待同步 | 126；观测到的 official main 为 `cba0e2701c9e3e67a877a16dbbd7a577d477a630` |
| 当前工作 | 以 1421 WebApp 为唯一前端，先收口单用户、单 Profile、单主 Profile Host 的真实 Runtime 闭环；多 Profile Router 暂缓到单 Profile smoke 稳定后 |
| 中期顺序 | `docs/roadmap.md` |
| 能力事实 | `docs/capability-baseline.md` |

当前 Codex 基线上的定制仍按 patch map 分类；official main 已前进 126 个提交，
下一轮必须通过专用 `codex/sync-upstream-*` 分支同步。1421 WebApp 的 CSS、页面布局
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
本文只接收已经进入交付顺序的工作包，并继续用能力基线区分实现与验证。

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

## 当前里程碑：Gate 0 平台证据恢复

当前里程碑不是继续增加产品表面，而是让独立 Workspace、单 Profile Runtime、
PostgreSQL 迁移、安全拒绝和真实 Provider/MCP 链路重新获得同一组可复现证据。
组件所有权以 [系统架构](architecture.md) 为准，安全门禁以
[安全模型](security-model.md) 为准。

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

完成以上 smoke，并补齐空白 PostgreSQL migration/security/Workspace 证据后，
再进入 M2 的 Enterprise Supervisor Copilot 闭环。

## Codex 上游同步与定制收敛

- [ ] 通过新的 `codex/sync-upstream-*` 分支集成已观测到的后续 126 个官方提交。
- [ ] 按 Chat/Responses 转译规范修正输出语义：标准 Chat 文本不再根据 Tool
  call 推断 phase，流累计器保持 Message/Tool 首次出现顺序，Provider-specific
  phase/reasoning 扩展默认关闭并按 Provider 隔离；真实第三方地图卡片 Turn 的
  live/history 对照必须通过。

同步门禁：`scripts/codex-upstream-status.sh`、
`scripts/codex-customization-status.sh`、patch map、生成物 drift、Web contract、
真实 app-server smoke 必须同时通过。

## 平台纵向闭环

- [-] 资源查询带 Organization/User/Profile 归属，双组织越权负向测试源码已经
  覆盖主要路径；基础迁移改写后尚未在空白 PostgreSQL 数据库重跑，因此当前不把
  旧结果记为通过。
- [-] 数据库模型为独立 Workspace/grant/lifecycle，Runner 只租赁 Run 并使用其
  选择的授权根；Run 取消、失败、恢复和 lease 过期不触发 Workspace 清理，浏览器
  文件/Git API 也直接按 Workspace 授权。代码与非 PostgreSQL 测试已就绪，当前
  fresh-schema 数据库证据待补。
- [ ] 增加登记现有执行根与真实 worktree lifecycle，并完成共享 Workspace 并发、
  越权、恢复和删除阻断矩阵。Thread 和 Run 始终只引用 Workspace，不拥有
  checkout。

## Gate 0 验证矩阵

- [x] `bash -n scripts/*.sh` 和本地启动脚本 help/status 路径。
- [-] 1,210 个浏览器测试、typecheck、build、no-desktop、Codex contracts，
  以及真实 Codex/DeepSeek Provider 的 10 项平台 E2E 通过；main-ui-parity
  仍会报告尚未并入参考基线的有意浏览器 UI 扩展。
- [x] `cargo fmt --all --check`、`cargo test --workspace --locked`。
- [ ] 在空白 PostgreSQL 数据库上重跑 migration/restart、两组织安全、Workspace
  授权、Git Runtime 与 Run Orchestrator ignored integration tests。基础迁移已经
  随独立 Workspace 模型改写，改写前的通过结果不再作为当前证据。
- [x] `npm run check:codex-generated`、`npm run check:codex-contracts`、fixtures、
  Feature Policy 和真实 `--require-manifest` smoke。
- [x] 状态脚本已复核；当前集成基线为 `6e5a2d6b8d14`，观测到的 official
  main 已前进到 `cba0e2701c9e`，126 个待同步提交留给下一专用同步分支处理。
- [x] Fake Server HTTP/static/WebSocket 端到端启动验证。
- [x] Git status/diff 审查，确认没有未分类 Codex 差异或意外用户文件。

## 当前发布边界与下一里程碑

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

### M2 Enterprise Supervisor Copilot

这一里程碑在单 Profile 上验证企业多 Agent 最小闭环，不建设第二套 Agent
Runtime，也不提前开放完整 Agent Studio。开始功能实现前，必须先补齐本分支的空白
PostgreSQL migration/security/Workspace 证据。

1. [ ] 用真实 Codex Profile、app-server 和 Web 投影验证
   spawn、message/follow-up、wait、interrupt、完成后继续执行以及父子 Thread
   轨迹恢复；失败、乱序、刷新和 Profile 重启必须收敛到同一投影。
2. [ ] 建立版本化 Supervisor Policy 的发布与不可变快照，将授权版本通过正式
   `thread/start.developerInstructions` 绑定根 Thread；恢复沿用原版本，升级必须
   显式迁移并审计。
3. [ ] 以代码管理两个可评审的 Agent Definition Manifest，分别映射到真实可发现
   的 Runtime Role。第一版只使用有限清单；未发现或未授权时明确失败。
4. [ ] 提供只读企业数据 MCP 与有界仿真 MCP。Phase 1 只承诺经过验证的
   Task/Profile 级资源范围，不把 Prompt 或模型提交的身份当作授权。
5. [ ] 将 Inline Visualization 的 Run/Thread 作用域升级为持久 Artifact 身份、
   Schema、provenance 和授权，使不同子 Thread 可以安全交接成果，刷新与 Profile
   重启后仍能解析同一 Artifact。
6. [ ] 以一个真实企业用例验证根 Supervisor、两个 Domain Agents、审批、取消、
   部分失败、Artifact 交接和最终报告引用；关键结论必须可追溯到 Artifact。

完成定义与专题架构第 26.2 节一致：这一步证明 Codex 原生多 Agent 能力可以被平台
治理和观察，不宣称已经具备多组织 Agent Catalog、Runtime Role 级动态权限或长期
Blackboard。

M2 之后的多用户治理、Capability-gated Studio、生产 GA，以及有条件的 Task
Knowledge Ledger 不在本文件展开。阶段顺序、进入条件和退出条件统一由
[产品与工程路线图](roadmap.md) 管理。进入下一阶段时，再把该阶段的近期任务和
验证矩阵移入本开发计划。
