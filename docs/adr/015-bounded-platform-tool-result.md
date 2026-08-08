# ADR-015：使用有界通用 Platform Tool Result

状态：已接受（2026-08-08），实现待完成

## 背景

Platform event projection 当前识别 `network-case-tool-result.v1`。这把仓网协议泄漏到
平台，并鼓励其他领域继续增加分支；完整 Tool 结果还会扩大模型上下文和浏览器载荷。

## 决定

1. Platform Tool SDK 统一生成 `platform-tool-result.v1`。
2. 通用 envelope 只包含稳定 status、有界 summary、Work State revision/change、Artifact
   references、blocking inputs、diagnostics 和分页信息。
3. 大型结果只返回引用；Secret、路径、Runtime request ID、完整事件和思维链禁止进入。
4. `failed`、`timeout`、`cancelled`、`unavailable`、`partial` 和 `completed` 使用类型化
   语义，不解析正文判断。
5. 领域扩展只能放在受 schema 和大小限制的 domain payload；平台核心不按领域分支。

## 否决方案

- 为每个领域在 event projection 增加解析器；
- 让 Assistant 重新抄写完整 Tool payload；
- 静默截断后仍标记完整成功。

## 后果与验证

供应链 envelope 必须直接替换，不双读。Python/Rust/TypeScript 使用同一 schema fixture，
测试覆盖大小、分页、未知状态、Secret/path 拒绝和任意领域 payload。
