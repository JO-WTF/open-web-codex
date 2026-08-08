# ADR-012：开发数据库只允许显式重建

状态：已接受（2026-08-08），实现待完成

## 背景

项目处于研究开发阶段，不要求保留历史 Thread，但已应用 migration 被修改后，SQLx 会
拒绝启动。若在启动路径中忽略 checksum、猜测旧 schema 或自动修表，会把开发便利变成
不可审计的兼容层，并可能破坏 Provider/Secret 配置。

## 决定

1. 已应用 migration 不再修改；当前 schema 变化只新增 migration。
2. 不兼容的开发数据通过显式、人工确认的重建命令处理，不进入服务启动路径。
3. 重建命令可以保留 Provider Definition 和加密 Secret 关联，但必须先使用与服务器
   大版本匹配的 PostgreSQL client 导出并校验，再执行破坏操作。
4. 脚本必须确认目标是开发数据库，失败时在任何删除前停止，不记录 Secret 明文。
5. CI 维护 migration filename/hash 完整性检查。

## 否决方案

- 忽略 SQLx checksum；
- 启动时自动修改 migration history；
- 双读旧表和新表；
- 重新要求用户每次手工输入 Provider key。

## 后果与验证

开发数据库可以无历史负担重建，但 schema 事实仍可重复。验证必须覆盖导出失败、客户端
版本不匹配、空库迁移、Provider 恢复和连续两次启动。
