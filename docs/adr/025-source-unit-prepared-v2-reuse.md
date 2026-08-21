# ADR-025：Source Unit 选择与 Prepared v2 新鲜复用

状态：Accepted
日期：2026-08-21

补充并局部替代 [ADR-023](023-workspace-prepared-inputs-and-domain-geospatial-execution.md) 与
[ADR-024](024-warehouse-copilot-contract-simplification.md) 中 Data 检查、准备输入和复用的对应段落。
Runtime、Platform、Workspace 授权、MCP Resource、Artifact 和 Role 权限所有权不变。

## 背景

Data 需要支持 CSV、XLSX sheet 和任意 JSON array，同时避免把 preview 当成完整数据，也不能让
未选来源的弱角色评估阻塞任务。旧检查握手和 prepared 输入还无法表达精确 unit、完整映射和来源
新鲜度，导致 Agent 重复检查或错误复用。

## 决定

1. Data 按用户目标选择 `required_roles`，inspect 为每个 exact source unit 返回有界字段、样本类型、代表值、preview/total 和角色评估。角色评估不是全局业务缺口；只有选中 unit 的语义、单位、值或冲突无法确定时才询问。
2. `workspace_source_profile.v2` 的 inspect 结果只在当前握手中使用。`prepared_network_input.v2` 是 Data→Network 的唯一持久化业务文件；它保存选中 unit、resolved mappings、raw/admin SHA、国家、roles、role_counts 和一个 `selected_source_identity`。
3. inspect 只有在调用方显式传入 exact prepared candidate path，且候选为 ready、国家/角色匹配、选中来源和映射仍 fresh 时才复用。相同 prepared 内容按规范路径稳定选择；不同内容返回一次 `prepared_selection_required`；未选来源变化不影响复用。
4. prepare 在完整读取前后校验 source snapshot；`ready` 写 create-new 文件，`needs_input` no-write 一次交接，`source_changed` 只允许一次重新 inspect。Network/Maps 只接受 v2 ready 输入，直接加载 prepared snapshot 不依赖 raw 文件；复用和 geography 准备另行校验 raw/admin 新鲜度。

## Owner、持久化与边界

Data Tool 拥有 unit 解析、全量读取、映射、规范化和 prepared 文件；Network/Maps 拥有其 typed Resource 和交付。Platform 不保存映射、复用 registry、revision、fingerprint 或 workflow state；Codex Runtime 不做领域解释。Role TOML 继续显式声明 MCP 权限，Skill 只描述目标角色、业务询问和 typed 终态交接。

## 被否决方案

- 用文件名、preview、国家或历史 child 状态猜角色和“最新”来源；
- 为 source profile 建立 Resource、Platform 状态机或跨任务缓存；
- 看到任意 prepared 文件就自动复用，或让未选来源变化阻塞当前目标；
- 在 Skill 中重复 Tool 已强制的路径、schema、hash、create-new 和安全规则。

## 验证与退出条件

Planner/Maps unit、schema、stdio、freshness、source-change、XLSX/JSON unit、非标准表头和真实/确定性
Copilot 门验证上述合同。若未来官方 Runtime/SDK 提供等价的 typed source-unit/provenance 能力，在
保留 Data/Network owner 和失败语义后删除领域重复实现；此前不增加兼容读取或第二套复用机制。
