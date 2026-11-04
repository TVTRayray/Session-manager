// Full-text mode: debounced search (ripgrep via `api.search`), the hit list that
// replaces the session table, and the snippet pane that replaces the
// conversation. State lives on the view; this file holds the logic and drawing.

import { div } from "gpui-kit";
import { v_flex, h_flex, v_virtual_list } from "gpui-base";
import * as api from "./api.js";
import { highlight, flowTokens, previewRuns, agoShort, ago, RESUME_W } from "./model.js";
import { tint, engineIcon, pill, listPx, rowBox } from "./look.js";
import { tableFrame, HEADER_H } from "./list.js";
import { ENGINE_LABEL } from "./model.js";

// A virtual list places rows by a stated height and never measures them, so the
// height is the sum of the row's parts and every line states its own line height.
const HIT_PAD_Y = 12;
const HIT_GAP = 4;
const TITLE_LH = 20;
const META_LH = 16;
const PREVIEW_LH = 20;
export const HIT_H = HIT_PAD_Y * 2 + TITLE_LH + META_LH + PREVIEW_LH + HIT_GAP * 2;
const HIT_LEFT = 14 + 22 + 8; // row padding + engine icon
const PREVIEW_SIZE = 12.5;
const DEBOUNCE_MS = 150;

export const searchTerms = (view) => view.searched.toLowerCase().split(/\s+/).filter(Boolean);

/** Starts a search now (the debounce already elapsed); only the newest request may publish. */
export function runSearch(view, cx) {
  const query = view.input.value().trim();
  const token = ++view.searchToken;
  if (!query) {
    view.hits = []; view.truncated = false; view.searched = ""; view.searching = false; view.searchError = null;
    return;
  }
  view.searching = true;
  cx.spawn(async (cx) => {
    try {
      const result = await api.search(query, { limit: 50, includeTools: view.includeTools });
      if (token !== view.searchToken) return;
      view.hits = result.hits ?? [];
      view.truncated = Boolean(result.truncated);
      view.searched = query;
      view.searchError = null;
      view.hitSel = 0;
      view.hitProject = "all";
      view.hitScroll.scroll_to_item(0);
    } catch (e) {
      if (token !== view.searchToken) return;
      view.hits = [];
      view.searchError = e.message;
    }
    view.searching = false;
    cx.notify();
  });
}

/** Debounces typing: 150 ms after the last keystroke. */
export function scheduleSearch(view, cx) {
  const token = ++view.searchToken;
  view.status = null;
  if (!view.input.value().trim()) {
    runSearch(view, cx);
    cx.notify();
    return;
  }
  view.searching = true;
  cx.notify();
  cx.timer.after(DEBOUNCE_MS, (cx) => {
    if (token !== view.searchToken) return;
    runSearch(view, cx);
    cx.notify();
  });
}

const hitRun = (t, el) => el.bg(tint(t.accent, "40")).text_color(t.foreground).font_semibold();

/** One line, clipped at the column edge, with the first match kept in view. */
function previewLine(t, text, terms, width) {
  const lead = Math.floor(((width - HIT_LEFT - RESUME_W) / PREVIEW_SIZE) * 0.3);
  return h_flex().flex_none().h(PREVIEW_LH).line_height(`${PREVIEW_LH}px`).overflow_hidden().whitespace_nowrap()
    .text_size(PREVIEW_SIZE).text_color(t.muted)
    .children(previewRuns(highlight(text, terms), lead).map((run) => (run.hit ? hitRun(t, div().flex_none().child(run.text)) : div().flex_none().child(run.text))));
}

/** A whole snippet: flex items cannot flow as inline text, so it wraps word by word. */
function snippetBlock(t, text, terms) {
  return h_flex().w_full().flex_wrap().line_height("20px").text_size(13).text_color(t.muted)
    .children(flowTokens(highlight(text, terms)).map((run) => (run.hit ? hitRun(t, div().max_w_full().child(run.text)) : div().max_w_full().child(run.text))));
}

function hitRow(view, hit, index, t, terms) {
  const on = view.hitSel === index;
  const project = String(hit.cwd ?? "").split("/").filter(Boolean).pop() ?? "";
  return h_flex().id(hit.path).h(rowBox(HIT_H)).w_full().items_center().pl(14).overflow_hidden()
    .bg(on ? t.selected : index % 2 === 0 ? t.popover : tint(t.foreground, "0a")).hover((s) => s.bg(on ? t.selected : t.hover)).cursor_pointer()
    .child(div().w(22).mr(8).flex_none().flex().items_center().child(engineIcon(t, hit.engine, 16)))
    .child(v_flex().flex_1().min_w(0).gap(HIT_GAP)
      .child(div().flex_none().h(TITLE_LH).line_height(`${TITLE_LH}px`).truncate().text_size(13.5).text_color(t.foreground).when(on, (e) => e.font_semibold()).child(hit.name || hit.id || hit.path))
      .child(h_flex().flex_none().h(META_LH).line_height(`${META_LH}px`).gap(8).text_size(11).text_color(t.dim).overflow_hidden()
        .child(div().flex_none().child(ENGINE_LABEL[hit.engine] ?? hit.engine))
        .child(div().min_w(0).truncate().child(project))
        .child(div().flex_none().child(agoShort(hit.modified_ms))))
      .child(previewLine(t, hit.snippets?.[0] ?? "", terms, view.tableW)))
    .child(div().w(RESUME_W).h_full().flex_none().flex().items_center().pl(4)
      .child(div().w(30).h(26).flex().items_center().justify_center().rounded(5).text_size(12).text_color(t.muted).hover((s) => s.bg(t.hover)).child("▶")));
}

/** The full-text hit list, in the middle column. */
export function hitTable(view, t) {
  const hits = view.hitsShown();
  const terms = searchTerms(view);
  const list = hits.length
    ? v_virtual_list("hits-list", hits.length, listPx(HIT_H), (i) => hits[i].path,
      (range) => {
        view.hitVis = { start: range.start, end: range.end };
        return hits.slice(range.start, range.end).map((h, k) => hitRow(view, h, range.start + k, t, terms));
      })
      .track_scroll(view.hitScroll)
      .on_item_click((key, cx) => view.clickHit(key, cx))
      .size_full()
    : null;
  const header = h_flex().flex_none().h(HEADER_H).px(14).items_center().border_b(1).border_color(t.border).text_size(11).text_color(t.dim)
    .child(div().flex_1().child("命中会话")).child(div().child("更新"));
  const footer = view.truncated
    ? div().flex_none().h(30).px(14).flex().items_center().border_t(1).border_color(t.border).text_size(12).text_color(t.blocked).child("结果已截断，请缩小关键词")
    : null;
  return tableFrame(view, t, { header, list, listId: "hits-list", count: hits.length, menu: null, footer });
}

/** The right column in full-text mode: every snippet of the selected hit. */
export function hitDetail(view, t, button) {
  const hit = view.currentHit();
  if (!hit) return div().flex_1().min_w(0).flex().items_center().justify_center().text_color(t.dim).text_size(12).child("选择左侧的命中查看片段");
  const terms = searchTerms(view);
  return v_flex().flex_1().min_w(0).h_full()
    .child(v_flex().flex_none().px(16).pt(14).pb(10).gap(6).border_b(1).border_color(t.border)
      .child(h_flex().gap(10).items_center()
        .child(div().flex_1().min_w(0).truncate().text_size(15).font_semibold().text_color(t.foreground).child(hit.name || hit.id))
        .child(button("hit-locate", "在列表中定位", (cx) => view.locateHit(cx)))
        .child(button("hit-resume", "恢复会话", (cx) => view.resumeHit(cx), false, "solid")))
      .child(h_flex().gap(8).items_center().text_size(11).text_color(t.dim)
        .child(pill(t, ENGINE_LABEL[hit.engine] ?? hit.engine, t.muted, false))
        .child(div().min_w(0).truncate().child(hit.cwd ?? ""))
        .child(div().flex_none().child(ago(hit.modified_ms)))))
    .child(v_flex().id("snippets").flex_1().min_h(0).overflow_y_scrollbar().p(16).gap(10)
      .child(div().text_size(11).text_color(t.dim).child(`${hit.snippets?.length ?? 0} 个命中片段`))
      .children((hit.snippets ?? []).map((s, i) => v_flex().id(`snip-${i}`).p(12).rounded(8).border_1().border_color(t.border).bg(t.raised)
        .child(snippetBlock(t, s, terms)))));
}
