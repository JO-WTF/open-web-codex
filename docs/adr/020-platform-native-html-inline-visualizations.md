# ADR-020：Platform 原生 Workspace HTML Inline Visualization

状态：已接受（2026-08-18）

关联：[ADR-018](018-built-in-network-copilot-runtime-closure.md)、[ADR-005](005-map-reply-cards.md)。当前实现与验证状态仍以[系统架构](../architecture.md)和[能力基线](../capability-baseline.md)为准。

## 背景

Codex 原生 `::codex-inline-vis{file="..."}` 只读取 Profile 内、按 Thread 分区的可视化目录。普通 Agent 只能安全地写授权 Workspace，不能也不应获得 `CODEX_HOME` 或 Thread 私有目录写权限。因此，用户要求生成并展示普通交互 HTML 时，模型既不知道正确的生产合同，也无法把 Workspace 文件作为原生可视化安全地提交给 Web。

## 决定

Platform 新增一个窄的、非 Artifact 的消息完成期快照合同：

1. Agent 在当前授权 Workspace 创建 UTF-8 `.html` 文件，并在最终 Agent Message 的独立行输出 `::codex-inline-vis{workspace_file="relative/path.html"}`。
2. Platform 只处理已完成的 Agent Message。它从该事件解析出的权威 Run、Thread 与 Workspace 获得身份，不相信浏览器输入、绝对路径或模型提供的 Profile 路径。
3. Platform 通过 `GitRuntime` 的 no-follow Workspace 读取边界验证 source，限制为安全相对路径和 2 MiB HTML；随后以 create-new、Thread-scoped 的不可泄露文件名快照进 Profile 原生 visualization root，并将浏览器投影改写为官方 `::codex-inline-vis{file="..."}`。
4. 历史读取按 Runtime Item ID 复用已持久化的完成期投影，保证刷新、重连与实时消息得到同一引用。快照失败时保留 `workspace_file` 指令，前端明确显示不可用状态；不截图、不转图片、不复制为 Artifact，也不静默回退。

此能力只服务临时对话内 HTML 展示：不创建 Artifact、跨 Run 引用、通用数据面或模型可写的 Profile 文件 API。地图继续使用已存在的 typed `artifact` 合同。

## 被否决的方案

- 让模型直接写 `CODEX_HOME/visualizations`：会绕过 Profile 权限和 Thread ownership。
- 让浏览器根据 Workspace 路径即时读取或复制 HTML：浏览器会成为文件授权与持久化 owner，且刷新语义不稳定。
- 把任意 HTML 提升为 Artifact 或截图：前者制造不必要的持久交付物与第二数据面，后者丢失交互能力。
- 扩展 Codex Core/app-server：官方原生 directive 与 viewer 已足够，新增 Core seam 没有必要。

## 后果与验证

Platform Server 负责授权快照和浏览器投影；Codex Runtime 仍拥有 Agent Message 和原生 `file` 指令语义；Web 继续只渲染授权的 Thread-scoped native file。验证至少覆盖安全 directive、Workspace containment、create-new snapshot、Thread 作用域读取、失败显式不可用，以及实时与历史文本一致性。

若官方 app-server 提供可携带 Workspace provenance 的 typed inline-visualization Item，此快照/改写层应迁移到官方合同并删除；不保留双路径。
