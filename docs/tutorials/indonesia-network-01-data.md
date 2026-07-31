# 印尼仓网 1：发布并核验大型规划数据

本篇只完成一件事：把印尼仓网的 10 个文件作为一个不可变 Dataset Release 发布，并让
Data Agent 核验它。暂不计算时效、成本或选址。

比[单 Agent 配送审计](web-single-agent-delivery-audit.md)增加的难点是：数据规模更大、
文件之间有关联、客户坐标必须位于省界内，并且结果要成为后续 Agent 可复用的持久
Resource。

预计用时 20–30 分钟。Data Tool 会流式检查 240,000 个客户，通常需要几十秒。

先阅读[案例总览](supply-chain-agent-tutorial.md)。

## 完成后的结果

你会得到：

```text
indonesia-warehouse-network-tutorial@1.0.0   Dataset Release
tutorial-indonesia-data@1.0.0                Agent Release
indonesia_dataset_inspection.v1              durable Resource Artifact
```

正确 inspection 的关键事实为：

| 检查项 | 期望值 |
| --- | ---: |
| 客户数 | 240,000 |
| 年需求单位 | 6,908,721 |
| 当前省级行政区 | 38 |
| 中心仓 | 3 |
| 前置仓 | 8 |
| 候选点 | 20 |
| 报价行 | 1,148 |

## 1. 认识这 10 个文件

文件位于
[`releases/1.0.0`](../../tools/supply-chain-network-planner/examples/indonesia-tutorial/releases/1.0.0/)。

| 文件 | 角色 | 为什么需要 |
| --- | --- | --- |
| `dataset-manifest.json` | 数据说明和内部哈希 | 固定来源、规模、角色和生成版本 |
| `province-boundaries.geojson` | 38 省边界 | 检查客户、仓库和候选点是否在正确省内 |
| `customers.csv.gz` | 240,000 个合成客户 | 客户坐标、省份和年度需求 |
| `customer-assignments.csv.gz` | 当前客户到前置仓关系 | 保存现状，而不是先假定全部重分配 |
| `warehouses.csv` | 3 中心仓和 8 前置仓 | 名称、类型、位置、容量和当前负荷 |
| `warehouse-links.csv` | 中心仓到前置仓关系 | 定义两级补货网络 |
| `candidate-locations.csv` | 20 个候选前置仓 | 最终有限候选优化的搜索空间 |
| `transport-quotes.csv` | 干线和末端报价 | 后续计算当前与候选成本 |
| `planning-policy.json` | 距离、速度和工作时长 | 固定计算口径 |
| `validation-report.json` | 生成器对账结果 | 让独立 Tool 核对生成时的期望值 |

这些文件共同形成一个数据版本。只上传 `customers.csv.gz` 再让模型搜索其他文件，会失去
文件之间的一致性和授权边界。

## 2. 在 Workspace 发布数据

本篇属于高级 Builder 路径；快速体验会通过 Tutorial Blueprint 安装同一受审数据。
手工发布时，在 Web 中：

1. 选择目标 Workspace，打开右侧 **Files**。
2. 点击头部或空状态中的 **Add data**。
3. 在 **Publish a data release** 中点击 **Choose release files**。
4. 从 `releases/1.0.0` 同时选择上表全部 10 个文件。
5. 确认 Manifest 自动填入 Dataset ID
   `indonesia-warehouse-network-tutorial` 和版本 `1.0.0`。
6. **Name** 填 `Indonesia Warehouse Network Tutorial`。
7. **What this data is for** 填
   `Synthetic, source-locked data for the progressive Indonesia warehouse-network tutorials.`。
8. 核对各文件角色与上表一致。
9. 点击 **Publish release**。

正确结果：

```text
Published indonesia-warehouse-network-tutorial@1.0.0
10 files · published
```

平台发布时重新计算每个文件和整个 Release 的 SHA-256。不要把源码目录中的预生成哈希
手工填成平台哈希；平台是本次 Workspace Release 身份的权威所有者。

## 3. 基于 Data 模板发布 Agent

打开 **Agent Studio → Agents → New Agent**，填写：

| 字段 | 值 |
| --- | --- |
| Agent ID | `tutorial-indonesia-data` |
| Version | `1.0.0` |
| Display name | `Tutorial Indonesia Data Agent` |
| Description | `Validates one exact Indonesia warehouse-network Dataset Release.` |
| Reviewed capability template | `Enterprise Data Agent · 4.0.0` |

**Responsibilities**：

```text
核验一个平台授权的印尼仓网 Dataset Release
检查文件哈希、客户与分配对齐、省界、公式、报价完整性和生成器对账
只发布有界的数据检查结果，不返回原始客户行或选择网络方案
```

**Custom Agent instructions**：

```text
只使用平台附加到本 Agent instructions 的精确 Dataset Release 身份。
调用 supply_chain_indonesia.inspect_indonesia_dataset_release，并使用完整的 workspace_id、
release_id、dataset_id、version 和 content_sha256。使用 Tool 的确定性校验，不扫描
Workspace、不运行终端命令、不直接打开 CSV/GeoJSON，也不把客户行复制进上下文。

Tool 成功后读取并核对有界 inspection Resource。最终回答先输出 Tool 返回的原样
ARTIFACT_HANDOFF，随后报告数据版本、规模、来源、checks 和 warnings。任何哈希、文件、
省界、公式或报价校验失败时停止；不得猜测或改用其他数据。
```

选择：

```text
Authorized data · Indonesia Warehouse Network Tutorial · 1.0.0
Output · indonesia_dataset_inspection.v1
```

然后依次点击：

```text
Create draft → Validate → Publish
```

选择 Dataset Release 后，这个 Agent 只与当前 Workspace 兼容。平台在启动前校验
Workspace、Release ID、版本和内容哈希；Runtime 只收到逻辑身份，不收到浏览器提供的
本地路径。

## 4. 直接启动 Data Agent

回到主页面，在 Workspace 行点击 **New task**。在 **Start a task** 中选择
**Agent**，再选择 **Tutorial Indonesia Data Agent · 1.0.0**。

发送：

```text
核验我授权的印尼仓网教程数据。报告客户、需求、省份、中心仓、前置仓、候选点和报价规模，
说明来源、完成的检查和任何警告。不要做时效、成本或选址分析。
```

这里只需要一个 Agent，因此不要创建 Supervisor。Supervisor 只有在多个独立职责需要
协调和交接时才增加价值。

## 5. 审阅结果

正确执行应包含：

1. 一个真实的 `supply_chain_indonesia.inspect_indonesia_dataset_release` Tool 调用；
2. Tool 参数与 Agent 自动附加的精确 Dataset Release 身份一致；
3. Tool 返回 `resource_name` 和完整结构化 `data_ref`；
4. `indonesia_dataset_inspection.v1` Artifact 状态为 `ready`；
5. inspection 报告 240,000 客户、6,908,721 需求单位、38 省、3 中心仓、8 前置仓、
   20 候选和 1,148 报价；
6. 原始客户行、服务器路径和 Resource URI 不出现在最终报告；
7. 刷新页面后，Tool 事件、Artifact 和最终回答仍能恢复。

Data Tool 实际完成的检查包括：

- 每个 Workspace 文件与平台 Manifest 的大小和 SHA-256 一致；
- 240,000 个客户和 240,000 条分配逐行对齐；
- 客户、仓库和候选位置在声明的省界内；
- 距离和服务天数能按 policy 重算；
- 当前仓和全部候选的报价矩阵完整；
- 当前负荷、覆盖和生成器校验报告相互对账。

## 6. 失败应该是什么样

以下情况必须明确失败：

| 问题 | 正确结果 |
| --- | --- |
| 少上传一个文件 | Data Tool 报告文件合同不完整 |
| 文件内容发布后被改动 | 平台或 Tool 报告哈希不一致 |
| Agent 未绑定 Dataset Release | Agent 报告没有精确授权身份 |
| 在其他 Workspace 启动 | Agent 不出现在兼容选择列表，或启动前拒绝 |
| Tool 不可见 | Agent 报告能力缺口，不用终端手工启动 MCP |

不要用“先读源码里的同名文件”作为补救。源码示例和当前 Workspace Release 不是同一个
授权对象。

## 7. 你自己的数据应怎样准备

一个可复用的业务 Dataset Release 至少应写清：

- 每个文件的逻辑名称、角色、媒体类型和口径；
- 数据时间范围、币种、单位和分类；
- 主键、外键和跨文件对账规则；
- 来源、许可、提取时间和生成版本；
- 缺失、重复、越界和异常值的处理规则；
- 哪些字段允许模型看到，哪些只能由 Tool 流式处理。

不要把“数据在某个目录”当作数据目录。Agent 需要的是一个可发现、可授权、可验证的
Release 身份。

下一篇：

[印尼仓网 2：只分析当前单层时效](indonesia-network-02-service-baseline.md)
