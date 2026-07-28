# AGENTS.md 合规与仓网规划专项审计报告

> 审计快照，不是新的项目事实权威。代码、生成合同、可复现测试及
> `docs/capability-baseline.md` 仍是当前能力判断的权威来源。本报告记录
> 2026-07-28 对提交 `0bab11e855` 的只读审查结果，供整改排序和验收使用。

## 1. 结论

当前项目**不能判定为符合最新 `AGENTS.md`，也不能把现有仓网案例提升为可信、灵活
的仓网规划能力**。

固定 happy path 确实可以运行：仓网 Python 测试 15/15 通过，两个 MCP 的真实 stdio
smoke 通过；Web 177 个测试文件、1,237 个测试和全部启用的 Platform Rust 测试也通过。
但是这一事实只证明了一个由 6 条订单、3 个需求点、2 个现有仓、杭州/无锡 2 个固定
候选仓、12 条手写路线和 100 个需求单位组成的 fixture。它没有证明输入同源、真实
路线、业务数值正确性、执行预算、取消恢复、多数据集适配或跨用户隔离。

本次共登记：

- **P0：5 项**。公共/反代部署或任何真实业务试用前必须阻断。
- **P1：8 项**。扩大数据、启用多用户或把固定案例泛化前必须修复。
- **P2：4 项**。紧随核心边界整改，避免继续累积错误能力声明和维护风险。

最危险的五个问题是：

1. 匿名调用 `/api/sessions/local` 可直接取得首个 Owner 的 7 天 Session；
2. governed Supervisor 可沿默认 full-history fork 绕过 exact Role allowlist，重启/
   resume 后按 Role 实例计数也会丢失；
3. MCP 错误与 Artifact content 可把原始输入值、本地路径和内部 Resource URI 送入
   模型、日志或浏览器；
4. Data Artifact、规划 Snapshot 和路线矩阵之间没有机器可验证的 provenance，手写
   fixture 可以伪装成 `navigation`；
5. Planner 缺少数组、输出、CPU 时间、并发、取消和复杂度预算，现有“bounded”
   能力声明不成立。

因此建议把当前状态明确降级为：

> 单 Profile、本机、固定 fixture 的实验性多 Agent 仓网演示；不得用于公共部署、
> 多用户环境或真实仓址投资决策。

## 2. 审计范围与方法

### 2.1 适用规范

已完整复核：

- 根 `AGENTS.md`；
- `apps/web/AGENTS.md`；
- `codex/AGENTS.md`；
- `codex/codex-rs/tui/src/bottom_pane/AGENTS.md`。

重点映射的规范包括：单一事实源、typed/general 扩展、异步稳定身份及全部终态、范围
正确的持久化、模型可见输入输出有界、当前阶段的授权门禁、浏览器不得接收路径/内部
协议、Runtime 能力只能来自正式发现、Patch Map 约束、失败/恢复/并发验证，以及当前
事实文档不得把目标写成现状。

### 2.2 代码范围

本次覆盖：

- Browser WebApp、Platform Server、授权、Session、WebSocket、Artifact、Profile
  Host 和相关合同；
- `tools/supply-chain-network-planner` 的 Skill、Plugin/MCP 配置、数据边界、模型、
  Solver、Resource Store、测试和运行脚本；
- Enterprise Supervisor Policy、两个 Agent Definition、Runtime Role 物化和 E2E；
- 与 Agent Role governance、MultiAgent V2 spawn/resume 直接相关的 retained Codex
  seam；
- 当前架构、安全、能力基线、开发计划、仓网教程和 CI。

`codex/` 大部分是上游代码；本报告没有把 5,000 多个上游文件重新做逐行安全审计，
而是审查 Patch Map 声明的本地 seam、项目调用路径及与仓网/Supervisor 直接相连的
Runtime 路径。这是“项目自有代码和本地 Codex 差异的全面审查”，不是对整个
`openai/codex` 上游的重新认证。

### 2.3 严重级别

| 级别 | 含义 |
| --- | --- |
| P0 | 可导致 Owner 接管、治理边界绕过、Secret/PII 泄漏、服务不可用或真实业务决策建立在错误输入上；必须先阻断 |
| P1 | 当前受单用户/固定 fixture 限制，但扩大范围后会导致越权、错误决策、恢复不一致或 CI 假绿 |
| P2 | 主要是诊断、验证深度、provenance 和文档事实漂移；不应继续累积 |

## 3. 风险与修复优先级总表

| 顺序 | 级别 | 发现 | 主要危害 | Owning layer |
| --- | --- | --- | --- | --- |
| 1 | P0 | 匿名本地 Session 等价于 Owner 认证绕过 | 公共/反代部署被完全接管 | Platform Auth/部署 |
| 2 | P0 | exact Role allowlist/instance limit 可被 fork、restart、resume 绕过 | governed Agent 树和能力声明失真 | Codex Runtime retained seam |
| 3 | P0 | MCP 错误和 Artifact content 泄漏输入、路径、内部 URI | Secret/PII、服务器布局和内部协议泄漏 | MCP + Platform Artifact |
| 4 | P0 | Data→Snapshot→Route 无强 provenance | 错数据或伪造路线仍可通过 E2E 并产出投资建议 | Planner contracts + Policy/E2E |
| 5 | P0 | Planner 无完整执行/输出预算 | CPU/内存/上下文耗尽，无法取消或恢复 | Planner MCP |
| 6 | P1 | 同组织成员只有 Organization 粒度的 REST/WS 授权 | 无 Workspace/Project grant 仍可读写任务和实时输出 | Platform AuthZ/Event |
| 7 | P1 | 7 天 Session Token 暴露给 JS，Secure 配置未闭合 | XSS 可窃取 Owner Token，HTTPS 反代 Cookie 可降级 | Session/Browser |
| 8 | P1 | 不完整 RouteMatrix 被验证为 valid | 输入缺失被误判为网络不可行或低覆盖 | Planner validation |
| 9 | P1 | float、容差、魔数和单位/周期语义不可靠 | 100% 目标、极大成本、现状覆盖口径可被误判 | Planner model/solver |
| 10 | P1 | Resource 身份不幂等且写入非原子 | 重试产生重复 Artifact，崩溃可留下永久损坏资源 | Resource Store |
| 11 | P1 | Role、Policy、fixture 和平台目录硬编码 | 无法不改代码地运行另一地区/候选集/目标 | Extension/package lifecycle |
| 12 | P1 | `tools/**` 不进 CI，Enterprise E2E 存在假绿 | Planner 变更可绕过测试，错误指标仍通过 | CI/E2E |
| 13 | P1 | `check:main-ui-parity` 当前失败 | Web 最小差异门禁为红，交付状态不能声明完成 | Web delivery |
| 14 | P2 | 部分路由把原始 SQL 错误返回浏览器 | 暴露表、约束或内部实现细节 | Platform diagnostics |
| 15 | P2 | Validator 未重算业务不变量，zero-demand 语义冲突 | 结构合法不等于结果可信，同一数据先 valid 后 invalid | Planner validation |
| 16 | P2 | 结果未绑定 Planner engine/objective 版本 | 算法升级后同 Schema/Policy 被误当同口径 | Planner provenance/Policy |
| 17 | P2 | 当前事实文档和上游状态漂移 | 已知目标被写成已验证能力，风险被错误关闭 | Documentation |

## 4. P0 详细发现

### P0-1 匿名 `/api/sessions/local` 可取得 Owner Session

**违反规范**

- 浏览器入口必须经过真实认证/授权链；
- Secret/Session 不得因部署拓扑变化而失去边界；
- 单用户阶段不能被解释为公共端点的匿名 Owner 登录。

**证据**

- 路由始终挂载：`apps/web/server/src/routes/mod.rs:83-88`。
- Handler 没有认证、RemoteAddr、Origin、部署模式或一次性 bootstrap Secret 检查，
  直接选第一条 Owner membership、创建 7 天 Session，并在 Cookie 和 JSON 中返回
  token：`apps/web/server/src/routes/sessions.rs:24-93`。
- Server 默认 `127.0.0.1`，但 CLI 和部署脚本允许任意 bind：
  `apps/web/server/src/main.rs:42-44`、`scripts/deploy.sh:88-92,738-749`。
- `docs/mvp-runbook.md:64-67` 推荐在 `127.0.0.1:4800` 前放 HTTPS 反向代理。普通
  反代会把该端点一并暴露；仓库没有 route ACL 或可信代理模式。
- 现有安全测试反而断言匿名 local Session 创建成功：
  `apps/web/server/src/security_integration.rs:137-157`。

**利用条件与危害**

在默认仅本机且没有代理时，风险受 loopback 限制；一旦按运行手册配置反代，或将 bind
改为非 loopback，任意网络调用者都可执行：

```text
POST /api/sessions/local
Authorization: Bearer <response.session_token>
GET /api/me
```

结果即首个 Owner 身份，可继续访问 Workspace、Thread、Provider、Secret 驱动的
Runtime 操作和仓网 Artifact。这是公共/反代部署的完整认证绕过。

**根因和最小修复**

Owner 是 Platform Auth 所有的身份，但“本机免登录体验”被实现为无条件公开的产品
API。应引入 typed deployment auth mode：

- `local_loopback`：只在明确模式下挂载，直接验证真实 peer 为 loopback；不得信任
  未配置的 `Forwarded`/`X-Forwarded-*`；
- `public`：完全不挂载 `/sessions/local`，使用正式交互认证或外部可信身份入口；
- 启动时发现 local mode + 非 loopback bind、public URL 或不兼容代理配置，应失败
  关闭；
- 已签发 local Session 在切换模式时撤销。

**验收**

- public/反代模式匿名请求固定返回 404/401；
- local mode 的非 loopback peer、伪造 Forwarded、DNS rebinding 场景均拒绝；
- 自动化覆盖 bind、代理、模式切换和 Session 撤销；
- 能力基线只在这些测试通过后声明可外部访问。

### P0-2 governed Agent 可绕过 exact Role allowlist 和实例上限

**违反规范**

- capability gate 只能来自 typed Runtime discovery，并必须与真实执行一致；
- 每个异步 Agent 操作必须有稳定身份及可恢复的容量语义；
- Patch Map 对 retained seam 的声明必须由隔离测试证明。

**证据**

- `fork_turns` 缺省为 `all`：
  `codex/codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs:261-280`。
- full-history 分支禁止显式 `agent_type`，并跳过 `apply_spawn_agent_role`：
  同文件 `:68-82`。
- 该分支仍用 `role=None` 构造 `ThreadSpawn`：
  同文件 `:92-98` 及
  `codex/codex-rs/core/src/tools/handlers/multi_agents_common.rs:107-130`。
- AgentControl 只有在 SessionSource 含 Role 时才查询和预留
  `agent_role_spawn_limits`：
  `codex/codex-rs/core/src/agent/control/spawn.rs:404-419`。
- 冷恢复使用普通 `reserve_spawn_slot(None)`，没有恢复 Role 计数：
  同文件 `:129-190`；rollout resume 同样使用普通 slot：`:839-920`。
- `docs/custom-codex-patch-map.md:68` 和
  `docs/capability-baseline.md:164` 当前声称 Runtime 精确执行 allowlist 和 per-role
  limit，与以上路径冲突。

**危害**

受治理的 root 可以省略 `agent_type`，沿默认 full fork 创建 Role 为空的 child；该
child 不属于 Policy 的 `data_agent`/`network_planning_agent` allowlist，也不占这两
个 Role 的实例配额。重启后，已恢复的 Role child 不计入 `role_spawn_counts`，可再次
spawn 同 Role；close/spawn/resume 组合也可能超过上限。

当前 happy-path 模型按 Prompt 主动提供 Role 且使用非 full fork，所以真实演示可以
“恰好产生两个 Agent”，但这不能证明不可绕过的 Runtime 边界。Manifest 的
`exactRoleAllowlist=true` 和 `exactRoleInstanceLimits=true` 因此是错误能力声明。

**根因和最小修复**

Role 强制被分散在 tool handler 的某个分支，而容量恢复只重建 Agent metadata，不重建
其计数。修复必须进入 Runtime owning layer：

- 当 `agent_allowed_roles` 存在时，每个 spawn 路径都必须解析出一个允许的 Role；
- governed root 应拒绝 roleless/default spawn；full fork 要么禁止，要么用明确、
  允许且实际应用的继承 Role；
- Role 验证和 slot 预留集中在 AgentControl，不能由 handler 分支选择性跳过；
- restore/resume 必须按持久化 Role 原子恢复 Role slot；重复恢复保持幂等；
- Runtime 在修复前不得宣称 exact capabilities。

**验收**

- 对省略 Role、unknown Role、`fork_turns=all/none/N` 分别做拒绝/允许矩阵；
- 对并发 spawn、失败回滚、close、冷重启、metadata restore、rollout resume 做
  `limit=1` 测试；
- 实际 app-server governed Thread 尝试第三个/重复/roleless child 必须产生 typed
  terminal rejection；
- Web preflight 和 Capability Manifest 回归同时通过。

### P0-3 MCP 错误与 Artifact content 跨越模型/浏览器边界

**违反规范**

- Secret、PII、本地路径和内部 Resource URI 不得进入浏览器、模型可见诊断或不安全
  日志；
- Browser 只能接收 stable、bounded 的平台 DTO，不能原样透传 Runtime/MCP payload；
- 工具错误必须安全、结构化且保留服务端可观测性。

**证据**

Planner 文件和错误面：

- Planner 默认把插件根作为数据根：
  `tools/supply-chain-network-planner/supply_chain_planner/server.py:59-63,499-518`，
  `.mcp.json:29-43`。
- `prepare_network_snapshot` 接受模型提供的 `source_path`，可读取该根下任意 JSON：
  `server.py:138-170`。
- Data MCP 和 Resource Store 直接让 Pydantic/I/O 异常穿过 MCP：
  `data_server.py:95-113`、`resource_store.py:55-77`。
- 实际 stdio 复现中，读取 `.codex-plugin/plugin.json` 的错误回显插件名和
  `./.mcp.json`；在订单中加入 canary 客户名后，Data MCP 错误原样包含该值；读取
  不存在 Resource 的错误还包含绝对 Profile Resource 路径。

Artifact 浏览器面：

- `network_snapshot.v1` 含 `source_name`：
  `supply_chain_planner/models.py:338-342`。
- `current_coverage_result.v1` 和 `facility_location_solution.v1` 嵌套内部
  `DataRef`：同文件 `:410-445`。
- Platform materializer 原样持久化 MCP bytes：
  `apps/web/server/src/routes/artifacts.rs:247-280`。
- `/api/artifacts/{id}/content` 只检查 MIME、128 MiB 上限和 JSON 语法，然后原样返回：
  同文件 `:96-143`。
- Enterprise E2E 确实下载 `contents`，但“不含内部 URI”的 `safeTrace` 没有把
  `contents` 纳入检查：`apps/web/scripts/enterprise-supervisor-e2e.mjs:676-700`。

**危害**

畸形或包含敏感值的企业数据可通过验证错误进入 Agent 上下文、Runtime event 或日志；
已授权浏览器可以读到 `examples/network-input.json` 和 `supply-chain://...` 等规范
明确禁止公开的内部引用。再叠加 P1-1 的同组织授权缺口，暴露范围不止真正的数据 Owner。

**根因和最小修复**

- governed Planner 删除 `source_path`；只接受 Platform/Task 绑定的 typed case/source
  身份或授权 Artifact；
- Data MCP 只绑定专用数据目录，不以插件/Workspace 根作为默认数据边界；
- 捕获 ValidationError/I/O 错误，返回带 code、count、bounded field path 的安全
  diagnostics；原始值和绝对路径只进入经过脱敏的服务端日志；
- Artifact materialization 按 schema 生成外部 DTO，把嵌套 MCP ref 转成授权 Artifact
  ID/URL，把来源改成 opaque source ID；不得用“它是合法 JSON”替代外部合同。

**验收**

- 用 canary Secret/PII 构造 malformed source，MCP result、event、Server log、Artifact
  content 和最终报告均不得包含 canary；
- 不存在 Resource 的错误不含绝对路径；
- 下载全部仓网 Artifact，断言不含 `supply-chain://`、
  `supply-chain-data://`、绝对路径或 `examples/...`；
- 净化后内部依赖仍能通过授权 Artifact ID 解析。

### P0-4 Data、Snapshot 与 Route 没有可信 provenance

**违反规范**

- Data Agent Artifact 应是下游规划的单一事实源；
- Prompt 只能是防御纵深，不能承担数据授权或业务 provenance；
- typed Artifact 交接必须可由 owning layer 验证，而不是用调用顺序推断。

**证据**

Data→Snapshot：

- `prepare_network_snapshot` 只接受任意 inline `NetworkInput` 或 `source_path`，不接受
  `DataAgentRef`：`server.py:154-170`。
- `NetworkSnapshot` 只有 `source_name`，没有 `planning_dataset_ref`、
  `source_digest` 或 candidate overlay digest：`models.py:338-342`。
- Network Role 先要求读 Data Resource，随后又要求使用固定
  `examples/network-input.json`：
  `examples/runtime-roles/network-planning-agent.md:2-7`。
- E2E 只断言 Resource read 发生在 snapshot call 之前，没有比较内容：
  `enterprise-supervisor-e2e.mjs:567-601`。
- 动态复现中，Data Artifact 为 100 units、Snapshot 改为 1,059 units，仍能正常
  创建，合同中没有任何关联字段。

Route：

- Skill 要求 `map_utils.batch_geocode` 和 `map_utils.distance_matrix`：
  `skills/prepare-network-baseline/SKILL.md:22-34`。
- Network Definition 没有声明 map capability，只声明 Planner：
  `apps/web/server/resources/agent-definitions/network-planning-agent-v1.5.json:26-40`。
- Role 生成器只启用 Definition 的精确 allowlist：
  `apps/web/server/src/agent_definition.rs:317-367`。
- E2E 从本地 fixture 读取 12 条路线并整段放进 Prompt：
  `enterprise-supervisor-e2e.mjs:35-43,380-391`。
- `register_route_matrix` 完全信任模型提交的 provider、`method="navigation"` 和 rows，
  没有 provider request ID、生成时间/有效期或签名来源：
  `server.py:188-242`。

**危害**

Agent 可以先读正确 Artifact，再使用另一份网络输入；也可以把手写路线标成真实
navigation。现有 E2E 仍会看到正确的调用顺序、Schema 数量和报告格式并通过。仓址、
成本和覆盖率结论因而可能建立在未经授权或人为调参的数据上。

**根因和最小修复**

Planner 的输入合同所有权错误地落在模型和本地文件，而不在 Artifact/case contract：

- 定义 `planning_case.v2`：绑定上游 dataset Artifact ID/digest、候选 overlay
  Artifact ID/digest、规划周期、单位、目标和约束；
- Snapshot 创建时 canonical 比对基线字段；任一不一致返回 typed
  `source_mismatch`；
- 路线二选一：给 Role 精确增加有预算的 map capability，并记录 provider
  provenance；或只接受受治理的 `quoted_route_matrix` Artifact。fixture 必须明确标
  `test_fixture/quoted`，不能自称 navigation；
- E2E 校验内容 digest 和关键不变量，不再把 route rows 放入 Prompt。

**验收**

- 修改需求、现有仓、币种、周期或 SLA 任一字段，Snapshot 创建都拒绝；
- Network 实际读取的 Resource URI/Artifact ID 与 Data 输出完全一致；
- navigation 模式必须存在真实 map tool call 和 provider provenance；
- 缺凭据、模糊地理编码、部分路线失败和过期路线有 typed 终态；
- 两个完全不同的数据集无需改代码/Role/Prompt 即可运行。

### P0-5 Planner 没有完整的执行、输入和输出预算

**违反规范**

- 模型可见输入/输出必须有稳定上限；
- 异步操作必须有 operation identity、成功、失败、拒绝、取消、超时和中断终态；
- 关键路径不得通过重复全扫描或无界枚举扩张。

**证据**

- 只有 20 MiB 文件上限、100 个 catalog 条目和 14 个候选仓上限：
  `data_server.py:31-33`、`core.py:28-29,551-565`。
- `NetworkInput`、PlanningSource、RouteMatrix、allocation、issues 和
  ValidationResult 数组都没有 `maxItems`：
  `models.py:90-98,201-207,282-329,362-369,395-407,453-459`。
- Solver 同步枚举最多 `2^14` 个候选子集，每个子集重新运行 min-cost flow：
  `core.py:568-632`；没有 deadline、cancellation、semaphore、预算或高成本审批。
- 14 个候选、10 个需求、不可行目标的动态复现枚举 16,384 个子集，耗时
  **14.274 秒**。
- 20 个设施 × 500 个需求的不完整矩阵产生 10,000 条 issues；验证结果扩为 10,000
  warnings，结构化 JSON 为 **283,051 bytes**。
- 200 个设施 × 1,000 个需求 × 200 个 rates 的引用验证做约 4,000 万次扫描，动态
  复现耗时 **1.439 秒**。
- `list_planning_sources` 最多会顺序加载 100 个各 20 MiB 的完整 JSON：
  `data_server.py:168-176`。

**危害**

模型或畸形数据可耗尽 Profile Host 的 CPU/内存并把数十万字节诊断塞入模型上下文；
调用无法可靠取消，重启后也没有稳定 operation identity 可恢复。单用户环境同样会被
阻塞，因此这不是只在未来多用户阶段才成立的风险。

**根因和最小修复**

- 在 MCP schema 和 Server owning boundary 同时设置 item、总字节、设施、需求、
  OD pair、diagnostic、金额和整数范围；
- 预估复杂度，在执行前返回 typed `rejected_budget`，而不是运行后截断；
- 将 rate lookup 建索引，避免 `D × F × R`；
- 计算进入有 stable operation ID 的有界 worker，支持 deadline、cooperative
  cancellation、per-Profile semaphore、审批拒绝和 restart recovery；
- 模型只接收 `total_count + first N + truncated + continuation/artifact_ref`。

**验收**

- 最大允许规模和超一条边界；
- timeout、cancel、approval reject、两调用竞争、Server/Profile restart；
- 每种路径都有唯一 operation ID 和明确终态；
- 输出上限同时覆盖成功、验证错误和异常路径。

## 5. P1 详细发现

### P1-1 同组织内缺少 Project/Task/Workspace 粒度授权

规范授权链要求 Session→User→Organization membership→Project permission→Workspace
grant→Task/Thread→Run/Event→Artifact。当前 schema 没有 Project permission 或
user-task grant；大量路由只检查 `organization_id`：

- Project list/get/thread contexts：
  `apps/web/server/src/routes/projects.rs:20-45,83-109,220-249`；
- Task list/create/read/event/model mutation：
  `routes/tasks.rs:33-115,129-266,490-526`；
- Run list/get：`routes/runs.rs:68-110`；
- Runtime Agent tree/activity：`routes/runtime_agents.rs:595-616`；
- Supervisor binding：`routes/supervisor_policies.rs:21-63`；
- Artifact list/get/content：`routes/artifacts.rs:26-143,331-379`。

WebSocket 的 `LiveEvent` 只有 Organization + payload，转发条件也只有 Organization：
`apps/web/crates/platform-store/src/lib.rs:20-26`、
`apps/web/server/src/routes/events.rs:38-67`。终端事件会包含 workspaceId、terminalId
和 data：`event_projection.rs:335-460`。

相比之下，Workspace 文件和 authoritative Thread history 已检查 workspace grant 或
requested_by，导致同一用户在“实时流/读模型”可见、在权威历史却 404 的不一致。
现有安全测试只覆盖跨组织，不覆盖同组织无 grant。

当前单用户产品入口限制了实际暴露，但这是启用多用户前的明确 P1 gate。修复应由
Platform AuthZ 提供统一 typed decision；LiveEvent 必须携带可检查的
project/task/workspace/profile/user scope。同组织无 grant 的读、写、WS、Artifact 和
并发矩阵必须全部拒绝。

### P1-2 HttpOnly Session 同时暴露给 JavaScript，Secure 配置未闭合

- Server 把 token 放进 JSON：
  `apps/web/server/src/routes/sessions.rs:71-92,181-196`；
- Browser 写入 sessionStorage：
  `apps/web/browser/browser-entry.ts:58-60,91-106`、
  `apps/web/browser/session.ts:3-18`；
- REST 使用 Bearer，WS 首帧复用同一个 7 天 token：
  `apps/web/browser/client.ts:84-93,1019-1026`；
- Cookie 只有显式 `OPEN_WEB_CODEX_SECURE_COOKIES=true` 才加 Secure：
  `sessions.rs:221-247`；deploy/runbook 没有闭合该配置。

这使 HttpOnly 失去主要价值：任意同源 XSS 或恶意依赖都可读取长期 Owner token。
修复为 REST cookie-only；WS 在 upgrade 验证 Cookie，或签发单次、短时、绑定 Session
的 WS ticket。public/HTTPS 模式必须强制 Secure、可信代理配置和 HSTS；本机 HTTP
模式可通过显式模式例外。

### P1-3 不完整 RouteMatrix 会被验证为 valid

`register_route_matrix(require_complete=false)` 可以发布不完整矩阵，但该标志、
expected count 和 missing count 没进入 `RouteMatrix`：
`server.py:188-228`、`models.py:362-369`。Validator 只检查重复 pair：
`server.py:470-477`。

动态复现：示例 Snapshot 注册 0/12 routes，随后
`validate_network_resource` 返回：

```text
valid=true, errors=[], warnings=[]
```

下游优化会把缺路线当成不可覆盖，而不是无效输入。修复应持久化 completeness、
expected/unreachable/missing counts 和 provenance；所有声称 optimal 的工具必须拒绝
diagnostic/incomplete matrix。验收覆盖 0/12、11/12、重复、unknown 和真实
unreachable。

### P1-4 数值精度、极值和业务单位可产生错误结论

- Coverage 是 `float`：`models.py:384-392`；
- Solver 用 `ratio + 1e-12 >= target`：`core.py:615`；
- Validator 使用同类容差：`server.py:478-483`。

总需求 `10^18`、未覆盖 1 单位时，ratio 被舍入成 `1.0`，`target=1.0` 动态返回
`feasible=true`。此外：

- 金额没有整数位/指数业务上限，极大 Decimal 可触发 `InvalidOperation`；
- 最短路把 `10**30` 当无穷，合法的极大成本可提前停止：
  `core.py:323-346`；
- Schema 没有 demand unit、capacity cadence、fixed-cost period 或 forecast
  transform，只有自由文本 planning period；
- fixture 订单覆盖 2026-06-01 至 06-29，却声明 “2026 annualized planning units”；
  aggregator 只是原样求和；
- “actual current coverage” 是静态路线/当前分配模型的 75%，不是观察履约 55/90；
  超容量仅进入 issues。

修复应使用有界整数分数/Decimal 或交叉乘法判断覆盖目标，去掉魔数无穷，给数量和金额
业务范围，并显式建模单位、时间粒度、容量/成本周期和 forecast provenance。报告必须
并列区分 observed on-time ratio 与 modeled current-assignment SLA coverage。

### P1-5 Resource 身份不幂等，写入不具备崩溃一致性

PlanningDataset、Snapshot、Matrix 和 Scenario 都把当前时间写入 payload：
`models.py:290-292,340-342,368-369,407`。`ResourceStore.publish` 对整个 payload
求 hash：`resource_store.py:34-47`。

动态复现中，同一输入的 `dataset_id`/`snapshot_id` 相同，但 Resource ID 不同。重试
因此产生重复 Resource/Artifact，违背计划文档“同一输入产生相同工具结果”。写入又是
`if !exists -> write_bytes`，无原子 rename、fsync 或 existing digest 校验；预先放入
同名截断文件后 publish 仍报告成功。

修复应把观察时间移到不参与 semantic content address 的 metadata，或绑定稳定
operation identity；使用临时文件、fsync、原子 rename/exclusive create，并核验已存在
内容。验收覆盖同 idempotency key 重试、并发 publish、写半崩溃、磁盘满和重启。

### P1-6 当前仓网能力在 Role、Policy、fixture 和平台代码中硬编码

- Data Role 固定 `source_id=warehouse-network-fixture`：
  `examples/runtime-roles/data-agent-v1.1.md:5`；
- Network Role 固定 `examples/network-input.json`、杭州和无锡：
  `network-planning-agent.md:6-7`；
- Platform 用 `include_str!` 编译两个确切 Definition 和 Role 文件：
  `apps/web/server/src/agent_definition.rs:10-42`；
- Policy 固定先 Data、后 Network 的仓网流程：
  `apps/web/server/resources/supervisor-policies/enterprise-supervisor-copilot-v1.7.md:3-13`；
- Data source 合同禁止候选仓，候选集改从第二份本地 example 进入；
- Solver 只允许所有现有仓保持开启、最多 14 个候选、目标为“最少新增仓，再最低
  成本”，不支持关闭/扩容、预算、多周期、SKU、多层网络、库存或韧性。

这同时违反 typed/general 扩展和“Skill/Plugin/MCP 由 Runtime 发现、平台不模拟隐藏
Profile 配置”的目标边界。短期不必先做通用 Studio，但必须：

1. 把 source、candidate overlay、目标、约束和 route 变成 Task 绑定的 typed
   Artifacts；
2. Role 只描述方法和责任，不包含业务 ID、城市或文件名；
3. 将仓网 Definition/Policy/Skill/MCP 作为 versioned seed package 发布，由通用
   Catalog/Publication 服务读取；Platform core 不继续新增领域 `include_str!`；
4. Solver backend 用 typed capability/limit 声明，不静默把 v1 exact solver包装成
   通用能力。

### P1-7 `tools/**` 不受 CI 保护，Enterprise E2E 会假绿

- `.github/workflows/web-ci.yml:3-17` 的 paths 不含 `tools/**`；
- workflow 没有 Python setup、Ruff、pytest 或 MCP stdio smoke；
- `pyproject.toml` 配置 Ruff，但 dev dependency 只有 pytest，当前 venv 也没有 Ruff；
- planner stdio smoke 只做 snapshot→validate，不执行 route/scenario/compare/solver：
  `tests/stdio_smoke.py:92-132`；
- 完整 Supervisor smoke 硬编码 zsh、`/private/tmp` 和 Homebrew PostgreSQL，当前
  Linux 无法运行：
  `scripts/smoke-enterprise-supervisor-copilot.sh:1,29,48-58,78-83`；
- Enterprise E2E 只检查工具曾调用、Schema 最少数量、报告包含任意 `%`/`CNY`，
  不检查 70/95/100、成本、candidate ID、source digest 或 validation `valid=true`：
  `enterprise-supervisor-e2e.mjs:603-743`；
- 它还自动接受本 Run 的任意 MCP elicitation：
  同文件 `:159-177`。

修复后，改一行 Planner 算法必须触发 CI；故意修改 route、dataset demand、expected
metric、source digest 或 validation status 必须让 E2E 失败。审批只允许精确
server/mode/stage/request，意外审批立即失败，并覆盖 reject/expiry。

### P1-8 Web UI parity 门禁当前失败

`npm run check:main-ui-parity` 实际失败，报告 18 个 tracked 文件与 reviewed/main
基线不一致，另有 4 个 extra 文件。`docs/development-plan.md:214-218` 已承认该门禁
尚未通过。

这不等于 22 个 UI 功能都错误，但意味着 `apps/web/AGENTS.md` 要求的“近似 main、
差异最小且已评审”交付证据没有闭合。应逐项分类为必要 Platform seam、可移出逻辑或
应删除差异，再更新经过评审的 parity baseline；不得简单放宽脚本使 CI 变绿。

## 6. P2 详细发现

### P2-1 原始数据库错误返回浏览器

`routes/tasks.rs` 至少 5 处、`routes/organizations.rs` 至少 7 处使用
`PlatformError::internal(format!("{e}"))` 或等价形式，例如：

- `apps/web/server/src/routes/tasks.rs:50,107,243,313,507`；
- `apps/web/server/src/routes/organizations.rs:34,130,150,186,208,243,271`。

这可能暴露表名、约束、SQL 类型和内部拓扑。外部响应应返回稳定 error code 和安全
message；完整 cause 只写入结构化、脱敏日志并关联 request ID。

### P2-2 Validator 未证明业务正确性，zero-demand 合同前后冲突

当前 Validator 没有完整重算：

- `total = covered + uncovered`；
- 每个 demand 流量守恒及 facility capacity；
- end-to-end SLA；
- `fixed + variable = total cost`；
- solution/result refs、selected candidates 与 metrics；
- comparison delta；
- RouteMatrix 对 Snapshot 的完整性和 content digest。

`_analyze` 又把无订单 demand node 只记 warning，使 dataset quality 为 valid；后续
dataset validator 却因 distribution 与 network_input ID 不一致返回 invalid。相同
事实在相邻阶段先 valid 后 invalid，违反单一合同语义。

Validator 应加载依赖图、校验 digest 并重算业务不变量；zero-demand row 要么显式进入
distribution，要么从 projection 排除并在合同中声明。

### P2-3 规划结果未绑定 engine/objective 版本

Policy digest 封装 Role 和 tool 名称，但 Planner 输出没有 engine build、
objective version、solver parameters 或 optimality contract。算法改变后，同一
Policy/Schema 可产生不同业务语义，却仍被标记为同口径。

Snapshot/Result/Artifact provenance 应记录 tool contract、engine build、objective
version 和关键 solver settings；Policy 或 Artifact provenance 必须绑定相应 digest。

### P2-4 当前事实文档和 Codex 上游状态漂移

已确认的冲突包括：

- `docs/capability-baseline.md:164` 声称 exact Role enforcement 已验证，P0-2 证明
  存在绕过；
- `docs/capability-baseline.md:178,183` 声称 Artifact/browser 不暴露内部 URI/路径，
  P0-3 证明 raw content 不满足；
- `docs/enterprise-supervisor-copilot-plan.md:124-129` 把 Role 边界和“bounded
  planner”写成当前事实；
- 同计划 `:254-255,457-460` 要求同输入可重复，P1-5 证明 Resource ID 不稳定；
- 仓网教程同时描述旧的手工 Agent Settings 路径和当前 request-scoped Role，且把
  目标 provenance 当成已实现合同；
- `docs/development-plan.md:10,16,190` 记录上游待同步 142，审计当日
  `scripts/codex-upstream-status.sh` 返回 171，official SHA 已变化。

修复代码前先把能力基线降级并登记风险；代码和真实证据通过后再恢复能力声明。上游
171 个提交不是自动缺陷，但必须按专用 sync branch 和 Patch Map 流程审查，不能在本
修复中顺手混入。

## 7. 从固定案例到灵活仓网规划

### 7.1 当前能力边界

| 维度 | 当前实现 | 为什么不灵活 |
| --- | --- | --- |
| 数据 | 一个固定 source，6 条订单、3 个需求点 | Role 写死 source ID；没有生产 adapter/case binding |
| 候选 | 杭州、无锡两个本地 example 候选 | 与 Data Artifact 分离，靠 Prompt 声称 baseline 相同 |
| 路线 | 12 条手写 JSON，标成 navigation | Role 没有地图能力，也没有 provider provenance |
| 时间/产品 | 单周期、聚合单产品需求 | 无 SKU、库存、forecast、峰值/增长/中断 |
| 网络 | 单层 facility→demand | 无工厂/区域仓/前置仓多层，无关闭/扩容 |
| 目标 | 最少新增仓，再最低成本，coverage target | 无预算、多目标、韧性、碳、服务分层 |
| 执行 | 同步 exact subset + min-cost flow | 最多 14 候选；无 timeout/cancel/gap/worker identity |
| 证据 | Schema 数量和报告格式 | 不校验 source/route digest、精确指标或业务不变量 |

### 7.2 推荐目标合同

先把可信输入和执行闭合，再扩展 Solver；不要直接在现有 v1 payload 上继续加可选字段。

**`planning_case.v2`**

- 授权 dataset Artifact ID + semantic digest；
- candidate overlay Artifact ID + digest；
- 规划 horizon、quantity/capacity/cost unit 和 currency；
- objectives、hard constraints、soft penalties、scenario set；
- route requirement 和允许的 provider/method；
- Platform 绑定的 Task/Profile/Workspace scope，不接受模型提交这些授权身份。

**`route_matrix.v2`**

- snapshot semantic digest；
- provider kind、request/batch identity、generated/valid-until 时间；
- expected、ready、unreachable、missing pair counts；
- approximation/quoted/navigation 的不可混淆 provenance；
- 完整性和预算状态。

**`planning_operation.v1`**

- stable operation/idempotency ID；
- queued/running/succeeded/failed/rejected/cancelled/timed_out/interrupted；
- input/output digests、engine/objective versions；
- deadline、complexity estimate、approved budget、attempt；
- bounded progress 和诊断 Artifact。

**结果合同**

- 每个结果引用输入、路线和 engine digest；
- 用精确整数/Decimal 保留业务守恒；
- 明确 `optimal/feasible/infeasible/time_limited/rejected` 和 optimality gap；
- observed facts、modeled baseline、forecast/scenario 分开表达。

### 7.3 分阶段修复顺序

#### Gate A：恢复不可绕过的安全和治理边界

1. 关闭公共 local Session，完成 cookie-only Session；
2. 修复 Runtime Role/fork/restart/resume 限制；
3. 移除 governed `source_path`，净化 MCP error 和 Artifact 外部 DTO；
4. 能力基线立即降级到实际状态。

#### Gate B：让固定案例变成可信基准

1. Data Artifact→PlanningCase→Snapshot digest 强绑定；
2. 路线改为真实 map capability 或明确 quoted fixture；
3. 完整矩阵、精确数值、单位周期和业务不变量验证；
4. Resource 幂等、原子写入和 operation lifecycle；
5. P0/P1 负向测试进入 CI。

#### Gate C：去除案例硬编码

1. Role 不再包含 source、城市、文件名和固定候选；
2. Task 选择 versioned PlanningCase；
3. 仓网包从 Platform core 移到 versioned seed package；
4. 不改代码连续运行至少两个不同地区/币种/候选集/目标的案例。

#### Gate D：扩展规划模型

在 `v2` 合同和 backend capability 后增加：

- 多周期、peak/growth/disruption/lifecycle scenarios；
- SKU/产品组、库存和 safety stock；
- 工厂—区域仓—前置仓—需求多层网络；
- open/close/expand、分段容量、预算和服务分层；
- 成本、覆盖、韧性、碳等可配置目标；
- 带 time limit/optimality gap 的 MILP/CP-SAT 类 backend 接口。

超出 backend 声明的规模必须 typed unavailable/rejected，不能静默退回当前 exact
solver 或让模型心算。

### 7.4 “灵活案例跑通”的最低验收

只有同时满足以下条件，才可称为灵活仓网规划：

1. 不修改代码、Role、Policy 或 Prompt 模板，运行两个不同 source、不同城市数量、
   币种、候选集和目标；
2. Prompt 中没有 raw dataset、candidate JSON 或 route rows；
3. 每个 Snapshot 与 Data Artifact digest 强绑定，任何 baseline 改动都拒绝；
4. navigation 有真实 provider provenance；quoted/estimate 明确区分；
5. 同一输入和 idempotency key 重试得到同一 semantic Resource/Artifact；
6. 最大规模、超限、timeout、cancel、approval reject、restart 和并发都有明确终态；
7. Validator 重算需求、容量、SLA、成本和比较差异；
8. E2E 校验精确 source/route/result digest、关键指标、selected facilities 和
   `validation.valid=true`；
9. 再增加 unauthorized、PII canary、incomplete route、infeasible、100% 差一单位、
   peak/disruption 五类负向/压力案例；
10. 权威能力基线只声明真实执行过的 backend、规模和场景。

## 8. 验证结果

### 8.1 通过

| 命令/检查 | 结果 |
| --- | --- |
| Planner `pytest -q` | 15 passed |
| Planner 两个 MCP stdio smoke | passed；但 planner smoke 只到 snapshot→validate |
| `npm run lint` | passed |
| `npm run typecheck` | passed |
| `npm run test` | 177 files、1,237 tests passed；存在 React `act(...)` warnings |
| `npm run build` | passed；Mapbox chunk 1.868 MiB，Vite 给出 >500 KiB warning |
| Codex contract/generated/capability/feature-policy/fixture checks | passed |
| `npm run test:codex-harness` | passed |
| real `smoke:codex-app-server --require-manifest` | passed；Manifest 18 capabilities |
| `cargo fmt --all --check` | passed |
| `cargo test --workspace --locked` | 142 passed、9 ignored、0 failed |
| tracked-tree `check:no-desktop` | passed；13 个旧 UI module contract 均解析为 browser shim |
| `git diff --check` | passed |

### 8.2 失败或未完成

| 检查 | 结果与解释 |
| --- | --- |
| `npm run check:main-ui-parity` | failed；18 个内容差异 + 4 个 extra 文件 |
| 工作目录直接 `check:no-desktop` | 因未跟踪的空 `apps/web/src-tauri/gen/schemas` 目录失败；Git tracked tree 已单独证明通过，不作为代码缺陷 |
| Ruff | 当前 dev environment 没有安装，`pyproject` 也未把它列入 dev dependency |
| PostgreSQL ignored integration | 本环境未提供 disposable `TEST_DATABASE_URL`，未动态重跑；CI 有 Postgres service |
| 完整 Enterprise Supervisor E2E | 本次未重跑；现有 wrapper 硬编码 macOS 路径且需要 live Server/DB/Provider。已静态审查脚本并运行底层 Planner MCP |
| Codex upstream status | 当前基线后有 171 个 official commits 待专用同步 |

测试通过不能关闭本报告问题：现有测试恰好没有覆盖匿名 public mode、same-org no-grant、
roleless full fork、Role count restart、Data/Snapshot digest、raw Artifact content、
超限/取消和业务指标对账。

## 9. 已存在的正确控制

以下实现方向符合规范，应在整改时保留：

- Platform 主事件桥在 browser fan-out 前持久化 durable projection；
- Provider Secret 使用加密、身份绑定的 Secret Store，调试输出有脱敏测试；
- Workspace 文件/Git 路径有 containment、symlink 和 grant 检查；
- Profile Runtime Role 文件使用固定私有路径、内容 hash、原子写入和 symlink 拒绝；
- Agent Definition 对 MCP server/tool 使用精确 allowlist；
- Pydantic 模型默认 `extra=forbid`，Data source 使用 source ID 和路径 containment；
- Planner 已有 20 MiB 文件和 14 候选的局部限制，并能明确返回 infeasible；
- 浏览器普通 Runtime event projection 已移除大量 raw path/Resource URI；
- Web、Rust、合同和 fresh-Postgres CI 主体已建立。

这些控制说明问题可以在现有所有权边界内修复，不需要另建 Runtime、第二套 Agent
系统、浏览器 Planner 或历史兼容分支。

## 10. 审计限制与工作树

- 未对公共实例执行真实 Owner 接管；结论来自完整静态执行路径、部署路径和现有测试。
- 未在 disposable PostgreSQL 上重跑 ignored integration，也未使用真实 Provider 重跑
  Enterprise E2E。
- 性能复现为单机当前环境数据，绝对耗时会变化，但无预算和指数路径的事实不变。
- Codex 上游 171 个提交仅记录为同步风险，没有在本审计中合并或逐提交评估。
- 审计未修改产品代码。审计开始前已有
  `apps/web/package-lock.json` 两处 `dev: true` 变更，本报告未触碰它。

