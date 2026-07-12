# ADR-001: Web Server Framework

**状态**: 已接受  
**日期**: 2026-07-12  
**关联**: ADR-002, ADR-003

## 背景

平台需要一个独立的 HTTP/WebSocket 服务器来处理浏览器 API 请求、会话管理、项目/Task 编排以及与 Codex Runtime 的集成。当前 Gateway 基于 Tauri, 未来需要替换为独立的平台服务器。

## 决策

使用 **Axum 0.8** 作为 HTTP 服务器框架, **Tokio** 作为异步运行时。

## 理由

- Axum 和 Tokio 已在 Codex 工作区的依赖树中 (axum 0.8, tokio 1.x)
- Axum 的类型安全路由和提取器提供了良好的开发体验
- Tower 中间件生态支持 CORS、Trace、压缩等开箱即用
- Tokio 是 Rust 生态事实标准的异步运行时
- 与 Tauri 后的 axum 版本一致, 便于过渡期共存

## 替代方案

- **Actix-web**: 功能更全但破坏 tokio 生态一致性
- **Poem**: 更小但生态不成熟
- **Warp**: 不再活跃维护

## 影响

- 正向: 利用现有依赖知识; 成熟的 Tower 中间件生态
- 负向: axum 0.8 对某些高级模式 (例如 SSE 广播) 需要额外抽象
