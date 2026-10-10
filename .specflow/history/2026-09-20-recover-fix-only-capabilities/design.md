# Design: recover-fix-only-capabilities

## Classification

- Design Impact：visual。
- 本 Change 调整现有 TUI 的引擎 tabs、搜索状态、树层级和 Group Summary；不修改设计宪章、tokens、组件体系或主题，因此不是 `design_system`。
- 可执行视觉契约见 `ux-design.md`。

## Context and Constraints

- 当前架构以 `SessionEngine` 驱动 catalog/detail/launch/delete 分派，App 用 request ID 与 engine context 拒绝 stale 异步结果，TUI 通过 App state 渲染列表和详情。
- 当前 System Specs 只明确 Codex/Claude；Pi/OpenCode、搜索和父子 Agent 数据尚未进入当前分支契约与实现。
- 来源 commit `f81c1275036b1b2d8946dfee93505bb464b58f57` 同时修改多个能力和依赖，只能作为实现/测试证据，不能整体应用。
- 必须保留现有 terminal suspend/rebuild、cwd 验证、确认删除、canonical-root、局部解析降级和 stale-result 防护。
- 只做恢复能力所需的最小接线；不重写 catalog 抽象、不增加 speculative provider framework、不做设计系统重做。

## Chosen Architecture

### Engine dispatch

- 将 Pi、OpenCode 加入现有 engine 枚举和 engine-aware 分派，而不是建立第二套应用流程。
- 默认活动引擎显式改为 Pi：App/TUI 初始化必须选择 Pi 并产生首个 Pi catalog request；这是需要独立 app test 覆盖的行为变化，不从 enum 顺序隐式推导。
- 固定 TUI 顺序为 Pi、OpenCode、Codex、Claude；Tab/Shift+Tab 按该顺序正向/反向循环。所有 catalog/detail/delete 结果继续携带 engine/request identity，只有当前 context 接受结果。
- 复用现有外部命令前后 terminal lifecycle，不为 Pi/OpenCode 新建终端管理层。

### Pi data flow

- Catalog：递归扫描 Pi session 根目录内 JSONL，复用文件型引擎的 canonical-root 验证、逐文件 warning 和 mtime 排序。
- Metadata：解析真实 user summary 与最新 `session_info` 名称；仅从可验证的子代理 tool-result session 路径建立 `parent_session_id` 和 `agent_name`。
- Hierarchy：关系解析失败、父节点缺失、循环或 ID 不唯一时保留 detached root，数据可见性优先于猜测。
- Detail：沿用 viewport collector 和可见 block 配置，只增加 Pi record 映射；不得暴露 reasoning/transport noise。
- Launch：恢复使用 `pi --session SESSION_ID`，新建使用 `pi`，cwd 仍由现有边界验证。
- Delete：Pi 是文件型引擎；canonical target 必须位于 Pi root，symlink 出界拒绝。

### OpenCode data flow and SQLite boundary

- 数据库位置由 OpenCode engine root 解析到 `opencode.db`；catalog/detail 连接使用 SQLite read-only flags。
- Catalog 查询 `session` 中未归档记录，按 `time_updated` 倒序；先探测列，`parent_id`/`agent` 缺失时返回 null，而不是假定 schema 版本。
- Detail 以 catalog 产生的 session ID 参数化查询 `session`、`message`、`part`；按时间顺序组装 viewport。session ID 不作为路径，也不拼接到 SQL。
- App 不直接更新或删除 SQLite。删除调用当前平台的 `opencode session delete SESSION_ID`，以 CLI exit status 作为成功/失败；catalog 只在成功后更新。
- Launch 使用当前平台 OpenCode 可执行文件；恢复传 cwd 与 `--session SESSION_ID`，新建传 cwd；继续复用 cwd 校验和错误传播。

### Shared relation model

- 在现有 session list item 上增加可选 `parent_session_id`、`agent_name`，避免新建平行 tree model。
- 分组先建立 session ID 到唯一节点的索引，再递归附着；重复 ID 不用于父关联，循环节点 detached。
- 子代理始终跟随根 Main session 的 group；Group Summary 从同一棵构建后的树计算 root、递归 descendants 和 last active，避免渲染层二次推导不一致。

### Search state

- 搜索是 App 内的小型状态机：input pattern、last committed pattern、matching catalog indexes、current match。
- 匹配源是 session metadata 的规范化拼接文本，使用 Unicode-aware lowercase 后 substring 匹配；无需索引或新依赖，当前 catalog 规模下线性重算最小且可测试。
- catalog 刷新后按 last pattern 重算；选择命中时复用 tree path 打开祖先并触发正常 detail request。
- 渲染只对可见字符串切分 spans；当前匹配额外 underline，不改变数据或选择模型。

## Security and Safety Decisions

- 文件型引擎的发现、详情、删除都保留 canonical-root 检查；catalog 内路径不能作为信任证明。
- OpenCode SQLite 只读且 SQL 参数化；禁止应用内写库、schema migration 和基于 session ID 的路径访问。
- OpenCode 删除只通过显式确认后的 CLI 调用；参数作为单独 argv 传递，不经 shell。
- 外部命令运行前验证 cwd 是可访问目录；失败、非零退出和无 exit status 均保留当前上下文并呈现错误。
- 所有引擎切换清除 pending delete，防止跨 engine 执行陈旧 target。
- 父子关系不唯一时不猜测，避免错误 group 删除扩大影响范围。

## Data / API / Dependency Impact

- 内部数据：Session list item 增加可选 parent/agent 字段；group/tree summary 增加 root/subagent 递归计数。
- 内部 API：engine-aware catalog/detail/launch/delete 分派扩展到四 engine；不新增公开网络 API。
- SQLite：只读查询现有 OpenCode schema，无 migration、无写事务。
- 依赖：来源 commit 使用 SQLite Rust 依赖；Implementer 必须先检查当前 Cargo 依赖，再选择当前仓库已接受或来源已验证的最小方案。只为只读 OpenCode 查询引入必要依赖，不移植来源 commit 的无关依赖/lockfile 变化。
- 配置：不新增用户配置或默认引擎开关；Pi 是本 Change 明确规定并测试的启动默认值。Pi/OpenCode 根位置遵循现有 home-root 解析方式和平台可执行文件命名。
- 用户文档：实现验收后只最小更新 `README.md` 的支持引擎、默认 Pi、Tab 顺序、`/` 与 `n`/`N`、Main/Subagent Group Summary；不重排或改写无关章节。

## Alternatives Considered

- 整体 cherry-pick 来源 commit：拒绝；混合能力、依赖、README 和大范围实现，且当前基线已演进。
- 为每个引擎复制 App/TUI：拒绝；现有 engine-aware 流程可直接扩展，复制会产生四套状态机。
- 直接写 OpenCode SQLite 删除：拒绝；破坏所有权与 schema 安全边界，官方 CLI 已提供删除语义。
- 由 cwd 或 ID 猜测 Pi 父子关系：拒绝；证据不足时可能错误扩大组删除。
- 新建 search 服务或索引：拒绝；当前内存 catalog 线性 substring 足够，来源测试也覆盖该模型。
- 重做设计系统以容纳新状态：拒绝；现有主题与 TUI primitives 足够表达 tabs、highlight、detached 和 summary。

## Risks and Mitigations

- SQLite schema 演进：列探测、只读打开、参数化查询、fixture 覆盖缺失 `agent`。
- 深层/循环关系：唯一父索引、visited/cycle 防护、detached fallback、递归测试。
- 大 catalog 搜索重算：先保持 O(n) 简单实现；只有可测量输入延迟超过交互预算时再评估索引。
- 命令版本差异：对 argv/cwd 单元测试并在视觉验收使用已安装 CLI；不静默改用 shell。
- 视觉拥挤：保留现有布局，长搜索输入截取尾部并预留计数，Agent/ detached 使用短标签。

## Rollback

- 每个 domain 按拓扑独立提交和验收，可撤回 Pi/OpenCode dispatch、search/hierarchy rendering，而不触碰已有 Codex/Claude 与 terminal lifecycle。
- OpenCode 没有数据库写入或 migration，回滚无需数据修复。
- 若某引擎失败，应回滚该引擎选择性移植，而不是放宽路径、SQL 或删除边界。
