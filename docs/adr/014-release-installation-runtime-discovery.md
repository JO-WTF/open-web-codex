# ADR-014：Release、Installation 与 Runtime Discovery 分离

状态：已接受（2026-08-08），实现待完成

## 背景

已发布 Catalog 内容、已写入 Profile 的文件和 Runtime 当前可执行能力是三种不同事实。
把它们合并会导致发布成功后伪造 ready，也让版本、hash、Runtime Role 和 MCP 名称需要
用户手工同步。

## 决定

1. Tool、Skill、Agent、Supervisor、Copilot 共享 Draft revision 和不可变 Release 语义。
2. 服务器 Package Compiler 规范化内容、解析精确依赖、生成版本/hash/lock 和 Runtime
   bundle；用户不维护机器字段。
3. Catalog Service 拥有 Release；Installation Service 拥有 Profile 安装事务；Codex
   Runtime discovery 拥有当前可执行事实。
4. 状态分别表达 `published`、`authorized`、`installed`、`discovered`、`ready`、
   `degraded`、`unavailable` 和 `failed`。
5. Run 固定 installation snapshot；运行中发布或安装新版本不改变已有 Thread。
6. 不自动回退旧版本，不从路径或文件存在推断 Runtime ready。

## 否决方案

- Catalog row 直接代表 Runtime Agent；
- 浏览器写 `CODEX_HOME`；
- 每种资源实现独立的版本/hash 发布器；
- 启动时自动升级或降级 package。

## 后果与验证

需要统一 Catalog、Compiler 和 Installation Service，并覆盖并发发布、安装崩溃、reload
失败、discovery 缺失、Profile 重启和卸载占用。
