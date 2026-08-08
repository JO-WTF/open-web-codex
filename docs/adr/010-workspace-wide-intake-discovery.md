# ADR-010：Workspace 全域数据接入与 Task 级确认

状态：已被 [ADR-013](013-platform-data-intake-and-work-state.md) 取代（2026-08-08）

领域 Agent 不再遍历 Workspace 或发布 source/mapping 生命周期。Platform Data Intake 是
SourceAsset、画像、显式映射和 Dataset Release 的唯一 owner；领域包只实现业务合同、
validator 和 normalizer。本文件仅保留历史决策背景。

## 决定

授权 Workspace 是数据发现边界，不是 Thread 或 Draft。Data Agent 使用 Runtime
提供的可信 sandbox-state-meta.sandboxCwd，只扫描普通 .xlsx、.csv、.json 文件，
跳过符号链接、.git、依赖、构建缓存和已发布 Dataset Release，并受文件数、大小、
XLSX 压缩比、Sheet/列/样本行、JSON 深度/节点数限制。

发现只返回 opaque source_ref、显示名、媒体类型、大小、内容 hash 和有界结构画像。
它不返回宿主路径或完整数据表。source_profile.v1、mapping_proposal.v1、
planning-dataset.v2 都由 capability package 发布为不可变 Resource；用户确认保存
的是 Resource ID、revision 和 hash，而不是修改 Resource。

Platform 只负责 Workspace 授权、SourceAsset 生命周期、审计、幂等和
DataIntakeInputRequest 投影；Platform 不按状态自动 spawn Agent，也不拥有字段语义
或网络模型。Network Agent 拥有 Requirement Profile、Input Gap 和最终 Readiness
Review；Data Agent 拥有文件画像、候选映射和归一化。

## ADR-007 的边界调整

ADR-007 仍然约束不可变 Dataset Release、禁止任意路径输入和精确发布恢复。原先
否决的“由 MCP 遍历 Workspace 猜数据”不适用于本 ADR 的受限发现：这里的扫描由
可信 Turn Workspace 根、格式白名单、边界和 hash 共同约束，且扫描结果只是候选证据，
不会直接成为分析输入。分析仍必须经过用户确认、归一化 Release 和 TaskDatasetBinding。

## 验证门禁

必须覆盖两个 Thread 复用一个 Workspace、早期/后续 Draft 与已有文件同时可见、Excel
多 Sheet、CSV 编码/分隔符、JSON 嵌套结构、低置信度映射保持 blocked、重复 fingerprint
不重复请求，以及 symlink/逃逸/压缩炸弹/超限文件失败关闭。
