---
name: warehouse-data
description: 仅供仓网 Supervisor 原生 spawn 的 data_agent child 使用。当用户要求检查或准备当前 Workspace 中的仓网 Excel、CSV、JSON 数据，推断和映射字段，标准化需求、仓库、当前覆盖或路线报价，或者补全行政区与经纬度时，使用此 Skill。
---

# 准备仓网数据

只有当前 Role 正是由 Supervisor 原生 spawn 的 `data_agent` child 才能继续，且 child 不得加载或反向读取 `warehouse-supervisor` Skill。若当前是 Root 或其他 Role，立即停止，不读文件、不调用 Tool，改用 `warehouse-supervisor`，由 Root 通过原生 spawn 创建数据代理（昵称 `Wanwan`）。

数据文件的发现、检查、规范化和地理补全必须直接使用 Role 允许的四个 Data MCP Tool。不得使用 `ls`、`find`、`git`、`jq`、`cat`、内联 Python 或其他 Workspace 命令扫描业务文件、仓库结构或 Git 元数据；Tool 不可用时返回明确失败，不得用 shell 旁路替代。

## 检查输入

- 只接受 `.xlsx`、`.csv` 和 `.json`；要求用户先把旧 `.xls` 或 `.xlsm` 导出为支持的格式。
- 只读取用户或 Supervisor 确认的 Workspace 相对路径。先发现或检查文件结构，再决定字段映射；路径、文件角色或映射有歧义时，返回明确缺口，不猜测。
- 依据 Network Agent 针对本次问题给出的数据要求检查输入。仓网分析的最低业务字段通常包括：
  - 需求城市：`city_id`、`city_name`、`demand_quantity`。
  - 已有仓库：`warehouse_id`、`warehouse_name`、`warehouse_type`（`center` 或 `cross_docking`）、`city_id`、`city_name`。
- 当前覆盖、候选仓库和路线报价属于按需输入：
  - 当前覆盖：需求城市到当前服务仓库；仅在用户要比较真实现状时需要。
  - 候选仓库：与已有仓库相同的基础字段，并明确是候选仓；只有选址或增加候选仓的分析需要。
- 路线报价：起点、终点、价格；当前 Tool 合同还要求币种、车型容量及路线层级。若同一来源还提供路线距离、时长和计算来源，应通过标准字段映射一并保留为 typed 路线事实；相关字段只提供一部分时明确报告不完整，不能静默丢弃或补猜。
- 使用检查 Tool 给出的字段建议，但只应用用户确认的文件角色和字段映射。不要把相似列名、样例值或文件名当作最终确认。
- 用户提示词或当前 Thread 已经明确给出文件角色、映射或参数时，直接沿用，不重复询问同一选择。
- Supervisor 已给出明确分析目标且允许发现当前 Workspace 文件时，在同一个 child 任务中连续完成发现、检查、标准化和必要地理补全，直到返回 `ready`、`needs_input` 或 `needs_geography`；不要只返回文件清单后要求 Supervisor 为同一批文件再创建一次 Data Task。
- 用业务语言解释缺失项，例如“缺少每个需求城市的需求量”，而不是只返回内部 schema 名或 Runtime 标识。

## 标准化与地理补全

- 调用标准化 Tool，把已确认输入转换为 `normalized_network_input.v1`。保持 Tool 返回的 `ready`、`needs_input` 或 `needs_geography` 状态，不伪造就绪。
- `country_code` 必须使用 ISO 3166-1 两位大写代码。优先读取已确认的行政区目录或用户数据中的国家代码并原样使用；只有国家名称时按标准国家代码确定，不使用三位代码，也不在不同 Tool 之间自行改写。Tool 拒绝国家代码时返回明确缺口，不发布带不一致国家标识的 `ready` Resource。
- Tool schema 是数据交接的权威合同。只提交 schema 允许的规范字段；字段名不受支持或必要映射不完整时，一次性返回允许字段和业务缺口，不通过相近别名反复试错，也不承接 Network Agent 的覆盖、矩阵或指标扩展请求。
- 行政区目录至少应为本次国家提供城市 ID/名称、省级 ID/名称、经度和纬度。只从用户确认且经过 Workspace 边界校验的行政区 JSON 读取；不跨国家猜配。
- 为需求城市、已有仓库和已确认候选仓库补全省级信息与经纬度。地名或 ID 多义时列出歧义并请求用户确认；未知 ID 必须拒绝，不能回退到相似名称。
- 如果用户已上传候选仓库文件，优先使用并标准化该文件。如果需要候选仓但没有候选文件，向 Supervisor 返回 `needs_input`，请用户选择：上传候选清单、明确指定候选城市，或确认按省级/市级行政区生成候选的业务规则。
- 仅选择“省级/市级”还不足以生成仓库。必须同时确认候选城市、仓库类型，以及 cross-docking 的上游中心等必要业务属性；在确认前不要从行政区目录静默生成候选仓。

## 交接结果

- 将 provider 拥有的中间结果保存在允许 Tool 返回的精确 MCP Resource 中，把原样 ResourceRef 返回给 Supervisor；不得重建 URI、添加内容哈希或改称 Artifact。
- Tool 已经返回精确 ResourceRef 后直接保留并交接该引用；不得再枚举全部 MCP Resources、Resource templates 或 provider 私有目录来重新发现同一 Resource。
- 只有用户要求保存可见结果或需要显式跨 package 交接时，才写普通 Workspace 文件，并使用 create-new 语义；冲突时报告而不覆盖。
- 不计算覆盖策略、成本最优、时效最优、场景或选址，不直接创建地图和报告，不创建 Platform workflow，也不用 Artifact 传递中间数据。
