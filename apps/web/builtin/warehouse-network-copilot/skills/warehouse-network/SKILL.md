---
name: warehouse-network
description: 仅供仓网 Supervisor 原生 spawn 的 network_agent child 使用。当用户要求定义仓网数据需求，计算路线、成本、覆盖关系、时效满足率、仓网模拟或 p-median 选址，或者生成仓网地图与报告时，使用此 Skill。输入使用精确 typed MCP ResourceRef 或用户确认的 Workspace 相对文件。
---

# 分析仓网

只有当前 Role 正是由 Supervisor 原生 spawn 的 `network_agent` child 才能继续，且 child 不得加载或反向读取 `warehouse-supervisor` Skill。若当前是 Root 或其他 Role，立即停止，不读取数据、不调用 Tool，改用 `warehouse-supervisor`，由 Root 通过原生 spawn 创建网络代理。

所有分析必须直接消费上游 exact `ResourceRef` 和当前 Tool 返回的 bounded summary。不得使用 `ls`、`find`、`git`、`jq`、`cat`、内联 Python 或其他 Workspace 命令重读原始文件、中间 Resource、旧 Artifact 或最终报告来重建指标；Tool 合同不足时返回明确缺口，不得用 shell 旁路补协议。

## 定义数据要求

- 先根据用户目标向 Supervisor 说明必要数据和决策参数，不直接读取用户上传的原始业务文件；文件检查、字段映射、标准化和地理补全由 Data Agent 完成。
- 最低数据要求为：需求城市的 ID、名称和需求量；已有仓库的 ID、名称、仓型（`center` 或 `cross_docking`）及所在城市 ID、名称。
- 仅在对应分析需要时追加要求：真实现状比较需要当前覆盖；选址需要候选仓；成本分析需要报价或明确的补算规则；时效分析需要路线距离/时长；地图需要完整坐标。
- 只读取 Supervisor 提供的精确 `normalized_network_input.v1` ResourceRef。若状态不是 `ready`，说明缺口并停止，不绕过 Data Agent 或自行猜测输入。
- 如果 Supervisor 没有提供精确 ResourceRef，或者该 Resource 读取失败，立即向 Supervisor 返回 `needs_data` 和原始失败原因并停止。不得从 Workspace 文件、旧 Artifact、报告、模型文本、MCP Resource 列表或 provider 私有目录搜索、推断或恢复替代引用，也不得直接解析文件完成仓网任务。
- 用户提示词或当前 Thread 已经明确给出路线、成本、目标、时效、仓库变更或交付选择时，直接沿用，不重复询问同一选择。

## 构建路线与成本事实

- 先确认路线方案：曲面距离乘绕路系数，或者地图导航。
- 选择曲面距离时，要求明确绕路系数和平均速度；上下文没有时必须询问，不能使用国家或案例隐藏默认值。
- 选择地图导航时，先用规划 Tool 计算待请求的路线数量，向用户说明潜在接口调用量和费用，并取得明确许可后再调用地图工具。地图 provider 的结果只能通过经过验证的 Workspace 相对 JSON 文件注册。
- 路线矩阵覆盖（已有仓库 + 已确认候选仓库）到需求城市的必要末端路线，以及 cross-docking 所需的上游干线路线。复用能够逐对验证的既有路线，只计算缺失 pair，并报告复用数、计算数和缺失数。
- 成本矩阵优先使用用户报价。报价缺失时，明确列出缺失范围并询问统一币种和补算规则，例如每公里每单位成本、基础成本和车型容量；没有规则时保持 typed missing，绝不按零成本处理。
- 构建成本时明确使用“仅已有仓库”还是“已有 + 候选仓库”，并逐对复用可验证的已有成本事实。

## 计算覆盖、时效和成本

- 在覆盖计算前确认优化目标是 `min_time` 还是 `min_cost`。若用户只问时效覆盖且未指定成本优先，建议 `min_time`；若用户只问全网成本且未指定时效优先，建议 `min_cost`，并把选择讲清楚。
- 为每个需求城市计算唯一服务仓库，并保留对应距离、时长和成本。需要成本最优时必须提供完整成本矩阵；需要时效最优时必须提供完整路线矩阵。
- 询问一个或多个时效目标，例如 6、12、18 小时；按需求量计算每个目标的全网满足率。分仓库满足率必须来自确定性的 typed Tool 输出；当前合同没有该字段时明确说明能力缺口，不让模型自行汇总。不要擅自填入案例专属目标。
- 全网成本同时给出总成本和分仓库成本，明确币种与包含的干线/末端成本范围。
- 只有标准化数据包含当前覆盖时，才把基线命名为“真实现状”；否则明确使用“现有仓网优化覆盖”，不能把模型重算结果伪装成实际现状。

## 仓网模拟与规划

- 模拟增加、关闭或搬迁仓库时，复用当前有效的数据、路线、成本和基线 Resource。搬迁等价于关闭一个已有仓并启用一个候选仓。
- 模拟默认建议成本最优，但要向用户说明；上下文没有时效目标时先询问。输出活动仓库、仓库变动、城市重新分配、总成本、分仓库成本和全网时效满足率；分仓时效同样只能使用 typed Tool 结果。
- 关闭已有仓库必须得到用户明确许可；只关闭仓库的模拟不运行 p-median，也不创建候选仓、地图或报告，除非用户另有要求。
- 用户要求仓网规划时才调用 p-median。默认把全部已有仓库列入固定集合并提前告知；只有用户明确允许时，才把指定已有仓库列入可选集合。固定集合与可选集合必须完整、不重叠。
- p-median 必须明确新增候选仓数量、时效目标、求解时限和任何覆盖约束。默认优化方向可以建议“满足已确认时效约束下成本最低”，但时效阈值和覆盖率不能凭空生成。
- 输出方案总成本、分仓库成本、覆盖关系、各时效目标满足率，以及新增、保留和关闭的仓库；区分最优、仅可行、超时、不可行和不可用状态。
- 使用 comparison Tool 比较基线与模拟或规划结果，报告受影响城市、重新分配城市、成本变化、时效变化和仓库变动。

## 地图与交付

- “展示地图”“看看分布”“地图可视化”默认表示对话内地图卡片，不表示文件交付。不得因此生成网页、PNG、Workspace JSON 或调用 final Tool。
- 只展示需求城市和当前仓库分布时，调用 `prepare_network_distribution_map` 把精确标准化输入转换为 bounded GeoJSON Resource；该 Tool 不计算路线、成本、覆盖或优化。默认 `include_candidates=false`，除非用户明确要求展示候选仓。
- `prepare_network_distribution_map` 会返回 typed `map_card_handoff`。立即调用其中 `tool.server=map_utils`、`tool.name=create_map_card` 指定的 Tool，并把 `map_card_handoff.arguments` 整体原样作为调用参数；不得读取 GeoJSON、重新设计 sources/layers/legend、搜索其他地图模板或改写 handoff。用户要求展示候选仓时只通过前一 Tool 的 `include_candidates=true` 产生对应 handoff。
- `create_map_card` 成功后，把其 `structuredContent.embed.code` 原样作为独立段落放入回复，地图才会在 Web 对话中显示。不得只描述卡片已经创建，也不得把 embed 放进代码块、列表或引用。
- 分布地图链中任一 Tool 失败时立即返回明确失败；不得改用 Leaflet/HTML、脚本、GeoJSON/PNG 文件、Markdown 图片或其他自制展示。
- 行政区目录用于校验名称和坐标，不等于行政边界 GeoJSON。只有存在经过验证的边界 GeoJSON Resource 时才能叠加自定义边界；否则使用平台底图并明确说明，不从目录行伪造多边形。
- 地图卡片不是计算输入。用户要求查看当前分布时，不调用路线矩阵、成本矩阵、baseline、comparison 或 p-median。
- 用户明确要求可下载交付时，使用仓网 final Tool 生成自包含的对比地图 JSON 和规划报告 JSON。传入精确的标准化、基线、选址结果和 comparison ResourceRef，以及 create-new Workspace 相对输出路径。
- 中间 Resource 和导航文件不得注册为 Artifact；Artifact 只用于显式最终地图与报告。

## 共同约束

- 把这些 Tool 当作可组合能力，不要固化为单一 workflow；只调用当前用户目标需要的步骤。
- 不构造 Resource URI，不从标题或模型文本推断引用，不把 Workspace 路径冒充 Resource，不创造 Case、隐藏 revision 或第二业务状态机。
- 不编造路线、报价、成本规则、坐标、时效目标、仓库许可、求解结果、Runtime 就绪状态或交付完成。
