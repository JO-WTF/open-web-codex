# Hello Team：在 Web 发布第一个受治理 Supervisor

本篇使用当前 Web 框架完成一次真正的发布流程：

1. 基于已评审模板创建两个自定义 Agent；
2. 验证并发布两个不可变 Agent Release；
3. 创建一个只允许使用这两个 Agent 的 Supervisor；
4. 验证并发布 Supervisor Release。

预计用时 20–30 分钟。创建、验证和发布不调用模型，不会产生 Provider Token 费用。

先完成[Hello Agent](hello-agent-quickstart.md)。返回[教程总入口](../multi-agent-development-tutorial.md)。

## 完成后的结果

你会得到：

```text
tutorial-data-preparer@1.0.0
tutorial-network-analyst@1.0.0
          ↓
tutorial-network-supervisor@1.0.0
```

三个 Release 都应显示不可变版本和内容摘要，Supervisor 应精确绑定两个 Agent
Release 及其 Artifact 交付合同。

## 1. 先理解当前发布边界

当前 Web 不允许自定义 Agent 任意填写 Tool 名称。你必须选择一个已评审的
capability template：

- 模板固定 Runtime Role 的基础能力；
- 模板固定 MCP Server 和 Tool allowlist；
- 模板固定可用 capability roots；
- 用户可以定义职责、方法、停止条件和交付格式；
- 用户只能从模板允许的 Artifact 输入输出中收窄合同。

因此，instructions 不能授予新权限。写下“允许访问所有数据库”不会让 Agent 获得
数据库凭据；写下“可以执行任意命令”也不会扩大 Tool allowlist。

本篇使用两个当前已经发布的模板：

| 模板 | 用途 |
| --- | --- |
| Enterprise Data Agent `1.6.0` | 读取受控规划源并发布 `planning-dataset.v1` |
| Enterprise Network Planning Agent `1.5.0` | 读取 Dataset 并运行有界仓网比较 |

## 2. 准备供应链能力

在仓库根目录运行：

```bash
tools/supply-chain-network-planner/bin/setup-env

.local/open-web-codex/tool-envs/supply-chain-network-planner/bin/python \
  -m pytest tools/supply-chain-network-planner/tests -q

.local/open-web-codex/tool-envs/supply-chain-network-planner/bin/python \
  tools/supply-chain-network-planner/tests/stdio_smoke.py
```

期望：

```text
15 passed
Supply-chain MCP stdio smoke passed
```

如果能力包测试没有通过，不要继续发布 Agent。发布成功不能替代底层业务和 MCP
协议证据。

## 3. 发布数据准备 Agent

打开 **Settings → Agent Catalog**。在 **Create Agent draft** 中填写：

| 字段 | 值 |
| --- | --- |
| Agent ID | `tutorial-data-preparer` |
| Version | `1.0.0` |
| Display name | `教程数据准备 Agent` |
| Description | `从授权规划源准备经过验证的仓网数据集。` |
| Reviewed capability template | `Enterprise Data Agent · 1.6.0` |

Responsibilities 每行填写一项：

```text
检查授权规划源的范围和数据质量
构建并验证 planning-dataset.v1
报告单位、时间范围、缺失字段和限制
```

Agent instructions 填写：

```text
你是数据准备 Agent，只负责产生可验证的规划输入。

先列出并检查用户指定的授权规划源，再构建 planning-dataset.v1，最后调用验证能力。
只使用 capability template 提供的 Tool，不读取项目目录，不接受任意路径、SQL 或凭据。
返回准确的来源 ID、时间范围、单位、质量警告、resource_name 和原始 data_ref。
如果数据缺失、总量不一致或 Tool 不可用，明确失败或报告限制；不得猜测或伪造结果。
不得选择仓库、比较网络方案、修改源数据或创建下级 Agent。
```

选择模板后，保留输出合同：

```text
Output · planning-dataset.v1
```

然后：

1. 点击 **Create draft**；
2. 在 **Definitions and releases** 找到该 Agent；
3. 点击 **Validate**；
4. 确认出现 `Validated`；
5. 点击 **Publish**；
6. 确认出现 `Published 1.0.0`。

`Published` 后的内容摘要用于证明版本内容没有被静默替换。已发布版本不可编辑。

## 4. 发布网络分析 Agent

仍在 **Settings → Agent Catalog** 创建第二个 draft：

| 字段 | 值 |
| --- | --- |
| Agent ID | `tutorial-network-analyst` |
| Version | `1.0.0` |
| Display name | `教程网络分析 Agent` |
| Description | `基于已验证数据集比较有界仓网方案。` |
| Reviewed capability template | `Enterprise Network Planning Agent · 1.5.0` |

Responsibilities：

```text
读取同一份 planning-dataset.v1
建立并验证网络快照和路线矩阵
比较现有网络、杭州候选仓和无锡候选仓
用 Artifact 证据报告覆盖率、成本、假设和风险
```

Agent instructions：

```text
你是网络分析 Agent，只基于上游交付的 planning-dataset.v1 工作。

开始计算前必须读取并核对原始 data_ref，确认 Schema、需求总量、币种、规划周期和服务政策。
只使用 capability template 提供的有界规划 Tool。分别计算实际当前关系、现有网络优化基线和用户指定的候选方案。
所有用于结论的 snapshot、route matrix、scenario 和 comparison 都必须经过验证并保留准确的 resource_name。
区分事实、假设和分析；不得静默修改需求、现有设施、币种或服务政策。
如果路线不完整、目标不可行或 Tool 失败，保留已完成证据并明确说明缺口。
不得创建下级 Agent，最终业务取舍交给 Supervisor。
```

模板选择后，保留：

- `planning-dataset.v1` 输入；
- 模板默认勾选的所有网络规划输出。

依次点击 **Create draft → Validate → Publish**，确认出现：

```text
Published 1.0.0
```

## 5. 发布 Supervisor

打开 **Settings → Supervisors**。在 **Create Supervisor draft** 中填写：

| 字段 | 值 |
| --- | --- |
| Policy ID | `tutorial-network-supervisor` |
| Version | `1.0.0` |
| Display name | `教程仓网 Supervisor` |
| Description | `协调数据准备和网络分析，并交付可追踪的仓网建议。` |
| Maximum active child Agents | `2` |

Responsibilities：

```text
把仓网问题拆成数据准备和网络分析两个有界任务
保证 Agent 之间使用类型化 Artifact 交接
检查关键结论的证据、假设和限制
交付一份可执行但不越权的最终建议
```

在 **Platform behavior contract** 中选择平台管理者已经发布的精确版本。该内容只读，
不会由这个 Supervisor 草稿复制或修改。

Custom Supervisor instructions：

```text
你是本任务的 Root Supervisor，对最终交付负责。

先把数据准备任务委派给选中的数据准备 Agent。只有 planning-dataset.v1 已验证并返回原始 data_ref 后，才启动网络分析 Agent。
把同一 data_ref、用户目标和候选约束原样交给网络分析 Agent，不要把大型 Dataset 复制进消息。
只使用本 Release 绑定的 Agent；Agent 缺失或失败时不得改用默认 Agent，也不得由 Root 伪造专业结果。
必要时等待或补充询问现有 Agent，但不要创建未发布角色。
最终报告必须分为：事实、假设、方案比较、建议、风险、缺失证据。
每个关键数字引用对应 Artifact Schema 和 resource_name。
审批只能由平台审批卡和用户决定；Prompt 不得自行授权高风险操作。
```

在 **Allowed Agents** 中只选择：

- `教程数据准备 Agent · 1.0.0`；
- `教程网络分析 Agent · 1.0.0`。

**Required deliverables** 会根据两个 Agent 的 Artifact 合同生成。第一次练习保留全部
默认交付物，不要取消 `planning-dataset.v1` 或网络分析输出。

然后：

1. 点击 **Create draft**；
2. 在下方找到 `tutorial-network-supervisor`；
3. 点击 **Validate**；
4. 修复所有具体 validation issue；
5. 确认出现 `Validated`；
6. 点击 **Publish**；
7. 确认出现 `Published 1.0.0`。

Supervisor 发布时会绑定两个精确 Agent Release 和内容摘要。缺失或发生漂移时，任务
应在启动前失败，不能按显示名称寻找替代版本。

## 6. 核对发布结果

依次完成：

1. 在 **Settings → Agent Catalog** 点击 **Refresh**；
2. 确认两个 Agent 卡片都显示 `Published 1.0.0` 和内容摘要；
3. 在 **Settings → Supervisors** 点击 **Refresh**；
4. 确认 Supervisor 卡片显示 `Published 1.0.0` 和内容摘要；
5. 回到主页面，打开 Workspace 的 **Start governed supervisor** 选择框；
6. 确认 **教程仓网 Supervisor · 1.0.0** 出现在已发布目录中，然后取消弹框。

到这里，当前教程证明：

- Web draft 保存成功；
- validation 通过当前合同；
- Agent 和 Supervisor 形成不可变 Release；
- Supervisor 的 Agent/Artifact 依赖可以解析；
- Release 进入新的 Run 选择目录。

它还没有证明这份自定义 Release 已完成真实 Runtime、子 Thread、MCP 和 Artifact
E2E。当前发布级运行证据使用仓库内置 Enterprise Supervisor；下一篇会用它完成真实
多 Agent 闭环。不要把“出现在目录中”写成“已经在 Runtime 成功运行”。

## 7. 版本怎样继续演化

已发布的 `1.0.0` 不可原地修改。需要调整职责或 instructions 时：

1. 在 Agent 或 Supervisor 卡片点击 **New version**；
2. 使用新的语义版本，例如 `1.0.1`；
3. 保存、验证并发布；
4. 让新的 Supervisor Release 绑定新的 Agent Release；
5. 确认新版本进入 Run 选择目录；真实执行仍按独立 Runtime E2E 门禁验证。

原来的 Thread 继续绑定 `1.0.0`，不会被后来发布的版本静默改写。

## 常见问题

| 现象 | 先检查什么 |
| --- | --- |
| Agent Catalog 没有模板 | 代码发布的 Agent Definition 是否加载成功 |
| Agent 无法 Publish | 是否先 Validate，字段和 Artifact 合同是否完整 |
| Supervisor 无法 Validate | 是否选择已发布 Agent，交付物生产者和消费者是否匹配 |
| 发布目录中没有 Supervisor | 是否真正 Publish，刷新 Supervisor catalog |
| Agent 显示名称正确但依赖错误 | 检查精确 Release UUID、版本和内容摘要 |
| 想立即验证真实执行 | 使用下一篇已通过真实 E2E 的内置 Enterprise Supervisor |

## 完成检查表

- [ ] 两个 Agent 都显示 `Published 1.0.0`；
- [ ] Supervisor 显示 `Published 1.0.0`；
- [ ] Supervisor 只绑定这两个精确 Agent Release；
- [ ] Required deliverables 的生产者和消费者正确；
- [ ] Supervisor 出现在新的 Run 选择目录；
- [ ] 能解释 `Validated`、`Published` 与真实 Runtime E2E 的证据差异；
- [ ] 能解释为什么 instructions 不能扩大 capability template 的权限。

下一篇：

[仓网规划：运行并审阅企业 Supervisor](supply-chain-agent-tutorial.md)
