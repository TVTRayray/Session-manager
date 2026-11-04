import test from "node:test";
import assert from "node:assert/strict";
import {
  buildRows, buildProjects, filterItems, itemKey, moveSelection, toggleMark, toggleMany, rangeMarks,
  pruneMarks, summarizeMarks, applyDeleteResult, deleteTargets, ago, agoShort, hitZone,
  normalizePrefs, moveEngine, toggleEngine, sortRoots, toggleAllChildren, rootKeyOf, highlight, flowTokens, previewRuns,
  filterHits, hitProjects, projectMarkKeys, DEFAULT_PREFS,
} from "./model.js";

const mk = (engine, id, extra = {}) => ({
  engine, id, name: `name-${id}`, summary: "", parent_id: null, agent: null, cwd_exists: true,
  modified_ms: 1000, cwd: "/w/p/a", group: "p/a", path: `/x/${engine}/${id}.jsonl`, ...extra,
});

const items = [
  mk("claude", "c1", { modified_ms: 3000 }),
  mk("claude", "c1-sub", { parent_id: "c1", modified_ms: 3500, agent: "worker" }),
  mk("pi", "p1", { modified_ms: 2000, cwd: "/w/q/b", group: "q/b", name: "迁移计划" }),
  mk("codex", "x1", { modified_ms: 1000, cwd: "/w/q/b", group: "q/b" }),
];
const keys = (rows) => rows.map((r) => r.key);

test("rows are a flat list, newest root first, children collapsed", () => {
  const { rows } = buildRows(items);
  assert.deepEqual(keys(rows), ["claude:c1", "pi:p1", "codex:x1"]);
  assert.equal(rows[0].childCount, 1);
  assert.deepEqual(rows.map((r) => r.stripe), [1, 2, 3]);
});

test("expanding children keeps the stripes of roots below unchanged", () => {
  const { rows } = buildRows(items, { expanded: new Set(["claude:c1"]) });
  assert.deepEqual(keys(rows), ["claude:c1", "claude:c1-sub", "pi:p1", "codex:x1"]);
  assert.equal(rows[1].depth, 1);
  assert.deepEqual(rows.map((r) => r.stripe), [1, 1, 2, 3]);
});

test("a project filters the table; the title filter opens children", () => {
  const { rows } = buildRows(items, { project: "/w/q/b" });
  assert.deepEqual(keys(rows), ["pi:p1", "codex:x1"]);
  assert.ok(keys(buildRows(items, { query: "name-c1" }).rows).includes("claude:c1-sub"));
});

test("a child whose parent is filtered out becomes a root; cycles stay visible", () => {
  const { rows } = buildRows(items, { query: "worker" });
  assert.ok(rows.some((r) => r.key === "claude:c1-sub" && r.depth === 0));
  const cyc = [mk("pi", "a", { parent_id: "b" }), mk("pi", "b", { parent_id: "a" })];
  const r2 = buildRows(cyc, { expanded: new Set(["pi:a", "pi:b"]) }).rows;
  assert.equal(r2.filter((r) => r.type === "item").length, 2);
});

test("duplicate ids get distinct row keys but share a mark key", () => {
  const dup = [mk("claude", "d"), mk("claude", "d", { path: "/x/other.jsonl" })];
  const rows = buildRows(dup).rows;
  assert.notEqual(rows[0].key, rows[1].key);
  assert.equal(rows[0].mark, rows[1].mark);
});

test("filtering by engine, hidden engines and Chinese text", () => {
  assert.equal(filterItems(items, { engine: "pi" }).length, 1);
  assert.equal(filterItems(items, { query: "迁移" }).length, 1);
  assert.equal(filterItems(items, { hidden: ["claude"] }).length, 2);
  assert.equal(filterItems(items, { query: "q/b name-x1" }).length, 1);
});

test("engine sorting only applies inside a project", () => {
  const prefs = normalizePrefs({ order: ["codex", "pi", "claude"], sort: "engine" });
  assert.deepEqual(sortRoots(items, prefs, { project: "all" }).map((i) => i.id), ["c1-sub", "c1", "p1", "x1"]);
  assert.deepEqual(sortRoots(items.slice(2), prefs, { project: "/w/q/b" }).map((i) => i.id), ["x1", "p1"]);
  assert.deepEqual(keys(buildRows(items, { project: "/w/q/b", prefs }).rows), ["codex:x1", "pi:p1"]);
});

test("section sort inserts engine headers that are not selectable", () => {
  const prefs = normalizePrefs({ order: ["codex", "pi", "claude"], sort: "section" });
  const { rows } = buildRows(items, { project: "/w/q/b", prefs });
  assert.deepEqual(rows.map((r) => r.type), ["section", "item", "section", "item"]);
  assert.equal(rows[0].count, 1);
  assert.equal(moveSelection(rows, "codex:x1", 1), "pi:p1");
  assert.equal(moveSelection(rows, "pi:p1", 5), "pi:p1");
  assert.equal(moveSelection(rows, "nope", 1), "codex:x1");
  assert.deepEqual([...rangeMarks(new Set(), rows, "codex:x1", "pi:p1")].sort(), ["codex:x1", "pi:p1"]);
  assert.equal(buildRows(items, { project: "all", prefs }).rows.some((r) => r.type === "section"), false);
});

test("preferences are normalised and the last engine cannot be hidden", () => {
  assert.deepEqual(normalizePrefs(null), DEFAULT_PREFS);
  const p = normalizePrefs({ order: ["claude", "bogus", "claude"], hidden: ["x", "pi"], sort: "nope", default: "pi" });
  assert.deepEqual(p.order, ["claude", "pi", "codex"]);
  assert.deepEqual(p.hidden, ["pi"]);
  assert.equal(p.sort, "time");
  assert.equal(p.default, "all");
  assert.deepEqual(normalizePrefs({ hidden: ["pi", "codex", "claude"] }).hidden, ["pi", "codex"]);
  assert.deepEqual(moveEngine(DEFAULT_PREFS, "pi", -1).order, ["pi", "codex", "claude"]);
  assert.deepEqual(moveEngine(DEFAULT_PREFS, "pi", 1).order, ["codex", "pi", "claude"]);
  let q = toggleEngine(DEFAULT_PREFS, "pi");
  q = toggleEngine(q, "codex");
  assert.equal(toggleEngine(q, "claude"), q);
  assert.deepEqual(toggleEngine(q, "pi").hidden, ["codex"]);
});

test("project tree: recent, sections, filtered counts, selected stays", () => {
  const t = buildProjects(items);
  assert.equal(t.total, 4);
  assert.deepEqual(t.recent.map((p) => p.label), ["a", "b"]);
  assert.deepEqual(t.sections.map((s) => s.title), ["p", "q"]);
  assert.deepEqual(t.sections[1].projects[0].engines, { pi: 1, codex: 1 });
  const f = buildProjects(items, { engine: "claude", selected: "/w/q/b" });
  const b = f.sections.flatMap((s) => s.projects).find((p) => p.key === "/w/q/b");
  assert.equal(b.count, 0);
  assert.equal(f.recent.length, 1);
  const missing = buildProjects([mk("pi", "m", { cwd_exists: false })]);
  assert.equal(missing.sections[0].projects[0].missing, true);
  assert.deepEqual(projectMarkKeys(items, "/w/q/b").sort(), ["codex:x1", "pi:p1"]);
});

test("z folds all children onto their root, then unfolds them all", () => {
  const { rows } = buildRows(items, { expanded: new Set(["claude:c1"]) });
  const folded = toggleAllChildren(rows, items, "claude:c1-sub");
  assert.equal(folded.folded, true);
  assert.equal(folded.expanded.size, 0);
  assert.equal(folded.selKey, "claude:c1");
  assert.equal(rootKeyOf(rows, "claude:c1-sub"), "claude:c1");
  const open = toggleAllChildren(buildRows(items).rows, items, "claude:c1");
  assert.equal(open.folded, false);
  assert.deepEqual([...open.expanded], ["claude:c1"]);
});

test("hit zones", () => {
  assert.equal(hitZone(10, 620, false), "check");
  assert.equal(hitZone(10, 620, false, { check: false }), "row");
  assert.equal(hitZone(574, 620, true), "resume");
  assert.equal(hitZone(573, 620, true), "fold");
  assert.equal(hitZone(526, 620, true), "fold");
  assert.equal(hitZone(525, 620, true), "row");
  assert.equal(hitZone(540, 620, false), "row");
});

test("marks: toggle, many, range", () => {
  let m = toggleMark(new Set(), "a");
  assert.ok(m.has("a"));
  assert.ok(!toggleMark(m, "a").has("a"));
  m = toggleMany(new Set(), ["x", "y"]);
  assert.equal(toggleMany(m, ["x", "y"]).size, 0);
  const { rows } = buildRows(items);
  const r = rangeMarks(new Set(), rows, "claude:c1", "codex:x1");
  assert.deepEqual([...r].sort(), ["claude:c1", "codex:x1", "pi:p1"]);
  assert.deepEqual([...rangeMarks(new Set(["z"]), rows, "codex:x1", "pi:p1")].sort(), ["codex:x1", "pi:p1", "z"]);
});

test("marks survive filtering and are pruned against live items", () => {
  assert.deepEqual([...pruneMarks(new Set(["claude:c1", "pi:gone"]), items)], ["claude:c1"]);
});

test("summary lists engines, titles and filtered-out selections", () => {
  const marks = new Set(["claude:c1", "pi:p1", "codex:x1"]);
  const { visible } = buildRows(items, { engine: "pi" });
  const s = summarizeMarks(marks, items, visible, { titles: 2 });
  assert.equal(s.total, 3);
  assert.deepEqual(s.byEngine, { claude: 1, pi: 1, codex: 1 });
  assert.equal(s.more, 1);
  assert.deepEqual(s.hidden.map(itemKey).sort(), ["claude:c1", "codex:x1"]);
  assert.deepEqual(deleteTargets([items[0]]), [{ engine: "claude", id: "c1", path: "/x/claude/c1.jsonl" }]);
});

test("partial delete keeps the failed marks and reports why", () => {
  const { marks, failures } = applyDeleteResult(new Set(["claude:a", "pi:b"]), {
    deleted: [{ engine: "claude", id: "a" }],
    failed: [{ engine: "pi", id: "b", error: "boom" }],
  });
  assert.deepEqual([...marks], ["pi:b"]);
  assert.equal(failures.get("pi:b"), "boom");
});

test("time formats", () => {
  assert.equal(ago(0, 30_000), "刚刚");
  assert.equal(ago(0, 5 * 3600_000), "5 小时前");
  const now = Date.UTC(2026, 10, 4, 12);
  assert.equal(agoShort(now - 10_000, now), "刚刚");
  assert.equal(agoShort(now - 31 * 60_000, now), "31 分");
  assert.equal(agoShort(now - 22 * 3600_000, now), "22 时");
  assert.equal(agoShort(now - 3 * 86400_000, now), "3 天");
  assert.equal(agoShort(Date.UTC(2026, 8, 1, 12), now), "09-01");
  assert.equal(agoShort(Date.UTC(2025, 2, 1, 12), now), "2025-03");
});

test("highlight and hit helpers", () => {
  assert.deepEqual(highlight("请迁移会话", ["迁移"]), [{ text: "请", hit: false }, { text: "迁移", hit: true }, { text: "会话", hit: false }]);
  const hits = [
    { engine: "pi", cwd: "/w/a", modified_ms: 1 }, { engine: "claude", cwd: "/w/a", modified_ms: 5 }, { engine: "codex", cwd: "/w/b", modified_ms: 2 },
  ];
  assert.equal(filterHits(hits, { engine: "pi" }).length, 1);
  assert.equal(filterHits(hits, { hidden: ["claude"] }).length, 2);
  assert.deepEqual(hitProjects(hits).map((p) => [p.label, p.count]), [["a", 2], ["b", 1]]);
});

test("flowTokens breaks between wide characters and words, keeping every character", () => {
  const runs = highlight("docs/a.md 计划 迁移 done", ["迁移"]);
  const tokens = flowTokens(runs);
  assert.equal(tokens.map((t) => t.text).join(""), "docs/a.md 计划 迁移 done");
  assert.deepEqual(tokens.filter((t) => t.hit).map((t) => t.text), ["迁", "移"]);
  assert.deepEqual(tokens.slice(0, 2).map((t) => t.text), ["docs/a.md ", "计"]);
});

test("previewRuns keeps the first match in view", () => {
  const runs = highlight("一二三四五六七八九十迁移之后", ["迁移"]);
  assert.deepEqual(previewRuns(runs, 3).map((r) => r.text), ["…八九十", "迁移", "之后"]);
  assert.deepEqual(previewRuns(runs, 50), runs);
  assert.deepEqual(previewRuns(highlight("迁移", ["迁移"]), 3).map((r) => r.text), ["迁移"]);
  assert.deepEqual(previewRuns(highlight("没有", ["迁移"]), 3), [{ text: "没有", hit: false }]);
});
