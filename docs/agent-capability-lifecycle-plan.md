# Copilot 开发平台实施计划

| 字段 | 内容 |
| --- | --- |
| 状态 | 当前可执行工作计划 |
| 更新日期 | 2026-08-08 |
| 目标架构 | [Copilot 开发平台架构](supervisor-agent-skill-tool-architecture.md) |
| 当前事实 | [能力基线](capability-baseline.md) 与代码 |
| 参考案例 | 印尼仓网规划 Copilot |
| 兼容策略 | 不兼容项目历史版本；迁移所有当前调用方后删除旧路径 |

本文把目标架构拆成可直接实施的工作包。每项均写明 owner、合同、数据、具体改动、
删除项、测试和退出条件。工作包按依赖顺序执行；没有通过退出条件不得开始依赖它的
发布工作。

## 1. 完成范围与优先级

### P0：不完成就无法形成正确平台闭环

| ID | 问题 | 根因 | 对应工作包 |
| --- | --- | --- | --- |
| P0-1 | 当前服务无法启动，真实 E2E 未完成 | 已应用开发迁移被改写，验证环境失去可重复基线 | W0 |
| P0-2 | Platform Data Intake 与供应链 Case 双写来源和映射 | 通用数据生命周期被领域包重新实现 | W3 |
| P0-3 | Platform event projection 识别仓网 envelope | 缺少通用 Tool SDK 和结果合同 | W2 |
| P0-4 | 当前阶段目标与路线图仍把 Studio 放在后续 | 目标变化没有同步权威文档和门禁 | W0、文档收敛 |

### P1：不完成就会持续增加新 Case 成本和失败率

| ID | 问题 | 根因 | 对应工作包 |
| --- | --- | --- | --- |
| P1-1 | case revision/operation/readiness 每个领域重复实现 | 缺少平台 Work State 机制 | W1、W2 |
| P1-2 | Root 依赖子 Agent 自述进度 | 缺少可信只读 coordination projection | W2 |
| P1-3 | assignment 只靠自然语言 | 缺少 CollaborationContext 和 AssignmentContract | W4 |
| P1-4 | 版本、hash、Runtime Role 和依赖需手工维护 | 没有单一 Package Compiler | W5 |
| P1-5 | Tool/Skill/Agent/Supervisor 生命周期割裂 | 按页面和案例逐项补功能，缺少统一 Release/Installation 模型 | W5-W8 |
| P1-6 | 上下文缓存命中与 Token 开销不可诊断 | Provider 调用没有统一、逐调用、有界观测 | W9 |

### P2：不清理会形成长期维护债务

| ID | 问题 | 根因 | 对应工作包 |
| --- | --- | --- | --- |
| P2-1 | 供应链 server 保留大量未注册旧函数 | 迭代只停止暴露，没有删除旧实现 | W10 |
| P2-2 | 导航矩阵缺完整注册闭环 | 只完成费用确认，没有结果所有权闭环 | W10 |
| P2-3 | 文档保留 2.x/5.x、`planning-dataset.v2` | 当前事实没有随合同替换同步 | W0、W10 |
| P2-4 | 仅有仓网案例，无法证明通用性 | 公共能力未被第二领域验证 | W12 |
| P2-5 | 多用户边界只有设计，没有隔离矩阵 | 当前阶段单用户，但作用域仍可能退化为 singleton | W13 |

## 2. 固定实施规则

1. 新平台 DTO 写入 `apps/web/crates/platform-contracts`，浏览器不得接触 raw app-server。
2. 新持久服务优先建立独立 crate；route 只鉴权、解析 DTO 和调用 service，不承载状态机。
3. Runtime discovery 是运行可用性的 owner；数据库 Catalog 不能宣称 Runtime ready。
4. 所有新异步操作都有 UUID、幂等键和完整终态。
5. 所有内容上限放入类型化 `PlatformLimits` 配置或包 schema，不在各 route 散落魔法数字。
6. 不增加历史双读、旧字段推断、自动数据库修复或 Mock fallback。
7. 本计划默认不修改 `codex/codex-rs/**`。W8 验证现有 seam 失败后，才允许启动 Patch
   Map 评审；评审本身也不等于获准修改。
8. 每个提交只包含一个工作包的一个可验证边界；提交前运行 `git diff --check`。

## 3. W0：恢复可重复开发基线并删除目标冲突

### Owner 和依赖

- Owner：Platform Operations + Documentation。
- 前置：无。
- 阻塞：W1-W13 的真实数据库和 E2E 验证。

### W0.1 恢复数据库

1. 用 PostgreSQL 18 客户端只导出当前 Provider Definition 和加密 Secret 关联记录；
2. 校验导出文件不包含 secret 明文，记录行数和 SHA-256；
3. 停止服务，显式删除并重建开发数据库；
4. 从当前 migration 目录一次性初始化，不在 startup 中添加修复分支；
5. 恢复 Provider/Secret 记录并验证 Profile process 能读取；
6. 连续执行两次启动，证明 migration checksum 稳定；
7. 删除失败备份临时文件，不在仓库或日志保存 key。

需要补充脚本：

```text
scripts/rebuild-development-database.sh
  --preserve-provider-config
  --postgres-bin <PG18 bin>
  --confirm-development-only
```

脚本必须先导出并验证，再执行破坏操作；任何一步失败立即停止。它是显式运维命令，
不得被 `run-local.sh` 自动调用。

### W0.2 冻结当前 migration 纪律

- 当前开发库重建后，已应用 migration 永不修改；
- 新 schema 使用新的 migration，不再重写 `20260807000045`；
- 增加 CI：对主分支 migration filename + SHA 清单做差异检查；
- 文档化“研究阶段允许重建，不允许启动时猜测修复”。

落实已接受的运维决定：

```text
scripts/check-migration-integrity.sh
docs/adr/012-development-schema-rebuild-policy.md
```

### W0.3 收敛当前文档

- `product-vision.md`：第二阶段改为单 Profile Copilot Authoring Platform；
- `roadmap.md`：Studio 前移到 M2，多用户仍在 M3；
- `product-design.md`：删除 `planning-dataset.v2` 和“完整 Studio 后置”的冲突；
- `development-plan.md`：当前优先级指向本计划；
- `architecture.md`、`capability-baseline.md`：只保留已验证事实，不提前声称新架构已实现；
- 删除 current-state 文档里的 2.x/5.x 现状描述。

### 验证和退出

```bash
./scripts/test-web-rust.sh
cd apps/web && npm run typecheck && npm run test
./scripts/check-migration-integrity.sh
./scripts/run-local.sh --no-build
```

退出条件：空库迁移、Provider 恢复、两次重启和 `/health` 通过；当前权威文档对阶段目标
没有矛盾；真实 E2E 环境可用。

## 4. W1：定义公共合同和 ADR

### Owner 和依赖

- Owner：`platform-contracts` + Architecture。
- 前置：W0 文档收敛；数据库实现可与 W0 恢复并行编码，但不能验证退出。
- 不修改：Codex 生成协议。

### W1.1 新增平台资源 DTO

在 `apps/web/crates/platform-contracts/src/` 按领域拆文件，`lib.rs` 只 re-export：

```text
catalog.rs
work_state.rs
collaboration.rs
tool_results.rs
installations.rs
observability.rs
```

核心类型：

```rust
struct DraftMetadata { revision: i64, content_sha256: String, validation: ValidationState }
struct ReleaseIdentity { id: Uuid, resource_id: String, version: String, content_sha256: String }
enum CatalogResourceKind { Tool, Skill, Agent, Supervisor, Copilot }
enum InstallationState { Pending, Installing, Installed, Discovered, Ready, Degraded, Unavailable, Failed }
enum OperationStatus { Pending, Running, WaitingInput, Completed, Failed, Cancelled, Timeout, Interrupted }
struct ResourceRequirement { kind, resource_id, version_constraint, capabilities }
struct ResolvedResourceRequirement { release_id, version, content_sha256 }
```

每类 Draft 使用专属内容结构，不用一个无界 JSON 代替类型：

```rust
ToolPackageDraft
SkillPackageDraft
AgentDefinitionDraft
SupervisorDefinitionDraft
CopilotPackageDraft
```

公共字段组合为嵌套类型，避免复制版本/hash/owner 字段。所有 browser DTO 使用 opaque ID；
内部安装 DTO 可以包含 Runtime resource handle，但不可序列化给浏览器。

### W1.2 Work State 合同

```rust
struct WorkStateDefinition {
    definition_id: String,
    component_types: Vec<WorkComponentType>,
    dependencies: Vec<WorkDependencyRule>,
    deliverable_types: Vec<DeliverableContract>,
}

struct WorkStateSummary { id, revision, status, components, blockers, next_actions }
struct WorkComponentSummary { id, component_type, revision, state, safe_summary, content_sha256 }
struct WorkStateMutation { expected_revision, operation_id, changes, invalidations, deliverables }
struct WorkOperationSummary { id, operation_type, status, safe_error, started_at, finished_at }
```

`WorkStateMutation` 只能引用由当前 capability grant 允许的 component types。payload 使用
`ContentReference`：`DatasetRelease`、`Artifact` 或 `DomainResource` 三种明确 variant。

### W1.3 Collaboration 和 Tool result 合同

新增：

```rust
struct CollaborationContext
struct AssignmentContract
struct PlatformToolResult
struct ToolResultPage
struct ToolDiagnostic
struct BlockingInputSummary
struct DeliverableReference
struct ProviderCallMetric
```

`PlatformToolResult` 总序列化大小由统一 `ToolResultLimits` 验证器控制。SDK 与 Server 使用
同一 JSON Schema fixture，避免 Python/Rust/TypeScript 三套手写解释。

### W1.4 落实 ADR

实现必须逐项满足：

- ADR-012：开发数据库显式重建策略；
- ADR-013：Platform Work State 只拥有元数据，领域拥有 payload；
- ADR-014：Catalog Release、Profile Installation、Runtime Discovery 三状态分离；
- ADR-015：通用 `platform-tool-result.v1`；
- ADR-016：CollaborationContext 不可由浏览器或模型构造。

### 验证和退出

- Rust serde round-trip、未知枚举拒绝、大小边界、secret/path 不可序列化测试；
- TypeScript 由当前 Rust DTO 流程生成或精确合同测试，不手改 Runtime 生成物；
- ADR 与目标架构没有 owner 冲突。

退出条件：W2-W9 所需公共概念都有稳定类型，没有仓网名称或字段。

## 5. W2：实现 Work State、通用 Tool SDK 和 Root Coordination

### Owner 和依赖

- Owner：Platform Server。
- 前置：W1。
- 新 crate：`apps/web/crates/work-state-service`。
- 领域 Tool 通过服务 API/SDK 使用，不直接连接平台数据库。

### W2.1 数据库

新增一个 migration，建立：

```text
work_state_definitions
work_states
work_components
work_component_dependencies
work_operations
work_operation_inputs
work_operation_outputs
work_blocking_inputs
work_deliverables
work_state_events
```

每张表含 `organization_id`；具体资源继续含 profile/workspace/task/run owner。关键约束：

- `(work_state_id, revision)` 单调；
- `(work_state_id, idempotency_key)` 唯一；
- operation 终态不可回到 running；
- component `(id, revision)` 不可变；
- deliverable 只引用已授权 Artifact；
- dependency 两端属于同一 Work State；
- payload 不落入 `work_state_events`。

### W2.2 Service API

```rust
pub struct WorkStateService { db: PgPool, artifacts: ArtifactAuthorizer, datasets: DatasetAuthorizer }

impl WorkStateService {
    create_definition(actor, draft) -> WorkStateDefinition;
    create_state(actor, scope, definition_release_id, idempotency_key) -> WorkStateSummary;
    get_summary(actor, work_state_id) -> WorkStateSummary;
    begin_operation(actor, request) -> WorkOperationLease;
    apply_mutation(actor, lease, mutation) -> WorkStateSummary;
    fail_operation(actor, lease, failure) -> WorkOperationSummary;
    cancel_operation(actor, operation_id, expected_revision) -> WorkOperationSummary;
    mark_timeout(system_actor, operation_id) -> WorkOperationSummary;
    list_blocking_inputs(actor, filter, page) -> Page<BlockingInputSummary>;
    list_deliverables(actor, filter, page) -> Page<DeliverableReference>;
}
```

`begin_operation` 锁定输入 component revisions 并返回短期 lease；`apply_mutation` 在一个
事务内验证 lease、expected revision、dependency、Artifact/Dataset authorization、
写 component、失效下游、完成 operation、追加事件。失败不修改已提交 component。

### W2.3 通用 SDK

新增：

```text
sdk/python/open_web_copilot/
  tool.py
  result.py
  work_state.py
  artifacts.py
  data_intake.py
  testing.py
```

关键接口：

```python
@copilot_tool(input_model=Input, output_model=Output, side_effect="deterministic_write")
def build_matrix(ctx: ToolContext, request: Input) -> PlatformToolResult: ...

with ctx.work_state.operation("build_matrix", idempotency_key) as operation:
    operation.commit(changes=[...], artifacts=[...], summary="...")
```

SDK 自动产生 envelope、限制 summary/diagnostic/page、保留原错误 cause 的安全 code，
不允许返回本地路径、Secret 类型或超大 inline JSON。

### W2.4 Root Coordination Tool

新增平台内置只读 ToolPackage：

```text
capabilities/platform-coordination/
```

Profile Host 注入不可伪造 `CollaborationContext`。实现：

```rust
struct CoordinationQueryService {
    executions: RuntimeExecutionProjectionReader,
    approvals: ApprovalProjectionReader,
    work_states: WorkStateReader,
    artifacts: ArtifactReader,
}
```

Tool 方法仅调用 query service。所有查询强制当前 task/run scope、分页和字段白名单。
Supervisor 发布时 compiler 自动添加此只读 capability，不让用户选择写权限。

### W2.5 删除领域分支

- 删除 `apps/web/server/src/event_projection.rs` 中对
  `network-case-tool-result.v1` 的判断；
- 改为 `project_platform_tool_result(value: &Value)`，只解析通用 envelope；
- 未识别 domain payload 作为普通有界 Tool 结果，不影响 execution 状态；
- 原始 Runtime 事件继续完整持久化，主视图只投影安全摘要。

### 测试和退出

- 数据库：授权拒绝、revision 冲突、幂等重放、并发 mutation、依赖失效、所有终态；
- SDK：Python/Rust fixture 等价、16 KiB 边界、分页、secret/path 拒绝；
- Coordination：root scope、child execution、刷新恢复、越权 ID、不能写；
- Event projection：任意领域 envelope 都不需要平台分支。

退出条件：供应链之外的测试 fixture 可以创建 Work State、提交 component、由 Root 查询
状态且平台代码没有领域标识。

## 6. W3：统一 Data Intake

### Owner 和依赖

- Owner：Platform Data Intake。
- 前置：W1；Work State binding 依赖 W2。
- 重构目标：把当前 `routes/data_intake.rs` 中的状态机下沉到新 crate
  `apps/web/crates/data-intake-service`。

### W3.1 服务拆分

从 route 提取：

```rust
pub struct DataIntakeService {
    db: PgPool,
    source_store: SourceAssetStore,
    profile_analyzer: DataProfileAdapter,
}

impl DataIntakeService {
    create_draft(actor, workspace_id, idempotency_key) -> WorkspaceDataDraftSummary;
    add_source_revision(actor, draft_id, upload) -> SourceAssetSummary;
    create_session(actor, task_id, requirement) -> DataIntakeSessionSummary;
    analyze_sources(actor, intake_id, expected_revision) -> DataIntakeSessionSummary;
    propose_mapping(actor, intake_id, requirement_release_id) -> MappingProposal;
    confirm_mapping(actor, intake_id, expected_revision, selection) -> MappingRevision;
    answer_parameter(actor, intake_id, expected_revision, answer) -> DataIntakeSessionSummary;
    publish_dataset(actor, intake_id, expected_revision) -> DatasetReleaseSummary;
    bind_to_work_state(actor, release_id, work_state_id, component_type) -> WorkStateSummary;
}
```

Route 保留 multipart/HTTP 状态映射；所有状态转换、授权和事务进入 service。

### W3.2 DataRequirementContract

把当前 requirement JSON 收敛为发布资源：

```rust
struct DataRequirementContract {
    entities: Vec<EntityRequirement>,
    relations: Vec<RelationRequirement>,
    parameters: Vec<ParameterRequirement>,
    accepted_formats: BTreeSet<SourceFormat>,
    validator_capability: CapabilityRequirement,
}
```

Network Agent 发布“当前问题需要什么”，Data Agent/Intake 执行检查。合同允许按分析类型
选择 required entity，不再有一个全局完整 Dataset 前置。

### W3.3 领域适配器

Tool SDK 暴露：

```python
class DomainDataAdapter(Protocol):
    def requirement_contract(self, request: AnalysisIntent) -> DataRequirementContract: ...
    def profile_fields(self, profile: SourceProfile) -> MappingProposal: ...
    def normalize(self, release: DatasetReleaseHandle, mapping: MappingRevisionHandle) -> DomainPayloadRef: ...
    def validate(self, payload: DomainPayloadRef, requested_analysis: str) -> DomainReadiness: ...
```

适配器读取由 Platform 授权的 bounded row stream 或 staging handle，不扫描 Workspace。

### W3.4 删除供应链重复 owner

删除：

- `network_cases` SQLite 中的 `case_sources`；
- case mapping proposal/candidate/selection 表和 repository 方法；
- `discover_workspace_sources`、`inspect_workspace_sources` 作为供应链公开 Tool；
- Data Agent 通过目录遍历发现文件的指令；
- Work State 中源文件 revision 的重复记录。

保留：网络实体要求、字段别名、业务单位、行政区匹配、规范化和业务 readiness。

### 测试和退出

- CSV/JSON/XLSX 上传、revision、画像、显式映射、参数、发布、刷新恢复；
- 不支持格式、歧义字段、缺实体、越权 workspace/dataset、并发 revision 409；
- Data Agent 只取得 authorized intake/release handle；
- 同一个 Dataset Release 可被两个 Work State 授权绑定，不复制内容；
- 空 Workspace 和领域失败都不加载 Mock。

退出条件：任何领域的数据文件都经过同一个 Platform Intake；供应链数据库不再保存
SourceAsset 或 mapping 生命周期。

## 7. W4：CollaborationContext、AssignmentContract 与执行投影

### Owner 和依赖

- Owner：Platform Server + Profile Host + Codex Adapter。
- 前置：W1、W2。
- 不修改 Runtime 调度语义。

### W4.1 生成上下文

新增：

```rust
struct CollaborationContextBuilder {
    authorizer: CollaborationAuthorizer,
    catalog: ReleaseResolver,
    installations: InstallationResolver,
}

impl CollaborationContextBuilder {
    build_for_run(actor, run_id, supervisor_snapshot) -> CollaborationContext;
    narrow_for_execution(root_context, agent_release, assignment) -> AgentExecutionContext;
}
```

`build_for_run` 在 Run 启动事务内固定 organization/profile/workspace/task/run、Supervisor
Release、Installation snapshot、Work State/Dataset grants 和预算。context 的内部签名或
opaque handle 只在 Platform/Profile Host 使用，不进入 browser DTO。

### W4.2 编译 assignment

新增：

```rust
struct AssignmentCompiler;

impl AssignmentCompiler {
    compile(
        supervisor: &ResolvedSupervisor,
        agent: &ResolvedAgent,
        context: &CollaborationContext,
        request: AssignmentRequest,
    ) -> AssignmentContract;
}
```

校验：Agent 在 allowlist、capability 是交集、component/deliverable 类型已声明、预算
不越界、资源属于 task。自然语言 objective 有长度限制，完整数据不允许进入。

### W4.3 Runtime 桥接

沿用当前 exact role allowlist 和 request-scoped role 注入。Adapter 增加内部方法：

```rust
prepare_supervisor_thread(context, compiled_supervisor) -> ThreadStartConfig;
prepare_agent_assignment(context, contract, runtime_role) -> AgentAssignmentPayload;
```

如果官方 assignment metadata 没有对应字段，首期把机器合同编码为有明确 sentinel 和
schema 的受控 developer instruction fragment；它由 Adapter 生成，不由 Prompt parser
恢复。需要先证明现有 retained seam 无法正确完成，才评审 Codex 修改。

### W4.4 执行生命周期

保留 Codex Thread/Turn 为事实 owner；Platform projection 增加：

- `assignment_id`、`agent_release_id`、`work_state_id`；
- spawned/running/waiting/waiting_for_input/completed/failed/interrupted；
- cancelled/timeout/rejected；
- 首个 terminal sequence 和安全 result summary；
- Tool operation ID 关联，但不保存思维链和完整结果。

乱序规则集中在 `ExecutionProjectionReducer`，不在事件 handler 分散判断：

```rust
fn reduce(current: ExecutionProjection, event: ExecutionObservation) -> TransitionResult;
```

### W4.5 用户输入

继续复用官方 `request_user_input`：

- root/child 都先持久化为 Approval，再广播；
- `waiting_for_input` 关联 platform approval UUID；
- answer 精确投递 Runtime request，浏览器不见 Runtime ID；
- secret 永不落数据库、日志、事件、错误；
- 同时多个 Agent 请求独立展示，不阻塞 composer 或其他 Agent。

### 测试和退出

- context scope 收窄、Agent 越权、伪造 assignment、运行中 Release 变化不影响 snapshot；
- root/child 输入、刷新、stale version、delivery_unknown；
- 所有终态、乱序、重复事件、重启重放；
- Root 通过 Coordination Tool 获取状态，不需要 Agent 发送进度说明。

退出条件：一个 Supervisor 可以安全委派两个 Agent；权限和交付不依赖 Agent 名称或
自然语言解析；平台仍没有 spawn scheduler。

## 8. W5：统一 Catalog、Compiler、Release 和 Installation

### Owner 和依赖

- Owner：`supervisor-catalog` 重构为通用 `capability-catalog`；Profile Host 负责安装。
- 前置：W1、W4。

### W5.1 新 Catalog crate

新建 `apps/web/crates/capability-catalog`，迁移并删除 `supervisor-catalog` 中通用逻辑。
模块：

```text
drafts.rs
releases.rs
dependencies.rs
compiler.rs
installations.rs
validation.rs
sealing.rs
```

Service：

```rust
pub struct CapabilityCatalogService;

impl CapabilityCatalogService {
    create_draft(actor, kind, resource_id, idempotency_key) -> CatalogDraft;
    save_draft(actor, draft_id, expected_revision, content) -> CatalogDraft;
    validate_draft(actor, draft_id, expected_revision) -> ValidationReport;
    publish(actor, draft_id, expected_revision, release_kind) -> CatalogRelease;
    resolve_exact(actor, release_id) -> ResolvedRelease;
    deprecate(actor, release_id, reason) -> CatalogRelease;
}
```

第一次发布自动 `1.0.0`，默认后续 patch；发布请求可显式 `minor`/`major`，服务器在按
resource 加锁的事务中分配版本。Draft 不含 version 字段。

### W5.2 PackageCompiler

```rust
pub struct CopilotPackageCompiler {
    validators: ValidatorRegistry,
    runtime: RuntimeBundleCompiler,
    limits: PlatformLimits,
}

impl CopilotPackageCompiler {
    normalize_draft(&self, content: CatalogDraftContent) -> CanonicalDraft;
    validate(&self, draft: &CanonicalDraft, deps: &ResolvedDependencyGraph) -> ValidationReport;
    compile(&self, draft: CanonicalDraft, deps: ResolvedDependencyGraph) -> CompiledPackage;
}
```

Canonical JSON 排序、换行和文本规范化只在这里完成。内容 hash、execution semantics hash、
Runtime role、MCP inventory、Skill roots、Artifact contracts 和 lock manifest 都由 compiler
生成。代码 seed、Web Draft 和 Tutorial Blueprint 调同一接口。

### W5.3 数据库

统一：

```text
catalog_resources
catalog_drafts
catalog_releases
catalog_release_dependencies
catalog_release_capabilities
profile_installations
profile_installation_resources
installation_attempts
```

不同资源的内容放入明确 schema-versioned JSONB，但 owner、revision、release、依赖、
安装和审计只实现一次。数据库约束确保同 resource/version 唯一、Release 不可变、Draft
revision 单调、依赖指向精确 Release。

### W5.4 安装事务

```rust
pub struct InstallationService {
    profiles: ProfileRegistry,
    host: ProfileHost,
    runtime: CodexAdapter,
}

impl InstallationService {
    plan(actor, profile_id, copilot_release_id) -> InstallationPlan;
    install(actor, plan, idempotency_key) -> InstallationSummary;
    reconcile(system_actor, installation_id) -> InstallationSummary;
    uninstall(actor, installation_id, expected_revision) -> InstallationSummary;
    get_readiness(actor, installation_id) -> InstallationReadiness;
}
```

`install` 使用 staging directory、内容 hash 校验和原子替换；随后调用官方 reload/discovery。
Runtime 没有发现 exact resources 时状态为 `installed` 或 `unavailable`，不得写 ready。
失败保留安全诊断和原始 cause 分类，不自动换旧版本。

### W5.5 删除旧路径

- 删除手工 `include_str!` + 常量 hash 作为主要发布方式；seed 由 build/import 命令调用
  compiler 生成 Release；
- 删除 Agent Draft 中的 `version`；
- 删除 Supervisor/Agent/Tutorial 各自重复的 seal/hash/version 方法；
- 删除 `LEGACY_*`、版本启发式和旧 package fallback；
- migration seed 只引用编译生成 fixture，不手抄 hash。

### 测试和退出

- 三次 Draft 保存不改版本；并发 publish 获得唯一版本；同内容幂等；不同内容冲突；
- dependency cycle、缺 Release、capability 不满足、权限交集为空；
- 安装 crash recovery、staging cleanup、reload 失败、discovery 缺失、uninstall in-use；
- 代码 seed 与 Web Draft 编译结果字节一致。

退出条件：开发者不维护 semver/hash/role/MCP 名；所有五类资源共享一套 Catalog/Release/
Installation 状态机。

## 9. W6：Tool SDK、Tool Studio 与受控安装

### Owner 和依赖

- Owner：Tool SDK + Platform Tool Studio + Profile Host。
- 前置：W1、W5；Work State integration 使用 W2。

### W6.1 SDK package format

建立 `copilot-tool-package.v1`：

```text
tool-package.toml
src/
requirements.lock
schemas/
tests/
README.md
```

manifest 字段包括 package identity、Python 版本、Tool declarations、Resource declarations、
Secret slots、网络目标策略、副作用等级、超时/结果上限和 health check。禁止任意安装脚本。

提供 CLI：

```text
copilot-sdk tool init
copilot-sdk tool validate
copilot-sdk tool test
copilot-sdk tool pack
```

CLI 与 Server 共享 JSON Schema 和 fixture；Web 上传 `.copilot-tool` 包后由 Server 重新
验证，不能信任本地结果。

### W6.2 隔离测试 Runner

把当前 `routes/python_capabilities.rs` 中进程启动逻辑移到 `tool-runner-service`：

```rust
trait ToolPackageRunner {
    validate(bundle) -> PackageValidation;
    probe(bundle) -> McpInventory;
    test_tool(bundle, tool, args, workspace_grant) -> ToolTestResult;
}
```

首期固定 Python 版本、依赖 allow policy、CPU/内存/时间/输出限制、临时只读 package
目录和显式 workspace grant。未来容器化不改变接口。command、env 和完整 stderr 不回
浏览器，只返回安全分类和截断诊断。

### W6.3 Web Tool Studio

新增页面和 API：

```text
/codex/tools
/api/catalog/tools
/api/catalog/tool-drafts/{id}
/api/catalog/tool-drafts/{id}/validate
/api/catalog/tool-drafts/{id}/test
/api/catalog/tool-drafts/{id}/publish
/api/profile-installations/{profileId}/tools/{releaseId}
```

UI 分页展示 Draft、Release、Installation、Runtime readiness；上传 source bundle、编辑
schema/Secret slot、运行测试、发布、安装。首期不提供任意在线 IDE，避免把代码编辑器
误当作 package lifecycle。

### W6.4 现有 Python capability 迁移

- 将 `PythonCapabilityPublishRequest` 转为 ToolPackage Draft；
- 删除请求中的用户 version，改为 Draft revision；
- 删除按 workspace 直接写 package 目录的 route；
- 发布到 Catalog 后由 Installation Service 物化到 Profile；
- 当前供应链 MCP 作为 SDK reference package 导入，不再走特殊 launcher 发现。

### 验证和退出

- 恶意 zip 路径、超大包、依赖越界、shell launcher、secret 回显、网络策略拒绝；
- MCP initialize/listTools/callTool、错误、timeout、cancel、结果上限；
- publish/install/reload/discovery/restart/uninstall；
- 一个 Hello Tool 和供应链 Tool 都使用相同流程。

退出条件：算法工程师能从 SDK 创建并通过 Web 发布安装 Python Tool，Runtime 真实发现；
启动链路不执行 `source`，平台不按 Tool 名硬编码。

## 10. W7：中文 Skill 生命周期与 Skill Studio

### Owner 和依赖

- Owner：Catalog + Runtime 官方 Skill discovery/config。
- 前置：W5、W6。

### W7.1 Skill compiler

新增 `SkillPackageCompiler` validator：

- `SKILL.md` 必须为中文主体；代码、schema identifier 和正式产品名可保留英文；
- 检查适用范围、输入 owner、Tool capabilities、交付件、请求用户条件、失败处理、
  上下文规则和禁止 Mock fallback；
- 依赖 capability 必须由精确 Tool Releases 满足；
- 限制正文、示例和附件大小；禁止 Secret 和绝对路径。

### W7.2 Runtime integration

优先复用官方 `skills/list`、Skills config write 和 discovery invalidation。Profile Host：

```rust
install_skill_bundle(profile_id, bundle) -> RuntimeResourceHandle;
reload_skills(profile_id) -> RuntimeDiscoverySnapshot;
verify_skill_release(profile_id, release_id, snapshot) -> SkillReadiness;
```

Platform 不扫描 Skills 目录判断成功。若官方 API 只能 list 而不能 install，Installation
Service 负责安全物化，Runtime list 负责最终 readiness。

### W7.3 Web Skill Studio

- 中文结构模板和逐项校验；
- 选择 Tool capability，不显示 MCP server 内部名称；
- Preview 展示模型可见正文和机器声明；
- 运行示例任务，记录 Skill 是否被发现及 Tool 是否按预期调用；
- 发布、安装、停用、卸载和诊断。

### 验证和退出

- 中文规则、缺章节、无效 capability、过大正文、路径/Secret 拒绝；
- 安装后新 Thread discovery，卸载后新 Thread 不可见；
- Plugin/Project/Profile Skill 冲突语义按官方事实验证，不自行发明优先级；
- 仓网所有当前 Skills 迁入 Catalog，删除目录名启发式发现。

退出条件：Skill Release 的发布、安装和 Runtime 可见状态独立可审计，Agent 只引用精确
Skill Release/capability。

## 11. W8：Agent、Supervisor 和 Copilot Studio

### Owner 和依赖

- Owner：Catalog + Package Compiler + existing Runtime role seam。
- 前置：W4-W7。
- 默认不修改 Codex。

### W8.1 Agent Studio

重构当前 Agent Draft：

```rust
struct AgentDefinitionDraft {
    identity: DraftIdentity,
    purpose: String,
    instructions: String,
    skill_requirements: Vec<ReleaseRequirement>,
    tool_requirements: Vec<CapabilityRequirement>,
    data_permissions: Vec<DataPermissionTemplate>,
    input_contract: AgentInputContract,
    assignment_contract: AssignmentContractTemplate,
    deliverable_contracts: Vec<DeliverableContract>,
    execution_limits: AgentExecutionLimits,
    model_policy: ModelPolicy,
}
```

删除 `capability_template` 继承整个已有 Agent 的做法。改为组合精确 Skill Releases、Tool
capabilities 和可选的“Agent template”，template 只复制 Draft 初始值，不成为隐藏运行
依赖。发布时 compiler 解析所有精确依赖。

提供 **Test Agent**：创建隔离测试 Task/Thread，固定 Agent Release candidate、Dataset
grants 和预算，验证 Runtime exact role、Skill discovery、Tool allowlist、用户输入和
deliverable。测试记录不是生产 readiness 的替代。

### W8.2 Supervisor Studio

Draft 字段采用目标架构中的 `SupervisorDefinition`。自动注入 root coordination 只读
capability；用户选择 Agent Releases、协作策略、最终交付件、最大 active children 和
部分失败规则。UI 不提供固定阶段流程编辑器。

Validator 检查：

- Agent deliverable 能满足 Supervisor 最终交付；
- Agent 依赖图无循环，允许动态调用但不要求固定顺序；
- max children 和 context budget 在平台限制内；
- root 不继承 domain Tool，除非 Supervisor 明确承担该领域计算且通过权限评审；
- blocking input 有 owner，失败/timeout 有最终综合规则。

### W8.3 Copilot Builder

把 Supervisor、Agents、Skills、Tools、Secret slots、权限模板、Tutorial/E2E 组合为
CopilotPackage。发布前执行：

```text
resolve graph -> validate contracts -> compile runtime bundle
-> dry-run installation -> discovery probe -> test task -> publish release
```

不把一次 test task 的运行数据写进 Release；只保存测试结果、构建 provenance 和 lock。

### W8.4 判断是否需要 Codex 修改

在现有 exact role seam 上验证：

1. 用户定义 Agent 能否被安全物化并只在指定受治理 Thread 中可见；
2. spawn 时是否能按 exact role 应用指令、模型策略和 Tool allowlist；
3. Profile 重启和新 Thread 是否按 Installation snapshot 收敛；
4. 未授权普通 Thread 是否无法发现受治理 Role；
5. TUI/CLI 官方行为是否不受影响。

若全部成立，不修改 Codex。若某项失败，先证明 Platform/Profile Host 无法正确拥有该
语义，再运行 `scripts/codex-upstream-status.sh`、`scripts/codex-customization-status.sh`，
更新 Patch Map，提出最小 owning crate、协议、测试、重放步骤和退出条件。没有这份证据
不得编码。

### 测试和退出

- Web 连续保存、validate、publish、install、test Agent、test Supervisor；
- 多 Agent 动态顺序、部分失败、用户输入、超时、刷新、重启；
- 普通 Thread 隔离、exact release、运行中发布不漂移；
- 仓网 Copilot 全部由 Web/Compiler 资源组成，不依赖代码中的 6.0 常量。

退出条件：用户可以从 Web 组合 Tool + 中文 Skill + Agent + Supervisor 并发布真实
Copilot；当前 Runtime seam 足够，或已完成单独获准的最小 Codex 变更。

## 12. W9：上下文、Provider 缓存与执行可观测性

### Owner 和依赖

- Owner：Codex Adapter normalization + Platform observability + Web execution view。
- 前置：W4；可与 W5-W8 并行。

### W9.1 逐模型调用指标

平台从 Runtime 安全事件归一化：

```rust
struct ProviderCallMetric {
    call_id: Uuid,
    run_id: Uuid,
    thread_id: String,
    agent_execution_id: Option<Uuid>,
    provider_id: Uuid,
    model: String,
    input_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    tool_schema_tokens: Option<i64>,
    latency_ms: i64,
    first_token_ms: Option<i64>,
    compaction_count: i32,
    terminal_status: ProviderCallStatus,
}
```

不保存 prompt/reply 正文。标准 cached token 和 Provider 特有字段在 Adapter 中归一化；
未知字段不猜测为 0，使用 `None` 并记录 capability/diagnostic。

### W9.2 ContextBudgetPolicy

由 Copilot/Agent Release 声明并由 Platform 校验：

- assignment summary bytes；
- Tool result inline bytes；
- Resource page size；
- Artifact preview bytes；
- 最大连续 Tool inventory/schema 注入；
- 超预算时必须发布 Artifact/Resource 或分页，不得截断后伪装完整。

Runtime 仍拥有 compaction。Platform 只观察并限制自己注入的内容，不实现第二个
compactor/cache。

### W9.3 缓存 0 命中诊断

为每次调用记录以下可比较 fingerprint，不保存正文：

```text
stable_prefix_sha256
tool_inventory_sha256
skill_set_sha256
runtime_role_sha256
provider_cache_namespace
```

分析规则：

- cached token 缺失与真实 0 分开；
- stable prefix 改变时标记具体来源：role/skills/tools/provider config；
- 动态 case 状态放在消息尾部，不重写稳定 system/developer 前缀；
- Tool schema inventory 只注入 Agent allowlist，不注入 Profile 全量；
- 不为了命中缓存冻结错误上下文或引入本地 prompt cache。

### W9.4 Web

Agent execution 展开区显示安全指标：输入、缓存输入、输出、延迟、compaction、Tool
调用数和数据引用数。Root/Agent 卡片不展示思维链。Run 级视图可定位某次 600 秒调用
到底是大输入、Provider 延迟、工具等待还是未终止请求。

### 验证和退出

- 标准 Provider、DeepSeek top-level cache 字段、字段缺失、流中断；
- 两次稳定前缀调用证明 hash 一致，能观测缓存是否实际命中；
- 50 城市仓网任务不把原始 CSV/矩阵复制进调用；
- 单次 600 秒调用能由阶段耗时拆解，不再只有总时间。

退出条件：缓存命中率、上下文大小和长调用原因都可由有界指标解释；不以重试或扩大
timeout 掩盖问题。

## 13. W10：供应链参考实现迁移与清理

### Owner 和依赖

- Owner：Supply Chain ToolPackage + 两个 Agent/Skills。
- 前置：W2、W3、W5-W9。

### W10.1 领域模型

保留：

- `DemandCity`、`Warehouse`、`CurrentAssignment`；
- route/time/cost matrix；
- assignment/service/cost/scenario/location solution；
- geography、mapping、solver、report、map 的领域实现。

移除 `CaseRepository` 通用职责。新增 `SupplyChainWorkStateAdapter` 注册 component schema：

```text
network_requirements.v1
normalized_network_input.v1
route_matrix.v1
cost_matrix.v1
network_assignment.v1
service_metrics.v1
cost_summary.v1
scenario_result.v1
facility_location_solution.v1
network_planning_report.v1
network_comparison_map.v1
```

依赖和 invalidation 规则由 adapter 声明，持久化由 Work State Service 处理。

### W10.2 Tool inventory

所有公开 Tool 使用 `@copilot_tool`。删除 `server.py` 中未注册的 legacy snapshot、旧
Resource、旧 report 和旧 solver 函数。最终 inventory 由 package manifest + probe
测试精确锁定，不再靠阅读源码猜测。

导航闭环新增领域 Tool：

```text
plan_route_matrix
build_haversine_route_matrix
register_navigation_route_matrix
validate_route_matrix
```

Maps MCP 仍由 Network Agent 调用；供应链 Tool 不跨 MCP 调另一个 MCP。导航结果通过
Artifact/Domain Resource handle 注册，校验 route plan、数量、hash 和费用确认。

### W10.3 Agent/Skill

- Data Agent：只处理 Data Intake release、领域映射/规范化/地理校验；
- Network Agent：需求、矩阵、分析、模拟、求解、报告和地图；
- Root：只协调、读状态和综合，不直接使用供应链 Tool；
- Skills 全部中文，引用精确 capabilities 和 component/deliverable types；
- 删除 `planning-dataset.v2`、6.0 目录版本和旧 2.x/5.x package；由 Catalog Release
  替代源码目录版本号。

### W10.4 真实 E2E

完整扩展用例：

1. Web 上传 50 城市、11 仓、报价；
2. 发布 Dataset Release；
3. Web 安装或选择供应链 Copilot；
4. 创建任务并要求分析现有网络；
5. Data/Network 动态协作，输入卡确认绕路系数；
6. 无 current coverage 时分别给出时效优先和成本优先优化基线，不冒充实际方案；
7. 加入 current coverage 后输出实际方案；
8. 运行成本、模拟、p-median、时效约束和地图；
9. 刷新、断开、Profile 重启后恢复输入、Agent、Work State 和 Artifact；
10. 验证 Agent 卡片单终态、wait 不刷屏、Provider 指标有界；
11. 导航选择只验证费用确认和注册合同，批量真实计费接口单独受控测试。

### 验证和退出

- Python unit/ruff、MCP stdio smoke、SDK contract；
- Work State/Data Intake integration；
- Rust/Web/full real E2E；
- `rg` 确认当前代码/文档无旧 schema/version；
- `git diff --exit-code -- codex/codex-rs`，除已存在且另有范围的 Provider 观测 seam。

退出条件：仓网是平台资源的消费者，不再实现公共平台能力；完整真实链路通过。

## 14. W11：通用 Web 体验与新手交付

### Owner 和依赖

- Owner：Browser WebApp。
- 前置：W5-W10 API 稳定。

页面信息架构：

```text
Build
  Tools
  Skills
  Agents
  Supervisors
  Copilots

Run
  Workspaces and Data
  Tasks
  Artifacts
```

每个 Build 页面统一展示 Draft revision、validation、Release、dependencies、Installation
和 Runtime readiness。错误使用业务可理解语言，并可展开安全技术诊断。页面不展示
本地路径、Runtime request ID 和 Secret。

教程改为使用正式 UI：

1. Hello Tool + 单 Agent；
2. 两 Agent 协作；
3. 印尼仓网渐进案例；
4. 从本地 SDK 包导入；
5. 常见安装、发现、权限、输入和恢复故障。

退出条件：新用户只使用 Web 和 SDK 文档即可完成发布运行，不需要修改 migration、
capability JSON、`.mcp.json` 或 Profile 隐藏配置。

## 15. W12：第二个非供应链参考案例

### Owner 和依赖

- Owner：独立领域开发者；不得由供应链包复用内部代码。
- 前置：W11。

选择一个小型但真实的案例，例如“服务质量异常分析”：

- Tool：指标读取、异常检测、分段比较、报告；
- Skill：中文诊断方法；
- Agents：Data Quality Agent + Metric Diagnostic Agent；
- Supervisor：根据数据质量和证据缺口动态协调；
- 数据：CSV/JSON，通过统一 Intake；
- Work State：领域自有 component schema；
- Artifact：报告和图表。

验证标准：新增案例不修改 Platform event projection、Work State schema、Coordination
Tool、Data Intake 状态机或 Web 领域分支。若必须修改，先判断是缺失的通用 extension
point 还是错误抽象，并回到 W1/W2 修正，不能复制供应链实现。

退出条件：第二案例只新增 domain package、Skills、Agent/Supervisor Draft 和 renderer，
证明平台扩展点成立。

## 16. W13：多用户开放门禁

### Owner 和依赖

- Owner：Auth/Platform/Profile Host/Security。
- 前置：W1-W12；当前只实现 scope，不开放 UI。

完成矩阵：

- 两个 organization、user、profile、workspace 并发；
- Catalog Draft/Release visibility；
- Profile Installation、Runtime process/cache key；
- Data Intake、Dataset Release、Work State、Approval、Artifact；
- execution subscriptions 和 reconnect replay；
- Secret grant、Tool network/data policy；
- 猜测 UUID、跨 scope binding、共享 cache 污染；
- restart、cancel、timeout、installation reconcile。

所有拒绝必须在 owning service 发生，浏览器隐藏不是授权。门禁通过后再设计成员、邀请、
角色和租户管理 UI。

退出条件：两个用户无法读取、调用、安装或控制对方资源；Runtime 和缓存也按 Profile
隔离，然后才能把当前单用户阶段改为多用户可用。

## 17. 实施顺序和提交边界

```text
W0 baseline
 -> W1 contracts
 -> W2 work state + tool result + coordination
 -> W3 data intake convergence
 -> W4 collaboration contract
 -> W5 catalog/compiler/installation
 -> W6 tool SDK/studio
 -> W7 skill studio
 -> W8 agent/supervisor/copilot studio
 -> W9 observability (W4 后可并行)
 -> W10 supply-chain migration
 -> W11 browser/tutorial completion
 -> W12 second domain proof
 -> W13 multi-user gate
```

建议提交序列：

1. `docs: align copilot authoring platform architecture`
2. `ops: make development database rebuild explicit`
3. `platform: define catalog work state and collaboration contracts`
4. `platform: add durable work state service`
5. `sdk: add bounded platform tool result contract`
6. `platform: expose read only supervisor coordination tools`
7. `data: move intake lifecycle into platform service`
8. `runtime: bind typed collaboration context to governed runs`
9. `catalog: unify drafts releases compiler and installations`
10. `tools: publish python tool sdk and studio lifecycle`
11. `skills: publish chinese skill lifecycle and studio`
12. `agents: publish agent supervisor and copilot builders`
13. `observability: persist provider context and cache metrics`
14. `supply-chain: migrate network case to platform work state`
15. `web: complete reusable copilot authoring journey`
16. `e2e: verify supply-chain and second domain copilots`
17. `security: verify cross-user isolation gate`

## 18. 每个工作包的统一完成定义

一个工作包只有同时满足以下条件才完成：

1. owner、DTO、持久化和状态机落在正确层；
2. 新路径覆盖正常、失败、取消、超时、重启、并发和越权中适用的场景；
3. 旧 owner、旧 API、旧字段、旧 fixture、旧文档和 fallback 已删除；
4. 浏览器 DTO 不泄露 raw Runtime、路径或 Secret；
5. Runtime capability 由真实 discovery 证明；
6. 能力基线只记录已验证事实；
7. `git diff --check` 和受影响范围测试通过；
8. 需要真实边界的功能完成真实 Runtime/Web E2E，而不是只通过 Fake；
9. 未获单独批准不得扩大 `codex-rs` 差异。

## 19. 本轮立即执行顺序

当前应按以下顺序继续，不先扩展仓网业务功能：

1. 完成 W0 数据库恢复与文档收敛；
2. 实施 W1-W2，把通用 Work State、Tool result 和 Root coordination 建成平台能力；
3. 实施 W3，删除供应链的数据 Intake 双重 owner；
4. 实施 W4-W5，让协作、版本和安装成为统一合同；
5. 再迁移 Tool/Skill/Agent/Supervisor Web 创作链路；
6. 最后迁移仓网并跑真实扩展 E2E；
7. 用第二领域案例证明没有把仓网抽象硬编码进平台。

临时进度和问题快照保存在
`temporary-copilot-platform-refactor-audit-2026-08-08.md`。W10 真实 E2E 通过且能力基线
更新后删除该临时文档。
