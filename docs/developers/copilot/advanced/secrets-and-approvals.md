# 进阶：Secrets、审批与外部副作用

> **适合谁**：Tool 需要凭据、网络、写文件或调用外部系统的开发者
> **预计时间**：15 分钟
> **前置条件**：先完成无副作用的最小 Tool
> **完成结果**：Secret 不进入源码/浏览器/日志，副作用与审批终态明确

## Secret 的 owner

Provider 凭据属于平台加密 Secret，并只注入拥有它的 Profile 进程。不要把值写入
`copilot.toml`、Role、Skill、`runtime.toml`、测试 fixture、文档、日志或 Tool result。浏览器也
不能看到配置 key 路径。

当前 Tool runtime 的 `host` binding 只允许标准 proxy 变量；这不是任意 Secret 注入通道。领域
Tool 需要新凭据时，先由平台 Secret owner 建立 typed 注入合同，再让 Tool 读取被允许的变量。

## 审批按副作用决定

只读、确定性、无网络副作用的 Tool 可以在 Role 中评审为默认 approve。会写用户文件、发送消息、
修改外部状态或产生成本的 Tool 要有准确 annotations、最小权限和适当 approval mode。不要为了
减少弹窗而关闭全局审批。

非幂等调用不能自动重试，除非上游合同提供明确 idempotency key。取消、拒绝、超时、中断和失败
都必须保持独立终态，不能返回 fabricated success。

## Workspace 写入

使用当前授权 Workspace 的相对路径；校验 canonical/no-follow 边界；默认 create-new；同名时明确
返回冲突或通过正式 elicitation 让用户选择。不要覆盖上传源文件，也不要把临时文件当最终交付。
可运行范例见 [Provider SDK cookbook](../../../../packages/copilot-provider-sdk/examples/record-review/README.md)。

## 成功信号与失败跳转

成功信号：源码和输出中没有 Secret；Role 权限与真实副作用一致；失败/取消不改变外部状态或伪装
成功；create-new 冲突可重复验证。若 stdio 或 typed error 丢失，见
[Tool 准备或 stdio 失败](../testing-and-troubleshooting.md#tool-准备或-stdio-失败)。
