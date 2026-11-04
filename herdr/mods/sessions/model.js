// Pure logic for the sessions mod: project tree, rows, sorting, marks, hit
// zones and engine preferences. No Herdr imports, so
// `node --test model.test.mjs` runs it as is.

export const ENGINES = ["pi", "codex", "claude"];
export const ENGINE_LABEL = { pi: "Pi", codex: "Codex", claude: "Claude" };

/** The mark key of a session: `engine:session_id`. */
export const itemKey = (it) => `${it.engine}:${it.id}`;

/** The project a session belongs to. The `cwd` is the key so same-named folders stay apart. */
export const projectOf = (it) => it.cwd || it.group || "-";

// ---------------------------------------------------------------- preferences

export const DEFAULT_PREFS = { order: ENGINES.slice(), hidden: [], sort: "time", default: "all" };
export const SORTS = ["time", "engine", "section"];

/** Turns whatever is stored into valid preferences (unknown engines dropped, missing ones appended). */
export function normalizePrefs(raw) {
  const r = raw && typeof raw === "object" ? raw : {};
  const order = [];
  for (const e of Array.isArray(r.order) ? r.order : []) if (ENGINES.includes(e) && !order.includes(e)) order.push(e);
  for (const e of ENGINES) if (!order.includes(e)) order.push(e);
  let hidden = [...new Set(Array.isArray(r.hidden) ? r.hidden.filter((e) => ENGINES.includes(e)) : [])];
  if (hidden.length >= ENGINES.length) hidden = hidden.slice(0, ENGINES.length - 1);
  const sort = SORTS.includes(r.sort) ? r.sort : "time";
  const def = r.default === "all" || (ENGINES.includes(r.default) && !hidden.includes(r.default)) ? r.default : "all";
  return { order, hidden, sort, default: def };
}

export const visibleEngines = (prefs) => prefs.order.filter((e) => !prefs.hidden.includes(e));

/** Moves `engine` one place up (-1) or down (+1); the ends stay put. */
export function moveEngine(prefs, engine, delta) {
  const order = prefs.order.slice();
  const i = order.indexOf(engine), j = i + delta;
  if (i < 0 || j < 0 || j >= order.length) return prefs;
  [order[i], order[j]] = [order[j], order[i]];
  return normalizePrefs({ ...prefs, order });
}

/** Shows/hides an engine; the last visible engine cannot be hidden. */
export function toggleEngine(prefs, engine) {
  const hidden = prefs.hidden.includes(engine) ? prefs.hidden.filter((e) => e !== engine) : prefs.hidden.concat(engine);
  if (hidden.length >= ENGINES.length) return prefs;
  return normalizePrefs({ ...prefs, hidden });
}

// ---------------------------------------------------------------- filtering

export function tokensOf(query) {
  return (query ?? "").trim().toLowerCase().split(/\s+/).filter(Boolean);
}

export function matchesTokens(it, tokens) {
  if (!tokens.length) return true;
  const hay = [it.name, it.summary, it.cwd, it.group, it.agent, it.id].filter(Boolean).join("\n").toLowerCase();
  return tokens.every((t) => hay.includes(t));
}

/** Sessions of one engine (or "all") that match the query and are not hidden. */
export function filterItems(items, { engine = "all", query = "", hidden = [] } = {}) {
  const tokens = tokensOf(query);
  return items.filter((it) => !hidden.includes(it.engine) && (engine === "all" || it.engine === engine) && matchesTokens(it, tokens));
}

// ---------------------------------------------------------------- project tree

const lastSegment = (path) => String(path).split("/").filter(Boolean).pop() || path || "-";

/** `group` is "<parent>/<name>"; its parent segment titles the sidebar section. */
function sectionOf(it) {
  const parts = String(it.group || it.cwd || "").split("/").filter(Boolean);
  return parts.length >= 2 ? parts[parts.length - 2] : "其他";
}

/**
 * The sidebar: total, the three most recently active projects and the
 * remaining projects grouped by parent directory (newest first). Counts follow
 * the engine and title filters; projects with no match are hidden except
 * `selected`.
 */
export function buildProjects(items, { engine = "all", query = "", hidden = [], selected = "all" } = {}) {
  const pool = filterItems(items, { engine, query, hidden });
  const map = new Map();
  for (const it of pool) {
    const key = projectOf(it);
    if (!map.has(key)) map.set(key, { key, label: lastSegment(it.cwd || it.group), section: sectionOf(it), count: 0, newest: 0, engines: {}, existing: 0 });
    const p = map.get(key);
    p.count++;
    p.newest = Math.max(p.newest, it.modified_ms ?? 0);
    p.engines[it.engine] = (p.engines[it.engine] ?? 0) + 1;
    if (it.cwd_exists) p.existing++;
  }
  if (selected !== "all" && !map.has(selected)) {
    const known = items.find((it) => projectOf(it) === selected);
    map.set(selected, { key: selected, label: lastSegment(known?.cwd || known?.group || selected), section: known ? sectionOf(known) : "其他", count: 0, newest: 0, engines: {}, existing: 0 });
  }
  const projects = [...map.values()].map((p) => ({ ...p, missing: p.count > 0 && p.existing === 0 }));
  projects.sort((a, b) => b.newest - a.newest);
  const recent = projects.filter((p) => p.count > 0).slice(0, 3);
  const sections = new Map();
  for (const p of projects) {
    if (!sections.has(p.section)) sections.set(p.section, { title: p.section, newest: 0, projects: [] });
    const s = sections.get(p.section);
    s.projects.push(p);
    s.newest = Math.max(s.newest, p.newest);
  }
  return { total: pool.length, recent, sections: [...sections.values()].sort((a, b) => b.newest - a.newest) };
}

/** Mark keys of every session of a project (for "mark whole project"). */
export function projectMarkKeys(items, project, { engine = "all", query = "", hidden = [] } = {}) {
  return filterItems(items, { engine, query, hidden }).filter((it) => projectOf(it) === project).map(itemKey);
}

// ---------------------------------------------------------------- sorting and rows

/** Roots only; children stay time-ordered under their parent. */
export function sortRoots(items, prefs, { project = "all" } = {}) {
  const byTime = (a, b) => (b.modified_ms ?? 0) - (a.modified_ms ?? 0);
  const list = items.slice().sort(byTime);
  if (project === "all" || prefs.sort === "time") return list;
  const rank = (e) => prefs.order.indexOf(e);
  return list.sort((a, b) => rank(a.engine) - rank(b.engine) || byTime(a, b));
}

/**
 * The session table: optional engine section rows, then root sessions with
 * their children (collapsed until the parent's row key is in `expanded`; a
 * title filter opens everything). Odd/even `stripe` counts roots only, so
 * expanding children does not shift the stripes below.
 *
 * Returns `{ rows, visible }`; `visible` holds the mark keys of every listed
 * session (collapsed children included).
 */
export function buildRows(items, { engine = "all", query = "", expanded = new Set(), project = "all", prefs = DEFAULT_PREFS } = {}) {
  const tokens = tokensOf(query);
  let pool = filterItems(items, { engine, query, hidden: prefs.hidden });
  if (project !== "all") pool = pool.filter((it) => projectOf(it) === project);
  const visible = new Set(pool.map(itemKey));

  const ids = new Map();
  for (const it of pool) {
    const k = `${it.engine}\0${it.id}`;
    ids.set(k, (ids.get(k) ?? []).concat(it));
  }
  const parentOf = new Map();
  for (const it of pool) {
    if (!it.parent_id) continue;
    const c = ids.get(`${it.engine}\0${it.parent_id}`);
    if (c && c.length === 1 && c[0] !== it) parentOf.set(it, c[0]);
  }
  // A cycle has no root: cut its members loose so they still show up.
  for (const it of pool) {
    const seen = new Set([it]);
    let cur = parentOf.get(it);
    while (cur) {
      if (seen.has(cur)) { parentOf.delete(it); break; }
      seen.add(cur);
      cur = parentOf.get(cur);
    }
  }

  const children = new Map();
  const roots = [];
  for (const it of pool) {
    const p = parentOf.get(it);
    if (p) children.set(p, (children.get(p) ?? []).concat(it));
    else roots.push(it);
  }
  const byTime = (a, b) => (b.modified_ms ?? 0) - (a.modified_ms ?? 0);

  const rowKeys = new Map();
  const used = new Set();
  const rowKeyOf = (it) => {
    if (rowKeys.has(it)) return rowKeys.get(it);
    let k = itemKey(it);
    if (used.has(k)) k = `${k}@${it.path}`;
    used.add(k);
    rowKeys.set(it, k);
    return k;
  };

  const rows = [];
  let stripe = 0;
  const walk = (it, depth) => {
    const kids = (children.get(it) ?? []).slice().sort(byTime);
    const key = rowKeyOf(it);
    const open = tokens.length > 0 || expanded.has(key);
    if (depth === 0) stripe++;
    rows.push({ type: "item", key, mark: itemKey(it), item: it, depth, childCount: kids.length, expanded: open, stripe });
    if (open) for (const k of kids) walk(k, depth + 1);
  };
  const sorted = sortRoots(roots, prefs, { project });
  const sectioned = project !== "all" && prefs.sort === "section";
  let section = null;
  for (const r of sorted) {
    if (sectioned && r.engine !== section) {
      section = r.engine;
      rows.push({ type: "section", key: `s:${r.engine}`, engine: r.engine, label: ENGINE_LABEL[r.engine] ?? r.engine, count: sorted.filter((x) => x.engine === r.engine).length });
    }
    walk(r, 0);
  }
  return { rows, visible };
}

export const indexOfKey = (rows, key) => rows.findIndex((r) => r.key === key);

const selectable = (r) => r.type === "item";

/** The row key `delta` selectable rows away from `key`, clamped; the first one when `key` is unknown. */
export function moveSelection(rows, key, delta) {
  const idx = rows.map((r, i) => (selectable(r) ? i : -1)).filter((i) => i >= 0);
  if (!idx.length) return null;
  const at = idx.indexOf(indexOfKey(rows, key));
  if (at < 0) return rows[idx[0]].key;
  return rows[idx[Math.max(0, Math.min(idx.length - 1, at + delta))]].key;
}

/** The root row at or above `key` (a collapsed child falls back to its root). */
export function rootKeyOf(rows, key) {
  for (let i = indexOfKey(rows, key); i >= 0; i--) if (rows[i].type === "item" && rows[i].depth === 0) return rows[i].key;
  return key;
}

/** Mark keys of every session that is the parent of another one. */
function parentKeys(items) {
  const ids = new Set(items.map(itemKey));
  const out = new Set();
  for (const it of items) if (it.parent_id && ids.has(`${it.engine}:${it.parent_id}`)) out.add(`${it.engine}:${it.parent_id}`);
  return out;
}

/**
 * The `z` key: folds every parent session when any is open, otherwise opens
 * them all. Returns the new `expanded` set and the row to keep selected.
 */
export function toggleAllChildren(rows, items, selKey) {
  const anyOpen = rows.some((r) => r.type === "item" && r.expanded && r.childCount > 0);
  if (anyOpen) return { expanded: new Set(), selKey: rootKeyOf(rows, selKey) ?? selKey, folded: true };
  return { expanded: parentKeys(items), selKey, folded: false };
}

// ---------------------------------------------------------------- hit zones

export const CHECK_W = 36;
export const RESUME_W = 46;
export const FOLD_W = 48;

/**
 * Which part of a table row an x coordinate (relative to the row's left edge)
 * lands on: `check`, `resume`, `fold` (only with children) or `row`.
 */
export function hitZone(x, width, hasKids, { check = true } = {}) {
  if (check && x < CHECK_W) return "check";
  if (x >= width - RESUME_W) return "resume";
  if (hasKids && x >= width - RESUME_W - FOLD_W) return "fold";
  return "row";
}

// ---------------------------------------------------------------- marks (Sets of `engine:id`), always new Sets

export function toggleMark(marks, k) {
  const n = new Set(marks);
  if (n.has(k)) n.delete(k); else n.add(k);
  return n;
}

/** Marks all of `keys`, or unmarks them when they are all marked already. */
export function toggleMany(marks, keys) {
  const n = new Set(marks);
  const all = keys.length > 0 && keys.every((k) => n.has(k));
  for (const k of keys) { if (all) n.delete(k); else n.add(k); }
  return n;
}

/** `base` plus every session row between the two row keys (inclusive). */
export function rangeMarks(base, rows, fromKey, toKey) {
  const a = indexOfKey(rows, fromKey), b = indexOfKey(rows, toKey);
  const n = new Set(base);
  if (a < 0 || b < 0) return n;
  for (let i = Math.min(a, b); i <= Math.max(a, b); i++) if (rows[i].type === "item") n.add(rows[i].mark);
  return n;
}

/** Drops marks of sessions that no longer exist. */
export function pruneMarks(marks, items) {
  const live = new Set(items.map(itemKey));
  return new Set([...marks].filter((k) => live.has(k)));
}

/**
 * What a delete confirmation shows: totals per engine, the first few titles,
 * and the marked sessions the current filter hides.
 */
export function summarizeMarks(marks, items, visible, { titles = 5 } = {}) {
  const targets = items.filter((it) => marks.has(itemKey(it)));
  const byEngine = {};
  for (const it of targets) byEngine[it.engine] = (byEngine[it.engine] ?? 0) + 1;
  const hidden = targets.filter((it) => !visible.has(itemKey(it)));
  return {
    total: targets.length,
    byEngine,
    titles: targets.slice(0, titles).map((it) => it.name),
    more: Math.max(0, targets.length - titles),
    hidden,
    targets,
  };
}

/** The payload for `api delete`. */
export const deleteTargets = (targets) => targets.map((it) => ({ engine: it.engine, id: it.id, path: it.path }));

/** Applies an `api delete` result: deleted sessions lose their mark, failures keep it. */
export function applyDeleteResult(marks, result) {
  const n = new Set(marks);
  for (const d of result.deleted ?? []) n.delete(`${d.engine}:${d.id}`);
  const failures = new Map();
  for (const f of result.failed ?? []) failures.set(`${f.engine}:${f.id}`, f.error);
  return { marks: n, failures };
}

// ---------------------------------------------------------------- display helpers

export function ago(ms, now = Date.now()) {
  const s = Math.max(0, Math.floor((now - ms) / 1000));
  if (s < 60) return "刚刚";
  if (s < 3600) return `${Math.floor(s / 60)} 分钟前`;
  if (s < 86400) return `${Math.floor(s / 3600)} 小时前`;
  if (s < 86400 * 30) return `${Math.floor(s / 86400)} 天前`;
  return new Date(ms).toISOString().slice(0, 10);
}

/** Table column format: 刚刚, 31 分, 22 时, 3 天, then MM-DD (YYYY-MM across years). */
export function agoShort(ms, now = Date.now()) {
  const s = Math.max(0, Math.floor((now - ms) / 1000));
  if (s < 60) return "刚刚";
  if (s < 3600) return `${Math.floor(s / 60)} 分`;
  if (s < 86400) return `${Math.floor(s / 3600)} 时`;
  if (s < 86400 * 30) return `${Math.floor(s / 86400)} 天`;
  const d = new Date(ms), n = new Date(now);
  const p = (v) => String(v).padStart(2, "0");
  return d.getFullYear() === n.getFullYear() ? `${p(d.getMonth() + 1)}-${p(d.getDate())}` : `${d.getFullYear()}-${p(d.getMonth() + 1)}`;
}

export function tabName(it) {
  const name = (it.name || "session").replace(/\s+/g, " ").trim();
  const clipped = [...name].slice(0, 24).join("");
  return `${it.engine}: ${clipped}`;
}

/** Splits `text` into [{text, hit}] runs around every occurrence of a term (lowercase terms). */
export function highlight(text, terms) {
  const lower = text.toLowerCase();
  if (lower.length !== text.length || !terms.length) return [{ text, hit: false }];
  const mark = new Array(text.length).fill(false);
  for (const term of terms) {
    if (!term) continue;
    for (let at = lower.indexOf(term); at >= 0; at = lower.indexOf(term, at + term.length)) {
      for (let i = at; i < at + term.length; i++) mark[i] = true;
    }
  }
  const runs = [];
  for (let i = 0; i < text.length; i++) {
    const last = runs[runs.length - 1];
    if (last && last.hit === mark[i]) last.text += text[i];
    else runs.push({ text: text[i], hit: mark[i] });
  }
  return runs;
}

const WIDE = "\\u2e80-\\u9fff\\uf900-\\ufaff\\uff00-\\uffef";
const TOKEN = new RegExp(`[${WIDE}]|[^\\s${WIDE}]+\\s*|\\s+`, "g");

/** Highlight runs split into the pieces a line may break between: one wide character, or a word with its trailing space. */
export function flowTokens(runs) {
  return runs.flatMap((run) => (run.text.match(TOKEN) ?? []).map((text) => ({ text, hit: run.hit })));
}

/** Runs for a one-line preview: at most `lead` characters stay before the first match, so it is on screen. */
export function previewRuns(runs, lead) {
  const at = runs.findIndex((run) => run.hit);
  if (at < 0) return runs;
  const before = [...runs.slice(0, at).map((run) => run.text).join("")];
  const keep = Math.max(0, lead);
  const head = before.length > keep ? `…${before.slice(before.length - keep).join("")}` : before.join("");
  return [...(head ? [{ text: head, hit: false }] : []), ...runs.slice(at)];
}

/** Hits of a full-text search narrowed to the visible engines and the engine filter. */
export function filterHits(hits, { engine = "all", hidden = [] } = {}) {
  return hits.filter((h) => !hidden.includes(h.engine) && (engine === "all" || h.engine === engine));
}

/** Projects of the current hits, for the sidebar in full-text mode. */
export function hitProjects(hits) {
  const map = new Map();
  for (const h of hits) {
    const key = h.cwd || "-";
    if (!map.has(key)) map.set(key, { key, label: lastSegment(h.cwd), count: 0, newest: 0 });
    const p = map.get(key);
    p.count++;
    p.newest = Math.max(p.newest, h.modified_ms ?? 0);
  }
  return [...map.values()].sort((a, b) => b.newest - a.newest);
}
