# open-web-codex 开发计划

## 0. 文档信息与当前状态

| 字段 | 内容 |
| --- | --- |
| 计划基线 | 2026-07-14 |
| 当前里程碑 | M0 合同固化与 M1 平台纵向骨架（并行） |
| 默认分支 | `main` |
| Web 目录 | `apps/web` |
| Codex 目录 | `codex` |
| 产品需求 | `docs/product-design.md` |
| 能力基线 | `docs/capability-baseline.md` |
| 上游同步 | `docs/codex-upstream-sync.md` |

Codex subtree 已同步到官方 `openai/codex` 提交 `af71774d2645`，状态脚本报告待集成提交为 0。当前与官方树的 123 个差异全部是已分类的本地定制，没有 upstream-only 或 diverged 路径。第三方 Provider、TUI Provider、Capability Manifest 和旧历史兼容 seam 已按 patch map 重放；Chat DTO/转换/SSE 已集中到 `codex-api`，`codex-api/common.rs` 与官方对象一致。app-server Schema 无漂移，Runtime/app-server/TUI 定向验证通过。Capability Manifest v1 类型、Schema 和 `initialize` 已验证可用，真实本地 app-server Smoke 返回 18 个能力声明。

M1 平台已建立 Axum/SQLx/PostgreSQL workspace、Fake/Real Codex Adapter，以及 bootstrap/session、organization/membership、project、Task、Run 和版本化 Run event 投影。Item/Delta 会先以单调 sequence、稳定平台事件类型和脱敏 UI payload 落库，再向浏览器广播；Web reducer 可用 cursor 投影恢复活动状态并以 Codex Thread 历史校准终态。`apps/web/crates/profile-host` 已在过渡 Tauri app-server spawn 前 provision `CODEX_HOME`，但持久、隔离的原生 Profile Host 尚未完成。浏览器仍主要连接 loopback RPC/SSE Gateway；Git Worktree/Runner、持久审批、Lease、审计、完整 RBAC、幂等调度和认证 WebSocket 也尚未完成。当前 `/api/rpc`、permissive CORS 和 SSE query token 只能用于本地迁移期，不得作为多用户 Beta 边界。

本计划只记录当前有效工作。任务完成必须有代码、测试和运行证据，不能仅凭源码检查将 Runtime 或恢复能力标记为完成。

## 1. 执行规则

### 1.1 任务状态

- `[ ]` 未开始。
- `[-]` 进行中，同一工作流同时最多 2 个紧密关联任务。
- `[x]` 已完成，具备实现、测试和可复现证据。
- `[!]` 阻塞，必须记录阻塞任务、外部条件和可继续动作。

### 1.2 优先级与规模

| 标记 | 含义 |
| --- | --- |
| P0 | 当前里程碑门禁，不完成不能进入下一阶段 |
| P1 | 当前里程碑应完成，可在门禁前调整顺序 |
| P2 | 可延期，不阻塞核心闭环 |
| S | 单一模块、低风险、通常 1–2 工程日 |
| M | 跨少量模块、需要集成测试、通常 3–5 工程日 |
| L | 跨项目/状态机/迁移，必须拆分为多个 PR |

规模是任务拆分工具，不是交付日期承诺。L 任务不得以一个不可评审的 PR 实现。

### 1.3 单任务完成条件

1. 需求 ID、输入、输出、错误、权限和幂等语义明确。
2. 成功、拒绝、超时、重试、并发或恢复路径按任务类型覆盖。
3. API、Schema、Migration、Manifest 或运维文档同步更新。
4. 日志和错误不泄露 Secret、Prompt、Memory 正文或无界代码正文。
5. 通过最窄单元/集成测试，再通过所属模块门禁。
6. 跨 `codex`/`apps/web` 的协议变化必须通过真实 app-server Smoke。
7. UI 变化必须验证 Loading、Empty、Error、Forbidden、Offline 和目标视口。

### 1.4 分支与提交策略

- 普通功能：`codex/<简短功能名>` 或 `cursor/<简短功能名>-c007`（Cloud Agent）。
- 官方同步：只用 `scripts/sync-codex-upstream.sh --apply` 创建的 `codex/sync-upstream-<sha>`。
- 官方同步 PR 只包含 Runtime 同步、冲突解决、生成文件和对应测试，不混入 Web 功能。
- 合同变更顺序：Codex Rust 类型/方法 → 生成 Schema/Manifest/Fixture → Web 消费 → Feature Policy。
- 数据库变更必须前向兼容至少一个应用版本，提供升级和回滚/修复说明。

### 1.5 Codex 收敛检查点

每次修改 `codex/` 源码（不仅是生成物）必须：

1. 运行 `scripts/codex-customization-status.sh`，基线为 `codex-upstream/main`，**不得**用本仓库 `main` 代替。
2. 将结果刷新到 `.sync/codex-customization-inventory.json`；新增差异必须先归入 patch map 四类之一（`retain-core` / `upstreamed` / `move-out` / `drop`），**禁止**未登记的第 7 条 seam。
3. 高冲突文件（如 `app-server-protocol/src/protocol/common.rs`）只允许注册宏/生成层扩展，**不得**写入 Web、Profile、授权或浏览器逻辑。
4. 重放顺序固定为 patch map 的 1–6：Chat transport → Provider 元数据/缓存 → app-server Provider API → TUI Provider → legacy history → Capability Manifest → 生成物与 Web contract smoke。

## 2. 工作流与代码归属

### 2.1 四条并行工作流

| 工作流 | 目录 | 职责 |
| --- | --- | --- |
| Runtime | `codex/**` | 上游同步、app-server、Provider、Manifest、Schema、Fixtures |
| Platform | `apps/web/server`、`apps/web/crates`、`apps/web/migrations`，`src-tauri`（过渡） | PostgreSQL、API、Profile Host、Runner、Git、安全 |
| Experience | `apps/web/src` | 浏览器 Shell、Task 工作区、审批、Diff、Studio |
| Contract/QA | `apps/web/contracts`、`scripts`、`.github` | 合同消费、Smoke、E2E、兼容、恢复和发布门禁 |

### 2.2 目标模块边界

M1 结束前形成以下结构；具体 crate/package 名可在 ADR 中微调：

```text
apps/web/
  src/                          React Web
  server/                       HTTP/WebSocket 组合入口
  crates/
    platform-contracts/         浏览器 API/事件 DTO
    codex-contracts/            上游生成类型与 Manifest 解析
    codex-host/                 Profile/app-server 生命周期
    git-runtime/                Mirror/Worktree/Diff/Commit/Push
    platform-store/             PostgreSQL repository 与 migration
    run-orchestrator/           Scheduler、Lease、Run 状态机
  migrations/

codex/codex-rs/
  app-server-protocol/          app-server 协议事实
  app-server/                   Runtime bridge
  model-provider*/              Provider/模型目录
```

规则：

- React 不直接调用 Tauri `invoke`；所有浏览器调用经 `src/services` 中的平台客户端。
- Server 不把 app-server 原始 JSON-RPC 暴露给浏览器。
- Profile Host 不拥有业务 RBAC；调用前由 Platform Service 完成授权。
- Runner 不读取 Profile Secret；只接收最小执行凭据和受控 Workspace。
- Project Skill 的 Git 生命周期属于 Platform；Codex 负责原生发现/验证/执行语义。

## 3. 依赖与关键路径

```text
M0-A 官方同步稳定（已完成当前 checkpoint）
  -> M0-B 生成 Runtime 合同（Manifest 最小实现已完成，生成化待完成）
  -> M0-C Web 合同消费与真实 Smoke
  -> M1-A 数据/API 骨架
  -> M1-B Profile Host + Git Runtime
  -> M1-C 单用户 Task 纵向闭环
  -> M2 多用户、租约、审批与隔离
  -> M3 Capability-gated Studio
  -> M4 生产加固与删除 Tauri
```

可并行关系：

- M0 Runtime 同步期间，Platform 可完成 PostgreSQL Schema/接口 ADR，但不得按未冻结协议实现 Host Adapter。
- M1 Profile Host 与 Git Runtime 可并行；Task Orchestrator 在两者接口冻结后集成。
- Experience 可先用平台 Mock API 实现页面状态，但不得把 Mock 数据结构当正式协议。
- M3 各 Studio 模块独立，不阻塞已经完成的 Task 核心闭环。

## 4. M0 — Monorepo、上游与合同重建

**目标：** 获得可持续同步的 Codex 基线，以及由真实构建生成、Web 可消费的版本化合同。

### 4.1 M0-A 仓库与上游稳定

| ID | P/规模 | 状态 | 任务 | 验证 |
| --- | --- | --- | --- | --- |
| M0-A01 | P0/M | [x] | 将 CodexMonitor 与 `open-codex` 导入 monorepo | 新 clone 包含 `apps/web`、`codex` |
| M0-A02 | P0/S | [x] | 建立 `openai/codex` 状态与 guarded sync 脚本 | 当前待集成提交为 0，检查默认不改源码 |
| M0-A03 | P0/S | [x] | 建立根 AGENTS、CI、架构与同步规则 | GitHub CI 通过 |
| M0-A04 | P0/S | [x] | 将旧产品/计划文档改为 canonical 根文档入口 | 仓库内只有一套产品/里程碑事实 |
| M0-A05 | P0/M | [x] | 审查 CodexMonitor Web 迁移边界并保留过渡实现 | ADR、Tauri boundary 与独立 server 结构存在 |
| M0-A06 | P0/M | [x] | 审查 Codex Fork Provider WIP | Patch Map 已将提交归类为 upstreamed/retain/drop/check |
| M0-A07 | P0/L | [x] | 创建官方同步分支并合并选定 `openai/codex` checkpoint | subtree 已同步到 `af71774d2645`，状态为 synchronized |
| M0-A08 | P0/M | [x] | 将所有非生成 Codex 差异归类为 retain-core、upstreamed、move-out 或 drop，并维护可重放 seam 清单 | 每个差异有归属；每个 retain-core seam 有路径、原因、重放顺序、测试和删除条件 |
| M0-A09 | P0/M | [x] | 重点重验 Provider Wire API、模型缓存和当前 Provider 传播 | codex-api、model-provider、models-manager、app-server model_list 和 TUI scoped tests 通过；真实 Manifest Smoke 和 Web contract check 通过 |
| M0-A10 | P1/S | [-] | 固定首个兼容 Codex commit、Rust toolchain、target 和 binary digest | `.sync` 已固定 commit；兼容矩阵和 digest 待补 |

当前官方 main 与已集成基线均为 `af71774d2645`。比较结果为 123 个
local-only 路径、0 个 upstream-only 路径和 0 个 diverged 路径；全部
非生成差异已归入 patch map。Chat 请求 DTO、转换与 SSE 已集中到
`codex-api`；`core/src/client.rs` 只保留通用请求准备、认证/重试/遥测和
`WireApi` 传输选择。

M0-A07 拆分建议：

1. 只合并官方文件移动和无冲突提交。
2. 处理 Provider/API/模型目录冲突。
3. 处理 TUI 定制冲突。
4. 重新生成 Config/app-server Schema。
5. 独立提交行为修复，不把冲突解决与新功能混在一起。

### 4.2 M0-B Runtime Capability Contract

| ID | P/规模 | 状态 | 任务 | 验证 |
| --- | --- | --- | --- | --- |
| M0-B01 | P0/M | [x] | 定义 Manifest v1 Rust 类型与构建入口 | JSON Schema、TypeScript 与 roundtrip tests 已存在 |
| M0-B02 | P0/M | [x] | 从 app-server 方法注册表生成 Client/Server/Notification 方法集合 | Client/Server Request 与 Server/Client Notification 方法枚举均由注册宏生成；Manifest 引用四类 wire 名均已校验 |
| M0-B03 | P0/M | [x] | 从 experimental annotation 生成能力实验状态 | `apply_registry_derived_metadata` 在 `build_manifest` 中自动推导 `experimental`；注册表对照与 stable Schema 排除测试通过 |
| M0-B04 | P0/M | [x] | 定义能力 ID 与产品方法归属 policy | `manifest_method_policy` 与 `policy/feature-policy.v1.json` 已建立；`test:feature-policy` 覆盖全部 9 个 P0 功能 |
| M0-B05 | P0/S | [x] | Manifest 加入 build commit、version、target、protocol range | initialize 实测返回 |
| M0-B06 | P0/M | [-] | 定义 limits 与 structured reason | 类型与初始值已存在；生成规则和兼容测试待补 |
| M0-B07 | P0/M | [x] | `initialize` 返回 Manifest | `--require-manifest` 真实 Smoke 通过 |
| M0-B08 | P0/M | [ ] | 定义稳定错误 envelope/category/retryability | 遵循 `docs/architecture.md` 上游优先四层顺序（消费上游形状 → 生成元数据 → 最小 Runtime seam → Platform policy）；关键错误 Fixture |
| M0-B09 | P0/M | [x] | 生成合同 Bundle、每文件 SHA-256 和总摘要 | `generate:codex-contracts`、`fetch-codex-contracts` 与 `check:codex-generated` 漂移门禁已落地 |
| M0-B10 | P0/S | [x] | Codex CI 校验生成文件无漂移 | `.github/workflows/codex-protocol-ci.yml` 在 `codex/**` 变更时运行 schema fixture 测试 |
| M0-B11 | P1/M | [ ] | 生成真实 Thread/Approval/Provider/MCP/Multi-agent Fixtures | 离线回放通过 |
| M0-B12 | P1/S | [ ] | 发布兼容说明和已知限制模板 | 每个构建有机器/人可读说明 |

Manifest 不应手工重复维护方法名。可人工维护的是“能力方法归组”、产品语义版本和 **显式 exclude 列表**；实际方法集合与实验标记应从源码注册表生成并由 CI 对照。未写入 Manifest 的 wire 方法视为 Runtime 内部或未承诺能力，**不是**收敛缺陷。

### 4.3 M0-C Web 合同消费与真实 Smoke

| ID | P/规模 | 状态 | 任务 | 验证 |
| --- | --- | --- | --- | --- |
| M0-C01 | P0/S | [x] | 保留现有 Manifest Parser、Fixture Replay、Harness 自测 | Web CI 通过 |
| M0-C02 | P0/M | [x] | 将 `apps/web/contracts/codex` 分成 generated bundle 与 product policy | `policy/`、`generated/` 与 `check:codex-contracts` 已校验 feature policy 与 bundle |
| M0-C03 | P0/M | [x] | 从 Bundle 生成 Rust/TypeScript 消费类型 | `generate:codex-consumer-types` 生成 TS 与 `crates/codex-contracts`；`check:codex-generated` 禁止手改 |
| M0-C04 | P0/S | [x] | 以 SHA-256 缓存并固定 Bundle | `fetch-codex-contracts` 缓存与 `check:codex-generated` 固定摘要门禁 |
| M0-C05 | P0/M | [x] | Product Feature Policy 映射 UI 功能到能力/最低版本 | `test:feature-policy` 覆盖全部 9 个 P0 功能并与 `negotiateCapabilityManifest` 集成 |
| M0-C06 | P0/M | [x] | 真实 initialize + Manifest Smoke | 本地构建返回 18 个声明 |
| M0-C07 | P0/M | [ ] | 两 cwd Thread start/read/resume Smoke | 重启后归属和恢复正确 |
| M0-C08 | P0/M | [ ] | Provider 切换、force refresh、Turn Provider Smoke | 不串用模型缓存或凭据 |
| M0-C09 | P0/M | [ ] | 命令/文件/权限/输入审批 Server Request Smoke | 请求响应关联正确 |
| M0-C10 | P1/M | [ ] | Multi-agent 父子 Thread/Collab Item Smoke | Fixture 与实时事件一致 |
| M0-C11 | P1/M | [ ] | MCP inventory/OAuth/elicitation Smoke | 成功、取消、过期、失败覆盖 |
| M0-C12 | P0/S | [ ] | 将固定构建写入兼容矩阵 | serverBuild 状态 compatible |

### 4.4 M0 门禁

- [x] `G0-01` 官方同步分支完成，定制 patch map 已建立。
- [ ] `G0-02` Codex 构建生成 Schema、Manifest、Fixtures 和 Bundle 摘要。
- [ ] `G0-03` Web 从 Bundle 生成类型并通过破坏性变更检查。
- [ ] `G0-04` initialize、Thread、Provider 和 Approval 真实 Smoke 通过。
- [ ] `G0-05` 固定 Codex commit/target/digest 进入兼容矩阵。
- [x] `G0-06` `codex-customization-status.sh` 通过、inventory 已刷新、无未分类 `local-only` 源文件。

M0 完成前可以做平台 ADR、数据库设计和 UI 原型；不得合并依赖未冻结原始协议字段的生产 Host Adapter。

## 5. M1 — 单用户 Alpha 纵向闭环

**目标：** 一个管理员仅使用浏览器，从导入仓库到 Commit 完成真实任务，并能在刷新和 Host 重启后恢复。

### 5.0 当前实现快照

已落地并可从源码验证：

- Axum server、SQLx/PostgreSQL workspace、Migration Runner 和平台 DTO/error/event 基础类型。
- 一次性 bootstrap、密码登录 Session、`/me`，以及组织与成员、项目、Task、Run 的首批 API。
- Fake/Real `CodexAdapter`、Run start/cancel/message、Runtime event fan-out、Run terminal event 持久化和 Task 状态联动。
- 四个 PostgreSQL migration，覆盖 users/sessions、organizations/memberships、projects、tasks、runs 和 run_events 的当前原型表。

这些实现尚不能按 M1 完成：多数路由缺少统一 RBAC/幂等/错误门禁；Session 仍允许 Bearer，完整 Cookie/CSRF/logout/revoke 未完成；Profile、Workspace、Approval、Lease、Audit 等表与服务缺失；Run 还没有 Mirror/Worktree/Profile Host 编排；浏览器仍使用本地 RPC/SSE MVP。后续任务必须基于这些现有模块增量完成，不能另建平行 Server 或第二套 Task/Thread 模型。

### 5.1 M1-A Server 与数据层骨架

| ID | P/规模 | 任务 | 验证 |
| --- | --- | --- | --- |
| M1-A01 | P0/M | 记录 Web Server 技术选型 ADR：Rust/Axum 优先复用现有 Rust Core | ADR 与最小启动程序 |
| M1-A02 | P0/M | 创建顶层 Rust workspace 与 platform-contracts/store crates | core crates 不依赖 Tauri |
| M1-A03 | P0/M | 建立 PostgreSQL 容器化开发依赖与 Migration Runner | 空库升级/重复运行测试 |
| M1-A04 | P0/L | 创建 Organization、User、Session、Project、Profile 表 | FK、唯一性、软删策略测试 |
| M1-A05 | P0/L | 创建 Task、Run、Workspace、Event、Approval、Lease、Audit 表 | 状态/版本约束测试 |
| M1-A06 | P0/M | 统一 ID、时间、分页、错误和 Idempotency Key | API contract tests |
| M1-A07 | P0/M | 建立配置加载、Secret Provider 接口和启动校验 | 缺配置 fail-fast，不打印 Secret |
| M1-A08 | P1/M | 建立后台 Job/Lease 抽象，不提前引入外部队列 | 并发领取与过期回收测试 |

建议核心表：

```text
organizations, users, memberships, sessions
git_credentials, projects, project_members
profiles, profile_capabilities, profile_processes
tasks, runs, workspaces, run_events
approvals, control_leases, artifacts, audit_events
idempotency_keys, background_jobs
```

所有业务表从第一版包含 `organization_id`；即使 V1 单组织，也不得依赖全局单例查询。

### 5.2 M1-B Alpha 认证与初始化

| ID | P/规模 | 任务 | 验证 |
| --- | --- | --- | --- |
| M1-B01 | P0/M | 实现一次性 bootstrap token/首位 Owner 创建 | 第二次初始化拒绝 |
| M1-B02 | P0/M | 实现密码哈希或选定 OIDC 的 Alpha 最小方案 | 登录失败/限流/审计 |
| M1-B03 | P0/M | HttpOnly Session Cookie、CSRF、登出和吊销 | Cookie/CSRF 集成测试 |
| M1-B04 | P0/S | 实现 `/me`、全局错误与 request ID | 前端可恢复 401/403 |
| M1-B05 | P0/M | 初始化向导草稿与幂等提交 | 中途刷新可继续 |

Alpha 可以只有一名 Owner，但必须使用正式 Session/RBAC 接口，不能延续共享 Token 或 URL Token。

### 5.3 M1-C Profile Host

| ID | P/规模 | 任务 | 验证 |
| --- | --- | --- | --- |
| M1-C01 | P0/M | 抽离 Codex Home/环境/启动参数构建 | 路径与 Secret 单元测试 |
| M1-C02 | P0/M | Profile 目录布局、权限与原子创建 | 并发创建只有一个成功 |
| M1-C03 | P0/M | Profile 级文件锁与单主进程 Registry | 双进程启动拒绝/复用 |
| M1-C04 | P0/L | app-server spawn/stdin/stdout/shutdown 与崩溃监控 | 退出、超时、无效 JSON 测试 |
| M1-C05 | P0/M | 请求 ID、超时、取消、Server Request 关联 | 乱序/重复/迟到响应测试 |
| M1-C06 | P0/M | bounded event queue 与 lag/overload 行为 | 慢消费者不无限占内存 |
| M1-C07 | P0/M | Manifest 握手、版本判定和 Feature Policy | incompatible 不领取 Run |
| M1-C08 | P0/M | Provider 配置与 Secret 环境注入 | API/日志不返回明文 |
| M1-C09 | P0/M | 模型刷新与 Profile 默认 Provider/模型 | 重启后保持、缓存不串用 |
| M1-C10 | P0/L | Profile restart 后 Thread list/read/resume 恢复 | 真实二进制恢复测试 |
| M1-C11 | P1/M | 健康、最近错误、构建和能力快照 API | 状态转移测试 |

M1-C08/C09 边界：Platform 只做 Provider CRUD 编排、Secret 注入与授权；通过既有 app-server Provider API（`modelProvider/list`、`model/list`、`config/batchWrite` 等）调用 Runtime，**不得**在 `apps/web` 或 `codex/` 重实现 Wire 翻译、模型缓存或 Provider 元数据语义。

### 5.4 M1-D Git Runtime 与 Runner

| ID | P/规模 | 任务 | 验证 |
| --- | --- | --- | --- |
| M1-D01 | P0/M | Git URL/branch/ref 参数校验与错误分类 | 注入和非法 ref 测试 |
| M1-D02 | P0/M | Repository Mirror clone/fetch/锁 | 并发 Fetch 测试 |
| M1-D03 | P0/M | Worktree create/status/remove/prune | 临时仓库集成测试 |
| M1-D04 | P0/M | Workspace 路径归属和 symlink escape 防护 | 路径安全矩阵 |
| M1-D05 | P0/M | Diff/文件列表/二进制/大文件元数据 | 多类型 Fixture 仓库 |
| M1-D06 | P0/M | Commit 选择文件、作者和状态再校验 | TOCTOU/无变更测试 |
| M1-D07 | P0/M | Scheduler 领取、心跳、取消与超时回收 | 双 Worker 不重复执行 |
| M1-D08 | P0/M | provisioning 补偿与 cleanup Job | 半创建目录恢复 |
| M1-D09 | P1/L | rootless 执行容器接口；Alpha 可先受控进程隔离 | 环境/挂载/出网测试 |
| M1-D10 | P1/M | Artifact 写入、大小上限、权限和保留元数据 | 超限/中断测试 |

### 5.5 M1-E Task Orchestrator 与实时层

| ID | P/规模 | 任务 | 验证 |
| --- | --- | --- | --- |
| M1-E01 | P0/M | Task/Run 服务与幂等创建 | 重复点击仅一条 Run |
| M1-E02 | P0/L | 实现 Run 状态机和合法转移检查 | 全状态转移表测试 |
| M1-E03 | P0/L | 编排 Mirror→Worktree→Profile→Thread→running | 失败补偿测试 |
| M1-E04 | P0/M | Task 与 Codex Thread 稳定映射 | 继续 Run 不误建 Thread |
| M1-E05 | P0/M | Turn start/steer/interrupt 适配 | 活动 Turn 规则测试 |
| M1-E06 | P0/L | [x] app-server event→平台事件投影 | 关键 Item/Delta tests |
| M1-E07 | P0/M | [x] 单 Task 单调 sequence 与顺序落库 | 数据库 identity 唯一索引与 cursor reducer tests |
| M1-E08 | P0/M | 认证 WebSocket、订阅授权和 cursor replay | 断线/重复/越权测试 |
| M1-E09 | P0/M | 大输出截断和 Artifact 转存 | 单事件/单 Task 上限测试 |
| M1-E10 | P0/M | pending Approval 落库后再向 Web 推送 | 崩溃窗口测试 |
| M1-E11 | P0/M | Approval 决策 CAS 和 Codex 响应 | 并发决策只有一个成功 |
| M1-E12 | P0/M | Run/Host 巡检修正伪 running | 故障注入测试 |

### 5.6 M1-F Web Alpha 体验

| ID | P/规模 | 任务 | 验证 |
| --- | --- | --- | --- |
| M1-F01 | P0/M | Web App Shell、路由、Session 恢复、全局状态条 | 401/offline/maintenance UI |
| M1-F02 | P0/M | 初始化向导：Owner→Profile→Provider→Git→Project | 刷新恢复/错误路径 E2E |
| M1-F03 | P0/M | 项目列表、创建和 setup 状态 | Empty/failed/forbidden |
| M1-F04 | P0/M | 创建 Task 表单与能力兼容预检 | 幂等/分支消失/附件失败 |
| M1-F05 | P0/L | Task 三栏工作区与响应式 Tabs | 桌面/平板/手机截图 |
| M1-F06 | P0/L | 活动流：消息、计划、命令、文件、工具、错误 | Fixture visual tests |
| M1-F07 | P0/M | Composer send/steer/queue/stop 状态 | 快捷键和禁用态 tests |
| M1-F08 | P0/M | Approval 卡片与结构化输入 | 超时/已决/并发刷新 |
| M1-F09 | P0/M | Changes、文件选择、Commit Drawer | 大 Diff/二进制/无变更 |
| M1-F10 | P0/M | reconnect replay、重复事件 reducer | 刷新/离线 E2E |
| M1-F11 | P1/M | Profile/Provider 最小管理页 | 增删改选和模型刷新 |
| M1-F12 | P0/M | 键盘、焦点、ARIA、滚动和移动键盘 | a11y + viewport tests |

### 5.7 M1-G Alpha E2E 与门禁

- [ ] `G1-01` 空白开发环境一条命令启动 PostgreSQL、Server、Host/Runner 和 Web。
- [ ] `G1-02` 浏览器完成初始化、项目、Profile/Provider、Task、审批、Diff 和 Commit。
- [ ] `G1-03` 刷新页面后从 cursor 恢复，不重复显示或丢失关键事件。
- [ ] `G1-04` Profile Host 重启后恢复原 Thread 和 Provider/模型。
- [ ] `G1-05` Runner 在 provisioning/running 中崩溃后 Run 进入明确状态并可继续。
- [ ] `G1-06` 每个 Run 独立 Worktree，跨 Task/路径逃逸测试通过。
- [ ] `G1-07` 生产路径不使用共享 Token、SSE 查询 Token 或 `Access-Control-Allow-Origin: *`。
- [ ] `G1-08` Alpha 已知限制、备份方式和故障 Runbook 可由非作者复现。

## 6. M2 — 多用户 Beta 与强隔离

### 6.1 M2-A 组织、成员与 RBAC

- [ ] `M2-A01` 邀请创建、过期、使用和重发。
- [ ] `M2-A02` Membership/角色变更和最后 Owner 保护。
- [ ] `M2-A03` 项目成员与资源级权限中间件。
- [ ] `M2-A04` Session 列表、吊销、禁用用户全会话失效。
- [ ] `M2-A05` 资源不存在/无权限防枚举策略。
- [ ] `M2-A06` 权限变更实时影响新请求和 WebSocket 订阅。

### 6.2 M2-B Control Lease、审批中心与协作

- [ ] `M2-B01` Lease request/acquire/renew/release/expire/revoke 状态机。
- [ ] `M2-B02` 强制接管权限、原因和审计。
- [ ] `M2-B03` 全局审批中心、权限过滤和任务深链。
- [ ] `M2-B04` 审批过期、Run 终止、Profile 重启联动。
- [ ] `M2-B05` 评论与 Agent 消息分离。
- [ ] `M2-B06` 站内通知与已读状态；邮件/外部通知 P2。
- [ ] `M2-B07` 审计查询、项目范围和敏感字段脱敏。

### 6.3 M2-C 执行与 Secret 隔离

- [ ] `M2-C01` 每用户 Profile Home UID/目录权限验证。
- [ ] `M2-C02` rootless Runner、只读 Mirror、最小 Workspace 挂载。
- [ ] `M2-C03` Provider/Git/MCP Secret 引用和按需注入。
- [ ] `M2-C04` 默认出网策略和 Provider/MCP 域名规则。
- [ ] `M2-C05` CPU/内存/进程/磁盘/运行时长配额。
- [ ] `M2-C06` 两用户并发跨 ID、路径、Thread、事件和审批拒绝矩阵。

### 6.4 M2-D Push 与运营

- [ ] `M2-D01` Push、远端领先、认证和保护分支错误。
- [ ] `M2-D02` 禁止 Force Push 和隐式 Merge。
- [ ] `M2-D03` Runner healthy/draining/offline/version_mismatch。
- [ ] `M2-D04` Queue、Profile、磁盘、失败清理管理页。
- [ ] `M2-D05` 管理员暂停领取、排空和终止卡死 Run。

### 6.5 M2-E Provider 与 MCP Beta

Platform Provider/MCP 管理只做编排与授权；Runtime 继续拥有 wire 协议、模型目录与缓存隔离。不得在 Platform 或 `codex/` 复制 Provider 传输层。

- [ ] `M2-E01` 个人 Provider CRUD、当前 Provider 保护和模型刷新。
- [ ] `M2-E02` Secret 不回显、Base URL/协议/模型测试。
- [ ] `M2-E03` MCP inventory、Tools、Resources 和认证状态。
- [ ] `M2-E04` MCP 配置/Reload 的安全服务端适配。
- [ ] `M2-E05` OAuth state/callback/replay/cancel/error。
- [ ] `M2-E06` elicitation 持久化、审批、过期和取消。

### 6.6 M2 门禁

- [ ] `G2-01` 两名用户并发运行不同项目，无 Profile/事件/Secret/Workspace 串流。
- [ ] `G2-02` 所有写 API 通过 RBAC、状态、Lease、能力和幂等检查。
- [ ] `G2-03` Worker/Host/Server 故障注入后无永久 running/pending。
- [ ] `G2-04` Push 不存在 Force 路径，失败不丢本地 Commit。
- [ ] `G2-05` MCP OAuth/elicitation 和 Provider 操作具备完整审计。
- [ ] `G2-06` 无 Critical 安全问题，High 有明确修复计划。

## 7. M3 — Capability-gated Codex Studio

各模块只有在 Manifest、Fixture、真实 Smoke 和 unavailable UI 同时完成后才启用。

### 7.1 Profiles 与 Providers（优先）

- [ ] Profile 健康、构建、能力、认证、进程和最近错误。
- [ ] 停止、重启、重新认证、恢复验证和危险重置。
- [ ] Provider 详情、模型目录、上下文窗口和 Wire API。
- [ ] Profile/Provider 变更快照记录到新 Run。

### 7.2 MCP 与 Tools

- [ ] MCP list/read/create/update/disable/delete 的稳定安全语义。
- [ ] Reload 分步状态和活动 Turn 影响说明。
- [ ] Tool Schema、权限、来源、调用测试和结构化结果。
- [ ] OAuth/elicitation 完整生命周期与错误分类。

### 7.3 Plugins

- [ ] Marketplace 来源策略、list/read 与 fail-open 加载错误。
- [ ] install/update/enable/disable/uninstall 与回滚语义。
- [ ] Manifest、版本、完整性、依赖、Apps/MCP/Skills 摘要。
- [ ] 安装/升级权限差异确认。

### 7.4 Memory

- [ ] Compaction/Consolidation 状态与错误事件。
- [ ] 容量、最近成功、诊断和跨重启连续性。
- [ ] 导出权限、Artifact 交付和受控 Reset。
- [ ] UI 不展示/分析 Memory 正文。

### 7.5 Native Agents 与 Multi-Agent

- [ ] Agent list/read/create/update/delete/validate/reload。
- [ ] multi-agent mode、max threads/depth 与构建限制。
- [ ] 父子 Thread、角色、委派、交互、等待、返回、失败和取消轨迹。
- [ ] Run 保存 Agent 配置快照，不复制调度状态。

### 7.6 Skills

- [ ] 个人/项目 Skill list/read/create/update/delete。
- [ ] `SKILL.md`、scripts、references、assets 结构与路径安全验证。
- [ ] 隔离测试、超时、结果和 Artifact。
- [ ] 项目 Skill Git Diff/Commit/回滚；个人 Skill Profile 版本记录。
- [ ] 作用域、来源、覆盖关系和 Reload/Watch 状态。

### 7.7 M3 门禁

- [ ] `G3-01` 每个启用模块有 Manifest 支持状态、最低版本和策略映射。
- [ ] `G3-02` 每个写操作有权限、影响预览、错误、审计和刷新验证。
- [ ] `G3-03` unsupported/incompatible/degraded/experimental 状态 UI 已验收。
- [ ] `G3-04` 不存在 Web 自建 Agent/Memory/Skill/Plugin/MCP fallback Runtime。

## 8. M4 — 可靠性、安全、发布与 Web-only GA

### 8.1 可靠性与恢复

- [ ] PostgreSQL PITR/备份恢复演练。
- [ ] Profile Home 备份、恢复、权限和一致性校验。
- [ ] Repository Mirror 重建与 Worktree 清理恢复。
- [ ] Codex 进程崩溃、输出损坏、事件滞后和重启风暴测试。
- [ ] 事件投影重建、Approval 对账和 Lease 巡检。
- [ ] Codex canary/回滚不改写 Profile Home。

### 8.2 安全

- [ ] Session/CSRF/CORS/WebSocket Origin 安全测试。
- [ ] SSRF、路径穿越、symlink、Git 参数和恶意 URL 测试。
- [ ] Secret/Prompt/代码/Memory 日志泄漏扫描。
- [ ] rootless sandbox、出网和容器逃逸评审。
- [ ] 跨组织/项目/Profile/Task ID 授权测试。
- [ ] 依赖/SBOM/镜像签名和 Critical/High 漏洞门禁。

### 8.3 性能、容量与可观测性

- [ ] API、WebSocket、事件补发和 Task 列表基准。
- [ ] 20 并发 Run 初始目标与单 Profile 并发实测。
- [ ] 长输出、大 Diff、10 万事件、磁盘压力和清理吞吐。
- [ ] Run/Profile/Approval/Queue/Git/MCP/Provider 指标与结构化日志。
- [ ] 无事件、失联、磁盘、版本不兼容、重启风暴告警与 Runbook。

### 8.4 UX、部署与 Tauri 删除

- [ ] 全路由桌面/平板/手机截图回归。
- [ ] WCAG 2.2 AA、键盘、焦点、Screen Reader 和 200% 缩放。
- [ ] Chrome/Edge/Safari 目标版本；Firefox Beta 验证。
- [ ] 生产 Dockerfile、Compose、TLS/WSS、上传限制和 Secret 配置。
- [ ] 空白 Linux 主机安装、升级、回滚和恢复演练。
- [ ] Web 作为主客户端稳定运行一个 Beta 周期。
- [ ] 删除 Tauri Transport、窗口、托盘、更新器、原生通知和桌面发布物。
- [ ] CI 禁止重新引入 Tauri 依赖与共享 Token/SSE Preview。

### 8.5 M4 门禁

- [ ] `G4-01` 产品文档 V1 总体验收全部通过。
- [ ] `G4-02` 无 Critical/High 未处置安全问题。
- [ ] `G4-03` 备份恢复、Codex 升级/回滚由非作者完成。
- [ ] `G4-04` 达到性能、容量、可用性和可访问性目标。
- [ ] `G4-05` 生产构建和用户流程不依赖 Tauri 或本地桥接。

## 9. 平台 API 初始清单

API 名称可在实现 ADR 中调整，但资源和行为必须覆盖：

### 9.1 Session 与组织

```text
POST   /api/bootstrap
POST   /api/sessions
DELETE /api/sessions/current
GET    /api/me
GET    /api/team/members
POST   /api/team/invitations
PATCH  /api/team/members/:id
```

### 9.2 Projects 与 Tasks

```text
GET/POST /api/projects
GET/PATCH/DELETE /api/projects/:id
POST /api/projects/:id/verify
GET/POST /api/projects/:id/tasks
GET/PATCH /api/tasks/:id
POST /api/tasks/:id/runs
POST /api/runs/:id/cancel
POST /api/runs/:id/continue
```

### 9.3 Task 控制与事件

```text
POST /api/tasks/:id/messages
POST /api/tasks/:id/steer
GET  /api/tasks/:id/events?after=<sequence>
GET  /api/tasks/:id/ws
POST /api/tasks/:id/control-lease
DELETE /api/tasks/:id/control-lease
```

### 9.4 Approvals、Git 与 Artifacts

```text
GET  /api/approvals
GET  /api/approvals/:id
POST /api/approvals/:id/decision
GET  /api/tasks/:id/changes
POST /api/tasks/:id/commit
POST /api/tasks/:id/push
GET  /api/artifacts/:id
```

### 9.5 Profile 与 Studio

```text
GET/POST /api/codex/profiles
GET/PATCH /api/codex/profiles/:id
POST /api/codex/profiles/:id/restart
GET/POST/PATCH/DELETE /api/codex/profiles/:id/providers
POST /api/codex/profiles/:id/providers/:providerId/refresh-models
GET /api/codex/profiles/:id/capabilities
GET/POST/PATCH/DELETE /api/codex/profiles/:id/mcp-servers
GET/POST/DELETE /api/codex/profiles/:id/plugins
GET/POST/PATCH/DELETE /api/codex/profiles/:id/skills
```

公开 API DTO 只包含平台 ID 和稳定业务字段；app-server 原始 request ID 只保存在 Host/Approval 内部映射。

## 10. 测试与 CI 矩阵

| 变化范围 | 必跑 |
| --- | --- |
| 根文档/脚本 | `bash -n scripts/*.sh`、JSON 校验、link/diff check |
| React 类型/组件 | `npm run typecheck`、目标 Vitest、视觉/浏览器测试 |
| Web API/Store | 单元、Migration、PostgreSQL 集成、授权测试 |
| Profile Host | Fake Server 故障矩阵 + 真实 app-server Smoke |
| Git Runtime | 临时仓库集成、路径/注入、Windows/Linux 路径测试 |
| Codex 协议 | `just fmt`、protocol/app-server scoped tests、Schema 生成；`codex/` 源码变更时加跑 `scripts/codex-customization-status.sh` |
| Provider | codex-api/model-provider/models-manager/app-server/TUI scoped tests |
| 跨项目合同 | Bundle 校验、生成类型、Fixture replay、兼容/破坏检测 |
| Release | E2E、故障注入、恢复、安全、容量、浏览器矩阵 |

### 10.1 CI 分层

1. **PR 快速层（目标 < 15 分钟）：** 格式、类型、单元、合同、目标 crate。
2. **PR 集成层：** PostgreSQL、临时 Git、Fake/真实 app-server 目标 Smoke。
3. **main 夜间层：** 多 Profile/多 Run、故障注入、上游状态、大 Fixture。
4. **发布层：** 固定二进制、SBOM、签名、升级/回滚、安全与 E2E。

不允许因完整 Codex Suite 很慢而省略目标 crate 测试；完整 Suite 按 `codex/AGENTS.md` 要求在必要时单独批准。

## 11. 部署与环境计划

### 11.1 开发环境

- Node/Rust/Codex 工具链固定版本。
- Compose 提供 PostgreSQL 和可选 Object Storage。
- 支持 Fake app-server 快速开发与真实本地 Codex Smoke。
- 测试 Profile Home、Mirror、Worktree 使用临时目录，测试后清理。

### 11.2 Alpha 环境

- 单 Linux 主机或明确支持的平台。
- Web Server、Profile Host、Runner 可同进程/同容器编排，但目录和接口保持分层。
- PostgreSQL 独立持久卷；Profile Home 与 Repository Mirror 分卷。
- TLS 由反向代理终止，浏览器只访问同源 HTTPS/WSS。

### 11.3 Beta/GA 演进

- Profile Host/Runner 可以按容量独立扩展，但不在有数据前强制拆微服务。
- Runner 使用标签描述平台、容量、沙箱和 Codex 构建。
- Web 与 Codex 构建分别 canary；Feature Policy 可先关闭新能力。
- 数据库、Profile Home、Mirror 和 Artifact 有独立备份/恢复策略。

## 12. 风险登记与触发条件

| 风险 | 触发信号 | 责任工作流 | 应对 |
| --- | --- | --- | --- |
| 上游同步冲突过大 | 单次冲突 > 30 文件或行为测试广泛失败 | Runtime | 拆 checkpoint，重放最小定制 patch |
| Manifest 设计重复维护 | 方法名需在多处手改 | Runtime/Contract | 从注册表和 annotation 生成 |
| Profile 单进程瓶颈 | 同用户排队/延迟超目标 | Platform/Runtime | 实测限制与 Profile 队列，不共享 Home 多实例 |
| 事件表增长过快 | 单 Task >10万、DB 写入抖动 | Platform | 分块、归档、Artifact、索引与保留策略 |
| Web 与 Runtime 状态不一致 | unknown event/伪 running 增加 | Contract/Platform | cursor、巡检、Fixture、版本门禁 |
| 安全债阻塞 Beta | Critical/High 依赖或边界问题 | Platform/QA | 按 SLA 修复，Beta 前红线门禁 |
| Studio 拖延 Alpha | 核心 Task 未闭环但 Studio 并行扩张 | Product/Experience | 冻结 M3，资源集中 M1 |

## 13. 地图回复卡片交付计划

**目标：** 在不扩大 Codex 定制面的前提下，让地理相关回答可以把地图卡片嵌入回复任意位置。服务端生成或整理需要可视化的 GeoJSON 与样式元数据并保存为受控 Artifact，浏览器通过 Mapbox GL 渲染点、线、面、边界、路线和大规模地理数据，并支持全屏查看。实现细节以 `docs/adr/005-map-reply-cards.md` 为准。

### 13.1 设计边界

- 卡片是平台 DTO 和 Artifact 能力，不是新的 Codex Thread、Memory 或 Agent 调度语义。
- 模型正文只携带小型结构化标记或卡片引用；数 MB 级 GeoJSON 不进入 LLM 逐字输出路径。
- 触发策略优先使用系统提示、平台后处理和可审计工具结果；只有现有 app-server 事件无法稳定承载卡片引用时，才增加最小 Runtime 协议 seam。
- 浏览器只能访问授权后的平台 Artifact URL，不能看到 Profile 路径、Workspace 路径、原始 app-server request ID 或 Secret。
- UI 能力必须由 Feature Policy 和生成合同共同门控；未验证构建显示降级文本和下载入口。

### 13.2 卡片格式与数据合同

| ID | P/规模 | 任务 | 验证 |
| --- | --- | --- | --- |
| MAP-A01 | P1/S | 定义 `reply_card.v1` 外层 DTO：`id`、`kind`、`placement`、`title`、`summary`、`artifact_id`、`schema_version`、`fallback_text` | DTO 单测和向后兼容 Fixture |
| MAP-A02 | P1/M | 定义 `map_card.v1` schema：GeoJSON Artifact 引用、bbox、默认 viewport、layer/style 列表、交互开关和 size limits | JSON Schema、TypeScript、Rust 类型生成无漂移 |
| MAP-A03 | P1/M | 约定回复内嵌标记格式，例如小型 fenced block 或 typed placeholder，并实现安全解析器 | 嵌入在段落前、中、后、多卡片和畸形标记测试 |
| MAP-A04 | P1/M | 将卡片 schema 纳入 Capability Manifest / Feature Policy，不支持时保留普通回复 | `check:codex-contracts` 和 feature-policy 测试 |

### 13.3 服务端生成与 Artifact 管理

| ID | P/规模 | 任务 | 验证 |
| --- | --- | --- | --- |
| MAP-B01 | P1/M | 增加 `map-card-builder` 平台服务，接收坐标、路线、边界或已有 GeoJSON，输出规范化 GeoJSON + style metadata | 点/线/面/FeatureCollection 单元测试 |
| MAP-B02 | P1/M | 大型 GeoJSON 写入 Artifact 存储并记录 MIME、大小、hash、owner、Task/Run 绑定和保留策略 | 未授权、越权、过期和大小上限测试 |
| MAP-B03 | P1/M | 支持流式或后台生成路径：先展示 loading card，再用事件更新为 ready/error | 断线重连、重复事件和终态恢复测试 |
| MAP-B04 | P1/M | 增加 tool-result resolver，允许卡片 marker 的 `input_ref` 安全引用先前边界、路线或地理工具输出 | 大型 GeoJSON 不进模型正文；跨 Task/Run/Profile 引用被拒绝 |
| MAP-B05 | P1/M | 增加地理意图触发提示和平台后处理：经纬度、地名、路线/距离、边界和地理数据可视化 | prompt fixture、误触发/漏触发样本集 |
| MAP-B06 | P2/M | 接入可选地理工具或 MCP 结果作为卡片 builder 输入，不把外部 API key 暴露给模型或浏览器 | Secret redaction、工具失败和限流测试 |

### 13.4 浏览器渲染与体验

| ID | P/规模 | 任务 | 验证 |
| --- | --- | --- | --- |
| MAP-C01 | P1/M | 实现回复流中的 card slot renderer，保持正文顺序并支持多个卡片 | React 单测和消息 replay 测试 |
| MAP-C02 | P1/M | 使用 Mapbox GL 渲染 GeoJSON 点、线、面和样式，支持 bbox fit、图层开关、legend 和 tooltip | 组件测试、截图和目标视口验证 |
| MAP-C03 | P1/S | 增加全屏查看、键盘退出、移动端布局和加载/空/错误/无权限状态 | 可访问性和移动端视口测试 |
| MAP-C04 | P1/M | 增加 active map budget，离屏或超预算 Mapbox 实例进入 suspended 状态并可点击重新激活 | 多地图消息性能测试和资源释放断言 |
| MAP-C05 | P1/M | 对超大数据采用 simplify/tiling/download fallback 策略，避免阻塞主线程 | 大文件性能预算和 worker 测试 |

### 13.5 Codex 收敛与发布门禁

| ID | P/规模 | 任务 | 验证 |
| --- | --- | --- | --- |
| MAP-D01 | P0/S | 在实现前确认现有 Codex message/event 是否足以传递卡片标记；若触及 `codex/`，先运行上游状态脚本并更新 patch map 分类 | `scripts/codex-upstream-status.sh`、patch map 审查 |
| MAP-D02 | P1/M | 增加端到端 Smoke：模型回答中混排文本和地图卡片，Artifact 鉴权后由浏览器渲染 | 真实 app-server smoke + Web E2E |
| MAP-D03 | P1/M | 覆盖安全门禁：XSS、GeoJSON 属性注入、Mapbox token 暴露、Artifact 越权、事件重放 | 安全测试和审计日志断言 |
| MAP-D04 | P1/S | 发布能力说明：支持的数据规模、样式子集、降级行为和运维配置 | 兼容矩阵与用户文档 |

## 14. 建议的下一开发批次

当前不应扩展完整 Studio 或继续围绕旧 RPC/SSE Gateway 增加产品功能。建议按以下顺序执行：

### Batch 0：Codex 定制收敛与可重放记录（已完成）

维护 `docs/custom-codex-patch-map.md` 和 `.sync/codex-customization-inventory.json`：每次官方同步都重新比较 `HEAD:codex` 与 `codex-upstream/main`，按既定顺序重放核心 seam，并更新验证证据。

**本批已完成：**

1. Platform Server 启动路径复用 `apps/web/crates/profile-host` provisioner（`--codex-home` / `CODEX_HOME`）。
2. `codex/utils/home-dir` 恢复官方缺失 `CODEX_HOME` 拒绝语义（`move-out` 完成）。
3. inventory 与 patch map 已刷新；后续 `codex/` 变更继续满足 `G0-06`。

**证据：** 非生成差异已分类；Tauri 与 Platform Server 均在 spawn 前 provision Profile Home；Provider/TUI scoped tests、protocol tests、真实 Manifest Smoke 和 Web contract check 通过；官方 `af71774d2645` 已按清单重放，当前无待集成上游提交。

### Batch 1：生成式合同与安全平台边界（已完成）

1. `M0-B03/B04`：`experimental` 全自动声明生成、Feature Policy 单测覆盖。
2. `M0-B09` 与 `M0-C03/C04/C05`：CI 固定 bundle 摘要、从 Bundle 生成消费类型。
3. 真实 `--require-manifest` Smoke 保留；Schema/Manifest 漂移门禁由 `codex-protocol-ci` 与 `check:codex-generated` 承担。
4. Legacy `/api/rpc` 与 permissive CORS 默认关闭；本地迁移需 `CODEX_ALLOW_LEGACY_PROXY=1`。

**完成证据：** `just test -p codex-app-server-protocol`（281 passed）、`npm run check:codex-generated`、`npm run test:feature-policy`、Web contract check 通过。

### Batch 2：Profile Host 与持久审批最小闭环

1. 完成 `M1-C01` 至 `M1-C07` 的最小纵向：每用户 Home、单主锁、原生 app-server stdio 生命周期、请求关联、Manifest 门禁；用它替换 `RealCodexAdapter` 对 Tauri daemon 的依赖。
2. 将 Provider CRUD 编排、Secret 注入和模型刷新从 `src-tauri/src/shared/codex_core.rs` 移入 Profile Host/provider service；Tauri 过渡适配器只能复用该服务。
3. 增加 Profile/Capability/Approval/Audit migrations 和归属约束。
4. 将命令、文件、权限和结构化输入请求先持久化再通知，使用 CAS 决策。
5. 运行真实 Profile restart、Thread list/read/resume、Provider switch 与 Approval Smoke。

**完成证据：** Host 重启恢复同一用户 Thread；另一用户无法访问 Profile/Thread/请求；过期请求不复用。

### Batch 3：Git Runner 与 Task 纵向编排

1. 完成 Mirror/Worktree 路径安全与 Run workspace 状态。
2. 用幂等 Scheduler 串联 queued Run → Worktree → Profile → Thread → Turn。
3. 建立单 Task monotonic event sequence、持久 replay 和明确故障终态。
4. 将 Browser MVP 切到平台 Task DTO，完成初始化→项目→Task→审批→Diff→Commit 的单用户流程。

**完成证据：** 两次重复提交只产生一个 Run；刷新和进程重启可恢复；每个 Run 独立 Worktree；浏览器不提交服务器路径。

完成上述四批后再进入 M2 多用户 Beta。M2 的首个门禁是两用户跨 Profile、Thread、Workspace、事件、审批和 Secret 的系统性拒绝矩阵，而不是先增加更多页面。
