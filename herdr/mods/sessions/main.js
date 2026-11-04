// Sessions: Pi, Codex and Claude sessions in a three-column ledger (projects,
// session table, conversation) with full-text search in the same panel.
// Opened as a dialog (`mod.open:sessions`, ctrl-shift-s) or as a tab (sidebar).
//
// Parsing, scanning and deleting live in the Rust `sessions-manager api`
// commands; this file is state and keys. Pure logic is in model.js, drawing in
// toolbar.js, sidebar.js, list.js, search.js, engines.js and detail.js.

import { View, div } from "gpui-kit";
import { v_flex, h_flex, InputState, VirtualListScrollHandle, h_resizable, resizable_panel } from "gpui-base";
import { theme, props, closeOverlay, shared, setShared, nextCommand } from "herdr";
import * as api from "./api.js";
import {
  ENGINE_LABEL, buildRows, buildProjects, filterItems, filterHits, hitProjects, indexOfKey, moveSelection, itemKey, tabName,
  toggleAllChildren, toggleMark, toggleMany, rangeMarks, pruneMarks, summarizeMarks, deleteTargets, applyDeleteResult,
  hitZone, normalizePrefs, visibleEngines, projectMarkKeys, projectOf,
} from "./model.js";
import { sessionTable, rowHeight } from "./list.js";
import { sidebar, navKeys } from "./sidebar.js";
import { toolbar } from "./toolbar.js";
import { hitTable, hitDetail, runSearch, scheduleSearch, HIT_H } from "./search.js";
import { enginesPopover } from "./engines.js";
import { makeButton, declared } from "./look.js";
import { DetailPane } from "./detail.js";

const MARKS_KEY = "sessions.marks";
const PREFS_KEY = "sessions.engines";
const PANES_KEY = "sessions.panes";
const NARROW_W = 1040;

export default class Sessions extends View {
  init(_props, cx) {
    this.items = [];
    this.rows = [];
    this.visible = new Set();
    this.tree = { total: 0, recent: [], sections: [] };
    this.engineCounts = {};
    this.totalByEngine = {};
    this.warnings = [];
    this.loading = true;
    this.error = null;
    this.prefs = normalizePrefs(shared(PREFS_KEY));
    this.engine = this.prefs.default;
    this.project = "all";
    this.scope = props()?.scope === "text" ? "text" : "title";
    this.zone = "table";
    this.expanded = new Set();
    this.marks = new Set(shared(MARKS_KEY) ?? []);
    this.failures = new Map();
    this.running = api.runningKeys();
    this.selKey = null;
    this.anchor = null;
    this.rangeBase = null;
    this.press = null;
    this.projectPress = null;
    this.menu = null;
    this.enginesOpen = false;
    this.sidebarOpen = false;
    this.status = null;
    this.confirm = null;
    this.typing = false;
    this.width = 1400;
    this.tableW = 620;
    this.tableH = 600;
    this.sideW = 224;
    this.sideH = 600;
    this.vis = null;
    this.hitVis = null;
    this.viewportH = 0;
    this.selToken = 0;
    this.pendingFocus = props()?.focus ?? null; // {engine, id}: open with this session selected

    // Full-text state.
    this.hits = [];
    this.truncated = false;
    this.searched = "";
    this.searching = false;
    this.searchError = null;
    this.searchToken = 0;
    this.includeTools = false;
    this.hitSel = 0;
    this.hitProject = "all";

    this.handle = cx.focus_handle();
    this.handle.focus();
    this.scroll = VirtualListScrollHandle.new();
    this.hitScroll = VirtualListScrollHandle.new();
    this.input = InputState.new({});
    this.input.on("change", (_e, cx) => { this.queryChanged(cx); cx.notify(); });
    this.input.on("submit", (_e, cx) => { this.leaveInput(); cx.notify(); });
    this.input.on("focus", (_e, cx) => { this.typing = true; cx.notify(); });
    this.input.on("blur", (_e, cx) => { this.typing = false; cx.notify(); });
    if (this.scope === "text") this.input.focus();

    this.detail = new DetailPane((cx) => cx.notify(), {
      resume: (cx) => this.resumeSelected(cx),
      remove: (item, cx) => this.openDelete([item], cx),
      running: (item) => this.running.has(itemKey(item)),
    });

    cx.timer.every(4000, (cx) => {
      const next = api.runningKeys();
      if (next.size !== this.running.size || [...next].some((k) => !this.running.has(k))) { this.running = next; cx.notify(); }
    });
    // ctrl-shift-f while the panel has focus (keys.json: "mod:sessions") switches to full text.
    cx.spawn(async (cx) => {
      for (;;) {
        const c = await nextCommand();
        if (c === "search" || c === "sessions.search") { this.setScope("text", cx); this.input.focus(); cx.notify(); }
      }
    });
    this.load(cx);
  }

  close() {
    this.input?.release();
    this.scroll?.release();
    this.hitScroll?.release();
    this.detail?.release();
    super.close();
  }

  // ---- layout helpers ----

  narrow() {
    return this.width < NARROW_W;
  }

  /** Opened as a dialog (hotkey) rather than as a tab (sidebar entry). */
  isOverlay() {
    return Boolean(props()?.overlay);
  }

  dismiss() {
    if (this.isOverlay()) closeOverlay(props().overlay);
  }

  emptyText() {
    if (this.scope === "text") return this.searchError ?? (this.searching ? "搜索中…" : this.searched ? "没有找到匹配的消息" : "输入关键词开始搜索");
    return this.error ?? (this.loading ? "正在扫描会话…" : "没有匹配的会话");
  }

  // ---- data ----

  /** Cached catalogue first, then a fresh scan in the background. */
  load(cx) {
    this.loading = true;
    cx.spawn(async (cx) => {
      try {
        const cached = await api.list(true);
        if (cached.items?.length && !this.items.length) { this.setItems(cached.items, cx); cx.notify(); }
      } catch {}
      try {
        const fresh = await api.list(false);
        this.warnings = fresh.warnings ?? [];
        this.error = null;
        this.setItems(fresh.items ?? [], cx);
      } catch (e) {
        this.error = e.message;
      }
      this.loading = false;
      cx.notify();
    });
  }

  setItems(items, cx) {
    this.items = items;
    const pruned = pruneMarks(this.marks, items);
    if (pruned.size !== this.marks.size) { this.marks = pruned; this.saveMarks(); }
    this.rebuild(cx);
    this.applyFocus(cx);
  }

  /** Selects `pendingFocus` in the table: all projects, parents opened. */
  applyFocus(cx) {
    const f = this.pendingFocus;
    if (!f || this.scope !== "title") return;
    let cur = this.items.find((it) => it.engine === f.engine && it.id === f.id);
    if (!cur) return;
    this.project = "all";
    if (this.engine !== "all" && this.engine !== f.engine) this.engine = "all";
    for (let guard = 0; cur?.parent_id && guard < 10; guard++) {
      const parent = this.items.find((it) => it.engine === cur.engine && it.id === cur.parent_id);
      if (!parent) break;
      this.expanded.add(itemKey(parent));
      cur = parent;
    }
    this.rebuild(cx);
    const row = this.rows.find((r) => r.type === "item" && r.item.engine === f.engine && r.item.id === f.id);
    if (!row) return;
    this.pendingFocus = null;
    this.select(row.key, cx);
    this.detail.show(row.item, cx);
    this.scroll.scroll_to_item(indexOfKey(this.rows, row.key), "center");
  }

  saveMarks() {
    setShared(MARKS_KEY, [...this.marks]);
  }

  query() {
    return this.input.value();
  }

  queryChanged(cx) {
    if (this.scope === "text") scheduleSearch(this, cx);
    else this.rebuild(cx);
  }

  /** `set_value` does not emit `change`, so whoever clears the box applies it. */
  clearQuery(cx) {
    this.input.set_value("");
    this.queryChanged(cx);
  }

  /** The filter the table and the sidebar use: the box only filters titles in title mode. */
  titleQuery() {
    return this.scope === "title" ? this.query() : "";
  }

  rebuild(cx) {
    const hidden = this.prefs.hidden;
    const query = this.titleQuery();
    const { rows, visible } = buildRows(this.items, { engine: this.engine, query, expanded: this.expanded, project: this.project, prefs: this.prefs });
    this.rows = rows;
    this.visible = visible;
    this.tree = buildProjects(this.items, { engine: this.engine, query, hidden, selected: this.project });

    const pool = filterItems(this.items, { query, hidden });
    this.engineCounts = { all: pool.length };
    for (const it of pool) this.engineCounts[it.engine] = (this.engineCounts[it.engine] ?? 0) + 1;
    this.totalByEngine = {};
    for (const it of this.items) this.totalByEngine[it.engine] = (this.totalByEngine[it.engine] ?? 0) + 1;

    if (indexOfKey(rows, this.selKey) < 0) this.selKey = (rows.find((r) => r.type === "item"))?.key ?? null;
    this.syncDetail(cx, true);
  }

  selectedRow() {
    const r = this.rows.find((r) => r.key === this.selKey) ?? null;
    return r?.type === "item" ? r : null;
  }

  selectedItem() {
    return this.selectedRow()?.item ?? null;
  }

  markedItems() {
    return this.items.filter((it) => this.marks.has(itemKey(it)));
  }

  /** Points the detail pane at the selected session (debounced while moving). */
  syncDetail(cx, immediate = false) {
    const item = this.selectedItem();
    if (!item) { if (immediate) this.detail.show(null, cx); return; }
    if (immediate && !this.detail.item) { this.detail.show(item, cx); return; }
    const token = ++this.selToken;
    cx.timer.after(immediate ? 0 : 140, (cx) => {
      if (token !== this.selToken) return;
      this.detail.show(this.selectedItem(), cx);
      cx.notify();
    });
  }

  // ---- preferences ----

  setPrefs(next, cx) {
    this.prefs = normalizePrefs(next);
    setShared(PREFS_KEY, this.prefs);
    if (this.engine !== "all" && this.prefs.hidden.includes(this.engine)) this.engine = "all";
    this.rebuild(cx);
  }

  setEngine(engine, cx) {
    this.engine = engine;
    this.hitSel = 0;
    this.rebuild(cx);
    // The selection survives the switch but its row has moved.
    if (this.scope === "text") this.hitScroll.scroll_to_item(0);
    else this.scroll.scroll_to_item(Math.max(0, indexOfKey(this.rows, this.selKey)), "center");
  }

  cycleEngine(dir, cx) {
    const order = ["all", ...visibleEngines(this.prefs)];
    const i = Math.max(0, order.indexOf(this.engine));
    this.setEngine(order[(i + dir + order.length) % order.length], cx);
  }

  // ---- scope (title / full text) ----

  setScope(scope, cx) {
    if (this.scope === scope) { this.input.focus(); return; }
    this.scope = scope;
    this.menu = null;
    this.zone = "table";
    this.status = null;
    if (scope === "text") {
      this.hitProject = "all";
      this.hitSel = 0;
      if (this.query().trim()) runSearch(this, cx);
    } else {
      this.searchToken++;
      this.hits = []; this.searched = ""; this.searching = false; this.searchError = null; this.truncated = false;
      this.rebuild(cx);
    }
    this.input.focus();
  }

  runSearch(cx) {
    runSearch(this, cx);
  }

  /** Per-engine hit counts for the filter pills in full-text mode. */
  hitCounts() {
    const hits = filterHits(this.hits, { hidden: this.prefs.hidden });
    const counts = { all: hits.length };
    for (const h of hits) counts[h.engine] = (counts[h.engine] ?? 0) + 1;
    return counts;
  }

  hitsForEngine() {
    return filterHits(this.hits, { engine: this.engine, hidden: this.prefs.hidden });
  }

  hitsShown() {
    const hits = this.hitsForEngine();
    return this.hitProject === "all" ? hits : hits.filter((h) => (h.cwd || "-") === this.hitProject);
  }

  hitProjectList() {
    return hitProjects(this.hitsForEngine());
  }

  currentHit() {
    return this.hitsShown()[this.hitSel] ?? null;
  }

  clickHit(key, cx) {
    const hits = this.hitsShown();
    const i = hits.findIndex((h) => h.path === key);
    if (i < 0) return;
    const press = this.press ?? { x: 0, clicks: 1 };
    this.press = null;
    this.hitSel = i;
    this.zone = "table";
    this.status = null;
    if (hitZone(press.x, this.tableW, false, { check: false }) === "resume" || press.clicks >= 2) this.resumeHit(cx);
    cx.notify();
  }

  /** The catalogue entry of a hit; the catalogue is refreshed once when it is not there. */
  async itemForHit(hit) {
    let found = this.items.find((it) => it.path === hit.path);
    if (!found) {
      try {
        const fresh = await api.list(false);
        this.items = fresh.items ?? this.items;
        found = this.items.find((it) => it.path === hit.path);
      } catch {}
    }
    if (!found) throw new Error("会话目录里找不到这个会话，请先刷新");
    return found;
  }

  async resumeHit(cx) {
    const hit = this.currentHit();
    if (!hit) return;
    this.status = { text: "正在打开…", level: "info" };
    cx.notify();
    try {
      const item = await this.itemForHit(hit);
      await api.resume(item, tabName(item));
      this.dismiss();
    } catch (e) {
      this.status = { text: e.message, level: "error" };
    }
    cx.notify();
  }

  async locateHit(cx) {
    const hit = this.currentHit();
    if (!hit) return;
    try {
      const item = await this.itemForHit(hit);
      this.scope = "title";
      this.searchToken++;
      this.hits = []; this.searched = ""; this.searching = false; this.truncated = false;
      this.pendingFocus = { engine: item.engine, id: item.id };
      this.input.set_value("");
      this.rebuild(cx);
      this.applyFocus(cx);
      this.leaveInput();
    } catch (e) {
      this.status = { text: e.message, level: "error" };
    }
    cx.notify();
  }

  // ---- selection and marks ----

  select(key, cx, { reveal = false } = {}) {
    if (key == null) return;
    this.selKey = key;
    if (reveal) this.revealIndex(this.scroll, this.vis, indexOfKey(this.rows, key), (i) => rowHeight(this.rows[i]), this.rows.length);
    this.syncDetail(cx);
  }

  /**
   * Keeps row `index` on screen. Rows outside the visible window (shrunk to
   * what fits the viewport, one row of margin) are brought to the middle;
   * moving within the window does not scroll.
   */
  revealIndex(handle, vis, index, sizeOf, count) {
    if (!vis || index < 0 || !count) return;
    let end = vis.start;
    let used = 0;
    const limit = this.viewportH > 0 ? this.viewportH : Infinity;
    while (end < Math.min(vis.end, count) && used + sizeOf(end) <= limit) used += sizeOf(end++);
    const first = vis.start + (vis.start > 0 ? 1 : 0);
    const last = end - 2;
    if (index < first || index > last) handle.scroll_to_item(index, "center");
  }

  move(delta, cx, shift = false) {
    if (this.scope === "text") {
      const n = this.hitsShown().length;
      if (!n) return;
      this.hitSel = Math.max(0, Math.min(n - 1, this.hitSel + delta));
      this.revealIndex(this.hitScroll, this.hitVis, this.hitSel, () => HIT_H, n);
      return;
    }
    const next = moveSelection(this.rows, this.selKey, delta);
    if (shift) {
      if (this.rangeBase == null) { this.rangeBase = new Set(this.marks); this.anchor = this.selKey; }
      this.select(next, cx, { reveal: true });
      this.marks = rangeMarks(this.rangeBase, this.rows, this.anchor, next);
      this.saveMarks();
    } else {
      this.rangeBase = null;
      this.select(next, cx, { reveal: true });
    }
  }

  toggleRowMark(row) {
    this.rangeBase = null;
    this.marks = toggleMark(this.marks, row.mark);
    this.anchor = row.key;
    this.saveMarks();
  }

  toggleFold(row) {
    if (this.expanded.has(row.key)) this.expanded.delete(row.key); else this.expanded.add(row.key);
  }

  clearMarks() {
    this.marks = new Set();
    this.failures = new Map();
    this.rangeBase = null;
    this.saveMarks();
  }

  /** A click on a table row; what it means depends on where the press landed. */
  clickRow(key, cx) {
    const row = this.rows.find((r) => r.key === key);
    if (!row || row.type !== "item") return;
    const press = this.press ?? { x: 999, clicks: 1, shift: false };
    this.press = null;
    this.status = null;
    this.menu = null;
    this.zone = "table";
    const zone = hitZone(press.x, this.tableW, row.childCount > 0);
    if (zone === "check") {
      this.select(key, cx);
      this.toggleRowMark(row);
    } else if (zone === "resume") {
      this.select(key, cx);
      this.resumeRow(row, cx);
    } else if (zone === "fold") {
      this.select(key, cx);
      this.toggleFold(row);
      this.rebuild(cx);
    } else if (press.shift) {
      const from = this.anchor ?? this.selKey ?? key;
      this.marks = rangeMarks(this.marks, this.rows, from, key);
      this.saveMarks();
      this.select(key, cx);
    } else {
      this.anchor = key;
      this.rangeBase = null;
      this.select(key, cx);
      if (press.clicks >= 2) this.resumeRow(row, cx);
    }
    cx.notify();
  }

  rightClickRow(key, cx) {
    const row = this.rows.find((r) => r.key === key);
    if (!row || row.type !== "item") return;
    const press = this.press ?? { x: 0, y: 0 };
    this.zone = "table";
    this.select(key, cx);
    const it = row.item;
    const marked = this.marks.has(row.mark);
    const running = this.running.has(row.mark);
    this.menu = {
      kind: "row", key, x: press.x, y: press.y,
      items: [
        { label: running ? "跳转到终端" : "恢复会话", hint: "Enter", disabled: !it.cwd_exists && !running, run: (cx) => this.resumeRow(row, cx) },
        row.childCount > 0 ? { label: row.expanded ? "收起子会话" : "展开子会话", hint: "Space", run: (cx) => { this.toggleFold(row); this.rebuild(cx); } } : null,
        { label: "复制会话 ID", run: (cx) => { cx.write_to_clipboard(it.id); this.status = { text: "已复制会话 ID", level: "info" }; } },
        it.cwd ? { label: "复制工作目录", run: (cx) => { cx.write_to_clipboard(it.cwd); this.status = { text: "已复制工作目录", level: "info" }; } } : null,
        { label: marked ? "取消标记" : "标记", hint: "x", run: () => this.toggleRowMark(row) },
        { label: marked && this.marks.size > 1 ? `删除已标记的 ${this.marks.size} 个…` : "删除…", hint: "d", danger: true,
          run: (cx) => this.openDelete(marked && this.marks.size > 1 ? this.markedItems() : [it], cx) },
      ],
    };
    cx.notify();
  }

  openProjectMenu(key, x, y, cx) {
    this.menu = {
      kind: "project", key, x, y,
      items: [{
        label: "标记此项目全部会话",
        run: () => {
          const keys = projectMarkKeys(this.items, key, { engine: this.engine, query: this.titleQuery(), hidden: this.prefs.hidden });
          this.marks = toggleMany(this.marks, keys);
          this.saveMarks();
        },
      }],
    };
    cx.notify();
  }

  /** Picks a project in the sidebar and selects its first session. */
  selectProject(key, cx) {
    this.menu = null;
    if (this.scope === "text") {
      this.hitProject = key;
      this.hitSel = 0;
      this.hitScroll.scroll_to_item(0);
      return;
    }
    this.project = key;
    this.selKey = null;
    this.rebuild(cx);
    this.scroll.scroll_to_item(0);
  }

  // ---- actions ----

  async resumeRow(row, cx) {
    const item = row.item;
    this.status = { text: "正在打开…", level: "info" };
    cx.notify();
    try {
      await api.resume(item, tabName(item));
      this.status = null;
      this.dismiss();
    } catch (e) {
      this.status = { text: e.message, level: "error" };
    }
    cx.notify();
  }

  resumeSelected(cx) {
    const row = this.selectedRow();
    if (row) this.resumeRow(row, cx);
  }

  /** `z`: fold every parent session, or open them all. */
  toggleAll(cx) {
    const next = toggleAllChildren(this.rows, this.items, this.selKey);
    this.expanded = next.expanded;
    this.selKey = next.selKey;
    this.rebuild(cx);
    this.revealIndex(this.scroll, this.vis, indexOfKey(this.rows, this.selKey), (i) => rowHeight(this.rows[i]), this.rows.length);
    if (this.titleQuery()) this.status = { text: "筛选时子会话保持展开，清除筛选后 z 生效", level: "info" };
  }

  leaveInput() {
    this.handle.focus();
  }

  openDelete(targetItems, cx) {
    if (!targetItems.length) { this.status = { text: "先用 x 或复选框标记要删除的会话", level: "info" }; return; }
    const summary = summarizeMarks(new Set(targetItems.map(itemKey)), this.items, this.visible);
    const running = summary.targets.filter((it) => this.running.has(itemKey(it)));
    const targets = summary.targets.filter((it) => !this.running.has(itemKey(it)));
    this.confirm = { summary, running, targets, busy: false, error: null };
    this.leaveInput();
    cx.notify();
  }

  async runDelete(cx) {
    const c = this.confirm;
    if (!c || c.busy) return;
    if (!c.targets.length) { this.confirm = null; return; }
    c.busy = true;
    cx.notify();
    try {
      const result = await api.remove(deleteTargets(c.targets));
      const { marks, failures } = applyDeleteResult(this.marks, result);
      this.marks = marks;
      this.failures = failures;
      this.saveMarks();
      const gone = new Set((result.deleted ?? []).map((d) => `${d.engine}:${d.id}`));
      this.items = this.items.filter((it) => !gone.has(itemKey(it)));
      this.rebuild(cx);
      const bad = (result.failed ?? []).length;
      this.status = bad
        ? { text: `已删除 ${gone.size} 个，${bad} 个失败（原因见列表，标记已保留）`, level: "error" }
        : { text: `已删除 ${gone.size} 个会话`, level: "info" };
      this.confirm = null;
      this.load(cx);
    } catch (e) {
      c.busy = false;
      c.error = e.message;
    }
    cx.notify();
  }

  // ---- keys ----

  onKey(e, cx) {
    const key = e.key;
    const shift = e.modifiers.shift;
    const text = this.scope === "text";
    const done = () => { cx.stop_propagation(); cx.notify(); };

    if (this.confirm) {
      if (key === "escape" && !this.confirm.busy) this.confirm = null;
      else if (key === "enter") this.runDelete(cx);
      else return;
      return done();
    }
    if (this.menu) {
      if (key === "escape") { this.menu = null; return done(); }
    }
    if (this.enginesOpen) {
      if (key === "escape") { this.enginesOpen = false; return done(); }
      return;
    }
    this.status = null;

    if (text && key === "enter" && e.modifiers.control) { this.locateHit(cx); return done(); }

    if (this.typing) {
      if (key === "escape") {
        if (this.query()) this.clearQuery(cx); else this.leaveInput();
      } else if (key === "enter") {
        if (text) this.resumeHit(cx); else this.leaveInput();
      } else if (key === "down") this.move(1, cx);
      else if (key === "up") this.move(-1, cx);
      else return;
      return done();
    }
    if (e.modifiers.control || e.modifiers.alt || e.modifiers.platform) return;

    if (this.zone === "sidebar") {
      const keys = navKeys(this);
      const cur = text ? this.hitProject : this.project;
      const at = Math.max(0, keys.indexOf(cur));
      if (key === "j" || key === "down") this.selectProject(keys[Math.min(keys.length - 1, at + 1)], cx);
      else if (key === "k" || key === "up") this.selectProject(keys[Math.max(0, at - 1)], cx);
      else if (key === "right" || key === "l" || key === "enter" || key === "escape") this.zone = "table";
      else if (key === "/") this.input.focus();
      else return;
      return done();
    }

    let handled = true;
    switch (key) {
      case "escape":
        if (this.menu) this.menu = null;
        else if (this.query()) this.clearQuery(cx);
        else this.dismiss();
        break;
      case "j": case "down": this.move(1, cx, shift && key === "down" && !text); break;
      case "k": case "up": this.move(-1, cx, shift && key === "up" && !text); break;
      case "/": this.input.focus(); break;
      case "enter": if (text) this.resumeHit(cx); else this.resumeSelected(cx); break;
      case "tab": this.cycleEngine(shift ? -1 : 1, cx); break;
      case "r": if (text) this.runSearch(cx); else this.load(cx); break;
      case "left": case "h": {
        if (text) { if (this.hitProjectList().length) this.zone = "sidebar"; break; }
        const r = this.selectedRow();
        if (r && r.expanded && r.childCount > 0) { this.toggleFold(r); this.rebuild(cx); }
        else if (!this.narrow() || this.sidebarOpen) this.zone = "sidebar";
        break;
      }
      case "right": case "l": {
        const r = this.selectedRow();
        if (!text && r && r.childCount > 0 && !r.expanded) { this.toggleFold(r); this.rebuild(cx); }
        break;
      }
      default:
        if (text) { handled = false; break; }
        switch (key) {
          case "x": case "X": {
            if (shift || key === "X") this.clearMarks();
            else { const r = this.selectedRow(); if (r) this.toggleRowMark(r); }
            break;
          }
          case "space": {
            const r = this.selectedRow();
            if (r && r.childCount > 0) { this.toggleFold(r); this.rebuild(cx); }
            break;
          }
          case "z": this.toggleAll(cx); break;
          case "d": case "delete": {
            const r = this.selectedRow();
            this.openDelete(this.marks.size ? this.markedItems() : r ? [r.item] : [], cx);
            break;
          }
          default: handled = false;
        }
    }
    if (handled) done();
  }

  // ---- drawing ----

  confirmLayer(t, button) {
    const c = this.confirm;
    const { summary, running, targets } = c;
    const names = (list, max = 5) => list.slice(0, max).map((it) => `· ${it.name}`).concat(list.length > max ? [`… 另 ${list.length - max} 个`] : []);
    const section = (title, lines, color) => v_flex().gap(2)
      .child(div().text_size(12).font_semibold().text_color(color ?? t.muted).child(title))
      .children(lines.map((l, i) => div().id(`l-${title}-${i}`).text_size(12).text_color(t.muted).truncate().child(l)));
    const counts = Object.entries(summary.byEngine).map(([e, n]) => `${ENGINE_LABEL[e] ?? e} ${n}`).join(" · ");
    return div().absolute().inset_0().flex().items_center().justify_center().bg(t.background).opacity(1)
      .child(v_flex().w(540).max_h(560).p(20).gap(12).rounded(10).border_1().border_color(t.border).bg(t.popover)
        .on_mouse_down("left", (_e, cx) => cx.stop_propagation())
        .child(div().text_size(15).font_semibold().text_color(t.foreground).child(`永久删除 ${targets.length} 个会话？`))
        .child(div().text_size(12).text_color(t.ansi1).child("没有回收站，删除后无法恢复。"))
        .child(div().text_size(12).text_color(t.muted).child(counts))
        .child(section("将删除", names(targets)))
        .when(summary.hidden.length > 0, (e) => e.child(section(`其中 ${summary.hidden.length} 个已被当前筛选隐藏`, names(summary.hidden), t.blocked)))
        .when(running.length > 0, (e) => e.child(section(`${running.length} 个正在运行，不会删除`, names(running), t.blocked)))
        .when(Boolean(c.error), (e) => e.child(div().text_size(12).text_color(t.ansi1).child(c.error)))
        .child(h_flex().justify_end().gap(8)
          .child(button("c-cancel", "取消", () => { if (!c.busy) this.confirm = null; }, c.busy))
          .child(button("c-ok", c.busy ? "删除中…" : `删除 ${targets.length} 个（Enter）`, (cx) => this.runDelete(cx), c.busy || !targets.length, "danger"))));
  }

  render(cx) {
    const t = theme();
    const button = makeButton(t);
    const text = this.scope === "text";
    const saved = shared(PANES_KEY) ?? {};
    const showSide = !this.narrow() || this.sidebarOpen;

    const middle = text ? hitTable(this, t) : sessionTable(this, t, button);
    const right = text ? hitDetail(this, t, button) : this.detail.render(t, cx, { button });

    return v_flex().id("sessions").track_focus(this.handle).size_full().min_h(0).relative().bg(t.popover).text_size(13).text_color(t.foreground)
      .on_size_change((size) => { const w = declared(size.width); if (Math.abs(w - this.width) > 1) this.width = w; })
      .on_key_down((e, cx) => this.onKey(e, cx))
      .child(toolbar(this, t))
      .child(div().flex_1().min_h(0).child(
        h_resizable("sessions-panes").size_full()
          .on_resize((sizes) => { if (sizes.length === 3) setShared(PANES_KEY, { side: sizes[0], table: sizes[1] }); })
          .child(resizable_panel().size(saved.side ?? 224).size_range(180, 320).visible(showSide).child(sidebar(this, t)))
          .child(resizable_panel().size(saved.table ?? 620).size_range(420).child(middle))
          .child(resizable_panel().size_range(360).child(h_flex().size_full().min_w(0).child(right)))))
      .when(this.enginesOpen, (e) => e.child(enginesPopover(this, t)))
      .when(Boolean(this.confirm), (e) => e.child(this.confirmLayer(t, button)));
  }
}
