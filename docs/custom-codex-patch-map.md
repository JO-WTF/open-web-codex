 # Custom Codex Patch Map

 日期：2026-07-12
 基线上游 Commit：`f959e7fc9832dfa0ebfb6542ab1bbf829638ac24`
 定制源 Commit：`0de018a81223aacb6306b4f19ef7a54ee3bfcf8a`（JO-WTF/codex `open-codex` 分支）
 上游最新 Commit：`9e552e9d15ba52bed7077d5357f3e18e330f8f38`

 本文件记录来自 `JO-WTF/codex` fork 的 38 个定制提交的分类和处理策略。

 ## 分类说明

 | 分类 | 含义 |
 |------|------|
 | upstreamed | 功能已在上游被原生实现，不再需要重新应用 |
 | retain | 需要保留的定制补丁，同步后重新应用 |
 | drop | 不再需要（被新架构淘汰或无关） |
 | rewrite | 需要以不同的方式重新实现 |

 ## 定制提交清单

 ### Provider 定制（30 个提交）

 这些是 fork 的主要工作——在 OpenAI 上游基础上增加第三方/自托管 Provider 支持。

 | Commit | 描述 | 分类 | 说明 |
 |--------|------|------|------|
 | `33b2b54` | [WIP] 添加第三方 Provider 支持 | upstreamed | 上游已有 `BearerAuthProvider` 和 `models_endpoint` 模块 |
 | `5291e03` | 添加自定义 Provider 管理 | upstreamed | 上游的 Provider 管理系统已大幅演进 |
 | `4b63b68` | 修复 Provider 管理器交互 | upstreamed | 上游 TUI 和 Provider 管理已演进 |
 | `a11d213` | 改进自定义 Provider 引导输入 | upstreamed | 上游引导流程已演进 |
 | `59a36fb` | 避免 Provider 引导中空配置清除 | upstreamed | 上游已解决此类问题 |
 | `f3d6282` | 创建缺失的 CODEX_HOME 目录 | upstreamed | 上游 app-server 初始化已处理 |
 | `b83fa32` | 修复 Provider 配置序列化和 Wire API 选择 | upstreamed | 上游 Provider 配置系统已演进 |
 | `3583c3b` | 改进 Provider 管理器导航和模型刷新 | upstreamed | 上游有原生模型刷新机制 |
 | `822ba41` | 将模型列表限定到活动 Provider | upstreamed | 上游有 Provider 级模型作用域 |
 | `5cbd5d9` | 持久化 Provider 范围的模型目录 | upstreamed | 上游模型缓存已演进 |
 | `5f543a0` | 支持第三方 Provider 的 OpenAI /v1/models 格式 | upstreamed | 上游有 `bearer_auth_provider` + `models_endpoint` |
 | `5499301` | Merge PR（UI 交互问题） | upstreamed | 上游 UI 已演进 |
 | `b1685a5` | feat: 为自定义 Provider 添加上下文窗口支持 | check | 需要检查上游上下文窗口处理 |
 | `5b5d20c` | 简化 Provider 上下文窗口处理 | check | 同上 |
 | `a269992` | Merge PR（上下文窗口） | check | 同上 |
 | `22c8ff8` | 收紧 Provider 缓存身份审查 | upstreamed | 上游缓存机制已演进 |
 | `e5fd338` | Merge PR（缓存问题） | upstreamed | 上游缓存机制已演进 |
 | `e05d05b` | 审查 Provider 分支缓存间隙 | upstreamed | 上游缓存机制已演进 |
 | `8da5a67` | 改进 Provider 管理器分组 UI | drop | TUI 代码，我们的 Web 平台会替换 |
 | `5762148` | MSVC 目标跨平台通用通道恢复 | drop | TUI/codex 构建问题，不相关 |
 | `87c2d0d` | 修复损坏的 provider_popups.rs 和 MSVC 工具链 | drop | TUI 代码 |
 | `54797e8` | 添加模型级上下文窗口 TUI 配置 | drop | TUI 配置 UI，将用 Web 实现 |
 | `40bdf00` | 向 Provider 表单草稿添加 context_window 字段 | check | 数据层变更，可能需要 |
 | `fc47366` | 从 Provider 配置操作中移除 UI 消息，改用 tracing::info! | retain | 好的日志实践 |
 | `a3a5d08` | 修复 Provider 管理器弹出栈 | drop | TUI 弹出代码 |
 | `6bbb03f` | 在引导设置期间获取 Provider 模型 | upstreamed | 上游引导有原生模型获取 |
 | `3e9410d` | 添加单屏 Provider 表单 | drop | TUI 表单，用 Web 表单替换 |
 | `214749e` | 优化 Provider 表单占位符和加载状态 | drop | TUI 代码 |
 | `f65493a` | 对齐引导 Provider 设置与单页表单 | drop | TUI 代码 |
 | `ce19673` | 修复 Provider 表单视图导入 | drop | TUI 代码 |
 | `28b482e` | 为 Provider 获取导入模型端点 trait | upstreamed | 上游 API 已演进 |
 | `2ee8f72` | 重新导出 Provider 模型获取助手 | upstreamed | 上游 API 已演进 |
 | `88b620c` | 穷尽式处理 Provider 模型获取操作 | retain | 好的代码实践 |
 | `2ee3881` | 将选定 Provider 传播到活动 Turn | check | 需要检查上游 Turn 如何处理 |
 | `879d798` | 支持 Provider 表单键盘快捷键 | drop | TUI 快捷键 |
 | `2d2bcea` | 修复结构体解构中的重复 model_provider_id 绑定 | retain | 真正的代码修复 |

 ### Web 相关定制（3 个提交）

 | Commit | 描述 | 分类 | 说明 |
 |--------|------|------|------|
 | `0de018a` | Refactor post-save action handling with exhaustive match | retain | 真正的代码质量修复（非 TUI 相关） |
 | `87c2d0d` | Fix corrupt provider_popups.rs and update toolchain to MSVC | drop | TUI 代码 |
 | `576214` | Restore generic channel with MSVC target | drop | TUI 构建配置 |

 ### 合并提交（4 个）

 | Commit | 描述 | 分类 |
 |--------|------|------|
 | `7ff0ad6` | Merge PR #9 | n/a |
 | `e5fd338` | Merge PR #7 | n/a |
 | `a269992` | Merge PR #3 | n/a |
 | `5499301` | Merge PR #2 | n/a |

 ## 摘要

 | 分类 | 数量 | 处理方式 |
 |------|------|------|
 | upstreamed | ~20 | 同步后自动覆盖，无需额外操作 |
 | retain | ~3-4 | 同步后手工 cherry-pick 或重写 |
 | drop | ~10 | 明确放弃，由上游代码替换 |
 | check | ~4 | 同步后手动审查 |
 | n/a (merge) | 4 | 无操作 |

 ## 同步策略

 1. 运行 `scripts/sync-codex-upstream.sh --apply` 创建同步分支
 2. 在 `git subtree pull` 冲突阶段，优先保留上游结构
 3. 冲突文件中属于 `drop` 分类的，以上游为准
 4. 冲突文件中属于 `retain` 分类的，手工合并定制逻辑
 5. 冲突文件中属于 `check` 分类的，逐个审查
 6. 对 `retain` 提交进行 cherry-pick 或手工重新实现
