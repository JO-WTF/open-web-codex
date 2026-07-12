# CodexMonitor Codex Web Harness V1 开发任务计划

## 0. 执行规则

### 0.1 任务状态

- `[ ]` 未开始。
- `[-]` 进行中；同一仓库同一时刻只允许一个阶段内的少量关联任务处于此状态。
- `[x]` 已完成；必须同时具备代码、测试和验证证据。
- `[!]` 阻塞；必须记录阻塞的上游任务 ID，不允许用替代 Runtime 绕过。

### 0.2 Codex 执行顺序

1. 阶段 0 必须最先完成；之后阶段 1（Codex Rust）与阶段 2（CodexMonitor）并行执行。
2. 阶段 2 至阶段 11 按编号顺序执行；未通过当前 Web 阶段门禁，不进入下一 Web 阶段。
3. 阶段 1 是上游并行泳道，按 `G1-001` 至 `G1-007` 分批解锁 Web 模块，不要求等待全部 Rust 任务后才开始阶段 2。
4. 在阶段内按模块顺序执行；只选择本仓库依赖和关联 `CR-*` 依赖都已完成的第一个未完成任务。
5. 每次实现前读取关联代码、接口、Schema、测试和 PRD 需求，不凭计划文本猜测现有实现。
6. 每个任务只完成一个可验证功能点；超出一个 Pull Request 合理范围时继续拆分。
7. 每个功能点完成后运行最窄相关测试，再运行模块测试；阶段门禁运行阶段全量测试。
8. 修改 app-server 适配层前校验锁定的 Capability Manifest、Schema 和 Rust 构建版本。
9. 缺少 Codex bridge 时将任务标为 `[!]` 并关联 `CR-*`；禁止在 Web 项目实现 Agent Planner、子 Agent Scheduler、Memory Engine、Skill Interpreter、Plugin Runtime 或 MCP Runtime。
10. 不删除或覆盖用户已有修改；不把平台数据库变成 Codex Thread、Turn 或 Memory 的第二事实来源。

### 0.3 单任务完成条件

- [ ] 实现满足关联 PRD 编号和接口合同。
- [ ] 成功、失败、权限拒绝、重试或并发路径已覆盖。
- [ ] 日志、错误码和审计字段不泄露 Secret、Prompt、代码正文或 Memory 正文。
- [ ] 单元、集成、合同或 E2E 测试按任务类型通过。
- [ ] API、Schema、迁移、配置或运维文档随代码同步更新。
- [ ] 不引入未记录的 Tauri 依赖、桌面专属假设或 Codex Runtime 替代实现。

## 阶段 0：仓库基线与双项目合同

**仓库：** CodexMonitor

**依赖：** 无

**关联 PRD：** CAP-001 至 CAP-008、产品边界与双项目责任矩阵

### 模块 0.1：仓库与构建基线

- [x] `S0-M1-001` 记录当前分支、Node、Rust、Codex CLI/app-server、PostgreSQL 和操作系统版本。证据：`docs/development-baseline.md`。
- [x] `S0-M1-002` 跑通现有前端 Build、Typecheck、Lint 和 Unit Test，保存基线结果。证据：`docs/development-baseline.md`；993/999 tests passed，6 个既有 locale-sensitive failures 已记录。
- [!] `S0-M1-003` 跑通现有 Rust Build、Test 和 app-server Smoke Test，保存基线结果。部分完成：389 个 `--no-default-features` tests 通过；默认 `whisper-rs` 构建和 WindowsApps app-server 执行被阻塞，见 `docs/development-baseline.md`，后者由 `S0-M4-001` 解除。
- [x] `S0-M1-004` 盘点 Tauri commands、events、plugins、窗口 API、文件系统 API 和桌面发布脚本。证据：`docs/tauri-migration-inventory.md`。
- [x] `S0-M1-005` 建立“复用、抽离、Web 替换、删除、延期”代码迁移清单。证据：`docs/tauri-migration-inventory.md`。
- [x] `S0-M1-006` 建立禁止新增桌面功能和 Tauri 直接依赖的 CI 检查。证据：`scripts/check-tauri-boundary.mjs`、`check:tauri-boundary` 和 CI `tauri-boundary` Job。

### 模块 0.2：Capability Gap 与协议合同

- [x] `S0-M2-001` 盘点当前 `initialize`、Thread、Turn、审批、用户输入和事件协议。证据：`docs/codex-capability-gap.md`，并修正 `docs/app-server-events.md` 的 `thread/read` 漂移。
- [x] `S0-M2-002` 盘点 Agents、multi-agent、Skills、Plugins、MCP、OAuth、elicitation、compaction 和 Memory 已有能力。证据：`docs/codex-capability-gap.md`。
- [x] `S0-M2-003` 将缺失能力映射为 `CR-*` 上游任务并标注 Web 依赖模块。证据：`docs/codex-capability-gap.md` 的 V1 Capability Gaps。
- [x] `S0-M2-004` 定义 Capability Manifest v1 的能力 ID、版本、限制和状态字段。证据：`contracts/codex/capability-manifest.schema.json`、能力 ID 注册表和 V1 Fixture。
- [x] `S0-M2-005` 定义请求、响应、通知、错误和 Fixture 的版本规则。证据：`contracts/codex/README.md`、Error 与 Protocol Fixture Schema。
- [x] `S0-M2-006` 定义 `supported`、`unsupported`、`degraded`、`incompatible` 的统一语义。证据：Capability Manifest Schema 与合同说明；另含受策略控制的 `experimental` 状态。
- [x] `S0-M2-007` 定义 Web 与 Rust 项目的版本兼容矩阵和升级顺序。证据：`contracts/codex/compatibility-matrix.json`。
- [x] `S0-M2-008` 定义 Schema 破坏性变更检测和发布阻断规则。证据：`scripts/check-codex-contracts.mjs`、`check:codex-contracts` 和 CI `codex-contracts` Job。

### 模块 0.3：架构 ADR

- [x] `S0-M3-001` 固化 Browser -> Web Server -> Codex Host -> Profile -> app-server 调用链。证据：ADR 0001。
- [x] `S0-M3-002` 固化一个成员默认一个持久个人 Codex Profile。证据：ADR 0002。
- [x] `S0-M3-003` 固化一个 Profile 同时最多一个主 app-server 进程。证据：ADR 0002。
- [x] `S0-M3-004` 固化一个 Profile 可注册多个已授权 Workspace 和 Thread。证据：ADR 0002。
- [x] `S0-M3-005` 固化每个 Run 在 Task 边界内使用独立 Git Worktree。证据：ADR 0003。
- [x] `S0-M3-006` 固化平台 DB、Codex Profile、Git Repository 三类事实来源边界。证据：ADR 0001。
- [x] `S0-M3-007` 固化 Profile Home、Workspace、Artifact、Secret 的目录与加密边界。证据：ADR 0003。
- [x] `S0-M3-008` 固化 Run、Profile、Workspace、Approval、Control Lease 状态机。证据：ADR 0004。

### 模块 0.4：合同测试骨架

- [x] `S0-M4-001` 创建可启动真实 `codex app-server` 的测试 Harness。证据：`scripts/codex-app-server-smoke.mjs` 和受控假服务自测。
- [x] `S0-M4-002` 创建离线 Fixture 回放器并支持请求/通知顺序校验。证据：`scripts/replay-codex-fixtures.mjs` 和协议 Fixtures。
- [x] `S0-M4-003` 创建 Capability Manifest 解析与兼容判定测试。证据：`scripts/lib/codex-capability-manifest.mjs` 及其测试。
- [x] `S0-M4-004` 创建跨项目 Fixture 包下载、校验和缓存流程。证据：`scripts/fetch-codex-contracts.mjs`、Bundle Schema 和自测。
- [x] `S0-M4-005` 创建真实 Rust 构建 Smoke Test 的受控 CI Job。证据：`.github/workflows/codex-app-server-smoke.yml`。
- [x] `S0-M4-006` 创建未知字段、未知事件、无效 JSON、超时和进程退出测试。证据：未知方法 Fixture 与 app-server Harness 故障自测。

### 阶段 0 门禁

- [x] `G0-001` 现有构建与测试基线可重复执行。证据：`docs/development-baseline.md` 及阶段 0 验证命令。
- [x] `G0-002` 所有 V1 Capability Gap 均有唯一 `CR-*` 任务。证据：`docs/codex-capability-gap.md`。
- [x] `G0-003` Profile、Workspace、Thread 和事实来源 ADR 已冻结。证据：`docs/adr/0001` 至 `0004`。
- [!] `G0-004` Manifest、Schema、Fixture 和版本合同由两个项目共同确认。阻塞：本机未找到并行 Codex Rust 仓库，等待上游项目签收 `contracts/codex`。
- [!] `G0-005` 离线 Fixture 与真实 app-server Smoke Test 均可运行。离线回放已通过；阻塞：等待可执行且带 SHA-256 的 Codex Rust app-server 构建运行受控 Smoke。

## 阶段 1：并行 Codex Rust Bridge

**仓库：** Codex Rust 改造项目

**依赖：** `G0-002` 至 `G0-005`

**关联 PRD：** PRO、AGT、SKL、PLG、MCP、MEM、CAP 全部需求

### 模块 1.1：Capability 与 Schema

- [ ] `CR-001` 实现版本化 Capability Manifest 输出。
- [ ] `CR-002` 为每项能力输出版本、方法、事件、限制和实验状态。
- [ ] `CR-003` 导出请求、响应、通知和错误 Schema。
- [ ] `CR-004` 建立稳定错误码及可重试、不可重试、权限、兼容分类。
- [ ] `CR-005` 生成离线协议 Fixtures 和构建哈希。
- [ ] `CR-006` 实现客户端版本协商和不兼容拒绝。
- [ ] `CR-007` 增加 Schema/Fixture 一致性 CI。

### 模块 1.2：Profile、Thread 与 Memory

- [ ] `CR-101` 支持一个 app-server Session 注册多个 Workspace。
- [ ] `CR-102` 支持 Workspace 注册、查询、更新和移除事件。
- [ ] `CR-103` 完善 Thread list、read、start、resume、archive 和恢复错误。
- [ ] `CR-104` 输出 context compaction 开始、完成和失败事件。
- [ ] `CR-105` 输出 memory consolidation 开始、完成和失败事件。
- [ ] `CR-106` 提供 Memory 状态、诊断和容量读取 bridge。
- [ ] `CR-107` 提供 Memory 导出和受控重置 bridge。
- [ ] `CR-108` 增加同一 Codex Home 重启后的 Thread/Memory 连续性测试。

### 模块 1.3：Native Agents 与 Multi-Agent

- [ ] `CR-201` 提供 Agent list、read、create、update、delete bridge。
- [ ] `CR-202` 提供 Agent 配置校验和 reload 结果。
- [ ] `CR-203` 提供 `multi_agent_enabled`、`max_threads`、`max_depth` 配置 bridge。
- [ ] `CR-204` 输出父子 Thread、派生、委派、等待、返回和汇总事件。
- [ ] `CR-205` 输出子 Agent 失败、取消、超限和部分完成语义。
- [ ] `CR-206` 提供运行引用的 Agent 配置快照字段。
- [ ] `CR-207` 增加真实 multi-agent Smoke Test 和回放 Fixtures。

### 模块 1.4：Skills

- [ ] `CR-301` 提供个人与项目 Skill list、read bridge。
- [ ] `CR-302` 提供 Skill create、update、delete bridge。
- [ ] `CR-303` 提供 `SKILL.md`、scripts、references、assets 结构校验。
- [ ] `CR-304` 提供路径、符号链接、大小和文件类型安全错误。
- [ ] `CR-305` 提供 Skill reload/watch 和发现状态。
- [ ] `CR-306` 提供隔离 Skill test hook 和测试结果事件。
- [ ] `CR-307` 提供作用域、覆盖关系、来源和版本字段。
- [ ] `CR-308` 生成创建、校验、测试、reload、失败 Fixtures。

### 模块 1.5：Plugins

- [ ] `CR-401` 提供 Plugin list、read 和 Manifest bridge。
- [ ] `CR-402` 提供 Plugin install、update、enable、disable、uninstall bridge。
- [ ] `CR-403` 输出 Plugin 来源、版本、完整性、依赖和兼容状态。
- [ ] `CR-404` 输出 Plugin 权限及升级前后权限差异。
- [ ] `CR-405` 输出 Plugin 提供的 Skills、MCP、Apps 和其他能力。
- [ ] `CR-406` 定义安装失败、部分安装、回滚和依赖冲突语义。
- [ ] `CR-407` 生成完整生命周期和权限变化 Fixtures。

### 模块 1.6：MCP 与 Tools

- [ ] `CR-501` 提供 MCP 配置 list、read、create、update、delete bridge。
- [ ] `CR-502` 提供 MCP reload、status 和分步连接测试 bridge。
- [ ] `CR-503` 提供工具发现、Schema、权限和调用状态事件。
- [ ] `CR-504` 提供 OAuth start、callback completion、cancel 和错误 bridge。
- [ ] `CR-505` 提供 elicitation 请求、响应、过期和取消协议。
- [ ] `CR-506` 提供 MCP Server 启用、停用和活动 Turn 影响语义。
- [ ] `CR-507` 提供认证、网络、协议、工具发现的结构化错误。
- [ ] `CR-508` 生成 OAuth、elicitation、reload、调用和失败 Fixtures。

### 模块 1.7：Rust V1 冻结发布

- [ ] `CR-601` 运行全量 Rust Unit、Integration 和真实 app-server Test。
- [ ] `CR-602` 运行旧 Fixture、新 Fixture 和跨版本回放测试。
- [ ] `CR-603` 运行多 Workspace、多 Thread 和长时间进程泄漏测试。
- [ ] `CR-604` 发布 Manifest、Schema、Fixtures、二进制和校验和。
- [ ] `CR-605` 发布 SBOM、已知限制、迁移和回滚说明。
- [ ] `CR-606` 固定 V1 支持窗口和兼容版本范围。
- [ ] `CR-607` 触发 CodexMonitor 跨项目合同与真实 Smoke Test。

### 阶段 1 门禁

- [ ] `G1-001` `CR-001` 至 `CR-007` 完成后才允许合并 Web Capability 适配器。
- [ ] `G1-002` `CR-101` 至 `CR-108` 完成后才允许完成 Profile Host 和 Memory 治理。
- [ ] `G1-003` `CR-201` 至 `CR-207` 完成后才允许启用 Native Agents 与 multi-agent UI。
- [ ] `G1-004` `CR-301` 至 `CR-308` 完成后才允许启用 Skills 写入与测试。
- [ ] `G1-005` `CR-401` 至 `CR-407` 完成后才允许启用 Plugin 生命周期操作。
- [ ] `G1-006` `CR-501` 至 `CR-508` 完成后才允许启用 MCP 写入、OAuth 和 elicitation。
- [ ] `G1-007` `CR-601` 至 `CR-607` 完成后才允许删除 Tauri 和发布 GA。

## 阶段 2：共享核心抽离

**仓库：** CodexMonitor

**依赖：** 阶段 0；模块 2.1、2.2 的合同生成任务依赖 `G1-001`

**关联 PRD：** CAP-001 至 CAP-008、REPO-001 至 REPO-005

### 模块 2.1：Cargo Workspace 与合同类型

- [ ] `S2-M1-001` 创建顶层 Cargo Workspace。
- [ ] `S2-M1-002` 创建 `crates/platform-contracts`。
- [ ] `S2-M1-003` 创建 `crates/codex-contracts` 并从上游 Schema 生成 Rust 类型。
- [ ] `S2-M1-004` 生成 TypeScript Capability、请求、响应和事件类型。
- [ ] `S2-M1-005` 增加生成代码版本与 Rust 构建哈希校验。
- [ ] `S2-M1-006` 增加禁止手工修改生成类型的 CI 检查。

### 模块 2.2：Codex Host Adapter

- [ ] `S2-M2-001` 创建 `crates/codex-host-adapter`。
- [ ] `S2-M2-002` 抽离 app-server 子进程启动、stdin/stdout 和关闭逻辑。
- [ ] `S2-M2-003` 抽离请求 ID 关联、超时、取消和并发请求管理。
- [ ] `S2-M2-004` 抽离通知解析、未知事件容错和 Event Sink。
- [ ] `S2-M2-005` 抽离 Codex Home、环境变量和启动参数构建。
- [ ] `S2-M2-006` 实现 Profile 级进程锁和 Session Registry 接口。
- [ ] `S2-M2-007` 实现 Capability handshake 和版本判定接口。
- [ ] `S2-M2-008` 实现 Workspace 注册与 Thread API 适配接口。
- [ ] `S2-M2-009` 移除核心层对 Tauri `AppHandle`、`State`、Emitter 和窗口类型的引用。

### 模块 2.3：Git Runtime

- [ ] `S2-M3-001` 创建 `crates/git-runtime`。
- [ ] `S2-M3-002` 抽离远端验证、Clone/Mirror 和 Fetch。
- [ ] `S2-M3-003` 抽离 Worktree create、status、remove 和 prune。
- [ ] `S2-M3-004` 抽离文件列表、Diff、二进制和大文件元数据。
- [ ] `S2-M3-005` 抽离 Commit、分支命名和作者信息。
- [ ] `S2-M3-006` 抽离 Push、远端领先、认证失败和保护分支错误。
- [ ] `S2-M3-007` 对 Git 参数、路径、引用名和命令输出做结构化处理。
- [ ] `S2-M3-008` 增加临时仓库集成测试和 Windows/Linux 路径测试。

### 模块 2.4：过渡适配器

- [ ] `S2-M4-001` 让现有 Tauri command 调用新核心 crate。
- [ ] `S2-M4-002` 让现有 daemon/backend 调用新核心 crate。
- [ ] `S2-M4-003` 保持现有桌面行为测试通过。
- [ ] `S2-M4-004` 建立 Tauri Transport 与 Web Transport 的 Feature 级切换点。
- [ ] `S2-M4-005` 禁止业务组件直接调用 `invoke` 或监听 Tauri Event。
- [ ] `S2-M4-006` 建立桌面专属模块冻结清单。
- [ ] `S2-M4-007` 增加核心 crate 不依赖 `tauri` 的架构测试。

### 阶段 2 门禁

- [ ] `G2-001` 新核心 crates 不依赖 Tauri。
- [ ] `G2-002` Tauri 与 daemon 现有核心测试继续通过。
- [ ] `G2-003` 独立测试程序可用指定 Profile 启动 app-server 并完成一个 Thread。
- [ ] `G2-004` 两个 Profile 使用不同 Codex Home，同一 Profile 并发启动只产生一个主进程。
- [ ] `G2-005` Git Runtime 在临时仓库完成 Worktree、Diff、Commit 和 Push 冲突测试。

## 阶段 3：Web Server 与数据层

**依赖：** 阶段 2

**关联 PRD：** AUTH、ORG、REPO、RUN、EVT、AUD 基础需求

### 模块 3.1：Server 骨架

- [ ] `S3-M1-001` 创建 `server` Rust crate 和 Axum 启动入口。
- [ ] `S3-M1-002` 实现配置文件、环境变量、Secret 引用和启动校验。
- [ ] `S3-M1-003` 实现结构化日志、Request ID、Trace 和错误响应规范。
- [ ] `S3-M1-004` 实现 `/health/live`、`/health/ready` 和依赖健康检查。
- [ ] `S3-M1-005` 实现优雅停机和后台任务排空。

### 模块 3.2：PostgreSQL Schema

- [ ] `S3-M2-001` 建立 migration、连接池、事务和测试数据库工具。
- [ ] `S3-M2-002` 创建 users、sessions、organizations、memberships。
- [ ] `S3-M2-003` 创建 projects、project_memberships、repositories、Git credentials 引用。
- [ ] `S3-M2-004` 创建 codex_profiles、profile_bindings、capability_snapshots。
- [ ] `S3-M2-005` 创建 tasks、task_members、codex_thread_mappings、runs。
- [ ] `S3-M2-006` 创建 workspaces、workspace_registrations、runner_leases。
- [ ] `S3-M2-007` 创建 run_events、run_snapshots、approvals、control_leases。
- [ ] `S3-M2-008` 创建 comments、notifications、artifacts、audit_events。
- [ ] `S3-M2-009` 创建 skill_publications、integration_operations 和 Secret metadata。
- [ ] `S3-M2-010` 为 organization、资源所有权、状态、时间和游标建立索引与唯一约束。
- [ ] `S3-M2-011` 增加空库升级、已有库升级、失败回滚和并发 migration 测试。

### 模块 3.3：认证与会话

- [ ] `S3-M3-001` 实现首个 Owner 初始化。
- [ ] `S3-M3-002` 实现邀请创建、验证、接受、过期和撤销。
- [ ] `S3-M3-003` 实现登录、退出、当前用户和会话续期。
- [ ] `S3-M3-004` 实现 HttpOnly、Secure、SameSite Cookie。
- [ ] `S3-M3-005` 实现 CSRF、CORS、CSP 和登录限流。
- [ ] `S3-M3-006` 实现会话列表、单会话吊销和全部会话吊销。
- [ ] `S3-M3-007` 实现禁用用户后现有 Session 和 WebSocket 失效。
- [ ] `S3-M3-008` 增加认证枚举、重放、过期和并发测试。

### 模块 3.4：RBAC 与资源 API

- [ ] `S3-M4-001` 实现组织角色、项目角色和资源所有权授权函数。
- [ ] `S3-M4-002` 实现 Organization、Member 和 Invitation API。
- [ ] `S3-M4-003` 实现 Project、Project Member 和 Repository API。
- [ ] `S3-M4-004` 实现 Codex Profile 和 Capability 查询 API 骨架。
- [ ] `S3-M4-005` 实现 Task、Run、Workspace 和 Thread Mapping API 骨架。
- [ ] `S3-M4-006` 实现游标分页、过滤、排序和幂等键。
- [ ] `S3-M4-007` 生成 OpenAPI 并校验前端 Client 类型。
- [ ] `S3-M4-008` 为全部资源增加跨组织、跨项目、ID 枚举和越权测试。

### 模块 3.5：实时与后台任务

- [ ] `S3-M5-001` 实现认证 WebSocket 建连和订阅授权。
- [ ] `S3-M5-002` 实现事件序号、游标确认和断线补发。
- [ ] `S3-M5-003` 实现事件幂等、增量合并和未知事件隔离。
- [ ] `S3-M5-004` 实现快照加增量恢复接口。
- [ ] `S3-M5-005` 实现权限变化后订阅重算和强制退订。
- [ ] `S3-M5-006` 实现 PostgreSQL 后台 Job 领取、重试和死信状态。
- [ ] `S3-M5-007` 实现清理、通知、过期审批和租约巡检 Job 骨架。
- [ ] `S3-M5-008` 增加断线、重复、乱序、过期游标和 10,000 事件测试。

### 阶段 3 门禁

- [ ] `G3-001` 两个测试用户和两个组织的数据访问严格隔离。
- [ ] `G3-002` Token 和 Secret 不进入 URL、localStorage、普通日志或 API 响应。
- [ ] `G3-003` OpenAPI、数据库迁移和 WebSocket 合同测试通过。
- [ ] `G3-004` 数据库只保存平台映射与治理状态，不复制 Codex Memory 或 Thread 正文作为事实来源。

## 阶段 4：Codex Profile Host

**依赖：** 阶段 3、`G1-001`、`G1-002`

**关联 PRD：** PRO-001 至 PRO-008、MEM-001 至 MEM-007、CAP-001 至 CAP-008

### 模块 4.1：Profile Home

- [ ] `S4-M1-001` 实现用户到默认个人 Profile 的创建与绑定。
- [ ] `S4-M1-002` 实现 Profile Home 路径分配、权限和所有权校验。
- [ ] `S4-M1-003` 实现 Profile 配置、身份和 Secret 引用注入。
- [ ] `S4-M1-004` 禁止 Profile 使用宿主任意路径或其他 Profile Home。
- [ ] `S4-M1-005` 实现 Profile 状态 ready、auth_required、degraded、incompatible、stopped。
- [ ] `S4-M1-006` 实现 Profile 创建、读取、更新和停止 API。
- [ ] `S4-M1-007` 实现 Profile 导出、退出身份、Memory 重置、删除四个独立用例。
- [ ] `S4-M1-008` 为所有危险操作增加资源名确认、活动 Turn 检查和审计。

### 模块 4.2：App-server 生命周期

- [ ] `S4-M2-001` 实现 Profile Session Registry。
- [ ] `S4-M2-002` 实现数据库租约与进程锁组合的单主实例保护。
- [ ] `S4-M2-003` 实现 app-server 启动、初始化、心跳和优雅停止。
- [ ] `S4-M2-004` 实现 PID、Host、版本、启动时间和最近错误记录。
- [ ] `S4-M2-005` 实现并发启动请求幂等合并。
- [ ] `S4-M2-006` 实现异常退出的有界退避重启。
- [ ] `S4-M2-007` 实现活动 Turn 存在时的安全重启策略。
- [ ] `S4-M2-008` 实现 Host 进程重启后的 Profile Session 重建。
- [ ] `S4-M2-009` 增加进程泄漏、锁失效、双主和重启风暴测试。

### 模块 4.3：Capability 门控

- [ ] `S4-M3-001` 启动时读取并验证 Capability Manifest。
- [ ] `S4-M3-002` 保存 Profile Capability Snapshot 和 Rust 构建哈希。
- [ ] `S4-M3-003` 实现 API 级能力检查和统一 unsupported 错误。
- [ ] `S4-M3-004` 实现 incompatible Profile 禁止新 Run。
- [ ] `S4-M3-005` 实现能力变化事件和前端缓存失效。
- [ ] `S4-M3-006` 增加 Manifest 缺失、损坏、降级和版本漂移测试。

### 模块 4.4：Workspace 与 Thread 注册

- [ ] `S4-M4-001` 校验用户、项目、Profile 和 Workspace 授权关系。
- [ ] `S4-M4-002` 将新 Workspace 注册到 Profile app-server。
- [ ] `S4-M4-003` 保存 workspace_registration 和 Codex 返回标识。
- [ ] `S4-M4-004` 创建 Task 时启动新 Codex Thread 并保存唯一映射。
- [ ] `S4-M4-005` Continue/Recover 时读取并恢复原 Codex Thread。
- [ ] `S4-M4-006` Workspace 清理时移除注册但不删除 Profile 和历史 Thread。
- [ ] `S4-M4-007` 增加同 Profile 多 Workspace、多 Thread 和越权注册测试。

### 模块 4.5：Memory 与恢复

- [ ] `S4-M5-001` 读取并展示 compaction 与 consolidation 状态元数据。
- [ ] `S4-M5-002` 实现 Memory 诊断 API，不返回未授权正文。
- [ ] `S4-M5-003` 实现 Memory 导出任务、下载授权和审计。
- [ ] `S4-M5-004` 实现 Memory 重置前影响范围计算。
- [ ] `S4-M5-005` 实现重置后的 app-server reload 和验证。
- [ ] `S4-M5-006` 实现 Profile 重启后的 Thread/Memory 连续性检查。
- [ ] `S4-M5-007` 增加两个 Profile 相似任务的 Thread/Memory 串用测试。
- [ ] `S4-M5-008` 增加损坏 Home、失败导出、失败重置和回滚测试。

### 阶段 4 门禁

- [ ] `G4-001` 同一 Profile 并发启动始终只有一个主 app-server。
- [ ] `G4-002` 同一 Profile 可同时注册两个 Workspace 并运行独立 Thread。
- [ ] `G4-003` Host 重启后 Profile、Workspace 注册和已知 Thread 可恢复。
- [ ] `G4-004` 两个 Profile 的 Home、身份、Thread 和 Memory 无交叉。
- [ ] `G4-005` 缺失或不兼容 Capability 会阻止操作且没有 fallback Runtime。

## 阶段 5：Runner 与任务执行

**依赖：** 阶段 4

**关联 PRD：** REPO-001 至 REPO-005、RUN-001 至 RUN-007、EVT-001 至 EVT-005、APR-001 至 APR-005

### 模块 5.1：Repository 与 Worktree

- [ ] `S5-M1-001` 实现 Git URL、凭据和默认分支验证。
- [ ] `S5-M1-002` 实现受控 Repository Mirror 创建和 Fetch。
- [ ] `S5-M1-003` 实现项目 ready、setup_failed 和 retry 状态。
- [ ] `S5-M1-004` 为每个 Run 创建 Task 边界内的独立 Worktree。
- [ ] `S5-M1-005` 实现分支命名、重名和保护分支规则。
- [ ] `S5-M1-006` 实现 Worktree 状态、保留期和只读归档。
- [ ] `S5-M1-007` 实现幂等 remove、prune 和失败清理重试。
- [ ] `S5-M1-008` 增加路径穿越、符号链接、并发创建和异常中断测试。

### 模块 5.2：队列与租约

- [ ] `S5-M2-001` 实现 queued Run 的 PostgreSQL 原子领取。
- [ ] `S5-M2-002` 实现 Runner 注册、心跳、容量和 draining 状态。
- [ ] `S5-M2-003` 实现 Runner Lease 续期、过期和失联判定。
- [ ] `S5-M2-004` 实现组织、项目、Profile 和 Runner 并发限制。
- [ ] `S5-M2-005` 实现排队原因、优先级和近似顺序。
- [ ] `S5-M2-006` 实现取消 queued/provisioning Run。
- [ ] `S5-M2-007` 增加重复领取、租约竞争、Runner 失联和 Server 重启测试。

### 模块 5.3：执行环境

- [ ] `S5-M3-001` 实现 Workspace 环境目录和最小环境变量。
- [ ] `S5-M3-002` 实现 rootless 容器启动接口。
- [ ] `S5-M3-003` 实现 CPU、内存、进程数、磁盘和运行时间限制。
- [ ] `S5-M3-004` 实现只读/可写挂载白名单和禁止 Docker Socket。
- [ ] `S5-M3-005` 实现 Git、Codex 和 MCP Secret 的短期引用注入。
- [ ] `S5-M3-006` 实现日志、环境和错误中的 Secret 脱敏。
- [ ] `S5-M3-007` 实现 Workspace 进程树跟踪和 Turn 级终止。
- [ ] `S5-M3-008` 确保取消一个 Run 不终止同 Profile 其他 Thread。
- [ ] `S5-M3-009` 增加容器逃逸面、资源耗尽和清理后凭据不可访问测试。

### 模块 5.4：Run 与事件

- [ ] `S5-M4-001` 实现 Run 状态机和合法转换校验。
- [ ] `S5-M4-002` 实现 Workspace 准备、Profile 注册、Thread start/resume 编排。
- [ ] `S5-M4-003` 实现 Turn start、follow-up、steer、queue 和 interrupt。
- [ ] `S5-M4-004` 归一化 Agent、计划、工具、命令、文件和错误事件。
- [ ] `S5-M4-005` 分配稳定事件序号并写入展示缓存和审计索引。
- [ ] `S5-M4-006` 实现 completed、failed、interrupted、cancelled 终态摘要。
- [ ] `S5-M4-007` 实现基于原 Thread 的 Continue 和新 Run。
- [ ] `S5-M4-008` 增加未知状态、重复终态、app-server 崩溃和网络中断测试。

### 模块 5.5：审批、输入与 Artifact

- [ ] `S5-M5-001` 持久化命令、文件、权限和用户输入请求。
- [ ] `S5-M5-002` 实现 Approval 与原始请求 ID、Profile Session、Run 的绑定。
- [ ] `S5-M5-003` 实现同意、拒绝、过期、取消和首个合法决策胜出。
- [ ] `S5-M5-004` 实现 Runner/Profile 失联时冻结审批。
- [ ] `S5-M5-005` 实现结构化用户输入投递和失败转 interrupted。
- [ ] `S5-M5-006` 实现大输出截断、Artifact 化和授权下载。
- [ ] `S5-M5-007` 增加并发审批、过期响应、无效请求 ID 和大输出测试。

### 阶段 5 门禁

- [ ] `G5-001` 20 个并发测试 Run 的 Worktree、容器、事件和权限无交叉。
- [ ] `G5-002` queued、running、waiting、completed、failed、interrupted、cancelled 全状态通过。
- [ ] `G5-003` 取消目标 Run 不影响同 Profile 其他 Run。
- [ ] `G5-004` Runner/Host/Server 故障不会产生永久 running 或伪 completed。
- [ ] `G5-005` Secret 不进入事件、日志、Artifact 元数据或浏览器响应。

## 阶段 6：Web 核心任务闭环

**依赖：** 阶段 5

**关联 PRD：** NAV、DSH、PRJ、TSK、CMP、DIF、UX、RSP、A11Y 基础需求

### 模块 6.1：前端平台层

- [ ] `S6-M1-001` 创建 `PlatformClient`、HTTP Client 和统一错误类型。
- [ ] `S6-M1-002` 创建 WebSocket Client、游标、重连和订阅 Store。
- [ ] `S6-M1-003` 从 OpenAPI/Schema 生成或校验前端类型。
- [ ] `S6-M1-004` 建立 Query Cache 与实时事件合并规则。
- [ ] `S6-M1-005` 将业务状态从 localStorage 迁移到 Server。
- [ ] `S6-M1-006` localStorage 仅保留主题、面板尺寸和用户私有草稿。
- [ ] `S6-M1-007` 建立权限与 Capability Selector，禁止按钮各自拼条件。
- [ ] `S6-M1-008` 建立 App Shell、路由、面包屑、用户菜单和连接状态。
- [ ] `S6-M1-009` 实现全局 Loading、Offline、Reconnecting、Stale 和 Fatal Error。

### 模块 6.2：登录与初始化

- [ ] `S6-M2-001` 实现登录、退出、邀请接受和过期邀请页面。
- [ ] `S6-M2-002` 实现会话恢复、失效跳转和安全目标 URL。
- [ ] `S6-M2-003` 实现组织初始化分步向导。
- [ ] `S6-M2-004` 实现个人 Codex Profile 创建和身份连接步骤。
- [ ] `S6-M2-005` 实现 Capability 检查和 incompatible 阻断步骤。
- [ ] `S6-M2-006` 实现 Git Credential 和首个 Repository 验证步骤。
- [ ] `S6-M2-007` 实现向导草稿、返回修改和最终激活。

### 模块 6.3：工作台与项目

- [ ] `S6-M3-001` 实现工作台“需要我处理”队列。
- [ ] `S6-M3-002` 实现我的运行中、最近 Task 和最近项目。
- [ ] `S6-M3-003` 实现工作台无项目、无 Task、无权限和离线状态。
- [ ] `S6-M3-004` 实现项目列表、搜索、筛选、排序和分页。
- [ ] `S6-M3-005` 实现项目创建、Repository 测试和异步准备状态。
- [ ] `S6-M3-006` 实现项目概览、健康状态、活跃 Task 和成员摘要。
- [ ] `S6-M3-007` 实现项目 Task 表格与返回上下文恢复。
- [ ] `S6-M3-008` 实现项目设置导航和分域保存骨架。
- [ ] `S6-M3-009` 实现全局命令面板和项目/Task 搜索。

### 模块 6.4：创建 Task

- [ ] `S6-M4-001` 实现 Task 目标、基线分支、附件和创建提交。
- [ ] `S6-M4-002` 实现 Profile、Native Agent、模型和推理等级选择。
- [ ] `S6-M4-003` 依据 Capability 禁用不兼容选项并显示原因。
- [ ] `S6-M4-004` 实现附件上传进度、类型/大小校验和失败重试。
- [ ] `S6-M4-005` 实现高级执行、审批和环境设置。
- [ ] `S6-M4-006` 实现表单草稿、字段错误和服务端失败保留输入。
- [ ] `S6-M4-007` 实现幂等创建并直接进入 queued Task。
- [ ] `S6-M4-008` 增加重复提交、分支消失、附件失败和权限变化 E2E。

### 模块 6.5：Task 工作区

- [ ] `S6-M5-001` 实现稳定的 Task Header、状态、分支、租约和主操作。
- [ ] `S6-M5-002` 实现 Task 导航、状态分组、搜索和 Run 历史。
- [ ] `S6-M5-003` 实现活动流的消息、计划、工具、命令、审批和系统事件。
- [ ] `S6-M5-004` 实现开放时间线、虚拟化、折叠和长输出 Artifact 链接。
- [ ] `S6-M5-005` 实现阅读锚点、自动跟随和“有新活动”操作。
- [ ] `S6-M5-006` 实现 Composer、多行输入、附件、Queue、Steer 和 Stop。
- [ ] `S6-M5-007` 实现 waiting_input 结构化输入和 waiting_approval 卡片。
- [ ] `S6-M5-008` 实现 Inspector 的 Changes、Files、Logs、Run Details Tabs。
- [ ] `S6-M5-009` 实现 completed、failed、interrupted 终态摘要与 Continue。
- [ ] `S6-M5-010` 实现刷新、断网、Host 重启后的事件与 Thread 恢复界面。
- [ ] `S6-M5-011` 增加 10,000 事件、长标题、长路径、大日志和快速状态变化测试。

### 模块 6.6：响应式与可访问性

- [ ] `S6-M6-001` 实现宽屏三栏、窄屏双栏和手机单栏。
- [ ] `S6-M6-002` 实现可调整且有最小/最大约束的 Task 面板。
- [ ] `S6-M6-003` 实现手机 Activity、Changes、Approvals、Details 底部 Tabs。
- [ ] `S6-M6-004` 处理虚拟键盘、安全区域、横竖屏和触控目标。
- [ ] `S6-M6-005` 实现键盘完成登录、创建 Task、发送、审批和 Continue。
- [ ] `S6-M6-006` 实现 Drawer、Dialog、Tab 和新事件焦点管理。
- [ ] `S6-M6-007` 为图标按钮、状态、表格和实时区域增加可访问名称。
- [ ] `S6-M6-008` 验证对比度、非颜色表达和 `prefers-reduced-motion`。
- [ ] `S6-M6-009` 为每页实现 Loading、Empty、No Results、Error、Forbidden、Offline。
- [ ] `S6-M6-010` 执行桌面、平板、手机浏览器截图与交互回归。

### 阶段 6 门禁

- [ ] `G6-001` 普通浏览器无需 Tauri、本地 CLI 或共享 Token 完成首个真实 Task。
- [ ] `G6-002` 刷新、标签切换和短时断网不丢失服务端状态。
- [ ] `G6-003` 创建、发送、输入、停止和继续可由键盘完成。
- [ ] `G6-004` 所有核心页面通过状态矩阵和三类视口验收。
- [ ] `G6-005` 前端业务组件不直接调用 Tauri Transport。

## 阶段 7：多用户协作与审批

**依赖：** 阶段 6

**关联 PRD：** ORG、COL、APR、APV、NOT、AUD 协作与审批需求

### 模块 7.1：成员与权限

- [ ] `S7-M1-001` 实现组织成员列表、邀请、角色修改、禁用和移除。
- [ ] `S7-M1-002` 实现最后一个 Owner 保护。
- [ ] `S7-M1-003` 实现项目成员和组织/项目权限来源展示。
- [ ] `S7-M1-004` 实现 Task 成员可见范围。
- [ ] `S7-M1-005` 实现角色变化后 API 和 WebSocket 权限立即生效。
- [ ] `S7-M1-006` 实现 Platform Admin 与业务内容权限分离。
- [ ] `S7-M1-007` 增加跨资源 ID、历史 URL、缓存和实时订阅越权测试。

### 模块 7.2：Control Lease

- [ ] `S7-M2-001` 实现获取、续期、释放和过期。
- [ ] `S7-M2-002` 实现请求控制和当前持有者移交。
- [ ] `S7-M2-003` 实现离线宽限期和授权用户接管。
- [ ] `S7-M2-004` 实现管理员强制回收和理由审计。
- [ ] `S7-M2-005` 服务端拒绝非持有者控制消息。
- [ ] `S7-M2-006` 租约转移时隔离用户私有草稿。
- [ ] `S7-M2-007` 增加两个会话并发获取、续期和接管测试。

### 模块 7.3：审批中心

- [ ] `S7-M3-001` 实现待处理、已决、过期审批列表。
- [ ] `S7-M3-002` 实现项目、风险、类型、状态和时间筛选。
- [ ] `S7-M3-003` 实现命令、目录、文件范围、风险和关联 Diff 详情。
- [ ] `S7-M3-004` 实现普通批准、拒绝和允许范围批准。
- [ ] `S7-M3-005` 实现高风险二次确认和禁止批量同意。
- [ ] `S7-M3-006` 实现审批结果同步到 Task 和通知。
- [ ] `S7-M3-007` 实现过期、Runner 失联和请求已决按钮状态。
- [ ] `S7-M3-008` 实现返回 Task 的稳定深链接和上下文恢复。
- [ ] `S7-M3-009` 增加并发决策、越权、过期和投递失败 E2E。

### 模块 7.4：评论、通知与审计

- [ ] `S7-M4-001` 实现 Task 评论、编辑/删除边界和成员提及。
- [ ] `S7-M4-002` 保证评论不写入 Codex Thread 历史。
- [ ] `S7-M4-003` 实现在线观察者和租约变化同步。
- [ ] `S7-M4-004` 实现站内通知、未读计数、已读同步和深链接。
- [ ] `S7-M4-005` 实现登录、成员、权限、租约、审批、执行和 Secret 审计。
- [ ] `S7-M4-006` 实现审计资源、操作者、时间、结果和 Request ID 查询。
- [ ] `S7-M4-007` 对审计字段执行脱敏和不可变性测试。

### 阶段 7 门禁

- [ ] `G7-001` 同一 Task 同时只有一个有效控制者。
- [ ] `G7-002` 禁用或降权用户的现有连接立即失去对应能力。
- [ ] `G7-003` 重复、并发和过期审批得到唯一一致结果。
- [ ] `G7-004` 评论、通知、Agent 消息和审计对象边界清晰。
- [ ] `G7-005` 两个浏览器会话的审批、接管和权限变化 E2E 通过。

## 阶段 8：Codex Studio

**依赖：** 阶段 7、`G1-003`、`G1-004`、`G1-005`、`G1-006`

**关联 PRD：** PRO、AGT、SKL、PLG、MCP、MEM 全部需求

### 模块 8.1：Profiles

- [ ] `S8-M1-001` 实现 Profile 列表、当前 Profile 和状态筛选。
- [ ] `S8-M1-002` 实现身份、Codex 版本、Capability、进程和错误概览。
- [ ] `S8-M1-003` 实现已注册 Workspace、Thread 和活动 Turn 列表。
- [ ] `S8-M1-004` 实现健康检查和安全重启。
- [ ] `S8-M1-005` 实现 Memory/compaction/consolidation 状态与诊断。
- [ ] `S8-M1-006` 实现导出、退出身份、Memory 重置、删除的独立 Danger Zone。
- [ ] `S8-M1-007` 为活动 Turn、失败操作和 incompatible 状态提供恢复入口。
- [ ] `S8-M1-008` 增加跨 Profile 切换时缓存清空和越权测试。

### 模块 8.2：Native Agents

**依赖：** `CR-201` 至 `CR-207`

- [ ] `S8-M2-001` 实现 Agent 列表、搜索、作用域和可用状态。
- [ ] `S8-M2-002` 实现 Agent 创建、读取、编辑和删除/停用。
- [ ] `S8-M2-003` 直接映射 Codex 原生字段，不创建平台私有模板格式。
- [ ] `S8-M2-004` 实现保存前配置 Diff 和原生校验错误。
- [ ] `S8-M2-005` 实现模型、说明和工具权限配置。
- [ ] `S8-M2-006` 实现 multi-agent 开关、最大线程和最大深度配置。
- [ ] `S8-M2-007` 实现 reload 结果和新 Turn 生效验证。
- [ ] `S8-M2-008` 实现 Run 使用的 Agent 配置快照查看。

### 模块 8.3：Multi-Agent 轨迹

- [ ] `S8-M3-001` 解析父子 Thread 和协作事件关系。
- [ ] `S8-M3-002` 展示派生、委派、等待、返回和汇总状态。
- [ ] `S8-M3-003` 展示子 Agent 名称、任务、状态、耗时和产出入口。
- [ ] `S8-M3-004` 展示子 Agent 失败、取消、超限和部分完成。
- [ ] `S8-M3-005` 刷新和重连后从 Codex 事件恢复关系图。
- [ ] `S8-M3-006` 禁止浏览器根据时间或文本猜测调度关系。
- [ ] `S8-M3-007` 实现主 Thread 与子 Thread 活动流切换。
- [ ] `S8-M3-008` 增加最大深度、最大线程、子 Agent 失败和父 Turn 中断 E2E。

### 模块 8.4：Skills Studio

**依赖：** `CR-301` 至 `CR-308`

- [ ] `S8-M4-001` 实现 Personal、Project、Plugin Provided Skill 分组。
- [ ] `S8-M4-002` 实现作用域、来源、版本、状态和覆盖关系列表。
- [ ] `S8-M4-003` 实现 Skill 创建和 Codex 辅助生成入口。
- [ ] `S8-M4-004` 实现 `SKILL.md`、scripts、references、assets 文件树。
- [ ] `S8-M4-005` 实现文本编辑、未保存状态和文件级错误。
- [ ] `S8-M4-006` 调用原生 Skill 校验并定位结构、路径和内容错误。
- [ ] `S8-M4-007` 创建隔离测试 Task 并展示发现、触发、调用和结果。
- [ ] `S8-M4-008` 实现 Personal Skill 发布、reload 和新 Turn 生效验证。
- [ ] `S8-M4-009` 实现 Project Skill 写入 `.agents/skills` 并生成 Git Diff。
- [ ] `S8-M4-010` 实现版本、停用、回滚和历史运行引用。
- [ ] `S8-M4-011` 实现路径穿越、符号链接、大小、类型和脚本安全检查。
- [ ] `S8-M4-012` 增加校验失败、测试失败、Git 冲突、reload 失败和回滚 E2E。

### 模块 8.5：Plugin Manager

**依赖：** `CR-401` 至 `CR-407`

- [ ] `S8-M5-001` 实现已安装和可安装 Plugin 列表。
- [ ] `S8-M5-002` 实现 Manifest、来源、版本、完整性和兼容详情。
- [ ] `S8-M5-003` 实现提供的 Skills、MCP、Apps 和能力清单。
- [ ] `S8-M5-004` 实现安装前权限确认。
- [ ] `S8-M5-005` 实现安装 pending、成功、失败和重试。
- [ ] `S8-M5-006` 实现升级及权限差异逐项确认。
- [ ] `S8-M5-007` 实现 enable、disable、uninstall 和影响提示。
- [ ] `S8-M5-008` 只有原生状态复查成功后更新最终 UI 状态。
- [ ] `S8-M5-009` 实现来源策略、管理员权限和操作审计。
- [ ] `S8-M5-010` 增加部分安装、依赖冲突、权限升级和卸载失败 E2E。

### 模块 8.6：MCP 与 Tools

**依赖：** `CR-501` 至 `CR-508`

- [ ] `S8-M6-001` 实现 MCP Server 列表、状态、来源和工具数量。
- [ ] `S8-M6-002` 实现 Server 创建、编辑、停用和删除。
- [ ] `S8-M6-003` 使用服务端 Secret 引用，禁止明文回填。
- [ ] `S8-M6-004` 实现 reload 和 DNS、进程、协议、认证、发现分步测试。
- [ ] `S8-M6-005` 实现 OAuth start、callback、完成、取消和失败页面。
- [ ] `S8-M6-006` 绑定 OAuth state、Session、Profile 并防重放。
- [ ] `S8-M6-007` 将 elicitation 映射为持久结构化用户输入。
- [ ] `S8-M6-008` 实现工具列表、Schema、权限和项目可用范围。
- [ ] `S8-M6-009` 实现工具最近调用的非敏感元数据。
- [ ] `S8-M6-010` 实现活动 Turn 下停用/删除的阻断或中断反馈。
- [ ] `S8-M6-011` 增加 OAuth 重放、elicitation 并发、Secret 泄漏和连接失败 E2E。

### 阶段 8 门禁

- [ ] `G8-001` Profiles、Agents、Skills、Plugins、MCP 全部由 Capability 驱动。
- [ ] `G8-002` 缺失 bridge 只显示 unsupported，不存在 Web 替代 Runtime。
- [ ] `G8-003` 一个真实 multi-agent Task 的完整父子轨迹可恢复。
- [ ] `G8-004` Personal 和 Project Skill 完成创建、校验、测试、发布和回滚。
- [ ] `G8-005` Plugin 完整生命周期和 MCP OAuth/elicitation 使用真实 Rust 构建通过。
- [ ] `G8-006` Memory 重置、Plugin 升级和 MCP 授权均有权限确认与审计。

## 阶段 9：Git 交付与平台管理

**依赖：** 阶段 8

**关联 PRD：** GIT、DIF、ADM、AUD、项目设置需求

### 模块 9.1：Changes 与 Files

- [ ] `S9-M1-001` 实现变更文件树、状态、统计、搜索和筛选。
- [ ] `S9-M1-002` 实现 Unified 和 Split Diff。
- [ ] `S9-M1-003` 实现大 Diff 分段加载和虚拟化。
- [ ] `S9-M1-004` 实现新增、修改、删除、重命名和冲突状态。
- [ ] `S9-M1-005` 实现二进制和超大文件元数据视图。
- [ ] `S9-M1-006` 实现只读文件浏览和路径面包屑。
- [ ] `S9-M1-007` 实现测试结果、命令来源和 Artifact 关联。
- [ ] `S9-M1-008` 保存用户 Diff 模式、面板和选中项偏好。
- [ ] `S9-M1-009` 确保 Diff/Files 不提供未审计通用编辑器。
- [ ] `S9-M1-010` 增加大文件、二进制、删除、重命名和长路径测试。

### 模块 9.2：Commit 与 Push

- [ ] `S9-M2-001` 实现 Commit Drawer 和最终文件范围。
- [ ] `S9-M2-002` 展示测试摘要、未跟踪文件和敏感文件警告。
- [ ] `S9-M2-003` 实现 Commit Message 编辑和校验。
- [ ] `S9-M2-004` 服务端校验 Commit 权限和 Control Lease 独立性。
- [ ] `S9-M2-005` 实现 Commit 执行、结果和审计。
- [ ] `S9-M2-006` 实现 Push 独立确认和权限校验。
- [ ] `S9-M2-007` 阻止 Force Push 和保护分支违规。
- [ ] `S9-M2-008` 实现远端领先、认证失败和网络失败恢复。
- [ ] `S9-M2-009` 保证 Push 失败不丢失本地 Commit 和 Worktree。
- [ ] `S9-M2-010` 增加并发 Commit、远端冲突、凭据失效和重试测试。

### 模块 9.3：设置与管理

- [ ] `S9-M3-001` 实现项目 General、Repository、Execution 设置。
- [ ] `S9-M3-002` 实现 Approvals、Members、Retention 设置。
- [ ] `S9-M3-003` 实现项目归档、禁用、删除和凭据替换 Danger Zone。
- [ ] `S9-M3-004` 实现个人资料、通知、主题和 Session 管理。
- [ ] `S9-M3-005` 实现 Runner 列表、详情、容量、版本和最近错误。
- [ ] `S9-M3-006` 实现队列、暂停接单、排空和强制终止。
- [ ] `S9-M3-007` 实现审计筛选、详情、脱敏和受审计导出。
- [ ] `S9-M3-008` 增加最后 Owner、危险操作、Runner 失联和批量失败测试。

### 阶段 9 门禁

- [ ] `G9-001` 有权限用户可审查 Diff、Commit 和 Push。
- [ ] `G9-002` Git 状态与 UI 状态一致，失败不只使用短暂 Toast。
- [ ] `G9-003` 保护分支、禁止 Force Push 和权限边界通过测试。
- [ ] `G9-004` 所有设置和管理页面具备完整状态矩阵。
- [ ] `G9-005` 所有危险操作均有资源名确认、影响说明和审计。

## 阶段 10：可靠性、安全与兼容性

**依赖：** 阶段 9；跨版本冻结任务依赖 `CR-601` 至 `CR-607`

**关联 PRD：** CAP、非功能需求、安全边界、页面验收矩阵

### 模块 10.1：跨项目合同 CI

- [ ] `S10-M1-001` 锁定 `CR-601` 至 `CR-607` V1 Rust 构建。
- [ ] `S10-M1-002` 每次 Rust 发布运行 Manifest 和 Schema Diff。
- [ ] `S10-M1-003` 每次 Web 依赖更新运行全量 Fixture 回放。
- [ ] `S10-M1-004` 夜间运行真实 app-server 全能力 Smoke Test。
- [ ] `S10-M1-005` 测试向前升级、版本回滚和 incompatible 恢复。
- [ ] `S10-M1-006` 测试未知新增能力不影响已支持模块。
- [ ] `S10-M1-007` 测试被删除或降级能力自动关闭 UI/API。
- [ ] `S10-M1-008` 发布 Web/Rust 兼容矩阵报告。

### 模块 10.2：安全测试

- [ ] `S10-M2-001` 执行 SAST、依赖、容器、License 和 Secret 扫描。
- [ ] `S10-M2-002` 执行 CSRF、XSS、CSP、Session fixation 和限流测试。
- [ ] `S10-M2-003` 执行 SSRF、DNS Rebinding 和出网策略测试。
- [ ] `S10-M2-004` 执行路径穿越、符号链接逃逸和任意文件读取测试。
- [ ] `S10-M2-005` 执行跨组织、跨项目、跨 Profile 和 ID 枚举测试。
- [ ] `S10-M2-006` 执行 Profile Home、Worktree、Artifact 和容器隔离测试。
- [ ] `S10-M2-007` 执行恶意 Skill 文件和脚本边界测试。
- [ ] `S10-M2-008` 执行 Plugin 来源、完整性和权限升级测试。
- [ ] `S10-M2-009` 执行 MCP OAuth 重放、callback 劫持和 Secret 泄漏测试。
- [ ] `S10-M2-010` 执行审批绕过、请求 ID 重用和失联投递测试。
- [ ] `S10-M2-011` 验证日志、Trace、分析和导出不含敏感正文。
- [ ] `S10-M2-012` 关闭全部 P0/P1 安全问题。

### 模块 10.3：故障恢复

- [ ] `S10-M3-001` 注入 Browser/WebSocket 断网和恢复。
- [ ] `S10-M3-002` 注入 Web Server 重启和后台 Job 重领。
- [ ] `S10-M3-003` 注入 Codex Host 重启和 Profile Session 重建。
- [ ] `S10-M3-004` 注入 app-server 崩溃和有界重启失败。
- [ ] `S10-M3-005` 注入 Runner 失联、租约过期和恢复。
- [ ] `S10-M3-006` 注入 PostgreSQL 短时中断和事务回滚。
- [ ] `S10-M3-007` 注入 Workspace 半创建、半注册和清理失败。
- [ ] `S10-M3-008` 注入 Memory 导出/重置、Plugin 安装和 MCP OAuth 中途失败。
- [ ] `S10-M3-009` 执行数据库备份恢复和 Profile Home 保护演练。
- [ ] `S10-M3-010` 验证未知状态不被标记 completed，所有 Run 收敛到明确状态。

### 模块 10.4：性能与容量

- [ ] `S10-M4-001` 测试 100 个在线 Web Session。
- [ ] `S10-M4-002` 测试 20 个并发 Run 和组织/项目/Profile 限流。
- [ ] `S10-M4-003` 测试同 Profile 多 Thread 的原生并发上限。
- [ ] `S10-M4-004` 测试 10,000 事件补发和快照恢复。
- [ ] `S10-M4-005` 测试超大日志、大 Diff 和大量 Task 列表。
- [ ] `S10-M4-006` 测试 app-server 长时间运行、内存增长和进程泄漏。
- [ ] `S10-M4-007` 测试 Workspace/Artifact 磁盘水位和清理吞吐。
- [ ] `S10-M4-008` 输出 API、事件、队列、Run 和 UI 性能报告。

### 模块 10.5：可观测性

- [ ] `S10-M5-001` 统一 request、organization、profile、project、task、thread、run、runner 关联字段。
- [ ] `S10-M5-002` 增加 API、WebSocket、队列和数据库指标。
- [ ] `S10-M5-003` 增加 Profile 进程、重启、恢复和 Capability 指标。
- [ ] `S10-M5-004` 增加 Run、审批、multi-agent 子 Thread 和清理指标。
- [ ] `S10-M5-005` 增加 Skill、Plugin、MCP 操作结果和错误分类指标。
- [ ] `S10-M5-006` 配置 Runner/Host 失联、Run 无事件、清理失败和磁盘告警。
- [ ] `S10-M5-007` 配置不兼容版本、Profile 重启风暴和跨租户拒绝异常告警。
- [ ] `S10-M5-008` 编写每个核心告警的排查与恢复 Runbook。

### 模块 10.6：UI 质量

- [ ] `S10-M6-001` 对 PRD 页面矩阵执行桌面、平板和手机截图回归。
- [ ] `S10-M6-002` 检查字体、图标、颜色、容器、间距和视觉层级。
- [ ] `S10-M6-003` 检查长文本、路径、状态和按钮不溢出或遮挡。
- [ ] `S10-M6-004` 检查面板调整、异步状态和 Hover 不引发布局位移。
- [ ] `S10-M6-005` 执行 Chrome、Edge、Safari 目标版本兼容测试。
- [ ] `S10-M6-006` 执行键盘、焦点、Screen Reader、对比度和减少动态测试。
- [ ] `S10-M6-007` 修复全部 P0/P1 视觉、响应式和可访问性问题。
- [ ] `S10-M6-008` 保存最终浏览器截图和核心交互证据。

### 阶段 10 门禁

- [ ] `G10-001` 锁定 Rust 构建通过全量合同、真实 Smoke、升级和回滚测试。
- [ ] `G10-002` 高危和中高危安全问题为零。
- [ ] `G10-003` Profile、Thread、Memory、Workspace 和数据库恢复演练通过。
- [ ] `G10-004` 达到 PRD 性能与容量目标。
- [ ] `G10-005` 所有 P0/P1 UI、响应式和可访问性问题关闭。
- [ ] `G10-006` 不存在 Agent/Memory/Skill/Plugin/MCP fallback Runtime。

## 阶段 11：删除 Tauri 与 GA

**依赖：** 阶段 10、`G1-007`

**关联 PRD：** V1 全部 P0 需求、WF-01 至 WF-16、总体验收标准

### 模块 11.1：纯 Web 功能门禁

- [ ] `S11-M1-001` 浏览器完成登录、Profile 创建和身份连接。
- [ ] `S11-M1-002` 浏览器完成项目、Task、原生 multi-agent 和 Thread 恢复。
- [ ] `S11-M1-003` 浏览器完成审批、结构化输入、Control Lease 和协作。
- [ ] `S11-M1-004` 浏览器完成 Agent、Skill、Plugin、MCP 和 Memory 治理。
- [ ] `S11-M1-005` 浏览器完成 Diff、Commit、Push、Runner 和审计管理。
- [ ] `S11-M1-006` 全流程不使用本地 CLI、桌面桥接、扩展或共享 Token。
- [ ] `S11-M1-007` Beta 使用 Web 稳定运行一个发布周期。
- [ ] `S11-M1-008` 所有 V1 Tauri 功能均标记为 Web 已替代或明确删除。

### 模块 11.2：Tauri 删除

- [ ] `S11-M2-001` 将 Web 设置为唯一应用入口。
- [ ] `S11-M2-002` 删除 `/web` 和 Transport 双运行条件分支。
- [ ] `S11-M2-003` 删除 `src/services/tauri.ts` 和 Tauri Event Adapter。
- [ ] `S11-M2-004` 删除窗口、托盘、更新器、dictation 和原生通知代码。
- [ ] `S11-M2-005` 删除 Finder/Explorer、外部编辑器和本地路径功能。
- [ ] `S11-M2-006` 删除 `src-tauri`，保留已抽离的独立 crates。
- [ ] `S11-M2-007` 删除 `@tauri-apps/*`、Tauri CLI 和桌面 npm scripts。
- [ ] `S11-M2-008` 删除桌面图标、安装包、签名、更新和发布配置。
- [ ] `S11-M2-009` 删除 iOS/移动生成文件和不再支持的代码。
- [ ] `S11-M2-010` 清理死代码、未使用样式、桌面文案和 Feature Flag。
- [ ] `S11-M2-011` 增加禁止重新引入 Tauri 包、crate、invoke 和 event 的 CI 检查。
- [ ] `S11-M2-012` 更新 README、AGENTS、架构图、开发命令和部署文档。

### 模块 11.3：生产发布

- [ ] `S11-M3-001` 创建生产 Web/Server/Runner Dockerfile。
- [ ] `S11-M3-002` 创建 PostgreSQL、Server、Codex Host、Runner 的 Compose 配置。
- [ ] `S11-M3-003` 创建反向代理、TLS、WebSocket 和上传限制配置。
- [ ] `S11-M3-004` 固定 Web 与 Codex Rust 版本、镜像摘要、SBOM 和签名。
- [ ] `S11-M3-005` 创建初始化、配置、Secret 轮换和备份恢复文档。
- [ ] `S11-M3-006` 创建数据库、Profile Home、Rust 构建升级和回滚 Runbook。
- [ ] `S11-M3-007` 创建管理员、普通用户和故障排查手册。
- [ ] `S11-M3-008` 从空白 Linux 主机执行部署和全流程 Smoke Test。
- [ ] `S11-M3-009` 执行生产灰度、监控确认和回滚演练。
- [ ] `S11-M3-010` 发布版本说明、已知限制和兼容矩阵。

### 阶段 11 门禁

- [ ] `G11-001` `rg "tauri|src-tauri|@tauri-apps"` 只命中历史迁移说明或零命中。
- [ ] `G11-002` Node 构建不安装 Tauri 包，Rust 构建不依赖 Tauri crate。
- [ ] `G11-003` 生产流程不生成桌面安装包或桌面更新产物。
- [ ] `G11-004` 空白 Linux 主机可部署完整平台并通过 V1 全流程。
- [ ] `G11-005` 用户只使用浏览器完成 PRD WF-01 至 WF-16。
- [ ] `G11-006` PRD 第 27 节全部验收标准通过后发布 GA。
