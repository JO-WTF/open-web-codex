# ADR-004: Codex Adapter 抽象模式

**状态**: 已接受  
**日期**: 2026-07-12  
**关联**: ADR-001, ADR-003

## 背景

平台需要与 Codex app-server 通信来启动 Thread、执行 Task、观察事件和处理审批。在开发环境中, 连接真实的 app-server 进程引入复杂性和依赖。需要一个抽象层允许离线开发和测试。

## 决策

使用 **Trait-based Codex Adapter** 模式:

```rust
#[async_trait]
pub trait CodexAdapter: Send + Sync {
    /// 发送 initialize 请求并获取 Capability Manifest
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResponse>;

    /// 启动新 Thread
    async fn start_thread(&self, params: ThreadStartParams) -> Result<ThreadStartResponse>;

    /// 发送用户消息到现有 Thread
    async fn send_message(&self, thread_id: &str, message: &str) -> Result<()>;

    /// 从 Manfiest 中读取能力信息
    fn capability_manifest(&self) -> Option<&CapabilityManifest>;
}
```

两个实现:

- **RealCodexAdapter**: 通过 JSON-RPC over stdin/stdout 连接真实的 Codex app-server 进程
- **FakeCodexAdapter**: 内存中模拟 app-server 行为, 用于开发和集成测试

## 理由

- 开发环境无需启动 Codex 二进制
- 单元测试可以在 CI 中运行 (无需 Codex 环境)
- 真实和 mock 之间使用同一接口
- 未来可以增加 HTTP 传输 (Remote Codex Adapter)

## 替代方案

- **No abstraction**: 直接调用 app-server 进程, 测试需要真实环境
- **Mock library**: mockall 等自动 mock, 但失去对模拟行为的控制力

## 影响

- 正向: 离线开发可行; 集成测试不需要真实 app-server; 接口变更影响在一个地方
- 负向: trait 需要随协议演进维护; 增加一层间接调用
