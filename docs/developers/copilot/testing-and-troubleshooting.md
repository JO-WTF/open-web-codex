# Copilot 测试与排错

> **适合谁**：正在执行 `validate`、`check`、`sync` 或 Web 验收，遇到失败的开发者
> **预计时间**：5–20 分钟定位 owner
> **前置条件**：从仓库根目录使用 `./scripts/copilot.sh`；保留第一条真实错误
> **完成结果**：按失败阶段修根因，并得到可复现的成功信号

## 按阶段排错

| 首个失败阶段 | 它实际检查什么 | 先看哪里 | 不要做 |
| --- | --- | --- | --- |
| `init` | ID 与目标目录安全 | 命令参数、目录是否为空 | 覆盖现有包 |
| `validate` | manifest、Skill、Role、Tool、测试和 delivery 引用 | typed code 与相对路径 | 改 Profile 或重启服务 |
| `prepare` | lock、依赖、Tool 构建与 server entry | `pyproject.toml`、lock、`runtime.toml` | 运行时安装、`PYTHONPATH` |
| `dev` | 真实 app-server 是否发现 Skill 和 MCP server | Role policy、Tool 启动 stderr | 用模型回答冒充 ready |
| `test` | 本地确定性 Provider 下的原生 Role/Tool 正常链 | manifest case、Tool 参数与结构化结果 | 只检查最终文字 |
| `sync` | 完整 check、冷重启、相同组合是否被服务准备 | 首个失败 phase 或 restart | 手工复制进 Profile |
| Web | 真实 Provider、Task 选择、连续 Turn、交付 UI | Tool 卡片、typed result、服务日志 | 用 Prompt 固定内部流程 |

所有命令都支持适用范围内的 `--json`。机器输出里的 code、phase 和相对路径才是首要证据。

## Runtime 二进制缺失

现象：

```text
copilot: codex_binary_missing: the current checkout's dev-small Codex binary is missing
```

`validate` 不需要 Runtime；`dev`、`test`、`check` 和 `sync` 需要当前 checkout 的
`codex/codex-rs/target/dev-small/codex`。按仓库构建流程生成它，再重跑原命令。不要把另一份全局
`codex` 塞给教程，包装器有意固定当前源码对应的 Runtime。

成功信号：`./scripts/copilot.sh check ...` 能进入 `dev`，且最终四阶段全部通过。

## 目标目录不是空目录

现象：`destination_not_empty`。`init` 和 `tool init` 都拒绝覆盖非空目录。检查目标是否是未提交工作；
若是旧练习，选择一个明确的新目录或先由人工确认如何处理。不要自动删除或覆盖。

成功信号：新目标此前不存在，命令一次生成完整目录并立即返回 valid 摘要。

## Skill 名称或 frontmatter 不一致

现象：`frontmatter_mismatch`，通常同时给出 `skills/.../SKILL.md`。确认：

- 文件第一段是 `---` 包围的 frontmatter；
- `name` 与 `copilot.toml` 的 `[[skills]].id` 完全相同；
- `description` 是单行标量；
- `metadata.short-description` 使用两个空格缩进。

成功信号：`validate` 不再报告该相对路径。不要通过删除 Skill 声明来绕过真实引用。

## 静态验证失败

常见 code：

| code | 含义 | 修复方向 |
| --- | --- | --- |
| `missing_file` | manifest 引用的相对文件不存在 | 修路径或补当前文件 |
| `missing_reference` | Root、Role、server、Tool 或共享 package 没有唯一 owner | 对齐 ID 与 registry |
| `duplicate_id` | 同一作用域 ID 或 producer 重复 | 保留一个权威声明 |
| `invalid_field` | schema v1 不接受字段或本地/共享 Tool 形状混用 | 按 reference 删除错误字段 |
| `unsafe_symlink` / `invalid_path` | 路径越界、绝对路径或 symlink | 使用包内相对路径与真实目录 |
| `invalid_lock` | 依赖没有精确 hash lock | 重新生成 lock，不放 editable/path 依赖 |

先修输出中的第一条相对路径，然后重跑同一命令。成功信号是 `validate` 返回包摘要；这仍不代表
Runtime 可运行。

## 共享 Tool 找不到

现象：`registered Tool package ... was not found exactly once`。共享引用只能写：

```toml
[[tools]]
id = "supply_chain"
package = "warehouse-network-planner"
```

并且 `tools/` 下恰有一个一级目录，其 `tool.toml.id` 与 `package` 相同。仓库包装器已固定这个
registry，所以不要传另一个 root，也不要退回跨目录 `../` 路径。

成功信号：同一共享 Tool 能被单 Agent 和多 Agent manifest 同时 `validate`，实现仍只有一份。

## Role 找不到 Skill、Tool 或 server

Role 的 `name` 必须与 `[[agents]].id` 相同；启用的 Skill 必须出现在 `[[skills]]`；
`plugins.<tool>.mcp_servers.<server>` 中的 Tool ID 要对应 manifest，server ID 要来自该 Tool 的
`runtime.toml`。Role 只声明 policy，不声明 transport。

成功信号：`validate` 通过，随后 `dev` 的 Skill 与 MCP server discovery 都 ready。

## Tool 准备或 stdio 失败

如果失败停在 `prepare`：检查 Python/Node manifest 与精确 lock、Python module entry、声明的
platform package。不要在 Server 启动时执行 `pip install` 或 `npm install`。

如果失败停在 `dev` 或 stdio smoke：确认 stdout 没有普通日志污染 MCP 帧；诊断写 stderr；入口
模块能在准备好的依赖环境中启动；所需环境来自 typed binding，不依赖 shell 当前目录猜测。

成功信号：`dev` 观察到所有声明的 Skill/MCP server，且 `test` 从 canonical history 验证真实
Tool call 与结构化结果。

## 同步失败

`sync` 有两个明确终态：

- `check_failed`：服务没有重启；回到 payload 的 `failedPhase`；
- `restart_failed` 或 `composition_hash_mismatch`：check 已通过，但新服务未健康采用同一组合。

保留输出和本地 launcher 日志，修复对应 owner。不要手动重启后直接宣布同步成功，也不要手改
Profile。成功信号是 `state=synced`、包 ID 正确、`healthy=true`。

## Web 中找不到或没有真实调用

先区分两类：

1. **列表没有包**：确认刚才 `sync` 的 package ID、服务健康状态，并新建 Task；不要期待已有 Task
   自动换包。
2. **有包但没有 Tool 卡片**：确认请求确实落在该包、Role 启用了正确 server、Tool 调用没有
   typed failure。最终回答文字不能替代 Tool Item。

成功信号：新 Task 明确选择包，Web 展示真实 Tool 卡片、精确参数和结构化结果。

## Web 与真实业务验收

仓网三轮失败时保留失败状态，逐层检查：

- 上传文件是否都在同一授权 Workspace；
- Data 的完整 typed 结果，而不是有界预览，是否包含目标城市/候选仓；
- Network Tool 的 exact input ref、目标小时、策略和场景参数；
- 地图 producer 是否与 manifest delivery 完全匹配；
- 刷新后 canonical Task history 是否仍有同一 Tool/交付事实。

`unknown tool` 不等于 provider 不在线；它也可能是当前请求工具集合与历史选择不一致。要比较
实际请求 tools 与 Runtime Tool Item，不能添加重试、延迟或让模型改口掩盖。

成功信号：业务结论、typed result 和地图相互一致；失败、取消、需要输入也保持明确终态。

## 文档与示例 smoke

快速门（不要求已构建 Runtime）：

```bash
./scripts/smoke-copilot-docs.sh --skip-runtime
```

完整门（要求真实 app-server，执行 fresh 单/多 Agent init、Tool init、validate 与 quick `check`）：

```bash
./scripts/smoke-copilot-docs.sh --require-runtime
```

成功输出会给出已检查内部链接数量，并显示 `quick check=passed`。`sync` 会重启用户本地服务，Web
会使用真实 Provider，因此二者留在最终人工/Root 验收，不放进无副作用 docs smoke。
