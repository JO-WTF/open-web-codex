# ADR-023：Workspace 准备输入、可验证计算血缘与领域地理执行

状态：Accepted

日期：2026-08-18

补充并局部替代：[ADR-018](018-built-in-network-copilot-runtime-closure.md) 中 Data→Network 必须通过
`normalized_network_input.v1` MCP Resource 交接，以及导航需人工导入 Workspace JSON 的实现形态。
ADR-018 的 Runtime、Workspace 授权、MCP Resource、Artifact 与 Platform 所有权边界继续有效。

## 背景

此前 Data Agent 将完整清洗输入发布为 provider Resource，Network Agent 只接收该 ResourceRef。
这让数据准备成为跨 Agent 的中间结果转运，用户无法在 Workspace 审阅可复用输入；候选变化还需要
delta Resource 与派生 Resource。导航只产生计费估算，要求用户将外部 Provider 结果手工写成 JSON，
再由 Planner 登记。路线、成本、基线与方案的 Resource 又没有统一的输入身份，存在不同数据输入被
结构上有效但语义上错误地混用的风险。

## 决定

### 1. Data→Network 的唯一业务输入是准备好的 Workspace 文件

Data Tool 仍可在同一 Data Agent 内部发布 `source_profile.v1`，以避免把原始行带入模型上下文；但它
不再把完整规划输入作为跨 Agent Resource。`prepare_network_input` 与
`prepare_network_geography` 以 create-new 原子写入一个用户可见的
`prepared_network_input.v1` JSON，并返回：

- `prepared_input_relative_path`；
- `input_identity = { schema_version, content_sha256 }`；
- `ready` / `needs_input` / `needs_geography` 与有界质量结论。

该文件本身包含确认过的来源、映射、候选仓和质量问题；其 `issues` 是数据质量报告。候选、需求、
现网仓、分配、路线事实或报价的任何变化都产生新的完整准备输入，不再使用 candidate delta 或
in-place 修改。Network Agent 不读取 raw 文件；它只把确切相对路径交给 Network Tool，由 Tool 在
授权 Workspace 内解析、验证与读取。

这不是 Platform Data Object、版本表、`head`、数据 registry 或“最新输入”推断。跨独立 Task 的
复用仍由用户明确选择 Workspace 文件完成。

### 2. 计算 Resource 必须绑定输入身份，但不以整份数据作为缓存门

`route_matrix.v2`、`cost_matrix.v2`、基线、场景、选址结果和比较结果都记录
`input_identity`。每个消费 Tool 将当前准备输入的身份与所给计算结果逐一比较；不同则明确失败。
报告和地图只从一致的比较结果读取。

身份用于防止混用和检测用户手工修改后的过期矩阵，不用于决定能否复用。路线/成本仍按 lane/pair 的
端点、Provider、profile、规则与事实精确性复用；不因整份输入摘要变化而自动禁止仍有效的 pair。

### 3. 导航按三段领域合同自动执行

1. Planner `create_navigation_matrix_request` 从准备输入生成准确 lane 集、复用同输入已有导航 pair，
   并把缺失 lane、范围和估计计费元素写入 Workspace。
2. `map_utils.execute_navigation_matrix` 在原生外部费用确认后按 request 分批调用已配置 Provider，
   将 ready、unreachable 与 provider-error lane 写入 Workspace 结果文件；它不接受模型拼装的
   路线 JSON。
3. Planner `import_navigation_matrix` 验证输入身份、端点、scope、Provider provenance 和 prior matrix，
   再发布可计算的 `route_matrix.v2`。

外部执行、不可达和 Provider 故障是不同终态；不回退到 haversine，也不把零距离作为成功路线。

### 4. 地图拥有领域装配，不把业务样式留给模型猜测

`create_network_map_card` 消费 Network Tool 的确切 GeoJSON `data_ref`，从 profile 构建需求点、
末端线、干线、现有中心仓、现有 XD、候选仓和设施状态图层。模型仅在用户明确要求超出领域默认值的
样式时才调用底层 `create_map_card`。用户提供的行政区 GeoJSON 先经
`publish_workspace_geojson(require_polygon=true)` 校验和发布，再作为独立 polygon source 传给领域卡片。

地图 Provider 不可用只使地图/导航能力明确 unavailable；它不阻断数据、路线假设、成本、分析或选址。

### 5. 单/多 Agent 共用同一合同

单 Agent 与多 Agent 使用相同准备输入、输入身份、矩阵、方案、地图和报告合同。多 Agent 仅由
Supervisor 在需要准备数据时派发 Data child，随后续派同一 Network child；它不搬运业务数据或建立
Platform 工作流。两个 Copilot 仍是 ADR-019 要求的独立 Root package。

## 明确不做

- 不修改 Codex Runtime、Thread/Turn、Skills/MCP discovery 或 native Agent 协作；
- 不增加 Platform 仓网字段、数据 API、数据状态机、Resource Broker、任务快照或版本 registry；
- 不保留 Resource 与 Workspace 输入的双读/双写兼容；
- 不把导航 Provider 密钥、费用、原始响应或行政区内容暴露给浏览器或模型上下文。

## 验证

- Data Tool：原子写入、质量状态、候选完整重建、地理派生、路径逃逸与重复写入；
- Network Tool：不同输入身份的矩阵/基线/方案/报告/地图均拒绝，未修改 pair 可经 prior Resource
  复用；
- Navigation：request 计费元素、外部确认、Provider chunk、unreachable、error、取消、import 与
  prior 合并；
- Map：点/线/面、仓库三个正交属性、状态层、行政区 polygon、缺失地图能力；
- 同一 fixture 在单/多 Agent 下得到相同业务结论和最终交付合同。
