# 30 分钟做出第一个 Copilot

> **适合谁**：仓库内部第一次开发 Copilot、会读少量 Python/TOML 的工程师
> **预计时间**：30 分钟；第一次准备依赖可能更久
> **前置条件**：位于仓库根目录；Python 3.11+；当前 checkout 的 Codex Runtime 已构建
> **完成结果**：`record-review` 单 Agent Copilot 通过两条原生测试、同步到本地服务，并在 Web 真实调用 Tool

本教程只做一件业务中性的事：给一条记录和一个分数，确定它是“通过”还是“需要复核”。所有
示例都使用同一个记录 ID：`sample`。新人主线只处理生成的源码文件和统一命令，不需要修改任何
本机隐藏配置。

## 0. 确认统一入口

在仓库根目录运行：

```bash
./scripts/copilot.sh --help
```

期望输出中至少有：

```text
init PATH --name ID [--template single-agent|multi-agent]
tool init PATH --name ID
validate SOURCE
check SOURCE
sync copilots/PACKAGE
```

成功信号：命令退出码为 0。若提示 Runtime 二进制缺失，先看
[Runtime 二进制缺失](testing-and-troubleshooting.md#runtime-二进制缺失)。

## 1. 生成单 Agent 包

`record-review` 是本教程唯一的示例包 ID。直接生成到受信任的 `copilots/` 根下，后面才能用
同一个目录同步到本地服务：

```bash
./scripts/copilot.sh init ./copilots/record-review \
  --name record-review \
  --template single-agent
```

期望输出：

```text
Copilot package 'record-review' is valid.
  manifest: copilot.toml
  skills: 1
  agents: 1
  tools: 1
```

生成目录：

```text
copilots/record-review/
├── copilot.toml
├── agents/record-review-root.toml
├── skills/record-review-root/SKILL.md
└── tools/record-review-tools/
    ├── pyproject.toml
    ├── requirements.lock
    ├── runtime.toml
    ├── src/record_review_tools/
    │   ├── __init__.py
    │   ├── core.py
    │   └── server.py
    └── tests/test_core.py
```

这是一个可以立即验证的起点。内嵌 Tool 适合先学会完整闭环；当 Tool 要被多个 Copilot 复用时，
再用 `tool init` 把它建成根级共享包。成功信号是目录和摘要都出现。若目标目录已存在，见
[目标目录不是空目录](testing-and-troubleshooting.md#目标目录不是空目录)。

## 2. 把生成的 Tool 改成记录复核

Tool 是确定性业务执行者。它拥有输入校验和唯一判定规则，不能把这份规则再复制到 Skill 或
Prompt。

把 `copilots/record-review/tools/record-review-tools/src/record_review_tools/core.py` 完整替换为：

```python
"""Deterministic record-review rules owned by this Tool."""

from typing import Literal, TypedDict


class ReviewResult(TypedDict):
    """Stable typed result returned by ``review_record``."""

    status: Literal["accepted", "needs-review"]
    record_id: str
    score: int
    threshold: int


def review_record(record_id: str, score: int, threshold: int = 80) -> ReviewResult:
    """Review one record without guessing or external side effects."""

    normalized_id = record_id.strip()
    if not normalized_id:
        raise ValueError("record_id must not be empty")
    if isinstance(score, bool) or not 0 <= score <= 100:
        raise ValueError("score must be between 0 and 100")
    if isinstance(threshold, bool) or not 0 <= threshold <= 100:
        raise ValueError("threshold must be between 0 and 100")
    return {
        "status": "accepted" if score >= threshold else "needs-review",
        "record_id": normalized_id,
        "score": score,
        "threshold": threshold,
    }
```

把同目录的 `__init__.py` 完整替换为：

```python
"""Typed implementation package for Record Review."""

from .core import ReviewResult, review_record

__all__ = ["ReviewResult", "review_record"]
```

把同目录的 `server.py` 完整替换为：

```python
"""Expose record review through a typed FastMCP Tool."""

from mcp.server.fastmcp import FastMCP
from mcp.types import ToolAnnotations

from .core import ReviewResult
from .core import review_record as review_record_value

mcp = FastMCP("record_review_tools")
READ_ONLY = ToolAnnotations(
    readOnlyHint=True,
    destructiveHint=False,
    idempotentHint=True,
    openWorldHint=False,
)


@mcp.tool(structured_output=True, annotations=READ_ONLY)
def review_record(record_id: str, score: int, threshold: int = 80) -> ReviewResult:
    """Review one bounded record and return the exact typed decision."""

    return review_record_value(record_id, score, threshold)


if __name__ == "__main__":
    mcp.run()
```

把 `copilots/record-review/tools/record-review-tools/tests/test_core.py` 完整替换为：

```python
import unittest

from record_review_tools.core import review_record


class ReviewRecordTests(unittest.TestCase):
    def test_accepts_score_at_or_above_threshold(self) -> None:
        self.assertEqual(
            review_record("sample", 92, 80),
            {
                "status": "accepted",
                "record_id": "sample",
                "score": 92,
                "threshold": 80,
            },
        )

    def test_flags_score_below_threshold(self) -> None:
        self.assertEqual(
            review_record("sample", 63, 80),
            {
                "status": "needs-review",
                "record_id": "sample",
                "score": 63,
                "threshold": 80,
            },
        )

    def test_rejects_empty_record_id(self) -> None:
        with self.assertRaisesRegex(ValueError, "record_id"):
            review_record("  ", 92, 80)

    def test_rejects_out_of_range_score(self) -> None:
        with self.assertRaisesRegex(ValueError, "score"):
            review_record("sample", 101, 80)


if __name__ == "__main__":
    unittest.main()
```

不用改生成的 `pyproject.toml`、`requirements.lock` 或 `runtime.toml`。成功信号：判定规则只存在于
`core.py`，Server 只做 typed MCP 暴露。若想写 Workspace 文件或发布 Resource，当前例子还不需要
Provider SDK；需要时再看 [Provider SDK cookbook](../../../packages/copilot-provider-sdk/examples/record-review/README.md)。

## 3. 把工作方法写进 Skill

Skill 告诉 Agent 怎样使用业务能力，但不重复 80 分的判断公式。把
`copilots/record-review/skills/record-review-root/SKILL.md` 完整替换为：

```markdown
---
name: record-review-root
description: Review one scored record with the declared typed Tool and explain the verified decision.
metadata:
  short-description: Review one scored record
---

# Record Review

Handle the request in this Root Agent. Do not create child Agents.

Require a record ID and integer score. Use the declared `record_review_tools`
Tool for the decision; do not calculate or invent the status in model text.
If the user did not provide a threshold, use the Tool default. If required
input is missing, ask for it before calling the Tool.

Return the Tool's exact status, score, and threshold in plain language. A Tool
failure stays a failure; never turn it into an accepted review.
```

成功信号：Skill 写“什么时候调用、缺什么要问、如何解释”，没有复制 `score >= threshold`。若
frontmatter 校验失败，见 [Skill 名称或 frontmatter 不一致](testing-and-troubleshooting.md#skill-名称或-frontmatter-不一致)。

## 4. 把权限分工写进 Role

Role 决定这名 Root Agent 能用什么。把 `copilots/record-review/agents/record-review-root.toml`
完整替换为：

```toml
name = "record-review-root"
description = "Review one scored record directly with the declared typed Tool."
developer_instructions = """
Act as the Record Review Root Agent. Follow the record-review-root Skill, call the declared Tool for every decision, and do not create child Agents.
"""

[features]
multi_agent = false

[[skills.config]]
name = "record-review-root"
enabled = true

[plugins.record_review_tools]
enabled = true

[plugins.record_review_tools.mcp_servers.record_review_tools]
omit_tools_from = ["direct"]
default_tools_approval_mode = "approve"
```

这个 Tool 只读、无外部副作用，所以 Role 可以把它设为已批准。高风险写入或外部系统不能照抄；
见 [Secrets、审批与外部副作用](advanced/secrets-and-approvals.md)。成功信号：Role 只启用一个
Skill 和一个 MCP server。

## 5. 用 `copilot.toml` 装配，并声明两条测试

把 `copilots/record-review/copilot.toml` 完整替换为：

```toml
schema_version = 1
id = "record-review"
display_name = "Record Review"

[root]
skill = "record-review-root"
agent = "record-review-root"
task_skills = "none"

[[skills]]
id = "record-review-root"
path = "skills/record-review-root"

[[agents]]
id = "record-review-root"
role = "agents/record-review-root.toml"

[[tools]]
id = "record_review_tools"
root = "tools/record-review-tools"
runtime = "tools/record-review-tools/runtime.toml"

[[tests]]
id = "accepts-at-threshold"
prompt = "Review record sample with score 92 and threshold 80. Return the exact typed result."
target = { kind = "root" }
tool = "record_review_tools"
server = "record_review_tools"
tool_name = "review_record"
arguments = { record_id = "sample", score = 92, threshold = 80 }

[tests.expect]
structured_content = { status = "accepted", record_id = "sample", score = 92, threshold = 80 }

[[tests]]
id = "flags-below-threshold"
prompt = "Review record sample with score 63 and threshold 80. Return the exact typed result."
target = { kind = "root" }
tool = "record_review_tools"
server = "record_review_tools"
tool_name = "review_record"
arguments = { record_id = "sample", score = 63, threshold = 80 }

[tests.expect]
structured_content = { status = "needs-review", record_id = "sample", score = 63, threshold = 80 }
```

`copilot.toml` 不包含启动命令或依赖安装步骤；这些只属于 Tool 的 `runtime.toml`。成功信号：两个
测试都指向同一个 Root、Tool、server 和真实函数。字段说明见
[`copilot.toml` 参考](reference/copilot-manifest.md)。

## 6. 先做静态验证

```bash
./scripts/copilot.sh validate ./copilots/record-review
```

期望输出仍是 `skills: 1`、`agents: 1`、`tools: 1`，且没有错误。这里证明装配引用一致，不证明
Tool 能启动。失败时错误会包含稳定 code 和相对路径；按 code 跳到
[静态验证失败](testing-and-troubleshooting.md#静态验证失败)。

## 7. 一条命令跑完整检查

```bash
./scripts/copilot.sh check ./copilots/record-review --workspace "$PWD"
```

`check` 依次完成静态验证、依赖准备、真实 Runtime 发现和本地确定性正常链。期望输出：

```text
Copilot 'record-review' passed validate, prepare, dev, and test.
  validate: passed (... ms)
  prepare: passed (... ms)
  dev: passed (... ms)
  test: passed (... ms)
```

成功信号：四个阶段全部 `passed`，两条 manifest 测试都完成。若某阶段失败，不要反复重跑；按
阶段进入 [测试与排错](testing-and-troubleshooting.md#按阶段排错)。

## 8. 同步到本地服务

`sync` 会再次执行完整 `check`；只有通过后才冷重启本地服务，因此不会把失败包伪装成可用：

```bash
./scripts/copilot.sh sync ./copilots/record-review \
  --workspace "$PWD" \
  --json
```

期望输出的关键字段：

```json
{
  "ok": true,
  "state": "synced",
  "copilot": {"id": "record-review"},
  "restart": {"mode": "no-build", "healthy": true}
}
```

成功信号：`state` 是 `synced` 且 `healthy` 为 `true`。若 check 失败，服务不会重启；若重启失败，
跳到 [同步失败](testing-and-troubleshooting.md#同步失败)。不要手工复制内部运行目录。

## 9. 在 Web 做一次真实调用

1. 打开本地 Web；
2. 选择当前仓库对应的 Workspace；
3. 新建 Task，并在 Copilot 列表中明确选择 **Record Review**；
4. 发送：

   ```text
   请复核记录 sample，分数 92，阈值 80。必须使用已声明的工具并返回实际结果。
   ```

5. 在同一个 Task 继续发送：

   ```text
   仍然复核记录 sample，这次分数 63，阈值保持 80。
   ```

真正通过必须同时看到：

- 两轮都有 `review_record` Tool 卡片，而不是模型直接口算；
- 第一轮结构化状态是 `accepted`；
- 第二轮结构化状态是 `needs-review`；
- 分数、阈值和记录 ID 与请求完全一致；
- Tool 失败时 Assistant 不会声称成功。

如果 Web 找不到包或只有文字答案，跳到 [Web 中找不到或没有真实调用](testing-and-troubleshooting.md#web-中找不到或没有真实调用)。

## 完成检查

- [ ] 只通过 `./scripts/copilot.sh` 操作；
- [ ] Tool 拥有唯一判定规则和输入校验；
- [ ] Skill 只写方法，Role 只写分工和权限；
- [ ] `copilot.toml` 有两条不同结果的原生测试；
- [ ] `check` 四阶段通过；
- [ ] `sync` 返回健康；
- [ ] Web 两轮都出现真实 Tool 卡片和精确结构化结果。

下一步可以阅读[开发仓网 Copilot](warehouse-copilot.md)，学习怎样让两个独立 Copilot 包复用同一
组领域 Tool。
