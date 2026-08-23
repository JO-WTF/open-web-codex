# `copilot.toml` 参考

> **适合谁**：正在装配 Root、Skill、Role、Tool、测试或交付的开发者
> **预计时间**：10 分钟
> **前置条件**：已有一个 `./scripts/copilot.sh init` 生成的包
> **完成结果**：manifest 只有当前 schema v1 字段，且所有引用都有唯一 owner

## 顶层与 Root

```toml
schema_version = 1
id = "record-review"
display_name = "Record Review"

[root]
skill = "record-review-root"
agent = "record-review-root" # 可选；单 Agent 通常声明
task_skills = "none"         # 只接受 none 或 all
```

一个包只有一个 Root。`root.skill` 必须引用 `[[skills]]`；`root.agent` 若存在必须引用
`[[agents]]`。不要声明 mode、默认包或按提示词选择逻辑。

## Skill 与 Role

```toml
[[skills]]
id = "record-review-root"
path = "skills/record-review-root"

[[agents]]
id = "record-review-root"
role = "agents/record-review-root.toml"
```

路径相对 source root，禁止绝对路径、`..` 和 symlink。Skill 目录必须有 `SKILL.md`，其 frontmatter
name 必须等于 ID；Role 顶层 name 必须等于 Agent ID。

## 本地或共享 Tool

本地 Tool 三个字段必须齐全：

```toml
[[tools]]
id = "record_review_tools"
root = "tools/record-review-tools"
runtime = "tools/record-review-tools/runtime.toml"
```

共享 Tool 只能有两个字段：

```toml
[[tools]]
id = "supply_chain"
package = "warehouse-network-planner"
```

两种形状不能混用。共享 package 必须在仓库根 `tools/` registry 中恰好出现一次，且其
`tool.toml` 只含 `schema_version`、`id`、`runtime`。

## 原生测试

一个包最多 16 条测试：

```toml
[[tests]]
id = "accepts-at-threshold"
prompt = "Review record sample with score 92 and threshold 80."
target = { kind = "root" }
tool = "record_review_tools"
server = "record_review_tools"
tool_name = "review_record"
arguments = { record_id = "sample", score = 92, threshold = 80 }

[tests.expect]
structured_content = { status = "accepted", record_id = "sample", score = 92, threshold = 80 }
```

`target.kind` 是 `root` 或 `agent`；后者还要写 `agent`。目标 Role 必须实际启用对应 Tool/server/
method。arguments 和 expected result 是有界 JSON，不通过最终自然语言判定。

## 交付

`[[deliveries]]` 可选，最多 32 条，producer `(server, tool)` 不得重复。当前 kind：

- `workspace_artifact`：需要 `content_verifier`，可用受限 JSON Schema 或单行 Markdown marker；
- `inline_geojson_map_card`：媒体类型必须是
  `application/vnd.open-web-codex.map-card+json`，使用平台版本化 verifier。

完整示例见[仓网交付](../warehouse-copilot.md#4-精确声明浏览器交付)。

## 验证

```bash
./scripts/copilot.sh validate ./copilots/record-review --json
```

成功信号：`ok=true`，摘要中的 Skill/Agent/Tool/test ID 与源码一致。失败按 code 和相对路径进入
[静态验证失败](../testing-and-troubleshooting.md#静态验证失败)。
