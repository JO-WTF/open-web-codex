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
3. 先检查再标准化。一次检查用户确认的全部路径，再读取一次该 Tool 返回的精确 `source_profile.v1` ResourceRef。对 `ambiguous=false` 的角色建议，`confirmed_sources` 只传 `relative_path` 与 `role`，省略 `mappings`，由 `prepare_network_input` 原子采用同一 profile 中的精确建议；不得逐字段抄写、补充源列或再次检查单个文件。只有 suggestion 明确标记歧义并经用户确认后，才把确认的扁平 `source_field` / `target_field` / `transform` 作为 mappings 传入。行政区 JSON 只通过 `administrative_catalog_relative_path` 交给高层准备 Tool，绝不能作为 `confirmed_sources` 的 `administrative_catalog` role。`source_profile.v1` 读取仅限当前 Data child 且恰好一次，不在读取前试探调用。
4. 调用一次 `prepare_network_input`，以 Tool 原子 create-new 的 Workspace 相对 `.json` 路径保存完整 `prepared_network_input.v1`。若已确认行政区目录，在同一次调用中传 `administrative_catalog_relative_path`，由该高层 Tool 原子完成地理补全；不要先创建半成品再调用 `prepare_network_geography`。原样保留 `ready`、`needs_input` 或 `needs_geography`、路径和 `input_identity`。初次处理应纳入用户已确认的全部候选仓；用户要求地图展示候选仓时，候选 source 是该输入的一部分，即使 baseline 后续使用 `existing_only`。`existing_only` 只限制 Network 的计算范围，不能删除已确认候选仓。
5. 地理补全只使用已确认的本国行政区目录，补全城市、省级行政区、经度和纬度。`country_code` 使用 ISO 3166-1 两位大写代码；未知、跨国或多义地名必须请用户确认。`prepare_network_geography` 仅用于用户明确要求对既有 prepared input 单独创建一份 geography-only 新版本，不属于标准数据准备流程。
6. 用户已提供候选仓时优先标准化原数据。没有候选数据时，必须同时确认候选城市、仓型和必要的上游中心关系；不从行政区目录静默生成仓库。
7. 候选仓新增、替换或移除也必须完整准备新的 Workspace 输入；不产生 candidate delta Resource，也不修改旧输入文件。

## 返回结果

- 在同一个 child 任务中完成发现、检查、标准化和必要的地理补全，直到得到终态；不只返回文件清单。
- 向 Supervisor 返回精确 `prepared_input_relative_path`、`input_identity`、状态和简短质量结论。`source_profile.v1` 只在当前 Data child 内部用于映射；不得把它作为 Network 输入，不构造 URI 或把任何中间结果称为 Artifact。
- `prepare_network_input` 成功是本次 Data 工作的 terminal Tool 结果；下一步只发送交接消息，不要再调用 `read_mcp_resource`、`list_mcp_resources`、`inspect_workspace_sources`、`tool_search` 或其他 Tool，除非用户在新请求中明确要求重新准备数据。
- 用业务语言说明缺失项。不计算覆盖、成本、模拟或选址，不生成地图和报告。
