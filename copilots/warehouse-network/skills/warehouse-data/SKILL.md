---
name: warehouse-data
description: 仅供仓网 Supervisor 原生创建的 data_agent 使用。用于发现和检查当前 Workspace 中的仓网 Excel、CSV、JSON 数据，确认字段映射，标准化需求、仓库、当前分配、候选仓和路线报价，并补全行政区与坐标。
metadata:
  short-description: 准备并标准化仓网数据
---

# 准备仓网数据

只在当前 Role 为 `data_agent` 时执行。仅使用已允许的 Data MCP Tool 处理业务文件；不用 shell、Git、内联代码或 Workspace 扫描代替 Tool。Tool 不可用时返回明确失败。

## 工作流程

1. 只发现或读取用户确认的 Workspace 相对路径，支持 `.xlsx`、`.csv`和 `.json`。路径、文件角色或字段映射有歧义时返回缺口，不猜测。
2. 依据用户目标检查必要数据：
   - 需求城市：ID、名称、需求量。
   - 已有仓库：ID、名称、仓型（`center` 或 `cross_docking`）、城市 ID 和名称。
   - 真实现状对比才需要当前分配；选址才需要候选仓；成本分析需要币种、车型容量和路线报价；时效分析需要距离或运输时长。
3. 先检查再标准化。若 discover 返回 `outputs/warehouse-network/prepared/*.json` candidate，只有在需要验证复用时才把 discover 返回的 exact candidate path 与本次 raw path 一起传给 inspect，不从目录或文件名自行构造路径，也不要读取候选 JSON。调用 `inspect_workspace_sources` 时必填 `required_roles` 与 `country_code`；检查结果以内联 `source_profile`、`inspection_identity` 和 `inspected_relative_paths` 返回，不发布 Resource，也不要调用 `read_mcp_resource`。对 `ambiguous=false` 的 exact unit 角色建议，`source_selections` 传 `relative_path`、`unit_ref` 与 `role`，省略 `mappings`，由 `prepare_network_input` 原子采用同一 inline profile 中的精确建议；不得逐字段抄写、补充源列或再次检查单个文件。显式 mapping 对任何 exact unit 都允许。行政区 JSON 只通过 `administrative_catalog_relative_path` 交给高层准备 Tool，绝不能作为 `source_selections` 的 `administrative_catalog` role。调用 prepare 时原样传回 inspection identity 与 inspected paths；Tool 会重新检查并拒绝变化。
   - 若检查结果为 `selection_required`，按 Tool 的 `next_action` 选择更少的文件或 source units 后结束；若为 `prepared_selection_required`，只向 Supervisor 返回一次有界候选并请求用户选择；若为 `prepared_ready`，直接交接复用结果。不得把 inspect 的角色评估当成业务缺口，也不得再次 inspect、搜索 Tool、改写 mappings 或猜默认值。
4. 调用一次 `prepare_network_input`，传入 inline profile 对应的 `inspection_identity` 与 `inspected_relative_paths`，以 Tool 原子 create-new 的 `outputs/warehouse-network/prepared/*.json` 路径保存完整 `prepared_network_input.v2`；不得写入 Workspace 根目录或任何源数据目录。若已确认行政区目录，在同一次调用中传 `administrative_catalog_relative_path`，由该高层 Tool 原子完成地理补全；不要先创建半成品再调用 `prepare_network_geography`。原样保留 `ready`、`needs_input` 或 `source_changed`、路径、`input_identity`、角色计数和有界问题目录。初次处理应纳入用户已确认的全部候选仓；用户要求地图展示候选仓时，候选 source 是该输入的一部分，即使 baseline 后续使用 `existing_only`。`existing_only` 只限制 Network 的计算范围，不能删除已确认候选仓。
   - 若 prepare 返回 `outcome=needs_input` 或 `next_action=request_user_input`，同样只返回一次 Tool 的 `requirements` 并结束；`retryable=false` 明确禁止在没有新用户输入或新文件的情况下重复调用。
5. 地理补全只使用已确认的本国行政区目录，补全城市、省级行政区、经度和纬度。`country_code` 使用 ISO 3166-1 两位大写代码；未知、跨国或多义地名必须请用户确认。`prepare_network_geography` 仅用于用户明确要求对既有 prepared input 单独创建一份 geography-only 新版本，不属于标准数据准备流程。
6. 用户已提供候选仓时优先标准化原数据。没有候选数据时，必须同时确认候选城市、仓型和必要的上游中心关系；不从行政区目录静默生成仓库。
7. 候选仓新增、替换或移除也必须完整准备新的 Workspace 输入；不产生 candidate delta Resource，也不修改旧输入文件。

## 返回结果

- 在同一个 child 任务中完成发现、检查、标准化和必要的地理补全，直到得到终态；不只返回文件清单。
- 向 Supervisor 返回精确 `prepared_input_relative_path`、`input_identity`、状态、`candidate_warehouse_count`、有界 `candidate_warehouses` 及简短质量结论。目录被标记为截断时不得据此断言某个候选仓不存在。inline `source_profile` 只在当前 Data child 内部用于映射；不得把它作为 Network 输入，不构造 URI 或把任何中间结果称为 Artifact。
- `prepare_network_input` 成功是本次 Data 工作的 terminal Tool 结果；下一步只发送交接消息，不要再调用 `read_mcp_resource`、`list_mcp_resources`、`inspect_workspace_sources`、`tool_search` 或其他 Tool，除非用户在新请求中明确要求重新准备数据。
- `needs_input` 也是 terminal 结果：同一 child Turn 最多发送一次缺口消息，不重复调用、不自我重派、不等待不存在的答案。由 Supervisor 在 Root Turn 直接询问用户。
- 用业务语言说明缺失项。不计算覆盖、成本、模拟或选址，不生成地图和报告。
