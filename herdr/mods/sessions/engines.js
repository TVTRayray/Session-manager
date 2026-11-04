// The engine priority popover: display order, visibility, in-project sort and
// the engine selected on open. Changes apply at once; nothing to save.

import { div } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { ENGINE_LABEL, moveEngine, toggleEngine, visibleEngines } from "./model.js";
import { engineIcon } from "./look.js";

const SORT_LABEL = { time: "按时间", engine: "引擎优先", section: "按引擎分段" };

export function enginesPopover(view, t) {
  const prefs = view.prefs;
  const shown = visibleEngines(prefs);
  const small = (id, label, disabled, onClick) => div().id(id).w(26).h(26).flex().items_center().justify_center().rounded(5).text_size(13)
    .text_color(disabled ? t.dim : t.foreground).when(!disabled, (e) => e.cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, cx) => { onClick(cx); cx.notify(); })).child(label);

  const toggle = (id, on, disabled, onClick) => div().id(id).w(32).h(18).rounded(9).flex().items_center().flex_none()
    .bg(on ? t.accent : t.raised).border_1().border_color(t.border).opacity(disabled ? 0.45 : 1)
    .when(!disabled, (e) => e.cursor_pointer().on_click((_e, cx) => { onClick(cx); cx.notify(); }))
    .child(div().w(12).h(12).rounded(6).bg(on ? t.accent_foreground : t.dim).ml(on ? 17 : 2));

  const seg = (id, label, on, onClick) => div().id(id).h(26).px(10).flex().items_center().rounded(5).text_size(12).cursor_pointer()
    .bg(on ? t.selected : "#00000000").text_color(on ? t.foreground : t.muted).hover((s) => s.bg(on ? t.selected : t.hover))
    .on_click((_e, cx) => { onClick(cx); cx.notify(); }).child(label);

  const rows = prefs.order.map((e, i) => {
    const visible = !prefs.hidden.includes(e);
    return h_flex().id(`eng-${e}`).h(40).px(8).gap(8).items_center().rounded(6)
      .child(engineIcon(t, e, 18))
      .child(div().flex_1().text_size(13).text_color(visible ? t.foreground : t.dim).child(ENGINE_LABEL[e]))
      .child(div().text_size(11).text_color(t.dim).child(String(view.totalByEngine[e] ?? 0)))
      .child(small(`up-${e}`, "↑", i === 0, (cx) => view.setPrefs(moveEngine(prefs, e, -1), cx)))
      .child(small(`down-${e}`, "↓", i === prefs.order.length - 1, (cx) => view.setPrefs(moveEngine(prefs, e, 1), cx)))
      .child(toggle(`vis-${e}`, visible, visible && shown.length === 1, (cx) => view.setPrefs(toggleEngine(prefs, e), cx)));
  });

  return div().absolute().inset_0()
    .on_mouse_down("left", (_e, cx) => { view.enginesOpen = false; cx.notify(); cx.stop_propagation(); })
    .child(v_flex().absolute().top(62).right(14).w(340).p(12).gap(10).rounded(10).border_1().border_color(t.border).bg(t.popover).shadow_md()
      .on_mouse_down("left", (_e, cx) => cx.stop_propagation())
      .child(h_flex().items_center()
        .child(div().flex_1().text_size(14).font_semibold().text_color(t.foreground).child("引擎优先级"))
        .child(div().text_size(11).text_color(t.dim).child("Esc")))
      .child(v_flex().children(rows))
      .child(v_flex().gap(4)
        .child(div().text_size(11).text_color(t.dim).child("项目内排序"))
        .child(h_flex().gap(2).p(2).rounded(7).bg(t.raised)
          .children(Object.keys(SORT_LABEL).map((s) => seg(`sort-${s}`, SORT_LABEL[s], prefs.sort === s, (cx) => view.setPrefs({ ...prefs, sort: s }, cx)))))
        .child(div().text_size(11).text_color(t.dim).child("「引擎优先」和「按引擎分段」只在选中具体项目时生效；全部项目下仍按时间排序。")))
      .child(v_flex().gap(4)
        .child(div().text_size(11).text_color(t.dim).child("打开时选中"))
        .child(h_flex().gap(2).p(2).rounded(7).bg(t.raised).flex_wrap()
          .child(seg("def-all", "全部", prefs.default === "all", (cx) => view.setPrefs({ ...prefs, default: "all" }, cx)))
          .children(shown.map((e) => seg(`def-${e}`, ENGINE_LABEL[e], prefs.default === e, (cx) => view.setPrefs({ ...prefs, default: e }, cx))))))
      .child(div().text_size(11).text_color(t.dim).child("只影响这个面板，不改变 TUI 的引擎顺序。")));
}
