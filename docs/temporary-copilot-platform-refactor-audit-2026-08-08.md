# Copilot 平台重构临时审计

| 字段 | 内容 |
| --- | --- |
| 文档性质 | 临时审计与证据底稿，非权威 current-state 或目标架构 |
| 快照日期 | 2026-08-08 |
| 代码快照 | `75a3d5eb98a42e8556da34a19e1ca5bfe4571326` |
| 接受决策 | [ADR-018](adr/018-built-in-network-copilot-runtime-closure.md) |
| 删除条件 | 阶段一 canonical 文档完成收敛并形成双硬门可重放证据 |

本文保存本轮产品与技术审计的证据、失败链和推导过程。当前实现事实分别进入
[Architecture](architecture.md) 与 [Capability Baseline](capability-baseline.md)；目标
边界进入 ADR-018；近期任务进入 [Development Plan](development-plan.md)。本文后续关于
Clean Spine、Work State、Data Intake、typed handles 和 Resource Broker 的建议已经失效，
只保存失败证据。本文件到达删除条件后由 Git 历史保存，不演化成第二套架构文档。

## 1. 审计问题与总裁决

本轮从两个角度重新回答：

1. 产品是否让算法工程师以简单、直觉、规范的方式创建 Tool、Skill、Domain Agent、
   Supervisor 和 Copilot？
2. 当前技术架构是否保持 Runtime/Platform 所有权、通用扩展点和长期可删除性，而不是
   为仓网闭环叠加补丁？

总裁决：

> 产品北极星没有偏离，但实现形态已经偏离。真实多 Agent 运行内核开始成立，算法工程师
> 自助创作、发布、安装和发现的 Copilot 平台尚未成立。当前是“跨阶段原型 + 仓网最小
> E2E”，不是 M2 平台闭环完成。

方向仍然正确的部分：继续复用 Codex Runtime；Profile/Workspace/Task/Run scope 基础；
Root 用户输入、Agent execution、Work State、Provider metrics 的局部实现；仓网确定性
算法和真实运行证据。

偏离的核心不是“功能少”，而是同一事实有多个 owner、模型承担协议/事务、Projection
承担 workflow、SDK/Web/Installation 各走一条断开的路径。

## 2. 最新 Understand Anything 图谱

最新 `.ua/meta.json` 与当前 HEAD 对齐：

- `lastAnalyzedAt`: `2026-08-08T10:30:19.833Z`；
- `gitCommitHash`: `75a3d5eb98a42e8556da34a19e1ca5bfe4571326`；
- `analyzedFiles`: 6,789；
- 图谱节点 41,819，边 64,727。

图谱用于确认 Browser -> Settings、Server routes、Platform crates、Runtime adapter、
capability packages 和供应链工具之间的连接。但本次更新存在增量漏收：以下当前文件在
`.ua/fingerprints.json` 中已有 fingerprint，却在 graph nodes 中为 0：

- `apps/web/crates/capability-catalog/src/lib.rs`；
- `apps/web/crates/data-intake-service/src/lib.rs`；
- `apps/web/crates/work-state-service/src/lib.rs`；
- `apps/web/server/src/routes/capability_catalog.rs`；
- `apps/web/server/src/routes/data_intake.rs`；
- `apps/web/server/src/routes/work_state_gate.rs`；
- `tools/copilot-sdk/copilot_sdk/cli.py`；
- `tools/supply-chain-network-planner/supply_chain_planner/case_repository.py`。

因此所有关键结论均再次以 HEAD 源码、迁移和真实运行记录复核；图谱不能单独证明新增
模块已接入生产链。

## 3. 最新真实最小 E2E

证据来自 Codex 任务“开发牛马”：

- task ID：`019fdfc8-d475-7821-8ff6-a3b785a9d536`；
- turn ID：`019fe094-c071-75d2-bc4a-80d46166eabf`；
- 目标：先闭环最小真实链路；
- 最终结果：7/7，约 721 秒。

### 3.1 已证明

- 真实 DeepSeek Provider 与真实 Codex Runtime；
- Root、Network Agent、Data Agent 参与；
- 4 个 execution 全部 `completed`；
- 2 个官方 Root 输入均 `answered`，无 pending；
- Work State `network_input`、`route_matrix`、`network_deliverables` 均 `ready`；
- 50 城市、11 仓、550 条球面路线；
- 50,815 个需求单位完成 `optimized_existing_footprint` 分配；
- 生成 `network_planning_report.v1`；
- Python `100 passed`、Codex contracts、`git diff --check` 通过；
- 未新增 `codex-rs` 修改。

### 3.2 不能证明

- SDK -> Web import -> Catalog -> Compiler -> Profile Installation -> Runtime discovery；
- 用户从零创建 Skill、Agent、Supervisor 和 Copilot；
- current coverage、成本、场景、选址、地图；
- 浏览器刷新、断线、Runtime/Profile 重启；
- 第二领域和多用户隔离。

E2E 脚本直接上传文件、创建带仓网 component 的 Work State、复制仓库内置定义并发布
Supervisor Draft；基线 Prompt 注入 Work State ID、Run ID、字段形状、球面参数、Tool
规则、执行顺序和禁止事项。关键入口位于：

- `apps/web/scripts/enterprise-supervisor-e2e.mjs:378`：脚本手建 Work State；
- `apps/web/scripts/enterprise-supervisor-e2e.mjs:474`：复制当前 Supervisor 合同；
- `apps/web/scripts/enterprise-supervisor-e2e.mjs:828`：长 Prompt 注入内部接线。

这证明“预接好的仓网 Copilot 能运行”，不证明产品创作闭环。

## 4. “开发牛马”失败链

失败链比最终成功更能暴露架构问题。以下按发生顺序记录，不把最终 7/7 当作对它们的
豁免。

| 失败 | 直接根因 | 当时处理 | 架构含义 |
| --- | --- | --- | --- |
| `mapping_candidates_empty` | 映射器只读 `name/displayName`，真实需求使用 `entity/entity_type` | 增加字段支持和回归测试 | 没有唯一生成 schema，模型在翻译 wire shape |
| 修复后仍运行旧逻辑 | MCP 进程已加载旧 Python 模块 | 重启服务/MCP | 安装 generation 与 discovery 身份不清楚 |
| 重建开发库后 Provider 401 | 新环境使用了无效 Provider 配置 | 重新写入有效 Secret 后重跑 | Profile/Provider 初始化依赖隐式宿主状态，不是干净安装 |
| Data Agent 误读引用 | Workspace `source_ref` 被当成 MCP Resource URI | 在 Agent 指令中明确禁止 | 安全/类型边界被交给 Prompt，机制缺失 |
| 映射仍漂移 | Agent 使用 `source/source_refs/field_mappings`、`warehouses/demand_points`，Tool 只认另一组字段 | Data Server 增加 canonicalizer 和 aliases | 为成功率增加兼容方言，长期会继续扩散 |
| 标准化缺需求行 | 合法实体名 `demand_cities` 未被识别 | 再增加实体 alias | 领域 schema 未由 compiler 生成并锁定 |
| 子 Agent 空等待 | child 调用 Runtime 禁止的 `request_user_input`，Root 又没有可回答 blocker | 改为 Root 派发前收集参数 | 输入必须由 root-only 状态机约束，不是写一句指令 |
| Root 输入仍未出现 | Default mode 没有启用官方输入 capability | Adapter 为 governed Supervisor 打开正式 feature | capability gate 必须来自 Runtime 配置/发现 |
| 新 Adapter 未生效 | 启动脚本的 Cargo progress 参数失败被吞，直接构建又落在 `debug`，服务使用 `dev-small` 旧二进制 | 用正确 profile 重建并重启 | 构建、运行 generation 和验证证据必须同一身份 |
| 重复 deliverable mutation 失败 | Agent 直接管理 Work State operation/deliverable，重复登记 | 最终状态已就绪，作为非关键失败保留 | 事务与幂等不应是模型可见协议 |

源码又显示，为了使 Root 在 child terminal 后继续，`event_projection.rs` 注入一段包含
字段映射和交付要求的 `SUPERVISOR_CONTINUATION_PROMPT`；Data Intake confirmation 还会
全局 interrupt active Turns。这不是 E2E 日志中的单点 bug，而是平台 Projection 已成为
第二 workflow owner。

## 5. 产品经理审计

### 5.1 用户心智模型

算法工程师应只理解：

```text
Tool -> Skill -> Agent -> Supervisor -> Copilot
Data -> Task input
Deliverable -> report / dataset / map / decision
```

普通界面不应要求理解 semver、content/execution hash、Runtime Role、MCP server 名、
Plugin path、Work State component、Assignment contract、spawn limit 或 Artifact 内部 ID。

### 5.2 当前产品偏离

1. Studio 被拆在 Settings 的 Agents、Agent Catalog、Tool & Skill Studio、Supervisors；
   没有一个 Copilot Draft 或 Copilot Builder。
2. SDK 只有 `init/validate/test/pack`；没有 login、push、发布、安装、readiness。Web 没有
   SDK package import。
3. SDK `tool init` 同时生成 Skill、manifest、MCP JSON 和 shell launcher；没有 tests 时
   仍显示 validation passed。
4. Web 让用户编辑原始 Python、Plugin/MCP JSON、版本和内部合同。
5. UI 说“安装到 Workspace”，服务端又能在没有物化 Agent/Supervisor/Copilot 时显示
   installed。
6. Agent 编辑仍继承 capability template 并手填 semver，没有精确组合 Skill Releases、
   Tool capabilities、数据权限和交付合同。
7. 错误兜底要求普通用户查看服务器日志，中英文和 Draft/Release/Install 状态词汇不统一。
8. 12 分钟真实任务缺少以阶段、Agent、输入、产出、失败和取消为中心的产品体验。

### 5.3 接受的产品方向

- 主导航建立独立 Copilot Studio；Settings 只保留 Provider、Workspace、Runtime 与高级项；
- 一个 Copilot Draft 贯穿 Tool、Skill、Agent、Supervisor、测试、发布和安装；
- SDK 本地开发并 `push`，Web 审查、组合、测试、发布、安装和运行；
- Skill 默认通过中文引导生成 `SKILL.md`，专家模式才编辑 Markdown；
- Copilot Builder 展示业务目标、团队、能力、输入、权限、交付、测试和安装；
- 页面分别解释 released/installing/installed/discovered/ready，不允许假成功。

## 6. 技术架构审计

### 6.1 当前跨阶段状态

当前提交一次跨越 244 个文件，新增/修改 Platform crates、routes、migrations、Web、SDK、
Agent packages、供应链 MCP 和文档。结果不是按顺序完成 Gate 0 -> Catalog -> Installation
-> Authoring -> Data Plane -> Warehouse migration，而是：

- Runtime/bridge、用户输入、execution、Work State 有可保留的局部实现；
- Catalog、Data Intake、Assignment、Installation 有未接入或错误接入的骨架；
- 仓网最小 E2E 提前成立；
- 旧 Catalog、Case/Data 生命周期、path discovery 和 Settings Studio 尚未删除。

### 6.2 重复 owner

| 事实 | 当前多个 owner |
| --- | --- |
| Capability 内容/版本/hash | code seed、Supervisor Catalog、新 Capability Catalog、Tutorial、Web Draft |
| 安装/可用性 | Workspace Git publish、installation table、Adapter path selection、Runtime |
| source/mapping/dataset | Platform Data Intake route、data-intake-service、CaseRepository、Data Server |
| work/operation/readiness | Platform Work State、CaseRepository、Agent Prompt |
| 协同完成 | Runtime Root、Run、Event Projection continuation |
| 内容引用 | Workspace source ref、WorkResourceReference、DataAgentRef URI、Artifact ref |

### 6.3 最严重的架构偏离

1. 概率模型被当作协议适配器：字段别名、URI 拼接、引用转换、内部 ID 和顺序进入 Prompt。
2. 概率模型被当作事务客户端：operation、revision、幂等、mutation、deliverable 由 Agent
   管理。
3. Event Projection 被当作 workflow owner：主动续跑、发消息、interrupt。
4. Catalog 发布、安装、Runtime discovery 和 readiness 没有独立事实。
5. Profile 认证默认可继承宿主 `$HOME/.codex/auth.json`，干净 Profile 隔离不成立。
6. 公共平台层已出现供应链字段和工具协议分支，第二领域会继续复制补丁。

### 6.4 可信设计的边界

审计不建议在当前阶段建设复杂信任体系。硬机制只覆盖：身份/scope、capability grant、
typed handle、Root-only input、原子 ToolOutcome commit、完整终态、安装/discovery 和有界
Run completion。业务方法、证据解释、Agent 选择指导、冲突沟通和部分成功策略继续由
Skill/Supervisor 负责。

## 7. 已接受的基线

用户已经明确把本轮发现作为后续设计与开发基线，正式决定见
[ADR-017](adr/017-clean-copilot-platform-spine.md)。核心裁决是：

- 三平面、当前以模块化单体实施；
- Runtime 执行，Platform 治理，Data Intake 与 Domain Package 分工；
- 一个 Catalog/Compiler、一个 Profile Installation、Runtime discovery 作为可执行真相；
- typed handles 替代 `source_ref`/URI 字符串方言；
- Assignment Grant、ToolOutcome、child `needs_input`、Root 官方输入和
  `awaiting_synthesis`；
- Event Projection 不调度；
- 先建无领域 Clean Spine，再迁移仓网，最后以第二领域否证平台硬编码；
- 不兼容旧项目实现，原子切换后删除历史路径。

## 8. 保留、冻结和删除

### 保留

- `codex-rs` 与 Patch Map retained seams；
- Profile Host typed bridge、Runtime lifecycle/capability manifest；
- Approval、Root 用户输入、execution、Artifact 的通用投影；
- Work State 内部事务、revision、幂等和完整终态实现；
- 仓网领域 schema、算法、fixture、报告/地图 renderer 和真实 E2E 证据。

### 立即冻结

- 旧仓网 Prompt、aliases、continuation、path discovery 和模型可见事务协议；
- Settings 式 Studio 的继续扩展；
- 为扩展成本/场景/选址/地图而增加平台补丁。

### Clean Spine 替代后删除

- 旧 `supervisor-catalog`/capability template/旧 Agent-Supervisor routes 与 UI；
- cwd/Workspace/source repo capability root 扫描；
- false installation/readiness；
- Event Projection continuation、全局 interrupt、相关持久化；
- 模型可见 begin/apply/fail Work State Tool；
- CaseRepository 的 source/mapping/operation/readiness/dependency 公共基础设施；
- Data Server aliases、Workspace 扫描、Agent 构造 MCP URI；
- 手工版本/hash/Runtime Role/Blueprint 常量；
- 默认宿主 auth import；
- 互相竞争的 Settings Studio 页面。

## 9. 审计关闭条件

本文件只有在以下条件都满足后删除：

1. Architecture、Capability Baseline、Development Plan 与目标架构全部指向同一当前事实；
2. Clean Spine 从干净 Profile 的 SDK/Web 入口通过；
3. 仓网迁移不再依赖旧 Case、aliases、Prompt ID 和 Projection workflow；
4. 第二领域不修改 Platform 领域分支；
5. E2E 保存脱敏、机器可读、绑定 commit/Profile/installation/runtime 的 evidence。
