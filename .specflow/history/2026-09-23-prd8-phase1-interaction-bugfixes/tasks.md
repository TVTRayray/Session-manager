# Tasks: prd8-phase1-interaction-bugfixes

## 依赖关系声明
> `tui-interaction depends_on session-catalog` 设定主要体现 Phase 1 内实施与综合验证顺序（先保证目录树层级完整，再验证列表上的滚轮点击与宽字符排版），在代码实现与编译单元上两者相互独立，不存在底层强依赖。

---

## spec-01: session-catalog (F1: tintinweb 子代理识别与父子关联移植)

- [x] 1.1 在 `src/catalog.rs` 中实现 `tintinweb_child_identity(path: &Path) -> Option<(String, String)>`:
  - 读取 Pi 会话文件的 `session_info` 消息，识别形如 `<AgentType>#<8-hex-id>`（例如 `Explore#a1b2c3d4`）的名称。
  - 校验后缀 8 位字符严格为 16 进制字符（`is_ascii_hexdigit`），返回 `(agent_type, id8)` 元组。
- [x] 1.2 在 `src/catalog.rs` 中实现 `apply_tintinweb_relations(items: &mut [SessionListItem])`:
  - 遍历所有父会话候选项，扫描 `toolResult` 记录且 `toolName` 属于 `"Agent"` 或 `"subagent"` 的消息。
  - 提取 `subagentType` 与 `agentId`（截取前 8 位 hex），构建 `(agent_type, id8) -> Vec<(parent_session_id, parent_abs_path)>` 映射表。
  - 遍历尚未绑定父级的子会话，通过 `tintinweb_child_identity` 获取其身份。
  - 结合双来源（`recorder.cwd` / transcript `cwd` 以及 session header `parentSession` / `cwd`）准确匹配原始父级会话，消除 forked 副本歧义。
  - 为匹配成功的子会话设置 `parent_session_id` 与 `agent_name`；已具有原生 Pi 关系的子会话严格不覆盖；无法匹配者安全保留为独立根节点。
- [x] 1.3 在 `src/catalog.rs::scan_pi_session_dir` 中，在原生 `apply_pi_subagent_relations(&mut load.items)` 执行之后紧接着调用 `apply_tintinweb_relations(&mut load.items)`。
- [x] 1.4 编写针对 F1 的单元测试与回归用例 (`src/catalog.rs`):
  - `tintinweb_persisted_children_link_via_agent_id_prefix`: 验证标准 `Explore#a1b2c3d4` 命名与 `toolResult` 成功关联并展示为子节点。
  - `tintinweb_preserves_existing_native_subagent_link`: 验证已通过原生 `subagent` 绑定的子代理不被 tintinweb 逻辑覆盖。
  - `tintinweb_leaves_unmatched_subagent_as_independent_root`: 验证无匹配父会话的子代理安全保留为根会话，无 panic。
  - `tintinweb_forked_sessions_disambiguated_by_parent_path_and_cwd`: 验证父会话存在分叉副本时，子代理依据双来源工作目录与 `parentSession` 准确绑定原始父会话。

---

## spec-02: tui-interaction (F2: 滚轮视口鼠标点击准确定位 + F3: 宽字符与高亮渲染安全截断)

- [x] 2.1 修复 `src/app.rs::handle_mouse` 中的列表点击视口映射 (F2):
  - 将左键点击列表区域的行号计算调整为：
    ```rust
    let visible_row = event.row.saturating_sub(layout.list_panel.y).saturating_sub(1) as usize;
    let row = self.tree_state.get_offset().saturating_add(visible_row);
    ```
  - 确保索引 `self.visible_tree_identifiers().get(row)` 进行安全越界保护；点击末尾下方空白区安全忽略。
  - 保持未滚动（`offset == 0`）时的 baseline 行为 100% 不变。
- [x] 2.2 修复 `src/tui.rs::display_char_width` Unicode 变体选择符与结合标记计算 (F3):
  - 将 `\u{fe0f}` 等变体选择符范围 `(0xfe00..=0xfe0f).contains(&code)` 纳入 0 列宽，修正 `display_char_width` 与 `display_width`。
  - 确保 `0x0300..=0x036f` 等结合标记判定为 0 列宽。
- [x] 2.3 规范化 `src/tui.rs::group_text` 与 `render_styled_session_line` 宽字符渲染 (F3):
  - 确保 `🏷️` 与 CJK 文字（如 `🏷️ 未分类`）在 Span 内部或跨 Span 具有稳定的边界，避免 `Modifier::REVERSED` 在终端引发字符错位覆盖。
  - 验证 `By Tag` 分组模式下 `🏷️ 未分类` 分组头在「选中 → 取消选中」往复循环中「未分类」三字完整恢复，汉字「未」不丢失。
  - 确保收藏图标 `★` 在选中反色状态下完整显示，无半字符切断或高亮背景色割裂。
- [x] 2.4 实现基于列宽与结合序列的安全截断 (F3):
  - 改进摘要与会话文本的截断逻辑：基于终端显示列宽 `display_width` 进行步进截断。
  - 结合标记处理：若遇到 `e\u{301}`，保证结合标记不被孤立遗留在截断边界。
  - 极窄可用宽度保护：针对 0、1、2 列等极窄宽度，安全截断或填充，严禁发生 panic、字符切碎或越界超宽。
- [x] 2.5 编写针对 F2 与 F3 的自动化测试与回归用例 (`src/app.rs`, `src/tui.rs`):
  - `mouse_click_with_scroll_offset_selects_visually_aligned_item`: 模拟滚轮下滚 10 行后点击第 3 行，验证选中数据索引为 13。
  - `mouse_click_without_scroll_preserves_baseline_index`: 验证无滚动时点击第 5 行依然选中数据索引 5。
  - `mouse_click_out_of_bounds_safely_ignored`: 验证点击列表下方空白区域不 panic。
  - `group_text_uncategorized_roundtrip_selection_preserves_characters`: 验证 `ByTag` 分组头在选中/取消选中后文本内容严格等于初始文本。
  - `display_char_width_treats_variation_selector_as_zero_width`: 验证 `\u{fe0f}` 宽度为 0，`🏷️` 总列宽为 2。
  - `truncate_summary_preserves_combining_mark_sequences_at_boundary`: 验证 `e\u{301}` 不会切碎，不留孤立结合标记。
  - `truncate_summary_handles_mixed_cjk_ascii_and_emoji`: 验证混合文本按列宽准确截断。
  - `truncate_summary_safe_at_zero_one_two_column_limits`: 验证在 0、1、2 等极窄宽度限制下安全运行且不越界、不 panic。

---

## 阶段验收与人工 TUI 交互协议 (Manual TUI Acceptance Protocol)

- [ ] 3.1 准备测试会话数据：
  - 创建包含 `@tintinweb/pi-subagents` 生成的 `Explore#xxxxxxxx` 会话文件及对应父会话 JSONL（含双来源 `cwd` 验证）。
  - 创建包含结合标记（如 `Café` 用 `e\u{301}`）、Emoji、收藏 `★`、无标签（触发 `未分类`）的会话。
- [ ] 3.2 执行 F1 终端验证：
  - 启动 Sessions Manager，切换至 Pi 引擎。
  - 检查会话树列表：tintinweb 生成的子代理必须缩进挂载在父会话下，标识为 `↳ Explore`。
- [ ] 3.3 执行 F2 鼠标交互验证：
  - 鼠标滚轮下滚列表 5 行以上。
  - 鼠标左键点击当前视口第 2 行。
  - 观察：高亮焦点必须精准停留在指针所指行，右侧详情面板展示的会话与所指行一致。
  - 再次滚轮上滚 2 行并点击，确认无累积偏移误差。
- [ ] 3.4 执行 F3 宽字符与选中恢复验证：
  - 按 `g` 键进入 `By Tag` 分组模式，找到 `🏷️ 未分类` 分组头。
  - 使用方向键或鼠标点击将光标选中该分组头，观察高亮样式；随后移动光标离开该分组头。
  - 确认：「未分类」三个汉字完整恢复，汉字「未」清晰完好，无字符缺失。
  - 选中标记为 `★` 的会话行，在 Kitty / Ghostty / 默认终端中观察 `★` 是否完整展示，无左半切断。
  - 缩小列表面板宽度至极窄，确认混合 CJK/Emoji/结合标记截断无乱码、无崩溃。
- [ ] 3.5 自动化回归全量验证：
  - 运行 `cargo test`，确保全工程测试 100% 通过，0 warnings，0 regressions。
