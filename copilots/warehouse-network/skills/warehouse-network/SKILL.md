---
name: warehouse-network
description: 仅供仓网 Supervisor 原生创建的 network_agent 使用。用于定义数据要求，计算路线、成本和时效达标率，模拟仓库变动，执行 p-median 选址，以及生成仓网地图和报告。输入必须是精确 typed MCP ResourceRef 或用户确认的 Workspace 相对路径。
---

# 分析仓网

只在当前 Role 为 `network_agent` 时执行。仅使用已允许的 MCP Tool 和精确 ResourceRef；不用 shell、Git、Workspace 扫描、Resource 枚举或内联代码重建数据和指标。缺少 Tool 必需输入时返回 `needs_context` 或 `needs_data` 并停止。

## 通用执行规则

- 以准备调用的 Tool schema 为交接边界：只传它实际接受的必需引用、参数、用户许可和交付要求。
- 直接沿用用户或当前 Thread 已经明确的选择；不重复询问。
- Tool 成功后仅使用同一次 structured result 中的指标和下游引用。result 与 comparison 必须来自同一精确结果。
- Tool 返回的交付描述和 Platform 终态就是成功证据；不用文件或 Resource 重读二次核验。

## 数据与矩阵

- 最低数据包括需求城市 ID、名称、需求量，以及已有仓库 ID、名称、仓型和城市。真实现状对比需要当前分配，选址需要候选仓，成本和时效分析需要对应的报价、距离或时长。
- 只消费 Supervisor 提供的 `ready` `normalized_network_input.v1` ResourceRef：完整对象必须为 `server=supply_chain_data`、精确 `uri` 与该 schema，并原样作为 `normalized_input_ref` 传给 Network Tool。不得调用 `list_mcp_resources`、`list_mcp_resource_templates` 或 `read_mcp_resource` 来寻找、读取或验证它；Network Tool 会在 provider 内部消费该引用。上游国家代码必须保持 ISO 两位大写形式，有误时让 Data Agent 重新发布，不在 Network 层修补。
- 依用户目标选择 `warehouse_scope`：当前仓网或真实现状使用 `existing_only`；明确包含候选仓的模拟或规划使用 `all_warehouses`。
- 需要展示方案后的覆盖关系时，调用 `prepare_network_coverage_map` 并传入同一 exact normalized input 与已求解的方案结果；它生成坐标直线，不调用道路导航。
- 调用 `prepare_route_matrix` 选择 `provided`、`haversine` 或 `navigation`。已有 provided 路线事实适用时直接物化；否则才确认曲面距离估算或地图导航。估算需要绕路系数和平均速度；navigation 返回请求量与费用估算后，取得许可再调用外部地图能力并注册结果。
- 成本优先使用用户报价。`route_quotes` 为空且用户没有确认包含 `rules` 的补算规则时，不调用 `plan_cost_matrix`，不虚构币种或费率；直接将成本标为不可用并说明所需数据。

## 时效、模拟与选址

- 用户只问“当前仓网 N 小时时效达标率”时，在已有仓范围按 `min_time` 计算，同时返回城市等权时效达标率与需求量加权时效达标率。provided 时长存在时明确这是纯运输时长。
- 成本最优必须有完整成本矩阵，时效最优必须有完整路线矩阵。`current_assignments` 为空时直接使用 `optimized_existing_footprint`，不先试调 `actual_current`；只有输入包含当前分配时才将基线称为“真实现状”。
- 增加、关闭或搬迁单个仓库时调用 `assess_facility_change`；关闭已有仓必须获得用户许可。
- 只有用户要求仓网规划时才调用 p-median。默认保留全部已有仓；仅当用户明确允许时，才将指定已有仓列为可关闭。明确新增仓数、时效目标、求解时限和时效达标约束。
- 比较时报告成本、时效、活动仓库、受影响城市和重新分配城市；只使用 typed Tool 实际返回的字段。

## 地图与报告

- 只展示当前分布时调用 `prepare_network_distribution_map`，默认不包含候选仓。展示已有基线时把精确 `baseline_ref` 一并传入，使需求城市带有原始 `assigned_warehouse_id`、`distance_km`、`duration_hours` 和 `unit_cost`。已有前后方案比较时，把 `compare_network_scenarios` 或 `assess_facility_change` 返回的单一 `plan_comparison_ref` 传给 `prepare_network_comparison_map`；该引用已经绑定标准化输入、前方案、后方案和比较结果，需求城市会带有对应的 `baseline_*` 与 `facility_*` 原始字段。不分别拼装四个引用，也不为地图重算路线、成本或方案。
- 地图的标题、图层、筛选、颜色、大小、标签、悬浮信息和图例都属于当次展示意图：优先服从用户当前自然语言要求，再结合本次分析选择清晰表达；不得把展示样式当成业务数据或达标判定写回 Resource。
- 用户未指定样式时，采用下列默认参考；用户指定的样式优先。尺寸单位为屏幕像素。

  | 地图对象 | 默认参考样式 |
  | --- | --- |
  | 中心仓（`warehouse_type=center`） | 深蓝 `#1D4ED8`，半径 `11`，白色描边 `2.5` |
  | XD 前置仓（`warehouse_type=cross_docking`） | 橙色 `#F97316`，半径 `8`，白色描边 `2` |
  | 候选仓（`is_existing=false` 且 `opened_candidate=false`） | 紫色 `#7C3AED`，半径 `7`，白色描边 `2`，不透明度 `0.9` |
  | 新增启用仓（`opened_candidate=true`） | 洋红 `#C026D3`，半径 `11`，白色描边 `2.5` |
  | 需求城市（没有时效结果） | 蓝色 `#2563EB`，半径 `5`，白色描边 `1.25` |
  | 时效达标城市 | 绿色 `#16A34A`，半径 `6`，白色描边 `1.5` |
  | 时效未达标城市 | 红色 `#DC2626`，半径 `7`，白色描边 `1.75` |
  | Last mile 覆盖线（`kind=last_mile_assignment`） | 蓝色 `#2563EB`，宽度 `1.5`，不透明度 `0.6` |
  | 干线覆盖线（`kind=linehaul_connection`） | 深蓝 `#1E3A8A`，宽度 `2.5`，不透明度 `0.75` |

  仓库点图层置于需求城市之上；线图层置于所有点图层之下。图例仅列出本次实际展示的对象，并分别保留中心仓、XD 前置仓、候选仓、新增启用仓、时效达标、时效未达标、Last mile 和干线的名称。
- Planner 返回的 `data_ref.profile` 是从本次完整 GeoJSON 自动生成的有界概览，也是图层字段、类型、枚举值和几何类型的唯一模型可见真相。按它的 `discriminator_property` 与 `feature_types` 构造每个图层；筛选、表达式、标签和 tooltip 只使用对应 feature type 的 `properties`，不得从业务名或历史地图猜字段。纯分布数据没有时效字段时不猜测达标状态。
- 为仓库、需求城市和干线连接配置 `extensions.hover`。从对应 feature type 的可用字段中选择人类可读名称作为标题，并按本次实际存在的字段展示业务标识、仓型或需求量、服务关系、运输时长、距离、成本和方案状态；缺失字段直接省略，不制造通用 `name` 或其他替代字段。
- 将 Planner 返回的完整 `data_ref`（包括 `profile`）原样放入 `map_utils.create_map_card` 的 GeoJSON source，自行按上述展示意图构造标准 Mapbox Style `layers` 与可选 hover/legend；不得读取完整 Resource 内容，也不得改写数据引用、概览、原始指标或用户选择的阈值。地图 Tool 会依据 profile 拒绝不存在的字段和不兼容的几何图层。再将 `structuredContent.embed.code` 原样作为独立段落返回。地图 Tool 失败时明确报告，不制作 HTML、图片或文本地图替代。
- 用户要求“基于此图修改”时，当前 Turn 会带入用户显式选择的 exact `map_card_spec.v1`。纯标题、图层、颜色、图例、悬浮或视角调整只调用 `revise_map_card` 并提交有界 patch；不得重新读取或发布 GeoJSON。若要求新增城市→仓库覆盖线，先用 exact 分配结果调用 `prepare_network_coverage_map` 发布新几何，再创建或修订一张引用该几何的新卡片。
- 完整分析、模拟或规划达到可交付终态时，只调用一次 `publish_network_planning_report`，生成 create-new 中文 Markdown 报告。正文只概括关键结论，并原样保留 Tool 返回的链接。
- 只有用户明确要求导出地图文件时才调用 `render_network_comparison_map`。中间 Resource 和导航文件不是 Artifact。
