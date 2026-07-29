# 单 Agent：在 Web 审计配送承诺

本篇只解决一个问题：给定一批配送记录，计算订单按时率、需求加权按时率、迟到单号和
表现最差的省份。

这件事对人不难，但让没有指引的模型处理时很容易绕路：它可能扫描 Workspace、直接
读取 CSV、混淆订单口径和需求口径，或者忘记验证重复单号。我们会在 Web 中把确定性
规则做成一个 Python MCP Tool，再发布一个只能使用该能力和精确数据版本的 Agent。

预计用时 20–30 分钟。创建、验证和发布不调用模型；最后启动 Agent 时会产生一次模型
调用。

返回[教程标准与学习路径](README.md)。

## 完成后的结果

你将发布：

```text
delivery-commitment-audit@1.0.0        Dataset Release
delivery-audit@1.0.0                   Python capability package
tutorial-delivery-auditor@1.0.0        Agent Release
```

最终 Agent 的关键结果应为：

| 指标 | 期望值 |
| --- | ---: |
| 配送单数 | 18 |
| 按时单数 | 11 |
| 迟到单数 | 7 |
| 配送单按时率 | 61.11% |
| 需求加权按时率 | 67.49% |
| 最差省份 | Central Java |
| 迟到单号 | `S002`、`S005`、`S006`、`S008`、`S011`、`S012`、`S015` |

“配送单按时率”的分母是 18 个单号；“需求加权按时率”的分母是 1,830 个需求单位。
两者不是同一个指标。

## 1. 发布数据

教程数据位于
[`docs/tutorials/assets/delivery-audit`](assets/delivery-audit/)。
`deliveries.csv` 是合成数据，不代表真实业务表现。

在 Web 中：

1. 选择左侧准备使用的 Workspace。
2. 打开右侧 **Files**。
3. 点击文件面板顶部的数据库图标 **Add data**。
4. 点击 **Choose release files**，同时选择：
   `dataset-manifest.json` 和 `deliveries.csv`。
5. Manifest 会自动填入 Dataset ID `delivery-commitment-audit`、版本 `1.0.0` 和文件角色。
6. **Name** 填 `Delivery commitment audit`。
7. **What this data is for** 填
   `Synthetic shipment records for the first Web-authored Agent tutorial.`。
8. 点击 **Publish release**。

正确结果是右侧显示：

```text
Published delivery-commitment-audit@1.0.0
```

点击该 Release 的 **Copy identity**。剪贴板得到的是安全的逻辑身份：

```json
{
  "workspace_id": "...",
  "release_id": "...",
  "dataset_id": "delivery-commitment-audit",
  "version": "1.0.0",
  "content_sha256": "..."
}
```

这里没有服务器路径。`release_id` 和 `content_sha256` 共同防止 Agent 把“名称相同但
内容不同”的数据误当成当前输入。

## 2. 创建 Python MCP 与 Skill

打开左下角 **Agent Studio**，选择 **MCP → New Python MCP**。

基础字段填写：

| 字段 | 值 |
| --- | --- |
| Workspace | 刚才发布数据的 Workspace |
| Package ID | `delivery-audit` |
| Version | `1.0.0` |
| MCP server name | `delivery_audit` |
| Display name | `Delivery audit` |
| Description | `Validate one exact delivery release and calculate commitment performance.` |

**Tools** 使用
[`tools.json`](assets/delivery-audit/tools.json) 的完整内容。

**Python implementation** 使用
[`implementation.py`](assets/delivery-audit/implementation.py) 的完整内容。

Skill 字段填写：

| 字段 | 值 |
| --- | --- |
| Skill name | `delivery-commitment-audit` |
| Skill description | `Use when a user asks to audit the delivery commitment tutorial data.` |
| Skill instructions | 使用 [`skill-instructions.txt`](assets/delivery-audit/skill-instructions.txt) 的完整内容 |
| Input Artifact types | 留空 |
| Output Artifact types | `delivery_audit_report.v1` |

这个 package 的三层职责是：

```text
Skill：要求使用精确数据身份，并规定回答口径
Tool：声明一个稳定、类型化的调用入口
Python：校验文件并完成唯一的确定性计算
```

不要把同一套计算分别复制到 Skill 和 Agent instructions 中。否则规则改变时会出现多个
互相矛盾的事实来源。

## 3. 在发布前验证 Tool

先点击 **Validate**。正确提示为：

```text
MCP startup and Tool discovery passed for: audit_delivery_commitments.
```

这证明生成的 Server 能启动，并且 MCP 能发现 Tool；还没有证明业务结果正确。

接着：

1. **Tool to test** 填 `audit_delivery_commitments`。
2. **Arguments** 粘贴第 1 步 **Copy identity** 得到的完整 JSON。
3. 点击 **Test Tool**。

结构化结果应包含：

```json
{
  "schema_version": "delivery_audit_report.v1",
  "totals": {
    "shipments": 18,
    "on_time_shipments": 11,
    "late_shipments": 7,
    "on_time_rate": 0.611111,
    "demand_weighted_on_time_rate": 0.674863
  },
  "worst_province": "Central Java"
}
```

Tool 还会返回精确来源、各省结果、迟到单号和已执行的校验。不要发布一个只能启动、
但无法使用真实数据完成 Tool Test 的 package。因为结果的 `schema_version` 与 package
声明的输出类型一致，生成的 MCP 还会给出一个内容寻址的 `resource_name`；Platform
会把同一结果持久化为 `delivery_audit_report.v1` Artifact。

## 4. 发布 capability package

点击 **Publish package**。正确结果为：

```text
delivery-audit@1.0.0 published
Includes MCP delivery_audit and Skill $delivery-commitment-audit.
```

发布后 package 不可原地修改。Python、Tool Schema 或 Skill 任一内容变化，都要创建
新版本。

## 5. 发布单 Agent

仍在 **Agent Studio**，打开 **Agents → New Agent**，填写：

| 字段 | 值 |
| --- | --- |
| Agent ID | `tutorial-delivery-auditor` |
| Version | `1.0.0` |
| Display name | `Tutorial Delivery Auditor` |
| Description | `Audits one exact delivery Dataset Release with the reviewed delivery Tool.` |
| Reviewed capability template | `Delivery audit · 1.0.0` |

**Responsibilities** 每行一项：

```text
核验一个精确授权的配送数据版本
使用确定性 Tool 计算配送单和需求加权按时率
报告迟到单号、省份差异、来源与校验结果
```

**Custom Agent instructions** 填写：

```text
只处理平台附加到本 Agent instructions 的精确 Dataset Release。
使用 $delivery-commitment-audit，并调用 delivery_audit.audit_delivery_commitments。
Tool 参数必须原样使用平台注入的 workspace_id、release_id、dataset_id、version 和
content_sha256。不得扫描 Workspace、直接读取 CSV、运行终端命令或自行重算另一种按时口径。

最终回答必须引用精确的 delivery_audit_report.v1 resource_name，分别报告配送单按时率
和需求加权按时率，列出迟到单号、最差省份、数据版本和 Tool checks，但不暴露 data_ref
URI。Tool 失败、数据身份缺失或校验失败时停止，并明确报告失败；不得猜测。
```

选择 **Authorized data → Delivery commitment audit · 1.0.0**，保留输出：

```text
Output · delivery_audit_report.v1
```

然后依次点击：

```text
Create draft → Validate → Publish
```

这里的权限来自两个精确选择，不来自 instructions：

- capability template 决定 Agent 能看到 `delivery_audit` MCP 和哪个 Tool；
- Authorized data 决定 Agent 得到哪个 Dataset Release 身份。

## 6. 启动 Agent

回到主页面。在目标 Workspace 行点击机器人图标 **Start governed agent**，选择
**Tutorial Delivery Auditor · 1.0.0**。

发送：

```text
审计已授权的配送承诺数据。告诉我配送单按时率、需求加权按时率、迟到单号和表现最差的
省份，并说明使用的数据版本和完成了哪些校验。
```

用户创建的 Python MCP 默认采用逐次确认，而不是静默预批准。第一次调用时会出现
审批卡片。核对卡片中的 Agent、Server `delivery_audit`、Tool
`audit_delivery_commitments` 和参数里的五个数据身份字段，然后点击 **Accept**。
审批是 Runtime Tool 生命周期的一部分；不要为了省掉这个步骤把 package 改成隐藏
预批准。

## 7. 核对真实成功证据

本篇通过必须同时满足：

1. Thread 中出现并接受了 `delivery_audit.audit_delivery_commitments` 的真实审批；
2. 同一 Tool 卡片完成，参数中的五个身份字段与发布的数据版本一致；
3. Tool 结果是 `delivery_audit_report.v1`；
4. 页面出现状态为 `ready` 的同类型 Artifact，内容与 Tool 结构化结果一致；
5. 最终答案引用精确 `resource_name`，且 18、11、7、61.11%、67.49% 和
   Central Java 与 Artifact 一致；
6. 页面没有出现本地路径、Provider Key 或 MCP Resource URI；
7. 刷新页面后，审批、Tool 事件、Artifact 和最终答案仍能恢复。

如果模型直接打开 CSV 并算出相同数字，这篇教程仍然没有通过，因为运行时能力和数据
边界没有被验证。

## 8. 验证失败路径

在 Python editor 的 **Arguments** 中把 `content_sha256` 最后一位改掉，再点击
**Test Tool**。正确行为是明确失败，不能读取“差不多”的版本，也不能返回上一次缓存的
成功结果。

如果在 Agent draft 中不选择 **Authorized data**，Agent 启动后应报告缺少精确数据
身份，而不是搜索 Workspace。

## 9. 怎样改成自己的单 Agent 场景

可以替换：

- `deliveries.csv` 的业务字段；
- Tool 的输入 Schema 和确定性计算；
- Skill 的触发条件；
- Agent 的职责和报告格式。

不要直接照搬：

- 本教程的“按时”定义；
- 合成数据结论；
- 无 Secret 的标准库 Python 限制到需要凭据的外部系统。

需要 API Key、OAuth、Cookie 或数据库凭据时，应等待或使用平台类型化 Secret 绑定，
不能把 Secret 写进 Python、Skill、Prompt 或 Dataset。需要多个独立专业判断或需要
持久 Artifact 交接时，再引入 Supervisor；不要因为“任务看起来重要”就默认增加 Agent。

下一篇：

[印尼仓网 1：发布并核验大型规划数据](indonesia-network-01-data.md)
