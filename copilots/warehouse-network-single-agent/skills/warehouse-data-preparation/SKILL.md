---
name: warehouse-data-preparation
description: 发现、检查并标准化授权 Workspace 的仓网数据；在路线、时效、成本或选址分析前使用。
metadata:
  short-description: 准备仓网输入数据
---

# 仓网数据准备

只使用 Data MCP Tool 处理用户确认的 Workspace 相对 `.xlsx`、`.csv` 或 `.json`。先一次检查全部确认路径，使用返回的 inline `source_profile`、`inspection_identity` 与 `inspected_relative_paths`；不发布或读取 Data Resource。对 `ambiguous=false` 的角色建议，只传 `relative_path` 与 `role`，省略 `mappings`，由 Tool 原子采用同一 inline profile 的精确建议；只有 suggestion 明确歧义且用户已确认时才传扁平字段映射。不得根据预览样本手抄、猜测或改写无歧义映射。

调用 `prepare_network_input` 时必须原样传回 inspection identity 与 inspected paths；Tool 会重新检查完整文件并在路径集合或任一字节变化时返回 `source_inspection_changed`，不写 prepared 输出。

首次准备必须保留全部已确认候选仓；用户要求地图展示候选仓时，候选 source 必须纳入该输入，即使 baseline 使用 `existing_only`。`existing_only` 只限制 Network 的计算范围，不能删除已确认候选仓。任何候选、需求、现网仓、当前分配、路线事实或报价变化都完整准备一个新的 Workspace 输入，并 create-new 写入 `outputs/warehouse-network/prepared/*.json`，不得写入 Workspace 根目录或源数据目录。需要地理补全时使用已确认国家的行政区目录，直到得到 `ready` `prepared_network_input.v1`；原样保留 Tool 返回的 `prepared_input_relative_path`、`input_identity`、候选仓总数和有界候选仓目录，不构造 URI；目录被截断时不得断言候选仓不存在。

文件角色、字段映射、国家、候选仓、成本规则或数据缺口不明确时，只询问真正缺失的业务选择。完成后只返回精确 Workspace 输入路径、输入身份与有界数据质量结论，不计算覆盖、成本、方案或地图。
