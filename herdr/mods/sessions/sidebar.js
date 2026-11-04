// The left column: all projects, the most recent three, then the rest grouped
// by parent directory. In full-text mode it lists the projects of the hits.
//
// Not a virtual list (about 60 nodes), so nodes carry their own handlers.

import { div } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { ENGINES } from "./model.js";
import { engineColor, popupMenu, declared } from "./look.js";

export const NODE_H = 42;
const BAR_W = 150;

function folder(t, color) {
  const c = color ?? t.dim;
  return div().relative().w(14).h(14).flex_none()
    .child(div().absolute().left(0).top(1).w(6).h(3).rounded_t(2).bg(c))
    .child(div().absolute().left(0).top(3).w(14).h(9).rounded(2).bg(c).opacity(0.85));
}

function engineBar(t, p) {
  const total = ENGINES.reduce((n, e) => n + (p.engines?.[e] ?? 0), 0);
  if (!total) return div().h(3);
  return h_flex().h(3).w(BAR_W).rounded(2).overflow_hidden().bg(t.raised)
    .children(ENGINES.filter((e) => p.engines[e]).map((e) => div().id(`bar-${p.key}-${e}`).h(3).w(Math.max(2, Math.round((p.engines[e] / total) * BAR_W))).bg(engineColor(t, e))));
}

function node(view, t, p, { selected, showBar, idPrefix }) {
  const focused = selected && view.zone === "sidebar";
  return v_flex().id(`${idPrefix}-${p.key}`).h(NODE_H).mx(8).px(8).rounded(6).justify_center().gap(4).flex_none().cursor_pointer()
    .bg(selected ? t.selected : "#00000000").border_1().border_color(focused ? t.accent : "#00000000")
    .hover((s) => s.bg(selected ? t.selected : t.hover))
    .on_click((_e, cx) => { view.selectProject(p.key, cx); cx.notify(); })
    .on_mouse_down("right", () => { view.projectPress = p.key; })
    .child(h_flex().gap(8).items_center()
      .child(folder(t, p.key === "all" ? t.accent : t.dim))
      .child(div().flex_1().min_w(0).truncate().text_size(13).text_color(selected ? t.foreground : t.muted).when(selected, (e) => e.font_semibold()).child(p.label))
      .when(p.missing, (e) => e.child(div().flex_none().text_size(11).text_color(t.blocked).child("目录缺失")))
      .child(div().flex_none().text_size(11).text_color(t.dim).child(String(p.count))))
    .when(showBar, (e) => e.child(div().ml(21).child(engineBar(t, p))));
}

const heading = (t, label, id) => div().id(id).px(16).pt(12).pb(4).text_size(11).text_color(t.dim).child(label);

/** The nodes in the order the keyboard walks them. */
export function navKeys(view) {
  if (view.scope === "text") return ["all", ...view.hitProjectList().map((p) => p.key)];
  const keys = ["all"];
  for (const s of view.tree.sections) for (const p of s.projects) if (p.count > 0 || p.key === view.project) keys.push(p.key);
  return keys;
}

export function sidebar(view, t) {
  let body;
  if (view.scope === "text") {
    const projects = view.hitProjectList();
    body = [
      node(view, t, { key: "all", label: "全部命中", count: view.hitsForEngine().length }, { selected: view.hitProject === "all", showBar: false, idPrefix: "hp" }),
      heading(t, "命中的项目", "hp-title"),
      ...projects.map((p) => node(view, t, p, { selected: view.hitProject === p.key, showBar: false, idPrefix: "hp" })),
    ];
  } else {
    const tree = view.tree;
    const shown = (p) => p.count > 0 || p.key === view.project;
    body = [
      node(view, t, { key: "all", label: "全部项目", count: tree.total }, { selected: view.project === "all", showBar: false, idPrefix: "pj" }),
      ...(tree.recent.length ? [heading(t, "最近活跃", "pj-recent")] : []),
      ...tree.recent.map((p) => node(view, t, p, { selected: view.project === p.key, showBar: true, idPrefix: "recent" })),
      ...tree.sections.flatMap((s) => {
        const list = s.projects.filter(shown);
        return list.length
          ? [heading(t, s.title, `pj-sec-${s.title}`), ...list.map((p) => node(view, t, p, { selected: view.project === p.key, showBar: true, idPrefix: "pj" }))]
          : [];
      }),
    ];
  }
  return v_flex().size_full().min_h(0).relative().bg(t.chrome)
    .on_size_change((size) => { view.sideW = declared(size.width); view.sideH = declared(size.height); })
    .on_mouse_down("right", (e, cx) => {
      const key = view.projectPress;
      view.projectPress = null;
      if (key && key !== "all" && view.scope === "title") view.openProjectMenu(key, declared(e.local_position?.x ?? 0), declared(e.local_position?.y ?? 0), cx);
    })
    .child(v_flex().id("sidebar-scroll").flex_1().min_h(0).overflow_y_scrollbar().pt(8).pb(12).children(body))
    .when(view.menu?.kind === "project", (e) => e.child(popupMenu(t, { ...view.menu, width: view.sideW, height: view.sideH, onClose: (cx) => { view.menu = null; cx.notify(); } })));
}
