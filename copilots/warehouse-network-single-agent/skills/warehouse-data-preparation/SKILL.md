---
name: warehouse-data-preparation
description: 发现、检查并标准化授权 Workspace 的仓网数据；在路线、时效、成本或选址分析前使用。
metadata:
  short-description: 准备仓网输入数据
---

# 仓网数据准备

只使用 Data MCP Tool 处理用户确认的 Workspace 相对 `.xlsx`、`.csv` 或 `.json`。若 discover 返回 `outputs/warehouse-network/prepared/*.json` candidate，只有在需要验证复用时才把 discover 返回的 exact candidate path 与本次 raw path 一起传给 inspect，不从目录或文件名自行构造路径，也不读取候选 JSON。调用 inspect 时必填 `required_roles` 与 `country_code`，使用返回的 inline `source_profile`、`inspection_identity` 与 `inspected_relative_paths`；不发布或读取 Data Resource。对 `ambiguous=false` 的 exact unit 角色建议，`source_selections` 传 `relative_path`、`unit_ref` 与 `role`，省略 `mappings`，由 Tool 原子采用同一 inline profile 的精确建议；显式 mapping 对任何 exact unit 都允许。不得根据预览样本手抄、猜测或改写无歧义映射。

调用 `prepare_network_input` 时必须原样传回 inspection identity 与 inspected paths；Tool 会重新检查完整文件并在路径集合或任一字节变化时返回 `source_changed`，不写 prepared 输出。

若 inspect 返回 `selection_required`，按 Tool 的 `next_action` 选择更少的文件或 source units 后结束当前 Turn；若返回 `prepared_selection_required`，把有界候选交给用户选择一次并结束。`prepared_ready` 是可直接交接的复用终态，不再调用 prepare。若 prepare 返回 `outcome=needs_input` 或 `next_action=request_user_input`，执行同一终止规则；`retryable=false` 表示没有新用户输入或修正文件时禁止再次调用。缺少 `warehouse_type` 时要求源文件逐行补充 `center` 或 `cross_docking`，不把所有仓猜成同一仓型。

首次准备必须保留全部已确认候选仓；用户要求地图展示候选仓时，候选 source 必须纳入该输入，即使 baseline 使用 `existing_only`。`existing_only` 只限制 Network 的计算范围，不能删除已确认候选仓。任何候选、需求、现网仓、当前分配、路线事实或报价变化都完整准备一个新的 Workspace 输入，并 create-new 写入 `outputs/warehouse-network/prepared/*.json`，不得写入 Workspace 根目录或源数据目录。需要地理补全时使用已确认国家的行政区目录，直到得到 `ready` `prepared_network_input.v2`；原样保留 Tool 返回的 `prepared_input_relative_path`、`input_identity`、角色计数和有界问题目录，不构造 URI。

文件角色、字段映射、国家、候选仓、成本规则或数据缺口不明确时，只询问真正缺失的业务选择。`prepare_network_input` 成功是本次数据准备的 terminal Tool 结果；随后只返回精确 Workspace 输入路径、输入身份与有界数据质量结论，不再调用 `read_mcp_resource`、`list_mcp_resources`、`inspect_workspace_sources`、`tool_search` 或其他 Tool，也不计算覆盖、成本、方案或地图。
`needs_input` 同样是 terminal 结果：同一 Turn 最多询问一次，随后停止；只有用户在新 Turn 提供答案或上传修正文件后才重新检查。
