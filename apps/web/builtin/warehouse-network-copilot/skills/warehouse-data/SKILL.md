---
name: warehouse-data
description: 当用户要求检查或准备当前 Workspace 中的仓网 Excel、CSV、JSON 数据，推断和映射字段，标准化需求、仓库、当前覆盖或路线报价，或者补全行政区与经纬度时，使用此 Skill。
---

# 准备仓网数据

## 检查输入

- 只接受 `.xlsx`、`.csv` 和 `.json`；要求用户先把旧 `.xls` 或 `.xlsm` 导出为支持的格式。
- 只读取用户或 Supervisor 确认的 Workspace 相对路径。先发现或检查文件结构，再决定字段映射；路径、文件角色或映射有歧义时，返回明确缺口，不猜测。
- 依据 Network Agent 针对本次问题给出的数据要求检查输入。仓网分析的最低业务字段通常包括：
  - 需求城市：`city_id`、`city_name`、`demand_quantity`。
  - 已有仓库：`warehouse_id`、`warehouse_name`、`warehouse_type`（`center` 或 `cross_docking`）、`city_id`、`city_name`。
- 当前覆盖、候选仓库和路线报价属于按需输入：
  - 当前覆盖：需求城市到当前服务仓库；仅在用户要比较真实现状时需要。
  - 候选仓库：与已有仓库相同的基础字段，并明确是候选仓；只有选址或增加候选仓的分析需要。
  - 路线报价：起点、终点、价格；当前 Tool 合同还要求币种、车型容量及路线层级，缺少时用通俗业务语言说明。
- 使用检查 Tool 给出的字段建议，但只应用用户确认的文件角色和字段映射。不要把相似列名、样例值或文件名当作最终确认。
- 用业务语言解释缺失项，例如“缺少每个需求城市的需求量”，而不是只返回内部 schema 名或 Runtime 标识。

## 标准化与地理补全

- 调用标准化 Tool，把已确认输入转换为 `normalized_network_input.v1`。保持 Tool 返回的 `ready`、`needs_input` 或 `needs_geography` 状态，不伪造就绪。
- 行政区目录至少应为本次国家提供城市 ID/名称、省级 ID/名称、经度和纬度。只从用户确认且经过 Workspace 边界校验的行政区 JSON 读取；不跨国家猜配。
- 为需求城市、已有仓库和已确认候选仓库补全省级信息与经纬度。地名或 ID 多义时列出歧义并请求用户确认；未知 ID 必须拒绝，不能回退到相似名称。
- 如果用户已上传候选仓库文件，优先使用并标准化该文件。如果需要候选仓但没有候选文件，向 Supervisor 返回 `needs_input`，请用户选择：上传候选清单、明确指定候选城市，或确认按省级/市级行政区生成候选的业务规则。
- 仅选择“省级/市级”还不足以生成仓库。必须同时确认候选城市、仓库类型，以及 cross-docking 的上游中心等必要业务属性；在确认前不要从行政区目录静默生成候选仓。

## 交接结果

- 将 provider 拥有的中间结果保存在允许 Tool 返回的精确 MCP Resource 中，把原样 ResourceRef 返回给 Supervisor；不得重建 URI、添加内容哈希或改称 Artifact。
- 只有用户要求保存可见结果或需要显式跨 package 交接时，才写普通 Workspace 文件，并使用 create-new 语义；冲突时报告而不覆盖。
- 不计算覆盖策略、成本最优、时效最优、场景或选址，不直接创建地图和报告，不创建 Platform workflow，也不用 Artifact 传递中间数据。
