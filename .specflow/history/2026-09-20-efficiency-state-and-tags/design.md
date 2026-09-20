# Design: efficiency-state-and-tags

## Context and Constraints

- **现有架构**：
  - 会话浏览器目前支持 Pi, OpenCode, Codex, Claude 四大引擎，通过 `catalog.rs` 扫描原生只读目录生成 `SessionListItem`，再由 `app.rs` 和 `tui.rs` 组织树状展示并支持检索与删除。
  - 目前所有引擎数据均严格只读，没有任何本地工具自有的持久化存储。
  - 生产代码中存在对具体变体的引用（如 `active_engine: SessionEngine::Pi`，以及 `[Pi, OpenCode, Codex, Claude]` 数组）。
- **必须遵守的约束**：
  - **PRD-7 范围锁定**：仅覆盖第 1 节（引擎适配边界治理）与第 2 节（收藏/标签 + state.db 基础）。严禁引入 Markdown 导出、FTS5 全文搜索、Dashboard 或新增引擎。
  - **白名单治理（R1~R4）**：仅允许在 `catalog.rs`, `detail.rs`, `resume.rs`, `delete.rs` 及 `SessionEngine` 自身方法中出现 `match engine` / 具体变体引用。UI 与业务层不得对变体进行硬编码或条件分支。
  - **展示 label 与持久键解耦**：数据库存储与主键严禁使用 `SessionEngine::label()`，必须使用不变的非本地化 `storage_key(&self) -> &'static str`。
  - **独立持久化**：用户状态数据库必须且只能位于 `~/.session-manager/state.db`，严禁写入任何引擎数据目录。
  - **故障降级与未知 Key 容错不 panic**：如果 `state.db` 损坏、无法创建或 migration 失败，必须记录日志并安全降级到内存非持久化模式；如果遇到未知 engine key 记录，跳过并完整保留，绝不允许 panic 或误删。
  - **权限与错误提示**：若目录或文件无写权限，写入失败时需在 TUI 状态栏给出明确提示（如"无法保存：权限不足"），内存与存储保持一致，不崩溃。

## Chosen Approach

### 1. 引擎适配治理与解耦接口 (session-catalog)

在 `src/catalog.rs` 中为 `SessionEngine` 补充通用元数据、稳定持久键及能力查询方法：
```rust
impl SessionEngine {
    pub fn default_engine() -> Self {
        Self::Pi
    }

    pub fn tab_order() -> &'static [Self] {
        &[Self::Pi, Self::OpenCode, Self::Codex, Self::Claude]
    }

    /// 不可变、非本地化、全小写的持久化存储标识键
    pub fn storage_key(self) -> &'static str {
        match self {
            Self::Pi => "pi",
            Self::OpenCode => "opencode",
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }

    /// 从存储键安全反解析引擎枚举，未知 key 返回 None
    pub fn from_storage_key(key: &str) -> Option<Self> {
        match key {
            "pi" => Some(Self::Pi),
            "opencode" => Some(Self::OpenCode),
            "codex" => Some(Self::Codex),
            "claude" => Some(Self::Claude),
            _ => None,
        }
    }

    pub fn supports_resume(self) -> bool {
        // 当前所有已接入引擎均支持 resume
        true
    }

    pub fn supports_cli_delete(self) -> bool {
        // OpenCode 不支持基于文件系统的 CLI 直接物理删除
        !matches!(self, Self::OpenCode)
    }
}
```
重构消除泄漏：
- `app.rs` 初始化中的 `active_engine: SessionEngine::Pi` 替换为 `SessionEngine::default_engine()`；
- `tui.rs` 中的 Tab 栏渲染由遍历常量切片改为遍历 `SessionEngine::tab_order()`；
- `shortcut_probe.rs` 改用 `SessionEngine::default_engine()` 与通用 helper 构造 probe item。

### 2. 本地持久化状态库与稳定身份键 (persisted-state)

新建 `src/state.rs` 模块，负责 `~/.session-manager/state.db` 的生命周期与数据交互。

#### 稳定身份键设计：彻底解耦展示与存储
- **风险审查**：`SessionEngine::label()`（如 `"Pi"`, `"OpenCode"`, `"Codex"`, `"Claude"`）是 UI 展示名称，随时可能因多语言本地化、品牌变更或增加辅助提示而调整（例如 `"OpenCode (Local)"`）。以 `label()` 作为数据库主键会导致未来修改 UI 文案时，历史所有收藏与标签失联。
- **解耦设计**：数据库 `engine` 列使用 `SessionEngine::storage_key()`（固定小写协议串 `"pi"`, `"opencode"`, `"codex"`, `"claude"`）。联合主键为 `(engine, session_id)`。
- **未知 Key 兼容性**：若其他分支或后续版本在 `state.db` 中写入了新的 engine（如 `"windsurf"`），本版本读取时通过 `from_storage_key` 识别为 `None`，采取安全跳过策略：
  - 不触发 panic；
  - 内存中不装入该未知引擎会话；
  - 数据库操作（如更新已知引擎会话）绝对不执行清理未知 engine 的操作，完整保留原始数据库记录。

#### Schema 与 Migration 机制
- 使用 SQLite 的 `PRAGMA user_version` 管理 schema 版本：
```sql
-- Version 1 migration
CREATE TABLE IF NOT EXISTS session_meta (
    engine       TEXT    NOT NULL,  -- SessionEngine::storage_key() (e.g. "pi", "opencode", "codex", "claude")
    session_id   TEXT    NOT NULL,
    is_favorite  INTEGER NOT NULL DEFAULT 0,
    tags         TEXT    NOT NULL DEFAULT '',  -- JSON 数组，如 ["bug", "urgent"]
    updated_at   INTEGER NOT NULL,
    PRIMARY KEY (engine, session_id)
);
CREATE INDEX IF NOT EXISTS idx_session_meta_fav ON session_meta(is_favorite);
```
- 后续 PRD-7 第 4 节若需引入 FTS5，将在同一个 migration 链中递增版本号执行 `PRAGMA user_version = 2`。

#### 降级与安全机制
- 定义状态存储策略：
  ```rust
  pub enum StateBackend {
      Sqlite(Connection),
      InMemory(HashMap<(SessionEngine, String), SessionUserMeta>),
      Disabled,
  }
  ```
- 启动时尝试创建 `~/.session-manager` 目录并连接 `state.db`。若遇到文件损坏（SQLITE_CORRUPT）、磁盘满或其它致命 IO 错误，捕获错误并自动降级为 `InMemory` 临时模式（并在状态栏记录告警日志），确保应用 100% 可用。
- 写操作遇到 `ReadOnly` / 权限拒绝时返回显式 `StateError::PermissionDenied`，由 `App` 捕获并更新 `status_message` 提示用户。

#### 标签规范化与孤儿保留
- 用户输入的逗号分隔标签按以下规则清洗：
  1. 按逗号 `,` 分割；
  2. 去除各标签前后的 whitespace（`trim()`）；
  3. 过滤掉空字符串；
  4. 保持输入顺序进行去重（`dedup`）；
  5. 序列化为 JSON 字符串写入 SQLite。
- 扫描 catalog 时若发现 `state.db` 中有记录未在磁盘找到（Orphan），不执行 `DELETE` 操作，完整保留，避免目录临时卸载导致数据丢失。

### 3. TUI 交互与视图呈现 (tui-interaction)

#### 列表视觉
- 在 `SessionTreeNode` 与 `SessionListItem` 中携带 `is_favorite: bool` 与 `tags: Vec<String>`。
- 叶子节点摘要行：
  - 收藏：在摘要文本最前追加 `★ `；
  - 标签：在摘要文本后追加弱化主题色（`THEME_MUTED`）的 `#tag1 #tag2`；
  - 宽度受限时标签自动截断；子代理节点（`↳ Subagent`）不渲染收藏星标与标签徽标。

#### 弹窗设计 (TagModalState)
- 遵循 `DeleteModalState` 模式，在 `App` 中增加：
  ```rust
  pub struct TagModalState {
      pub target_session_id: String,
      pub target_engine: SessionEngine,
      pub input: String,
      pub cursor_position: usize,
      pub focus: TagModalFocus,
  }
  pub enum TagModalFocus {
      Input,
      Cancel,
      Save,
  }
  ```
- 互斥保证：`delete_modal_state`、`tag_modal_state`、`show_help_modal` 任意时刻仅允许一个处于激活状态。

#### 搜索框语法扩展
- 在现有 `/` 搜索逻辑中，解析用户输入：
  - 检测是否以 `fav:` 开头：若是，开启收藏过滤；
  - 检测正则/前缀 `tag:<pattern>`：若是，提取标签匹配词进行大小写不敏感匹配；
  - 提取剩余普通文本，作为普通元数据匹配项；
  - 所有条件取 `AND` 关系过滤；
  - 若仅输入 `tag:`（后无内容），平滑退化为普通文本搜索，不报错。

#### 分组模式扩展
- `GroupMode` 扩充：`ByTime`, `ByProject`, `ByTag`。
- `g` 键触发 `group_mode.toggle()` 循环切换。
- `ByTag` 分组逻辑：
  - 遍历当前会话列表：若会话无标签，放入 `未分类` 分组；
  - 若会话有 N 个标签，在每个标签对应的组中各挂载该会话（允许同一个 `session_id` 在不同标签组下重复出现）；
  - 分组图标使用 `🏷️`，显示主会话与子代理统计。

## Alternatives Considered

- **使用 `SessionEngine::label()` 作为主键列**：
  - **严重否定**：展示名属于展现层，若展示名变更或本地化，历史状态全部丢失。必须解耦为 `storage_key`。
- **使用 session 文件路径作为主键**：
  - 未采纳：用户重命名工作区、移动项目目录或在云盘同步时，绝对路径会改变，导致用户打的标签和收藏全部丢失。
- **引入复杂的 Trait 体系（如 `EngineAdapter`）**：
  - 未采纳：现有架构的枚举 match + 归一化输出结构已经非常清晰，四文件白名单配合编译器穷尽性检查足以提供最高等级的安全防护，过度设计 Trait 会徒增多态调度与心智负担。
- **将标签直接写入各个引擎的会话文件**：
  - 严禁采纳：违反工具对原生会话目录的只读承诺，且各引擎格式异构（JSONL vs SQLite），极易破坏引擎自身一致性。

## Data / API / Migration Impact

- **文件系统影响**：在用户家目录下新增 `~/.session-manager/state.db`，占用空间通常在数十 KB 至数 MB，对系统资源几乎零开销。
- **API / Struct 影响**：
  - `SessionListItem` 新增 `is_favorite: bool`, `tags: Vec<String>`。
  - `SessionEngine` 新增 `storage_key`, `from_storage_key` 及集中通用方法。
  - `App` 新增 `tag_modal_state: Option<TagModalState>`，`state_store: StateStore`。

## Risks and Rollback

- **风险 1：未来新增引擎写入了未识别记录**：
  - 缓解：由 `from_storage_key` 返回 `Option::None`，并在状态加载时安全跳过；所有写操作使用精确的 `(engine, session_id)` 主键，不执行全表清空，保证前向兼容与多版本共存数据安全。
- **风险 2：用户终端对 `★` 或 `🏷️` 等宽字符渲染异常**：
  - 缓解：使用 ratatui 标准 unicode 字符宽度测量；在超窄屏幕下提供平滑截断，确保光标与边框不被破坏。
- **回滚方案**：若状态存储出现异常，删除 `~/.session-manager/state.db` 即完全恢复至无状态初始模式，不影响任何引擎原生数据的安全与完整。
