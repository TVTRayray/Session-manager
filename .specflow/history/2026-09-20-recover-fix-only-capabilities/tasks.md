# Tasks: recover-fix-only-capabilities

> 实施只允许按下列 spec/证据从 `f81c1275036b1b2d8946dfee93505bb464b58f57` 选择性移植最小代码与测试。禁止 cherry-pick、merge、rebase、整体复制 commit 或顺手移植无关改动。

## 1. session-catalog：建立四引擎目录与关系数据基线

- [ ] 1.1 对照规范事实源 `specs/session-catalog.md` 审计当前 catalog 接口，只增加 Pi 与 OpenCode 所需的 engine、root/data-source 分派及 `parent_session_id`/`agent_name` 元数据；保留 Codex/Claude 现有实现。
- [ ] 1.2 从来源 `src/catalog.rs` 选择性移植 Pi JSONL 发现、最新 `session_info` 名称、可验证的子代理 tool-result 路径关联；继续复用现有 canonical-root 校验与逐文件降级。
- [ ] 1.3 从来源 `src/catalog.rs` 选择性移植 OpenCode `opencode.db` 只读查询；仅列出未归档 session，按更新时间倒序，探测可选 `parent_id`/`agent` 列并参数化后续 ID 使用。
- [ ] 1.4 加入 focused catalog tests：Pi 目录/名称/父子 Agent 关系、错误或歧义关系保持可见、OpenCode 未归档/排序/可选列、四引擎结果隔离与 stale result 拒绝。
- [ ] 1.5 运行 catalog focused tests，并记录准确命令与结果。

## 2. session-detail：在 catalog 身份与边界上恢复两类详情

- [ ] 2.1 依赖 1 完成后，对照规范事实源 `specs/session-detail.md`，从来源 `src/detail.rs` 选择性移植 Pi JSONL user/assistant 内容解析和分页窗口；不扩大可见敏感记录集合。
- [ ] 2.2 选择性移植 OpenCode session/message/part 的只读 SQLite、参数化详情查询和 viewport 渲染，保持 User/Assistant 标签、Markdown/代码边界与局部错误呈现。
- [ ] 2.3 保持文件型详情 canonical-root 拒绝；OpenCode 仅以 catalog session ID 查询已配置数据库，禁止把 ID 解释为路径或 SQL。
- [ ] 2.4 加入 focused detail tests：Pi transcript、OpenCode message/part、最新 user 窗口、未知/恶意 ID、文件 out-of-root、错误后不显示 stale detail。
- [ ] 2.5 运行 detail focused tests，并记录准确命令与结果。

## 3. session-launch：按引擎恢复启动与继续会话

- [ ] 3.1 依赖 1 完成后，对照规范事实源 `specs/session-launch.md`，从来源 `src/resume.rs` 选择性移植 Pi `pi --session SESSION_ID` 与 `pi` 新建命令的 engine-aware 分派。
- [ ] 3.2 选择性移植 OpenCode 当前平台命令、`--session SESSION_ID` 恢复和 cwd 新建语义；复用现有 cwd 验证、错误传播和 terminal suspend/rebuild 流程。
- [ ] 3.3 加入 focused executor/app tests：Pi/OpenCode 命令、参数、cwd、非零退出、缺失 cwd、返回后上下文、成功新建后的 catalog refresh。
- [ ] 3.4 运行 launch 与 terminal lifecycle focused tests，并记录准确命令与结果。

## 4. session-deletion：保持确认流程并区分安全边界

- [ ] 4.1 依赖 1 完成后，对照规范事实源 `specs/session-deletion.md`，保持单项/组删除确认与默认 Cancel；Pi 及其他文件型引擎继续 canonicalize 后仅删除 engine root 内文件。
- [ ] 4.2 从来源 `src/delete.rs` 选择性移植 OpenCode `opencode session delete SESSION_ID` 分派；不得直接写 SQLite，不得把 ID 当文件路径。
- [ ] 4.3 保持 group 删除逐项结果聚合、成功项移除、失败项保留，并在引擎切换时清空 pending target。
- [ ] 4.4 加入 focused deletion tests：Pi in-root/out-of-root/symlink、OpenCode CLI 参数与失败、OpenCode 组部分失败、切换引擎取消旧确认。
- [ ] 4.5 运行 deletion focused tests，并记录准确命令与结果。

## 5. tui-interaction：集成四引擎、层级、搜索与统计

- [ ] 5.1 依赖 1–4 完成后，对照规范事实源 `specs/tui-interaction.md`，将 Pi 设为默认启动引擎，并从来源 `src/app.rs`/`src/tui.rs` 选择性移植 Pi → OpenCode → Codex → Claude 的 Tab/Shift+Tab 循环；初始化必须请求 Pi catalog，切换时清除旧引擎 selection/detail/delete/search match 并请求新 catalog。
- [ ] 5.2 以 catalog 的父子/Agent 元数据构建递归树；组归属以根会话为准，展开父节点后选择后代，组删除收集全部后代；缺失父、循环、重复父 ID 显示为 detached root。
- [ ] 5.3 恢复 `/` 搜索输入、实时不区分大小写元数据匹配、Enter 提交、Esc 取消、`n`/`N` 循环跳转、刷新后重算，以及普通/当前命中的视觉高亮。
- [ ] 5.4 恢复 Group Summary：Sessions 总数、Main sessions、Subagents、Last Active；递归后代计入总数与 Subagents。
- [ ] 5.5 更新帮助/状态提示且保持 modal、list/detail 焦点、鼠标和布局快捷键优先级；窄终端时搜索光标不得越界。
- [ ] 5.6 加入 focused app/TUI/PTY tests：默认启动进入 Pi 并请求 Pi catalog、四引擎切换、层级异常、递归计数、搜索状态机、实时计数、`n`/`N`、高亮样式、输入模式吞掉全局键、PTY 快捷键。
- [ ] 5.7 运行 app/TUI/PTY focused tests，并记录准确命令与结果。

## 6. 集成、Reviewer 与视觉验收

- [ ] 6.1 运行 `cargo fmt --check` 和项目现有静态检查；只修复本 Change 引入的问题。
- [ ] 6.2 在所有 focused tests 通过后运行完整 `cargo test`，记录命令、通过/失败数量及任何既有失败。
- [ ] 6.3 实现与测试完成后最小同步 `README.md`：仅更新支持引擎列表、默认 Pi、Pi → OpenCode → Codex → Claude 的 Tab 顺序、`/` 与 `n`/`N` 搜索快捷键，以及 Group Summary 的 Main sessions/Subagents；禁止无关改写或结构重排。
- [ ] 6.4 Reviewer 按五个 Delta Specs 对照实现 diff 与来源 commit，确认是选择性最小移植、无整体套用、无 PRD-7、无无关重构、无安全边界放宽，并确认 README 只包含 6.3 的最小同步。
- [ ] 6.5 Reviewer 按 `ux-design.md` 在可交互终端完成视觉验收：默认 Pi、四 tabs 连续切换、搜索实时计数/焦点/高亮、递归树与 detached 状态、Group Summary、空态和错误态。
- [ ] 6.6 将可复现命令、截图或终端录屏路径和 Reviewer 结论写入 Change 的 review 工件；任何 SQLite 写入需求、删除边界放宽或无法唯一关联父节点的方案转为 `needs_decision`。

## Completion Notes

仅记录可复现证据的命令和 Change 内路径；QA/Reviewer 结论写入 `reviews/`。Planner 未运行实现测试，本工件基于当前代码、六个 System Specs 与来源 commit 的静态审计。
