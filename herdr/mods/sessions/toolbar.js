// The top bar: title, search box with its title/full-text switch, engine
// filter, priority button, refresh status and close button.

import { div } from "gpui-kit";
import { v_flex, h_flex, Input } from "gpui-base";
import { ENGINE_LABEL, visibleEngines } from "./model.js";
import { engineIcon } from "./look.js";

export const TOOLBAR_H = 56;
const SEARCH_INFO_H = 28;

export function toolbar(view, t) {
  const text = view.scope === "text";

  const seg = (id, label, on, onClick) => h_flex().id(id).h(24).px(10).items_center().rounded(5).text_size(12).cursor_pointer()
    .bg(on ? t.selected : "#00000000").text_color(on ? t.foreground : t.muted).hover((s) => s.bg(on ? t.selected : t.hover))
    .on_click((_e, cx) => { onClick(cx); cx.notify(); }).child(label);

  const tab = (id, label, count, icon) => {
    const on = view.engine === id;
    return h_flex().id(`tab-${id}`).h(28).px(10).gap(6).items_center().rounded(14).flex_none().cursor_pointer().text_size(12)
      .bg(on ? t.selected : "#00000000").text_color(on ? t.foreground : t.muted).hover((s) => s.bg(on ? t.selected : t.hover))
      .on_click((_e, cx) => { view.setEngine(id, cx); cx.notify(); })
      .when(Boolean(icon), (e) => e.child(icon))
      .child(label).child(div().text_size(11).text_color(t.dim).child(String(count ?? 0)));
  };

  const hiddenCount = view.prefs.hidden.length;
  const counts = text ? view.hitCounts() : view.engineCounts;
  const status = view.status
    ? div().min_w(0).max_w(280).truncate().text_size(12).text_color(view.status.level === "error" ? t.ansi1 : t.muted).child(view.status.text)
    : div().flex_none().text_size(11).text_color(t.dim).child(view.loading || view.searching ? (text ? "搜索中…" : "刷新中…") : view.warnings.length ? `${view.warnings.length} 条警告` : "");

  const searchInfo = () => [
    div().flex_none().text_size(12).text_color(t.muted).child(view.searched ? `${view.hitsShown().length}${view.truncated ? "+" : ""} 个会话命中` : ""),
    h_flex().id("with-tools").flex_none().gap(6).items_center().cursor_pointer().text_size(12).text_color(t.muted)
      .on_click((_e, cx) => { view.includeTools = !view.includeTools; view.runSearch(cx); cx.notify(); })
      .child(div().w(26).h(14).rounded(7).bg(view.includeTools ? t.accent : t.raised).border_1().border_color(t.border).flex().items_center()
        .child(div().w(10).h(10).rounded(5).bg(view.includeTools ? t.accent_foreground : t.dim).ml(view.includeTools ? 13 : 2)))
      .child("包含工具输出"),
  ];

  // The close button sits outside the clipped part, so it can never be pushed off the panel.
  const main = h_flex().flex_none().h(TOOLBAR_H).px(14).gap(10).items_center()
    .child(h_flex().flex_1().min_w(0).gap(10).items_center().overflow_hidden()
    .child(div().flex_none().text_size(15).font_semibold().text_color(t.foreground).pr(4).child("会话"))
    .when(view.narrow(), (e) => e.child(div().id("side-toggle").h(28).px(8).flex().items_center().rounded(6).cursor_pointer().text_size(12)
      .text_color(view.sidebarOpen ? t.foreground : t.muted).bg(view.sidebarOpen ? t.selected : "#00000000").hover((s) => s.bg(t.hover))
      .on_click((_e, cx) => { view.sidebarOpen = !view.sidebarOpen; cx.notify(); }).child("项目")))
    .child(h_flex().flex_1().min_w(160).max_w(560).h(34).pl(6).pr(10).gap(8).items_center().rounded(8).border_1()
      .border_color(view.typing ? t.accent : t.border).bg(t.background).cursor_text()
      .on_mouse_down("left", (_e, cx) => cx.stop_propagation())
      .child(h_flex().flex_none().gap(2).p(2).rounded(6).bg(t.raised)
        .child(seg("scope-title", "标题", !text, (cx) => view.setScope("title", cx)))
        .child(seg("scope-text", "全文", text, (cx) => view.setScope("text", cx))))
      .child(div().relative().flex_1().min_w(0).h_full()
        .child(Input.new(view.input).size_full().text_size(13).text_color(t.foreground))
        .when(!view.input.value(), (e) => e.child(div().absolute().top(0).left(0).h_full().flex().items_center().text_size(13).text_color(t.dim)
          .on_mouse_down("left", () => view.input.focus())
          .child(text ? "搜索所有会话的正文" : "筛选标题或路径")))))
    .child(h_flex().flex_none().gap(2).items_center()
      .child(tab("all", "全部", counts.all))
      .children(visibleEngines(view.prefs).map((e) => tab(e, ENGINE_LABEL[e], counts[e], engineIcon(t, e, 14)))))
    .when(hiddenCount > 0, (e) => e.child(div().id("hidden-engines").flex_none().text_size(11).text_color(t.dim).cursor_pointer().hover((s) => s.text_color(t.foreground))
      .on_click((_e, cx) => { view.enginesOpen = true; cx.notify(); }).child(`已隐藏 ${hiddenCount} 个引擎`)))
    .child(div().id("priority").h(28).px(10).flex().items_center().flex_none().rounded(6).border_1().border_color(t.border).bg(t.raised)
      .text_size(12).text_color(t.foreground).cursor_pointer().hover((s) => s.opacity(0.85))
      .on_click((_e, cx) => { view.enginesOpen = !view.enginesOpen; cx.notify(); }).child("优先级"))
    .when(text && !view.narrow(), (e) => e.children(searchInfo()))
    .child(div().flex_1())
    .child(status))
    .when(view.isOverlay(), (e) => e.child(div().id("close").w(28).h(28).flex_none().flex().items_center().justify_center().rounded(6).cursor_pointer().text_color(t.muted)
      .hover((s) => s.bg(t.hover)).on_click(() => view.dismiss()).child("✕")));

  // Narrow panels have no room for the search info in the bar: it gets its own line.
  return v_flex().flex_none().border_b(1).border_color(t.border)
    .child(main)
    .when(text && view.narrow(), (e) => e.child(h_flex().flex_none().h(SEARCH_INFO_H).px(14).pb(6).gap(14).items_center().children(searchInfo())));
}
