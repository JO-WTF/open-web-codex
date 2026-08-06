# 印尼仓网 4：有限候选优化、确定性报告与地图

本篇完成完整案例：

1. 核验数据；
2. 计算当前时效和两级运输成本；
3. 在 20 个已审核候选中选择一个新前置仓；
4. 要求 2 天需求覆盖率至少达到 74%；
5. 在满足目标的候选中最小化年度决策成本；
6. 创建当前方案与入选方案的地图卡片；
7. 由 Network Tool 从七个精确来源 Resource 确定性生成完整报告；
8. Root Supervisor 只交付 `report.v1` 和可选的 `map.v3` 类型化引用。

比上一篇增加的是“完整有限候选优化”和独立 Visualization Agent。仍然不做连续空间
任意点求解，也不调用导航 API。

预计用时 30–45 分钟。先完成
[印尼仓网 3](indonesia-network-03-two-level-cost.md)。

## 已知答案

确定性优化会评估全部 20 个候选，并选择：

```text
CAN-JAMBI · Jambi Candidate
status = target_met
```

| 指标 | 当前 | Jambi | 增量 |
| --- | ---: | ---: | ---: |
| 1 天需求覆盖率 | 30.42% | 31.20% | +0.78 个百分点 |
| 2 天需求覆盖率 | 71.80% | 75.33% | +3.53 个百分点 |
| 3 天需求覆盖率 | 90.60% | 92.22% | +1.63 个百分点 |
| 年运输成本 IDR | 110,024,697,400 | 107,189,163,600 | -2,835,533,800 |
| 年度决策成本 IDR | 110,024,697,400 | 123,614,163,600 | +13,589,466,200 |

Jambi 承接 250,000 个年度需求单位，达到候选容量上限。它不是“印尼任意位置中的数学
最优点”，而是“当前 20 个已审核候选中，满足 74% 目标后年度决策成本最低的候选”。

## 1. 发布完整 Network Agent 版本

在 `tutorial-indonesia-network` 定义中点击 **New version**，创建 `3.0.0`。

Display name：

```text
Tutorial Indonesia Network Planner
```

Description：

```text
Evaluates the current network, the complete reviewed candidate set, and bounded map evidence.
```

**Responsibilities**：

```text
读取一个经过验证的印尼 Dataset inspection
核算当前服务基线和两级仓网事实
在完整 20 候选集上执行确定性目标约束优化
为入选方案发布兼容的 scenario、map manifest 和 GeoJSON Resources
从七个精确来源 Resource 生成权威决策报告 Resource 和 report.v1 交付 Artifact
```

**Custom Agent instructions**：

```text
开始前必须收到完整的 indonesia_dataset_inspection.v1 resource_name 和结构化 data_ref。
把 resource_name 原样作为 inspection_resource_name 传给 Domain Tool；data_ref 只保留
为交付来源，不作为 Tool 输入。缺少精确 resource_name 时停止，不列出 MCP Resources、
不读取 inspection、不扫描 Workspace、不猜 URI。

按任务需要选择最小分析。完整报告需要 service baseline 与 current Resource；需要有限
候选优化时，把用户给出的 target_service_days、target_demand_coverage 和
opening_amortization_years 原样传给 optimize_indonesia_new_warehouse。必须接受 Tool
对完整 20 候选集的结果，不按城市偏好改选。

优化 Tool 返回入选 candidate scenario 后，只有用户要求地图时才调用
prepare_indonesia_network_map，并把兼容 current 和 selected scenario 的精确
resource_name 分别作为 baseline_resource_name 和 candidate_resource_name。不得调用
导航、读取客户点、自己生成候选、复制内部 URI 或用模型重算成本。

每个 Domain Tool 在发布前原子校验 Resource。完整任务取得 inspection、service、
current、optimization、selected scenario、network map 和 geojson 的精确
resource_name 后，调用 prepare_indonesia_decision_report。只传这七个同 Server
Resource 名；不要传地图 Artifact ID 或 embed。Tool 发布
indonesia_decision_report.v1 Resource 和配对的 report.v1 交付 Artifact。报告 follow-up
只返回单行 REPORT_HANDOFF，其中包含 report_resource_name 和 report_artifact_id；一个
空行后独立复制 Tool 生成的报告 embed 一次。不要读取或复制报告 Markdown，不要增加前言
或结论，也不要调用 create_map_card。
```

Artifact contracts：

```text
Input · indonesia_dataset_inspection.v1
Output · indonesia_service_baseline.v1
Output · indonesia_current_network_analysis.v1
Output · indonesia_candidate_scenario.v1
Output · indonesia_location_optimization.v1
Output · indonesia_network_map.v1
Output · geojson.v1
Output · indonesia_decision_report.v1
Output · report.v1
```

依次点击：

```text
Save draft → Validate → Publish
```

## 2. 使用已有 Visualization Agent

本篇不需要再创建一个自定义地图 Agent。平台已经发布：

```text
Enterprise Visualization Agent · 2.0.0
```

它的权限边界是：

| 能力 | 是否允许 |
| --- | --- |
| 按精确 Resource 名调用 `prepare_indonesia_map_render` | 是 |
| 直接读取 map / GeoJSON Resource | 否 |
| 调用 `map_utils.create_map_card` | 是 |
| 读取客户明细 | 否 |
| 重新计算网络方案 | 否 |
| 调用仓网优化 Tool | 否 |
| 创建下级 Agent | 否 |

Visualization Agent 的职责不是“做分析”，而是把已经验证的地图说明和 GeoJSON 交给
浏览器渲染能力。它的单行 `MAP_HANDOFF` 只返回两个输入 Resource 名和地图 Artifact
ID，保留输入 provenance，但不包含 Resource URI 或 embed code。Tool 的
`structuredContent.embed.code` 在一个空行后作为独立指令出现一次，不能转义为
`MAP_HANDOFF` JSON 字符串，也不能根据 Artifact ID 重建。平台把该指令解析为事件的
类型化 `inlineArtifacts` 投影，浏览器只用这份投影解析同一个地图 Artifact。这样地图
来源、模型文本与浏览器渲染不会形成相互漂移的重复状态。

如果你的领域已有满足职责和权限的 Agent Release，应直接复用；不要为了显示自定义名称
复制一份相同 Agent。

## 3. 发布三 Agent Supervisor 版本

在 `tutorial-indonesia-network-supervisor` 中点击 **New version**，创建 `3.0.0`。

Allowed Agents：

- `Tutorial Indonesia Data Agent · 1.0.0`；
- `Tutorial Indonesia Network Planner · 3.0.0`；
- `Enterprise Visualization Agent · 2.0.0`。

设置 **Maximum active child Agents = 3**。这是并发和驻留上限，不是要求每次都创建
3 个 Agent。

选择 handoff：

| Artifact | Producer | Consumer | Required |
| --- | --- | --- | --- |
| `indonesia_dataset_inspection.v1` | Data | Network | 否 |
| `indonesia_service_baseline.v1` | Network | Supervisor | 否 |
| `indonesia_current_network_analysis.v1` | Network | Supervisor | 否 |
| `indonesia_candidate_scenario.v1` | Network | Supervisor | 否 |
| `indonesia_location_optimization.v1` | Network | Supervisor | 否 |
| `indonesia_network_map.v1` | Network | Visualization | 否 |
| `geojson.v1` | Network | Visualization | 否 |
| `indonesia_decision_report.v1` | Network | Supervisor | 否 |
| `report.v1` | Network | Supervisor | 否 |
| `map.v3` | Visualization | Supervisor | 否 |

**Custom Supervisor instructions**：

```text
你是 Root Supervisor，对最终印尼仓网决策报告负责。先整理用户目标和当前 Task 已有证据，
只调用能够补齐未解决证据缺口的已绑定 Agent。不要以固定角色数量、固定顺序、固定城市
或固定 Tool 次数作为完成条件。

Data Agent 只负责精确 Dataset inspection；Network Agent 负责当前网络、完整有限候选
优化和有界地图数据；Visualization Agent 只在已有兼容 map manifest 和 GeoJSON 且用户
要求地图时使用。Root 不调用业务 MCP、不读取 Workspace、不复制大数据，也不替代专业
Agent 计算。

所有跨 Agent 交接必须包含原样 schema、resource_name 和完整结构化 data_ref。消费方只把
resource_name 作为同一 supply_chain_indonesia Server 内 Domain Tool 的输入；Server
负责解析自己的 Resource Store，data_ref 只保留为交付来源。缺失时向原生产者请求补全；
不得列出 Resources、猜 URI 或按名称搜索替代 Artifact。Tool 或 Agent 失败时保留已完成
证据并报告未完成部分。

完整报告必须由同一个 Network Agent 调用 prepare_indonesia_decision_report 生成。调用
前必须具备 inspection、service、current、optimization、candidate、map 和 GeoJSON
七个精确 Resource 名；任何一项缺失都先补证据，不得用聊天摘要或猜测名称代替。Tool
拥有 indonesia_decision_report.v1 的内容、schema、digest、精确来源声明和类型化
checks，并返回配对的 report.v1 交付。Root 不自行编写、压缩或润色报告；
模型正文不是报告来源。

Network Agent 的 `REPORT_HANDOFF` 只允许包含 report_resource_name 和
report_artifact_id；其后独立出现的报告 embed 必须来自 Tool，并引用同一个 Artifact。
如果 Visualization Agent 返回 `MAP_HANDOFF`，必须核对其中的 map manifest、GeoJSON
Resource 名和地图 Artifact ID；该 JSON 不得包含 Resource URI 或 embed code。它后面
独立出现的地图 embed 必须来自 Tool 的 `structuredContent.embed.code`，并引用同一个
Artifact ID。最终响应只交付精确 `report.v1` embed；要求地图时再空一行交付精确
`map.v3` embed。不得根据 ID 重建指令，不得增加模型业务正文，也不得把指令放进代码
围栏、行内代码、列表、引用、表格或 HTML。
```

完成：

```text
Save draft → Validate → Publish
```

Supervisor 没有写死“Data → Network → Visualization”，所有 handoff 都是条件性的。
本篇 Prompt 同时要求数据核验、网络优化和地图，所以一个全新 Task 通常会自然形成这条
依赖路径；如果问题不要求地图，Visualization Agent 应被跳过。同一 Task 的后续问题
可复用仍有效的 inspection；当前版本不宣称支持跨 Task Artifact 复用。

## 4. 启动完整案例

在 Workspace 点击 **New task**。在 **Start a task** 中选择 **Supervisor** 和
**Tutorial Indonesia Network Supervisor · 3.0.0**，发送：

```text
分析现有印尼仓库网络并给出建议。

我需要知道当前网络的一日、两日和三日需求覆盖率、全网干线与末端年度运输成本、各省
时效表现，并按数据合同声明的省份排名口径分别列出表现最好和最需要改善的前三个省份。
然后判断能否在完整的已审核候选点中新增一个前置仓，使两日需求覆盖率达到 74%；建设
费用按 5 年摊销。请比较当前方案与入选方案的一日、两日、三日覆盖率，以及干线、末端、
运输总成本、年度固定成本、摊销建设成本和年度决策总成本，并创建一张地图展示两者差异。

请根据尚未解决的证据问题动态决定需要哪些 Agent，不要为了凑数量运行角色，也不要按
写死的 Agent 顺序执行。只使用平台授权的数据与类型化 Artifact；不要扫描 Workspace、
运行终端命令、读取原始客户行、调用导航服务或自行编造候选地点。距离采用教程声明的
球面距离乘系数方法，司机每天可行驶 6 小时。

完整报告必须由 prepare_indonesia_decision_report 对七个精确来源 Resource 交叉校验后
确定性生成；不要由模型重新组织。优化状态和金额保持 Tool 原样枚举与精确 IDR 整数，
不要换算单位或自行计算新比例。最终先交付平台验证的 report.v1，再交付独立的 map.v3；
不要复制或改写报告正文。
```

## 5. 审阅动态执行

这个完整问题通常需要三个角色，但正确性不由精确调用顺序决定。检查：

- Root 没有 Shell、文件扫描或业务 MCP 调用；
- Data 只调用 inspection/validation 能力；
- Network 只使用同一 inspection Resource；
- 优化 Tool 的 `evaluated_candidate_count` 为 20；
- `target_met_candidate_count` 为 6，并且与 evaluations 中 `target_met=true` 的数量一致；
- 入选 ID 为 `CAN-JAMBI`，状态为 `target_met`；
- Visualization 只收到 map manifest 和 GeoJSON，不收到客户数据；
- 同一角色没有超过 spawn limit；
- Agent 的 running、waiting、completed 或 failed 状态有稳定身份和非空说明；
- 任一失败都形成明确终态，不永久停在空白 wait。

允许的变化：

- 独立信息核对可以并行；
- Agent 可以先读取 handoff 再调用 Tool；
- Root 可以向同一个已创建 Agent 追加补充问题；
- 同一 Task 已有有效 Artifact 时可以跳过对应生产者；
- 不要求模型额外调用独立 validator，因为 Domain producer 已原子校验。

## 6. 审阅 Artifact 与地图

本篇完整任务应产生以下八个 Resource Artifact：

```text
indonesia_dataset_inspection.v1
indonesia_service_baseline.v1
indonesia_current_network_analysis.v1
indonesia_location_optimization.v1
indonesia_candidate_scenario.v1
indonesia_network_map.v1
geojson.v1
indonesia_decision_report.v1
```

这八项是本篇 Prompt 要求完整报告和地图后的证据集合，不是所有未来 Supervisor 任务的
固定 Workflow。只问局部问题时，未需要的条件性 Artifact 不应为了凑数量而生成。最终
响应还必须包含 Network Tool 交付的一个 `report.v1` 卡片，以及 Visualization Agent
交付的一个 `map.v3` 卡片；两者属于回复交付，不要用 Agent Activity 中 Resource
Artifact 的数量推断它们是否存在。

验收报告时，读取权威 `indonesia_decision_report.v1` Artifact，校验其 schema/version、
非空 payload 与重算 digest、七个精确来源 Resource 身份、类型化 checks、Dataset
Release 身份和 producer provenance，再确认 `report.v1` 的类型化 `inlineArtifacts`
投影指向同一 Artifact。不要校验报告措辞或模型消息正文：模型只传递有界 handoff 和
Tool 生成的 embed，不拥有报告内容。

地图 GeoJSON 是有界证据，只包含：

- 当前中心仓和前置仓；
- 入选 Jambi 候选仓；
- 当前和候选干线；
- 省级需求中心和时效差异摘要。

它不包含 240,000 个客户点，feature 数量必须小于 200。

`map_utils.create_map_card` 成功只创建 Artifact。地图真正显示还要求 Root 最终消息把
返回的嵌入指令作为独立段落输出。以下写法会被安全解析器拒绝：

```text
代码围栏中的指令
列表项中的指令
引用块中的指令
模型重新拼出来的近似指令
```

如果浏览器尚未配置公开 Mapbox Token，卡片仍应恢复为 `Ready`，并显示摘要、图例和
“配置 Mapbox Key”操作，但不会绘制地理底图。这是明确的凭据缺口，不是分析失败。

## 7. 审阅建议

正确建议应表达：

- 当前 2 天需求覆盖率为 71.80%，低于 74% 目标；
- Jambi 将其提升到 75.33%，目标成立；
- Jambi 让运输成本每年下降 IDR 2,835,533,800；
- 加上固定费用和开仓摊销后，年度决策成本增加 IDR 13,589,466,200；
- 因此它是“达到本目标的最低年度决策成本候选”，不等于“节省总成本”；
- 是否投资还需要服务改善的商业价值、实施可行性和真实路线复核。

Supervisor 可以建议进入更详细的商业论证，不能把教程计算当成最终投资批准。

## 8. 把方法迁移到真实场景

真实项目需要替换：

- 合成客户、仓库、分配、容量和报价；
- 规划系数和服务承诺口径；
- 候选点生成和审核流程；
- 开仓、固定和资本成本模型；
- 数据时效、质量阈值和审批规则。

可能需要新增独立 Agent：

- Finance Agent：只有当现金流、折现和投资回报会改变建议时；
- Risk Agent：只有当实施风险、数据不确定性或敏感性需要独立登记时；
- Routing Agent：只有当入围路线需要有缓存的导航矩阵时。

不要因为未来可能需要，就把这些职责全部写进当前 Supervisor。先定义清晰输入、
确定性 Tool、交付 Artifact 和停止条件，再发布新的 Agent Release。

完成后返回：

[教程标准与学习路径](README.md)
