// Shared drawing helpers: colors that follow the theme, engine icons, buttons,
// pills and the small popup menu.

import { div, image, svg } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { settings } from "herdr";

// Herdr's zoom scales what elements declare (`.h(38)`, `.left(10)`), but not what
// the host reports (sizes, mouse positions) nor a virtual list's `item_sizes`.
// This mod reasons in declared pixels; those three convert at the boundary.
export const zoom = () => Number(settings()?.zoom) || 1;
/** A reported length in declared pixels. */
export const declared = (reported) => reported / zoom();
/** The `item_sizes` entry for a row of height `h`: whole pixels, so no hairlines between rows. */
export const listPx = (h) => Math.max(1, Math.round(h * zoom()));
/** The height a row draws at so it fills exactly the `listPx(h)` the list gives it. */
export const rowBox = (h) => listPx(h) / zoom();

/** `color` with an alpha byte ("26"); a color that is not hex becomes transparent rather than opaque. */
export function tint(color, alpha) {
  const c = String(color ?? "");
  let m = /^#([0-9a-f]{6})(?:[0-9a-f]{2})?$/i.exec(c);
  if (m) return `#${m[1]}${alpha}`;
  m = /^#([0-9a-f])([0-9a-f])([0-9a-f])$/i.exec(c);
  if (m) return `#${m[1]}${m[1]}${m[2]}${m[2]}${m[3]}${m[3]}${alpha}`;
  return "#00000000";
}

export const engineColor = (t, engine) => ({ pi: t.ansi5, codex: t.ansi2, claude: t.ansi3 })[engine] ?? t.dim;

/** Claude and Codex ship full-color icons; Pi's is a white glyph, so it takes the engine color. */
export function engineIcon(t, engine, size = 16) {
  const path = `icons/${engine}.svg`;
  if (engine === "pi") return svg(path).size(size).flex_none().text_color(engineColor(t, engine));
  return image(path).size(size).flex_none();
}

export function pill(t, label, color, solid) {
  return div().flex_none().px(6).h(18).flex().items_center().rounded(4).text_size(10.5)
    .bg(solid ? tint(color, "26") : t.raised).border_1().border_color(solid ? tint(color, "40") : t.border)
    .text_color(solid ? color : t.muted).child(label);
}

/** `button(id, label, onClick, disabled, tone)` with tone "plain" | "danger" | "solid". */
export function makeButton(t) {
  return (id, label, onClick, disabled = false, tone = "plain") => {
    const bg = tone === "danger" ? t.ansi1 : tone === "solid" ? t.solid : t.raised;
    const fg = tone === "danger" ? t.accent_foreground : tone === "solid" ? t.solid_foreground : t.foreground;
    return h_flex().id(id).h(28).px(12).items_center().rounded(6).border_1().text_size(12).flex_none()
      .border_color(tone === "plain" ? t.border : bg)
      .bg(disabled ? t.raised : bg)
      .text_color(disabled ? t.dim : fg)
      .when(!disabled, (e) => e.cursor_pointer().hover((s) => s.opacity(0.85)).on_click((_e, cx) => { onClick(cx); cx.notify(); }))
      .child(label);
  };
}

export const MENU_W = 220;
export const MENU_ITEM_H = 30;

/**
 * A popup menu drawn inside a `relative` container of size `bounds`, opened at
 * (x, y) in that container and flipped when it would cross the right or bottom
 * edge. `items`: `{label, hint?, disabled?, danger?, run(cx)}`; a falsy entry is skipped.
 * A backdrop under it closes the menu on any other click.
 */
export function popupMenu(t, { x, y, width, height, items, onClose }) {
  const list = items.filter(Boolean);
  const h = list.length * MENU_ITEM_H + 12;
  const left = Math.max(4, x + MENU_W > width ? x - MENU_W : x);
  const top = Math.max(4, y + h > height ? y - h : y);
  return div().absolute().inset_0()
    .on_mouse_down("left", (_e, cx) => { onClose(cx); cx.stop_propagation(); })
    .on_mouse_down("right", (_e, cx) => { onClose(cx); cx.stop_propagation(); })
    .child(v_flex().absolute().left(left).top(top).w(MENU_W).p(6).rounded(8).border_1().border_color(t.border).bg(t.popover).shadow_md()
      .on_mouse_down("left", (_e, cx) => cx.stop_propagation())
      .on_mouse_down("right", (_e, cx) => cx.stop_propagation())
      .children(list.map((it, i) => h_flex().id(`menu-${i}`).h(MENU_ITEM_H).px(8).gap(8).items_center().rounded(5).text_size(12.5)
        .text_color(it.disabled ? t.dim : it.danger ? t.ansi1 : t.foreground)
        .when(!it.disabled, (e) => e.cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, cx) => { onClose(cx); it.run(cx); cx.notify(); }))
        .child(div().flex_1().min_w(0).truncate().child(it.label))
        .when(Boolean(it.hint), (e) => e.child(div().text_size(11).text_color(t.dim).child(it.hint))))));
}
