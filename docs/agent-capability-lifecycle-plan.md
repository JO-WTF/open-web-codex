# Copilot 开发平台历史实施输入

| 字段 | 内容 |
| --- | --- |
| 文档性质 | 阶段二候选研究输入，不是当前计划 |
| 状态 | 已被 ADR-018 替代；内容待阶段二重新裁决 |
| 更新日期 | 2026-08-08 |
| 当前执行 | [开发计划](development-plan.md) |
| 接受决策 | [ADR-018](adr/018-built-in-network-copilot-runtime-closure.md) |
| 当前事实 | [Capability Baseline](capability-baseline.md) |
| 兼容策略 | 不兼容项目历史实现；更新全部调用方后原子删除旧路径 |

本文保存 ADR-017 时期对公开 SDK/Studio/Catalog 的实施研究。下文 W0-W12 全部暂停，
不得创建阶段一任务、类型、表、route、migration 或验收门。其中 Work State、Data Intake、
SourceAsset/DatasetRelease/DomainResource、Resource Broker、Assignment Grant、Run
Completion、Root-only 输入中继和 installation snapshot 已被 ADR-018 否决，不是以后可直接
恢复的 backlog。阶段二开始前必须依据阶段一证据重新写计划；不能从本文复制合同。

## 1. 固定实施规则

1. 当前以模块化单体实现 Control/Data 逻辑模块；不为未来规模拆微服务。
2. Codex Runtime 继续拥有 Thread、Turn、context、Agent 调度、Skills/Plugins/MCP、Tool
   execution 和官方输入；Platform 不创建第二个 scheduler。
3. 一个事实只能有一个 owner。新 owner 可执行并通过边界验证后，在同一切换中删除旧
   owner、调用方、fixture、UI、migration/seed 和 current-state 文档。
4. Browser 只消费 bounded platform DTO；不得接触 raw JSON-RPC、Runtime request ID、
   路径、Secret 或内部 installation layout。
5. 模型不构造授权、scope、URI、hash、Run/Work State ID、revision、operation ID、CAS
   或通用 mutation。
6. Skills 只指导业务方法、判断和沟通；硬权限、类型、状态机、终态、安装和恢复由机制
   约束。
7. 不新增 dual read/write、aliases、路径扫描、版本 fallback、Prompt 补丁或假成功。
8. 默认不修改 `codex-rs`。官方能力和已登记 retained seam 都无法实现必要合同后，才按
   Patch Map 提交独立裁决。
9. 每个工作包以纵向可执行结果完成，不以“类型、表或页面已存在”完成。

## 2. 目标公共合同

下列名字是目标语义；实现时在 `platform-contracts` 中形成一个当前版本，不保留旧字段：

```text
CapabilityDraft
CapabilityRelease
CopilotLockManifest
ProfileInstallation
RuntimeDiscoveryObservation
RuntimeReadiness

AssignmentGrant
ToolOutcome
ChildNeedsInput
RunCompletionState

SourceAssetRef
DatasetReleaseRef
DomainResourceRef
ArtifactRef
```

关键状态：

```text
Draft -> Validated -> Released
Released -> Authorized -> Installing -> Installed -> Discovered -> Ready

Assignment:
prepared -> spawned -> running
                    -> succeeded | needs_input
                    -> failed | rejected | cancelled | timeout | interrupted

Run:
prepared -> awaiting_children -> awaiting_synthesis
         -> succeeded | partial | failed | rejected
         -> cancelled | timeout | interrupted | orchestration_incomplete
```

## 3. W0：冻结旧链并建立证据纪律

### Owner

Documentation + Platform Operations。

### 实施

- 将 [ADR-017](adr/017-clean-copilot-platform-spine.md) 加入所有设计/开发入口；
- 当前仓网链只保留回归，不再扩 aliases、Prompt 或 Projection workflow；
- E2E evidence schema 记录 commit、构建 profile/fingerprint、Profile generation、
  Provider definition identity、Release/Installation snapshot、Runtime manifest、Task/Run、
  assignments、inputs、terminals、artifacts 和后置测试；
- evidence 脱敏，禁止保存 Secret、Prompt 正文、内部路径和无界 Runtime payload；
- 图谱继续用于导航，但新增文件必须以源码和测试复核。

### 删除

- 删除 current-state 文档中的“真实 E2E 未完成”“服务仍因旧 migration 停止”等失效事实；
- 删除互相冲突的当前阶段表述。

### 退出

Canonical 文档分别拥有目标、现状、计划和决定；一次 E2E 可以生成机器可读 evidence，
且不能仅靠模型最终文本宣称成功。

## 4. W1：干净 Profile generation

### Owner

Profile Host + Provider Service。

### 合同

```text
ProfileGeneration
  organization_id, user_id, profile_id, generation_id
  codex_home_identity
  installation_snapshot_id
  codex_home_identity
  created_at, terminal_state, safe_failure
```

### 实施

- 从空 Profile 创建唯一 generation，不默认导入宿主 `auth.json`、Plugin、Skill、MCP、
  Memory 或 cache；
- Provider credential 只从加密 Platform Secret grant 注入；
- 进程、request map、discovery cache、event subscription 以 profile+generation 为 key；
- 明确 start/ready/failed/stopping/stopped 终态和 restart 语义；
- 构建 fingerprint 必须对应实际运行 profile，不能把 `debug` 产物当作 `dev-small` 证据。

### 删除

- `default_cli_codex_home()` 触发的隐式宿主认证导入；
- 依赖服务器 cwd 或用户宿主目录的 Profile 初始化。

### 验证与退出

空 Profile 冷启动、连续重启、无 Secret、错误 Secret、Runtime 启动失败、旧 generation
事件和并发 start 均有证据；两个 generation 不共享认证或 Runtime state。

## 5. W2：统一 Package、Catalog 与 Compiler

### Owner

Capability Catalog + Package Compiler。

### 合同

五类资源共用：

```text
Draft(identity, revision, author_content)
ValidationReport(errors, warnings, generated_preview)
Release(id, generated_version, canonical_content_sha256,
        execution_semantics_sha256, exact_dependencies, runtime_requirements)
CopilotLockManifest(exact Supervisor/Agent/Skill/Tool releases,
                    grants, secret slots, runtime bundle manifest)
```

### 实施

- Tool、Skill、Agent、Supervisor、Copilot 使用同一 Draft/Release service；
- Compiler 规范化内容、解析 exact dependency、收窄 grant、生成 Runtime bundle、计算 hash、
  事务分配版本并输出 lock manifest；
- code seed、Tutorial、Web Draft、SDK import 全部调用同一 compiler；
- Agent 精确组合 Skill Releases、Tool capabilities、data permissions、input/output 和
  execution limits；
- Supervisor 精确组合 Agent Releases 和最终交付，不保存内部 runtime/path 字段。

### 删除

- `supervisor-catalog` 作为第二 Release owner；
- reviewed capability template 继承；
- 手工 semver/hash/Runtime Role/MCP lock 常量；
- Tutorial 自算 package hash；
- 旧 Agent/Supervisor 发布 routes 和旧 seed owner。

### 验证与退出

同一 author content 由 SDK、Web、seed、Tutorial 得到同一 canonical result；并发发布只
产生一个正确版本；缺依赖、越权 grant、循环依赖和漂移均阻止 Release。

## 6. W3：SDK 到 Web 的单一路径

### Owner

Tool SDK + Catalog import API。

### 合同

```text
copilot-sdk login
copilot-sdk tool init
copilot-sdk tool dev
copilot-sdk tool test
copilot-sdk tool push
```

SDK package 只包含作者内容和声明，不包含服务器分配版本、宿主路径或最终 Runtime ID。

### 实施

- 受限 Python MCP 模板、JSON Schema、typed errors、bounded outcome、fixture 和 contract
  tests；
- dependency lock、launcher 与 Plugin/MCP materialization 由可信 SDK/Compiler 生成；
- `push` 创建或更新 Web Draft，返回可打开的 Draft identity；
- Tool 与 Skill 是独立用户对象；Tool init 不隐式发布 Skill；
- test 必须执行至少一个声明用例；没有测试返回 `not_tested`，不能成功通过门禁；
- 不支持任意 shell、任意在线依赖和未声明外部副作用。

### 删除

- Web 普通模式编辑原始 `.mcp.json`、Plugin JSON、launcher 和版本；
- SDK 作者手工维护服务器/版本/hash；
- “No tests directory; validation passed” 成功路径。

### 验证与退出

一个新 Tool 只用正式 SDK 文档完成 init/dev/test/push，并在 Web 出现同一个 Draft；恶意
路径、额外文件、未锁依赖、无测试和超限结果被明确拒绝。

## 7. W4：Profile Installation 与 Runtime discovery

### Owner

Installation Controller 持有 desired state；Profile Host 执行；Codex Runtime 持有
discovery observation。

### 合同

```text
ProfileInstallation
  installation_id, profile_id, profile_generation_id
  release_id, desired_state, materialized_sha256
  state, attempt_id, terminal, safe_failure

RuntimeDiscoveryObservation
  installation_id, runtime_generation_id
  exact skills, roles, mcp servers, tool inventory
  observed_at, diagnostics
```

### 实施

- staging -> integrity check -> atomic activate -> Runtime reload/restart -> discovery；
- Tool/Skill/Agent/Supervisor/Copilot 都必须真实物化或显式 unsupported；
- `installed` 只说明原子物化成功；`discovered` 只由 Runtime observation 产生；
- readiness 聚合 Release、grant、Secret、installation、discovery 和 health，不猜测；
- Task 绑定 immutable installation snapshot，升级不改变活动 Task。

### 删除

- 安装到 Workspace 的主链；
- Agent/Supervisor/Copilot 空操作 installed；
- readiness 从数据库 state 自我推导 discovery；
- cwd、Workspace、source repo、文件存在、Provider 名或错误文本 capability 扫描。

### 验证与退出

安装、hash 校验、原子切换、reload、discovery、health、失败、回滚、重启和并发 reconcile
分别可故障注入；Runtime inventory 与 lock manifest 完全匹配后才能 ready。

## 8. W5：Copilot Studio 与 Builder

### Owner

Browser WebApp + Platform authoring API。

### 实施

- 主导航提供独立 Copilot Studio：我的 Copilot、能力库、测试与发布、版本与安装；
- 一个 Copilot Draft 显示目标、成员、能力、数据、权限、交付、测试和安装；
- Skill 表单覆盖适用/不适用、输入、Tool、询问条件、失败、交付、预算和示例，生成中文
  `SKILL.md`；
- Agent 表单只要求责任、Skills、Tool capabilities、数据权限、输入、交付和停止条件；
- Supervisor 表单只要求总体责任、Agent Releases、冲突/部分失败/停止和最终交付；
- Builder 顺序执行合同校验、隔离测试、dry-run install、discovery、test task 和 Release；
- 普通用户只看到业务状态与修复动作；内部合同进入高级详情。

### 删除

- 普通导航中的 Runtime Agents；移到高级 Runtime 设置；
- Settings 中 Agent Catalog、Tool & Skill、Supervisors 的竞争式编辑器；
- 手工版本、模板、spawn limit、Artifact producer/consumer、raw JSON/Python 普通字段；
- “查看服务端日志”作为用户错误恢复。

### 验证与退出

首次使用者只按 UI/SDK 文档完成一个 Copilot；可访问性、中文术语、一致状态、保存冲突、
失败恢复和长任务取消通过产品 E2E。

## 9. W6：Assignment Grant 与能力可见性

### Owner

Assignment Service 编译；Codex Runtime 执行 Agent 生命周期；Tool Runner 执行 grant。

### 合同

```text
AssignmentGrant
  assignment_id, run_id, agent_release_id
  objective
  read_set[], write_set[]
  exact_capabilities[]
  expected_outputs[]
  parameters_snapshot
  time/cost/context budgets
  completion_criteria
```

### 实施

- Platform 根据 Task、installation snapshot、Supervisor selection 与授权生成 Grant；
- 模型只接收业务目标和有界引用，不接收可伪造 scope/authorization；
- Runtime `spawn_agent` 继续是唯一 spawn owner；Platform 不根据自然语言自动 spawn；
- Agent 只能发现 Grant 中的 exact Tool；Tool Runner 再校验 assignment identity；
- `CollaborationContextBuilder`/`AssignmentCompiler` 接入真实 Run，不再只是 library 类型。

### 删除

- Prompt 注入 Run/Work State/MCP/server/tool 内部 ID；
- 根据 Agent 名、Tool 名或 assignment 文本恢复权限；
- capability template 和 unbounded root capability inventory。

### 验证与退出

正常、未授权 Tool、错误 Agent Release、过期 installation、越权 read/write、重复 assignment
和 restart replay 均有 typed result；未授权 Tool 对模型不可见。

## 10. W7：ToolOutcome、Work State 内部事务与 Run Completion

### Owner

Tool SDK/Runner + Work State Service + Run Orchestrator。

### 合同

```text
ToolOutcome
  succeeded | needs_input | failed | rejected
  cancelled | timeout | interrupted
  summary, typed references, diagnostics

RunCompletion
  awaiting_children | awaiting_synthesis
  succeeded | partial | failed | rejected
  cancelled | timeout | interrupted | orchestration_incomplete
```

### 实施

- SDK 根据 Assignment Grant 内部 begin/commit Work State operation；
- write-set、expected schema、idempotency 和 revision 由服务端校验；
- Agent 只返回 outcome，不调用 begin/apply/fail 通用 mutation；
- child `needs_input` 是当前 assignment 终态；Root 使用官方输入，回答后新建 assignment；
- required assignments terminal 且 deliverable gate 满足后进入 `awaiting_synthesis`；
- Root 过早结束时最多一次通用、有界、幂等 synthesis recovery；
- Event Projection 只持久化/广播幂等视图。

### 删除

- 模型可见 `begin_work_operation/apply_work_state_mutation/fail_work_operation`；
- Supervisor continuation 的领域 Prompt 和 `supervisor_run_continuations`；
- Data Intake gate 的全局 Turn interrupt；
- Projection 中任何 Agent/Tool/领域 schema 调度分支。

### 验证与退出

成功、重复提交、CAS 冲突、child input、Root answer、失败、取消、超时、interrupt、Root
提前结束、刷新和重启都收敛；重复 deliverable 不再由模型处理。

## 11. W8：Typed handles 与 Resource Broker

### Owner

Data/Artifact authorization services + Resource Broker。

### 合同

```text
SourceAssetRef      { type, handle, revision, scope }
DatasetReleaseRef   { type, handle, release, scope }
DomainResourceRef   { type, handle, schema, producer, scope }
ArtifactRef         { type, handle, schema, retention, scope }
```

内部 MCP URI 不属于公共合同。

### 实施

- handle 是 opaque identity；Resolver 绑定当前 assignment、scope、grant、状态和 schema；
- Data Agent 只获得 SourceAsset/Dataset preparation 能力；Domain Agent 不获得 SourceAsset；
- Domain Tool 直接接受 DatasetReleaseRef/DomainResourceRef；
- 大型内容保持在 owner store，Agent 消息只含 handle 与有界摘要；
- hash 用于完整性，不用于授权或要求模型拼装。

### 删除

- `source_ref` 与 MCP Resource URI 的字符串方言；
- Agent 构造 `DataAgentRef` URI；
- Work State 的无 discriminator `{owner,type,id,hash}` 公共引用；
- `read_mcp_resource` 作为 Workspace SourceAsset 读取路径。

### 验证与退出

错误 variant、跨 Task/Profile、过期 revision、错误 schema、猜测 handle、hash 替换和越权
consumer 全部在 Resolver 拒绝；Prompt 文本无法扩大访问。

## 12. W9：Data Intake 单一 owner

### Owner

Platform Data Intake；Domain Package 只提供 requirement/validator/normalizer。

### 实施

```text
SourceAsset revision
-> DataIntakeSession
-> SourceProfile
-> MappingRevision
-> explicit confirmation / needs_input
-> immutable DatasetRelease
```

- 把 route 中的生命周期、SQL 和状态迁入 owning service；route 只鉴权/解析/调用；
- Domain Package 注册 DataRequirementContract、业务字段、单位、validator 和 normalizer；
- 映射只有一个生成 schema；不接收 camel/snake、entities/mappings 等多种方言；
- Dataset Release 通过 typed handle 绑定 Work State component；
- 用户确认由 Root-only 输入链完成。

### 删除

- CaseRepository 的 source/mapping/revision；
- 领域 Workspace 扫描、source profile store 和 mapping state machine；
- Data Server canonicalizer/aliases 和文件名反推 source ref；
- `data-intake-service` 只是校验、route 成为真正 service 的倒置结构。

### 验证与退出

CSV/JSON/XLSX、安全上限、映射歧义、用户确认、revision 失效、发布、重启、并发和越权
通过；供应链只看到 DatasetReleaseRef。

## 13. W10：仓网迁移与旧基础设施删除

### Owner

Supply Chain Domain Package + Platform migration owner。

### 实施

- 保留仓网需求 schema、行政区匹配、矩阵、覆盖、成本、场景、求解、报告和地图 renderer；
- Data Agent 使用 Data Intake；Network Agent 使用 DatasetReleaseRef/DomainResourceRef；
- 使用 W6/W7 Assignment/ToolOutcome/Run completion；
- 当前 coverage、成本、场景、选址、地图按业务请求选择，不写入平台；
- 完成一条原子切换，不维护旧/new 两套运行模式。

### 删除

- CaseRepository 的通用 identity/revision/operation/dependency/readiness/deliverable；
- `case_sources`、mapping tables、Workspace intake；
- 旧 Resource/template/snapshot/planning-dataset paths；
- `network-case-tool-result.v1` 和 Platform 中仓网分支；
- 6.0 手工包/hash、长 Prompt ID/顺序/字段合同；
- 为旧 E2E 保留的 hidden route、seed 和 fallback。

### 验证与退出

从 Copilot Studio 安装后的真实仓网任务覆盖基础、current coverage、成本、场景、选址、
地图、失败、取消、刷新和 Profile 重启；Platform 源码不含仓库/路线/成本/供应链分支。

## 14. W11：产品级 E2E、观测与证据

### Owner

Platform E2E + Provider Observability + Browser UX。

### 实施

- E2E 从干净 Profile 的“新建 Copilot”开始；
- 长任务持续展示 Agent、当前阶段、待输入、已产出、失败和取消；
- 每次 Provider call 保存有界 input/cache/output/tool-schema/latency/compaction 指标；
- 稳定前缀 fingerprint 用于解释 cache，不保存 Prompt 正文；
- evidence 自动核对 Release、Installation、Discovery、assignment terminal、Artifact hash；
- UI 与 capability baseline 不从模型答案推断成功。

### 验证与退出

正常、失败、取消、超时、断网、刷新、Runtime/Profile restart、乱序和重复事件有真实
evidence；12 分钟任务不表现为黑盒等待。

## 15. W12：第二领域否证与 M3 入口

### Owner

第二 Domain Package 团队 + Architecture review。

### 实施

- 由不熟悉仓网内部实现的算法工程师，仅使用正式 SDK 文档和 Web 创建第二 Copilot；
- 只新增 package、Skills、Agents、Supervisor、domain schema/validator/algorithm/renderer；
- 复用 Catalog、Installation、Assignment、input、Work State、handles、execution、Artifact
  和 evidence；
- 记录实际开发时间、平台变更数、失败类型和上下文成本。

### 退出

第二领域不修改 Platform 领域分支即可完成 E2E。若必须新增按领域名/Tool 名/schema 的
平台判断，返回 W2-W9 修正抽象，不以特例放行。通过后才进入 M3 两用户隔离矩阵。

## 16. 原子删除清单

以下不是“以后再清理”的建议，而是对应新 owner 退出门的一部分：

| 新 owner 通过 | 同一切换必须删除 |
| --- | --- |
| W2 Catalog/Compiler | supervisor-catalog 第二 owner、templates、手工版本/hash/seed compiler |
| W4 Installation | Workspace install、false installed/readiness、path-scanned discovery |
| W5 Studio | Settings 竞争入口、raw JSON/Python 普通编辑、手工内部字段 |
| W6 Assignment | Prompt 内部 ID/权限/能力注入、名称式角色选择 |
| W7 ToolOutcome/Completion | 模型低层 mutation、Projection continuation/interrupt |
| W8 handles | source_ref/URI 方言、Agent URI 构造、hash-as-authority |
| W9 Data Intake | Case source/mapping 生命周期、Data aliases/Workspace scan |
| W10 warehouse | Case 通用工作状态、平台供应链分支、旧 E2E hidden path |

删除包含代码、migration/seed、tests、fixtures、UI、tutorial 和 current-state 文档；不留
deprecated 字段或 runtime compatibility reader。

## 17. 统一完成定义

一个工作包只有同时满足以下条件才完成：

1. owner、typed input/output、capability gate、persistence scope 和状态机明确；
2. 正常、拒绝、失败、取消、超时、重启、并发和越权中适用场景有证据；
3. 旧 owner、旧 API、旧字段、fallback、fixture、UI 和文档已删除；
4. Browser DTO 不泄露 raw Runtime、路径、Secret 或内部 identity；
5. Runtime capability 由 discovery 证明，不由数据库或文件存在猜测；
6. E2E 从 owning 用户入口开始，并生成脱敏 evidence；
7. 受影响 Rust/Web/Python 检查、`git diff --check` 和真实边界验证通过；
8. Capability Baseline 只在证据完成后更新；
9. 没有新增未获批准的 `codex-rs` 差异。

## 18. 当前执行顺序

```text
W0
-> W1
-> W2 + W3
-> W4
-> W5
-> W6 + W7
-> Clean Spine A gate
-> W8
-> W9
-> W10
-> W11
-> W12
```

W8-W10 不得反向阻塞无数据 Clean Spine A；W10 通过前不继续扩展仓网功能；W12 通过前
不宣称平台具备通用算法工程师自助扩展能力。
