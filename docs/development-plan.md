# open-web-codex 开发计划

## 当前状态

| 字段 | 内容 |
| --- | --- |
| 更新日期 | 2026-07-23 |
| 当前分支 | `work` |
| Codex 基线 | `openai/codex` `6e5a2d6b8d148a5554fdceb6f399ca45bd1c78d9` |
| 上游待同步 | 48；观测到的 official main 为 `9d823343026e600dab694e41865ed60613da31b6` |
| 当前工作 | 以 1421 WebApp 为唯一前端，先收口单用户、单 Profile、单主 Profile Host 的真实 Runtime 闭环；多 Profile Router 暂缓到单 Profile smoke 稳定后 |

当前 Codex 基线上的定制仍按 patch map 分类；official main 已前进 48 个提交，
下一轮必须通过专用 `codex/sync-upstream-*` 分支同步。1421 WebApp 的 CSS、页面布局
和交互保持既有产品形态；差异集中在认证入口、`src/services/webClient.ts` Server
适配层，以及三个由完整文件哈希锁定的非视觉 Thread 上下文接线文件。平台具备原生 Profile Host、Provider 服务、
加密 Secret、持久审批、Git workspace 与租约式 Run 编排。桌面运行时、sidecar、
4732/4733 daemon Gateway、原始浏览器 RPC/SSE 和桌面发布链已经移除。认证后的
根入口与 `/web` 都只加载同一个 WebApp；旧根 App/Bridge 源码不进入生产构建，
清理工作按当前范围暂缓。

本文只记录当前有效状态和下一步。完成项必须有代码、测试或可重现运行证据。

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
- Canonical 文档只描述现态；历史决策只保留在 ADR/Git 历史中。

## 代码归属

| 层 | 目录 | 当前职责 |
| --- | --- | --- |
| Official Runtime | `codex/**` | Thread/Turn、工具、记忆、多 Agent、Skills、Plugins、MCP、Provider/TUI retained seams |
| Browser UI | `apps/web/src/WebApp.tsx`、相关组件/CSS | `main` 1421 WebApp 的既有页面、布局、交互和 UI 状态 |
| Browser transport | `apps/web/src/services/webClient.ts`、`apps/web/browser/client.ts` | WebApp 兼容方法到类型化 REST/WebSocket 的窄适配 |
| Platform server | `apps/web/server/**` | HTTP/WS、授权、DTO、服务组合、静态资源 |
| Profile | `apps/web/crates/profile-*` | 私有 `CODEX_HOME`、单主进程、app-server JSONL 生命周期 |
| Workflow | `apps/web/crates/run-orchestrator` | 幂等 Run、DB lease、heartbeat、恢复、取消 |
| Git | `apps/web/crates/git-runtime` | 私有 mirror、每 Run workspace、status、选择性 Commit |
| Security | `apps/web/crates/auth`、`approval-service`、`secret-store` | Session/RBAC、持久审批、加密凭据 |
| Contract | `apps/web/crates/*contracts`、`apps/web/contracts` | 浏览器 DTO、生成协议、Manifest、fixtures |
| Capability packages | `tools/**`、plugin/skill/MCP 包 | Runtime 可发现的工具、Skill、Plugin、MCP 声明；不得修改 Profile `config.toml` 或由 WebApp 伪造发现结果 |

## 当前边界债务 TODO

这些是当前实现中仍需按边界复审或迁移的项；在完成前不得把它们宣传为完整能力：

1. [ ] `tools/maps-mcp` 已改为 plugin/MCP 声明，Profile Host adapter 会在新建
   Thread 时把源码树或 workspace `tools/` 中发现到的本地 plugin roots 作为
   `selectedCapabilityRoots` 交给 Codex Runtime；当前已有临时 `CODEX_HOME` 的
   Codex CLI 第三方 Provider + MCP tool smoke；仍需要真实 Web/Profile Host
   新建 Thread discovery smoke，验证 `workspace_maps`、`create_map_card` 和
   selected capability roots 端到端可用。
2. [ ] map-card 仍是浏览器解析小型 marker 的 preview。Artifact-backed GeoJSON、生成
   card schema、平台 Artifact 权限、Mapbox/tiles renderer gate 和真实端到端 smoke 未完成。
3. [ ] `apps/web/src/features/threads/hooks/useThreadMessaging.ts` 中 `/apps`、`/status`、
   `/fast` 等本地命令需要逐项边界复审：纯 UI 状态命令可保留；凡是查询 Runtime
   capability、工具、MCP、Skills、Plugins 或模型上下文的命令必须改为 Runtime/typed
   app-server 合同，不能由 WebApp 生成模型式回答。
4. [ ] `apps/web/src/services/tauri.ts` 仍是浏览器适配兼容层命名，需在不改变 1421 UI
   行为的前提下拆名或迁移，避免继续暗示桌面/Tauri 边界存在。
5. [ ] Capability Manifest 仍有手工 Alpha 子集；必须继续收敛到由 Codex 生成事实驱动，
   Web feature policy 只能消费这些事实，不能自行声明 Runtime 支持。
6. [ ] 旧根 App/Bridge 未引用源码和 browser shims 仍待裁剪，避免未来功能误接回旧桥。

## 单 Profile 收口目标

近期目标是先让一个真实用户使用一个持久 Profile 可靠跑通，再扩展多 Profile。
该目标是部署范围收窄，不改变所有权边界：WebApp 不发现、不启动、不模拟
MCP/Skills/Plugins；Server/Profile Host 只负责单 Profile 生命周期、授权 workspace
和安全诊断；Codex Runtime 继续拥有 Thread/Turn、Provider、Skills、Plugins 和 MCP。

单 Profile 运行合同：

1. [ ] 启动期必须显式确定唯一 `profile_id`、`CODEX_HOME`、默认
   `workspace_id`、Runner workspace root 和 source root；Real mode 缺少
   `CODEX_HOME` 或 root 不一致时失败并给出可诊断错误。
2. [-] Server health/profile status 返回安全摘要，能确认当前 Profile Home
   identity、Profile Host state、Codex build/protocol/capability digest、Provider
   登录/模型目录状态和 MCP startup diagnostics；浏览器仍不得接收本地路径、凭据或
   raw JSON-RPC。当前已新增 Profile runtime status 安全摘要，包含 Profile Home
   fingerprint、Runtime health、capability 计数和 MCP server status 投影；Provider
   模型目录诊断已接入安全摘要，仍需用官方 OpenAI smoke 覆盖 file-backed auth 与远端刷新错误。
3. [ ] `tools/maps-mcp` 只通过 plugin/MCP 声明进入 `selectedCapabilityRoots`；
   不写入 Profile `config.toml`，不由 `run-local.sh` 注入，不由 WebApp 读取或拦截。
4. [-] MCP startup failure 归类并投影为安全诊断：capability root 未选择、`.mcp.json`
   缺失、`cwd` 解析错误、command 不存在、权限不足、Python/venv/pip 失败、
   package import 失败、MCP initialize 失败或 timeout。当前 runtime status 已投影
   Runtime 的 MCP server status；下一步要把 Runtime failureReason 归一到上述分类。
5. [ ] 新建 Thread 的单 Profile smoke 验证 `selectedCapabilityRoots` 包含
   `local-maps-mcp`，Runtime 能发现 `workspace_maps`，启动
   `./bin/maps-mcp-launcher`，并调用 `create_map_card` 返回
   `open-web-card map.v1` marker。
6. [-] 第三方 Provider smoke 使用真实 Codex Runtime 工具调用链验证：模型可见
   `workspace_maps` tool schema，Provider 返回标准 tool call，Runtime 执行 MCP tool，
   assistant 输出 map-card marker，浏览器只渲染 marker。当前新增
   `scripts/smoke-third-party-map-card-mcp.sh` 覆盖 Codex Runtime + Chat provider +
   `workspace_maps.create_map_card`，浏览器渲染由 `scripts/smoke-map-card-rendering.sh`
   覆盖。
7. [-] 官方 OpenAI Provider smoke 验证 `codex login` 与 Web 使用同一个
   `CODEX_HOME`，模型列表按当前 Profile/Provider 刷新且错误状态可诊断。当前新增
   `scripts/smoke-openai-provider-models.sh` 验证 file-backed auth 和 `model/list` 非空。单
   Profile 过渡期允许在 Profile 缺少 `auth.json` 时，从
   `OPEN_WEB_CODEX_IMPORT_CODEX_AUTH_FROM` 或默认 `~/.codex` 导入 file-backed
   登录态；多用户阶段必须替换为 Profile-scoped auth 设计。
8. [ ] 单 Profile 可以串行或按实测限制运行多个 Thread/Run；每个 Run 仍使用独立
   writable workspace，Thread resume/fork 只能使用已授权 workspace 映射。

短期 smoke 命令：

- `scripts/smoke-maps-mcp-launcher.sh`：验证 maps MCP launcher 可启动并能直接生成 `map.v1` marker。
- `scripts/smoke-third-party-map-card-mcp.sh`：使用 `THIRD_PARTY_PROVIDER_*`/`DEEPSEEK_API_KEY`
  等环境变量临时创建 `CODEX_HOME`，验证第三方 Chat provider 通过 Codex Runtime 调用
  `workspace_maps.create_map_card`。
- `scripts/smoke-openai-provider-models.sh`：导入 file-backed `auth.json` 到临时 Profile，
  通过 app-server `modelProvider/list` 和 `model/list` 验证官方 Provider 模型目录非空。
- `scripts/smoke-map-card-rendering.sh`：运行 reply-card parser、AssistantMessage 和 MapReplyCard
  相关前端测试，验证浏览器隐藏 marker 并渲染 map-card。

完成以上 smoke 后，再进入 M2 的按授权用户动态路由持久 Profile 和跨用户隔离矩阵。

## A. Codex 上游同步与定制收敛

- [x] 当前分支集成官方 main 到 `6e5a2d6b8d14`。
- [ ] 通过新的 `codex/sync-upstream-*` 分支集成已观测到的后续 48 个官方提交。
- [x] 将全部非生成差异分类为 `retain-core`、`upstreamed`、`move-out` 或
  `drop`，机器清单与 patch map 一致。
- [x] Chat DTO、Responses-to-Chat 转换、工具名反向映射和 SSE 翻译集中到
  `codex-api`；`core` 仅保留 `WireApi` transport dispatch。
- [x] Provider metadata、模型目录/缓存、app-server Provider API 与 TUI Provider
  workflow 按 owning layer 集中并有 scoped tests/snapshots。
- [x] 将 TUI Provider 配置与 onboarding 实现从高冲突 dispatcher/auth 文件拆入
  Provider 专用模块；上游父模块只保留窄挂接点，完整 TUI 3,233 用例通过。
- [x] 恢复官方 `ToolName` 实现，将 Chat namespace flattening 仅保留在
  `codex-api` transport 边界，避免污染官方协议语义。
- [x] Profile Home 创建、授权、Secret、Provider CRUD 和浏览器 DTO 移出
  `codex/`，由 Web 平台承担。
- [x] Schema、TypeScript、Manifest、fixtures 与真实 app-server smoke 对齐。
- [x] 当前 Runtime 验证矩阵通过：format、Provider、config、MCP、protocol、
  app-server、TUI focused tests 和真实 initialize smoke；最新 auth routing、
  thread fork、Turn diff、apply-patch 与 `PathUri` 回归均通过。

同步门禁：`scripts/codex-upstream-status.sh`、
`scripts/codex-customization-status.sh`、patch map、生成物 drift、Web contract、
真实 app-server smoke 必须同时通过。

## B. 平台纵向闭环

- [x] PostgreSQL schema 覆盖 User/Session、Organization/Membership、Profile、
  Secret、Project/Task/Run、Lease、Workspace、Approval/Audit 和 RunEvent。
- [x] 密码使用 Argon2id；旧 SHA-256 仅在成功登录后升级。
- [x] 资源查询带 Organization/User/Profile 归属；双组织越权负向测试通过。
- [x] 原生 Profile Host 直接管理 `codex app-server`，覆盖私有 Home、单主锁、
  有界事件、原位重启、Thread resume/read 和 Capability Manifest。
- [x] Provider 服务执行受控配置写入、Provider-scoped refresh/cache、选择与模型
  更新；凭据 AES-256-GCM 加密，只注入 Profile 子进程环境。
- [x] 已有 Thread 在 Turn 级切换 Provider 时会重建对应模型客户端；真实旧
  OpenAI Thread 切换 DeepSeek 后不再沿用 OpenAI transport。
- [x] app-server 审批先持久化并脱敏投影，再由版本 CAS 决策和审计。
- [x] 每个 app-server 进程实例拥有独立 UUID；审批响应同时校验实例和 request
  id，重启会取消旧实例请求，`delivery_unknown` 只允许相同决策重试。
- [x] Profile Host 在活跃 Turn 或未解决 Server Request 存在时拒绝凭据触发的
  重启，并覆盖 Turn 启动响应与 started 事件之间的竞态窗口。
- [x] Git Runtime 创建私有 mirror 和每 Run 独立 workspace，拒绝危险 source/ref，
  支持 lock、status、选择性 Commit 和 cleanup。
- [x] Run Orchestrator 支持 idempotency、`SKIP LOCKED` lease、heartbeat、恢复、
  cancellation/interrupt 和明确终态。
- [x] Task event 先持久化，按单 Task 单调 sequence REST replay，再组织隔离地
  WebSocket fan-out；订阅在 ready 前建立，首次连接与重连均执行有序 durable replay。
- [x] Thread/Turn 历史直接读取 Codex `thread/read` 与分页
  `thread/turns/list(itemsView=full)`；RunEvent 仅用于实时/可重建投影，不再拼装历史。

## C. 浏览器与传输收敛

- [x] 浏览器只使用 `/api` 类型化资源；原始 `/api/rpc` 不存在。
- [x] 实时通道为 `/api/events/ws`，Token 在首帧认证而非 URL；跨租户事件
  过滤测试通过。
- [x] 1421 WebApp 保持既有页面布局、字体、交互和功能分支；Thread 上下文接线
  不引入视觉或产品行为重设计。
- [x] `npm run check:main-ui-parity` 对 `apps/web/src` 执行 `main` 逐字节等价
  门禁；除 `webClient.ts` 及其测试外，三个必要接线文件以完整 SHA-256 固定内容。
- [x] WebApp 的 workspace、Thread/Turn、消息、durable replay/live、
  approval/user input、Provider/model、MCP/rate limit、文件预览和 Git status
  已切到类型化 Server 资源；Project/Task/Run/Thread 使用单次 joined context
  查询，文件、Git 与 MCP 始终跟随当前 Thread。消息渲染已恢复 Runtime/Skills/MCP 输出的
  `open-web-card map.v1` 与旧 `widget_type=map` 标记识别，可渲染小型内联地图预览；真实 Codex/DeepSeek/MCP
  纵向用例与核心浏览器 Thread 切换、历史恢复、运行态和文件预览回归通过。
- [x] 认证后的根入口和 1421 `/web` 都只加载 WebApp；旧 App/Bridge 源码仍保留
  但不进入生产构建，生产包不包含 `/api/rpc` 或 EventSource Gateway 调用。
- [x] 平台服务同源提供生产 browser build；Vite 仅在开发时代理 HTTP/WS。
- [x] 前端类型检查、单测与生产构建通过。

## D. 桌面运行时淘汰

- [x] 删除桌面 Rust crate、独立 daemon/sidecar、4732/4733 Gateway、原始
  RPC/SSE 与桌面发布入口。
- [x] 保留既有 React 状态树、文件、终端、语音、Git 和设置 UI；删除其 Tauri
  运行时依赖并通过 `src/platform/browser` 提供 Web 语义。
- [x] 删除桌面/iOS/Windows/macOS release workflows、脚本、图标、截图、网站和
  失效的项目 Skill。
- [x] 根 Cargo/NPM/Nix 构建改为 browser + platform server。
- [x] `scripts/run-local.sh` 改为单平台进程并保留前台、后台、状态、停止、
  Fake/Real 和外部数据库配置。
- [x] `scripts/deploy.sh` 提供单机 Release 部署入口：锁定依赖、隐藏详细构建
  日志、阶段进度、健康检查、持久部署状态、服务信息框和 Cargo target
  高水位增量缓存控制；缺少数据库配置时安全引导使用或创建固定的
  `open_web_codex`，凭据不回显且不进入进程参数；1421 Vite 继续仅用于开发。
- [x] CI 增加禁止桌面代码回流的静态门禁，并构建浏览器、平台 Rust 与
  PostgreSQL 集成测试。
- [-] 旧根 App/Bridge 已退出运行与生产构建；仅为旧 App 保留的未引用源码、
  tests 和 browser shims 待 WebApp 等价回归完成后裁剪。当前不会改动 1421
  WebApp UI。

桌面删除完成标准：源码、依赖、构建产物、CI、运行手册和发布入口均不存在；
`npm run check:no-desktop` 与仓库级搜索同时通过。

## E. 本分支最终验证矩阵

- [x] `bash -n scripts/*.sh` 和本地启动脚本 help/status 路径。
- [x] 1,124 个浏览器测试、typecheck、build、no-desktop、main-ui-parity、
  Codex contracts，以及真实 Codex/DeepSeek Provider 的 10 项平台 E2E 通过。
- [x] `cargo fmt --all --check`、`cargo test --workspace --locked`。
- [x] PostgreSQL migration/restart、两组织安全、Git Runtime 与 Run Orchestrator
  ignored integration tests。
- [x] `npm run check:codex-generated`、`npm run check:codex-contracts`、fixtures、
  Feature Policy 和真实 `--require-manifest` smoke。
- [x] 状态脚本已复核；当前集成基线为 `6e5a2d6b8d14`，观测到的 official
  main 已前进到 `9d823343026e`，48 个待同步提交留给下一专用同步分支处理。
- [x] Fake Server HTTP/static/WebSocket 端到端启动验证。
- [x] Git status/diff 审查，确认没有未分类 Codex 差异或意外用户文件。

## 当前发布边界与后续里程碑

本分支完成的是可持续同步的 Codex 定制、浏览器纵向平台边界和桌面运行时
淘汰，不等于 V1 GA。以下是当前仍真实存在的产品门禁：

### 浏览器等价语义复审清单

以下入口保留了原页面和调用行为，但受浏览器/服务端边界限制，适配完成后再决定
是否调整前端表达：

1. `Open in app` 对 HTTP/GitHub remote 可打开网页；服务器本地路径不能启动用户
   桌面应用，Reveal 当前复制服务器路径。
2. Codex 自更新与 Tailscale daemon 生命周期由部署管理，页面调用返回明确的
   deployment-managed 状态。
3. 任意 workspace Codex CLI args 可以持久化，但共享 Profile Host 不会按单
   workspace respawn；需先定义 Profile/Run 级安全策略。
4. local usage 可按 Run/Project 汇总 token 与 Turn 数；官方事件尚不提供可靠的
   model share 和 agent time，因此对应值不伪造。
5. 目录选择输入服务器路径；图片选择、拖放和导出使用浏览器 blob/download；
   浏览器不获得任意服务器文件系统访问权。

### M2 多用户 Beta

1. [ ] 将 Server 的单配置 Profile 组合改为按授权用户动态路由持久 Profile。
2. [ ] 完成 HttpOnly Cookie、CSRF、logout/revocation、登录限速和会话轮换。
3. [ ] 审批 expiry 和 operator repair workflow；进程实例隔离、重启取消和不确定
   投递重试已完成。
4. [ ] rootless Runner、出网策略、资源 quota、进程/文件系统强隔离。
5. [ ] Push 凭据、保护分支策略、显式 Push 和审计。
6. [ ] 两用户并发的 Profile/Thread/Workspace/Event/Approval/Secret 系统性隔离矩阵。

### M3 Capability-gated Studio

当前 map-card 只恢复对 Runtime/Skills/MCP 已输出标记的解析和小型内联预览；不再由 Web/Server 注入提示。Artifact-backed GeoJSON、生成合同、真实 smoke、Mapbox/瓦片渲染和权限下载仍属于后续门禁。

1. [ ] MCP inventory/config/OAuth/elicitation。
2. [ ] Plugins install/update/disable/uninstall 与来源策略。
3. [ ] Memory health/export/reset。
4. [ ] Native Agents、Skills validate/test/publish/rollback。
5. [ ] 每个模块只在生成合同、fixtures 与真实 smoke 一致后开放。

### M4 生产 GA

1. [ ] Artifact/日志存储、retention、备份恢复和灾难演练。
2. [ ] 可观测性、容量、rolling upgrade、兼容 canary 与回滚。
3. [ ] 完整 Diff/File/Logs/审查体验、可访问性和目标视口 E2E。
4. [ ] 安全评审、依赖/镜像 provenance、发布物 SBOM 和运维手册。

优先顺序固定为 M2 隔离与安全，再做 Studio 和体验扩展；不得以新增 UI
绕过 Profile、Runner、Capability 或授权门禁。
