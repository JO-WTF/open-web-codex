# Copilot 开发者快速开始（入口已迁移）

> **适合谁**：从旧链接进入的仓库开发者
> **预计时间**：1 分钟
> **前置条件**：位于仓库根目录
> **完成结果**：进入唯一开发者中心，不继续使用旧的裸 CLI 命令

当前唯一入口是 [Copilot 开发者中心](../developers/copilot/README.md)。

- 第一次开发：[30 分钟做出第一个 Copilot](../developers/copilot/first-copilot.md)
- 命令参数：[CLI 参考](../developers/copilot/reference/cli.md)
- 失败定位：[测试与排错](../developers/copilot/testing-and-troubleshooting.md)

统一命令：

```bash
./scripts/copilot.sh --help
```

成功信号：帮助中列出 `init`、`check` 与 `sync`。若失败，按
[Runtime 与 SDK 入口排错](../developers/copilot/testing-and-troubleshooting.md#按阶段排错)。
