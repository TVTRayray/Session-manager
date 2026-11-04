// The middle column: a ledger-style table of sessions (38 px rows, striped),
// plus the shared frame that full-text hits are drawn in.
//
// Rows of a virtual list cannot hold handlers, so clicks arrive through the
// list's `on_item_click` and the zone is decided from the press position
// recorded on the frame (`view.press`, see `hitZone` in model.js).

import { div } from "gpui-kit";
import { v_flex, h_flex, Scrollbar, v_virtual_list } from "gpui-base";
import { agoShort, CHECK_W, RESUME_W, FOLD_W } from "./model.js";
import { tint, engineIcon, pill, popupMenu, declared, listPx, rowBox } from "./look.js";

export const HEADER_H = 30;
export const ROW_H = 38;
export const SECTION_H = 28;
export const BATCH_H = 44;
const CHILD_INDENT = 30;
const PROJECT_W = 112;
const TIME_W = 60;

const RELATION = { subagent: "子代理", sidechain: "子代理", fork: "分支", continued: "续接" };

export const rowHeight = (r) => (r.type === "section" ? SECTION_H : ROW_H);

function checkbox(t, on) {
  return div().w(CHECK_W).h_full().flex_none().flex().items_center().justify_center().child(
    div().w(15).h(15).rounded(4).border_1().border_color(on ? t.accent : t.dim).bg(on ? t.accent : "#00000000")
      .flex().items_center().justify_center().text_size(10).text_color(t.accent_foreground).child(on ? "✓" : ""),
  );
}

function headerRow(t, showProject) {
  const cell = (label, w) => div().w(w).flex_none().text_size(11).text_color(t.dim).child(label);
  return h_flex().flex_none().h(HEADER_H).items_center().border_b(1).border_color(t.border)
    .child(div().w(CHECK_W).flex_none())
    .child(div().w(30).flex_none())
    .child(div().flex_1().min_w(0).text_size(11).text_color(t.dim).child("标题"))
    .when(showProject, (e) => e.child(cell("项目", PROJECT_W)))
    .child(div().w(TIME_W).flex_none().text_size(11).text_color(t.dim).flex().justify_end().child("更新"))
    .child(div().w(FOLD_W).flex_none().text_size(11).text_color(t.dim).flex().justify_center().child("子会话"))
    .child(div().w(RESUME_W).flex_none());
}

function sectionRow(row, t) {
  return h_flex().id(row.key).h(rowBox(SECTION_H)).w_full().items_end().pb(4).px(14).gap(8)
    .child(div().text_size(11).font_semibold().text_color(t.muted).child(row.label))
    .child(div().text_size(11).text_color(t.dim).child(String(row.count)));
}

function sessionRow(view, row, t, showProject) {
  const it = row.item;
  const child = row.depth > 0;
  const selectedDim = view.selKey === row.key;
  const marked = view.marks.has(row.mark);
  const failure = view.failures.get(row.mark);
  const running = view.running.has(row.mark);
  const relation = it.parent_id || it.relation !== "subagent" ? RELATION[it.relation] : null;
  const bg = selectedDim ? t.selected
    : marked ? tint(t.accent, "21")
    : child ? tint(t.foreground, "05")
    : row.stripe % 2 === 0 ? tint(t.foreground, "0a") : t.popover;
  const resumeColor = running ? t.working : it.cwd_exists ? t.muted : t.dim;
  const projectName = String(it.cwd || it.group || "").split("/").filter(Boolean).pop() ?? "";

  return h_flex().id(row.key).h(rowBox(ROW_H)).w_full().items_center().bg(bg).hover((s) => s.bg(selectedDim ? t.selected : t.hover)).cursor_pointer()
    .child(checkbox(t, marked))
    .when(child, (e) => e.child(div().w(CHILD_INDENT).flex_none()))
    .child(div().w(22).mr(8).flex_none().flex().items_center().child(engineIcon(t, it.engine, 16)))
    .child(h_flex().flex_1().min_w(0).gap(8).items_center()
      .when(running, (e) => e.child(div().w(7).h(7).rounded(4).flex_none().bg(t.working)))
      .child(div().min_w(0).truncate().text_size(13).text_color(child ? t.muted : t.foreground).when(selectedDim, (e) => e.font_semibold()).child(it.name))
      .when(!it.cwd_exists, (e) => e.child(div().flex_none().text_size(11).text_color(t.blocked).child("目录缺失")))
      .when(Boolean(failure), (e) => e.child(div().min_w(0).truncate().text_size(11).text_color(t.ansi1).child(`删除失败：${failure}`)))
      .when(child && Boolean(relation), (e) => e.child(pill(t, relation, t.muted, false))))
    .when(showProject, (e) => e.child(div().w(PROJECT_W).flex_none().pr(8).truncate().text_size(12).text_color(t.dim).child(projectName)))
    .child(div().w(TIME_W).flex_none().flex().justify_end().text_size(12).text_color(t.dim).child(agoShort(it.modified_ms)))
    .child(div().w(FOLD_W).flex_none().flex().justify_center().text_size(12).text_color(row.expanded ? t.accent : t.muted)
      .child(row.childCount > 0 ? `⑂ ${row.childCount}` : ""))
    .child(div().w(RESUME_W).h_full().flex_none().flex().items_center().justify_start().pl(4)
      .child(div().w(30).h(26).flex().items_center().justify_center().rounded(5).text_size(12).text_color(resumeColor)
        .hover((s) => s.bg(t.hover)).child(running ? "↗" : "▶")));
}

/**
 * The frame both lists share: measures itself, records presses for the hot-zone
 * logic and carries the popup menu. `body` is built by the caller.
 */
export function tableFrame(view, t, { header, list, listId, count, menu, footer }) {
  return v_flex().size_full().min_h(0).relative().bg(t.popover)
    .on_size_change((size) => { view.tableW = declared(size.width); view.tableH = declared(size.height); })
    .on_mouse_down("left", (e) => { view.press = { x: declared(e.local_position?.x ?? 99999), y: declared(e.local_position?.y ?? 0), clicks: e.click_count, shift: e.modifiers.shift }; })
    .on_mouse_down("right", (e) => { view.press = { x: declared(e.local_position?.x ?? 0), y: declared(e.local_position?.y ?? 0), clicks: 1, shift: false }; })
    .child(header)
    .child(div().relative().flex_1().min_h(0)
      .on_size_change((size) => { view.viewportH = declared(size.height); })
      .child(count ? list : div().p(20).text_size(12).text_color(t.dim).child(view.emptyText()))
      .child(Scrollbar.vertical(listId).viewport_from_layout().absolute().inset_0()))
    .when(Boolean(footer), (e) => e.child(footer))
    .when(Boolean(menu), (e) => e.child(popupMenu(t, { ...menu, width: view.tableW, height: view.tableH, onClose: (cx) => { view.menu = null; cx.notify(); } })));
}

function batchBar(view, t, button) {
  return h_flex().flex_none().h(BATCH_H).px(14).gap(10).items_center().border_t(1).border_color(t.border).bg(t.chrome)
    .child(div().text_size(12).font_semibold().text_color(t.foreground).child(`已选 ${view.marks.size} 个`))
    .child(button("bar-clear", "取消选择", () => view.clearMarks()))
    .child(div().flex_1())
    .child(button("bar-delete", "删除", (cx) => view.openDelete(view.markedItems(), cx), false, "danger"));
}

/** The title-mode table. */
export function sessionTable(view, t, button) {
  const rows = view.rows;
  const showProject = view.project === "all";
  const list = rows.length
    ? v_virtual_list("sessions-list", rows.length, rows.map((r) => listPx(rowHeight(r))), (i) => rows[i].key,
      (range) => {
        view.vis = { start: range.start, end: range.end };
        return rows.slice(range.start, range.end).map((r) => (r.type === "section" ? sectionRow(r, t) : sessionRow(view, r, t, showProject)));
      })
      .track_scroll(view.scroll)
      .on_item_click((key, cx) => view.clickRow(key, cx))
      .on_item_secondary_click((key, _e, cx) => view.rightClickRow(key, cx))
      .size_full()
    : null;
  return tableFrame(view, t, {
    header: headerRow(t, showProject),
    list,
    listId: "sessions-list",
    count: rows.length,
    menu: view.menu?.kind === "row" ? view.menu : null,
    footer: view.marks.size > 0 ? batchBar(view, t, button) : null,
  });
}

