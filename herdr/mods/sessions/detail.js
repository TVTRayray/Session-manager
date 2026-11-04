// The right-hand pane: one session's conversation. Messages are paged from the
// newest block backwards; thinking and tool blocks fold to a single line.

import { div, list } from "gpui-kit";
import { v_flex, h_flex, Scrollbar, TextView, VirtualListScrollHandle } from "gpui-base";
import * as api from "./api.js";
import { ago } from "./model.js";

const FOLDED = new Set(["thinking", "tool_call", "tool_output", "system"]);
const KIND_LABEL = { user: "你", assistant: "助手", thinking: "思考", tool_call: "工具调用", tool_output: "工具输出", system: "系统上下文", corrupted: "损坏行" };
const PAGE = 50;

const firstLine = (text, max = 110) => {
  const line = (text ?? "").split("\n").find((l) => l.trim()) ?? "";
  const chars = [...line.trim()];
  return chars.length > max ? `${chars.slice(0, max).join("")}…` : chars.join("");
};

export class DetailPane {
  constructor(onChange, actions = {}) {
    this.onChange = onChange; // (cx) => void : ask the host view to redraw
    this.actions = actions; // { resume(cx), remove(item, cx) }
    this.item = null;
    this.scroll = VirtualListScrollHandle.new();
    this.reset();
  }

  reset() {
    this.meta = null;
    this.blocks = [];
    this.total = 0;
    this.start = 0;
    this.expanded = new Set();
    this.full = new Map();
    this.focus = null;
    this.loading = false;
    this.error = null;
    this.note = null;
    this.stickBottom = false;
  }

  release() {
    this.scroll.release();
  }

  /** Switches to `item`; a stale answer for a previous item is dropped. */
  show(item, cx) {
    const same = this.item && item && this.item.path === item.path && this.item.modified_ms === item.modified_ms;
    if (same) return;
    this.item = item;
    this.token = (this.token ?? 0) + 1;
    this.reset();
    if (!item) return;
    this.loading = true;
    const token = this.token;
    cx.spawn(async (cx) => {
      try {
        const page = await api.detail(item, { limit: PAGE });
        if (token !== this.token) return;
        this.meta = page.meta;
        this.blocks = page.blocks;
        this.total = page.total;
        this.start = page.start;
        this.stickBottom = true;
      } catch (e) {
        if (token === this.token) this.error = e.message;
      }
      if (token === this.token) { this.loading = false; cx.notify(); }
    });
  }

  earlier(cx) {
    if (this.loading || this.start <= 0 || !this.item) return;
    const token = this.token, item = this.item, before = this.start;
    this.loading = true;
    cx.spawn(async (cx) => {
      try {
        const page = await api.detail(item, { before, limit: PAGE });
        if (token !== this.token) return;
        this.blocks = page.blocks.concat(this.blocks);
        this.start = page.start;
      } catch (e) {
        if (token === this.token) this.note = e.message;
      }
      if (token === this.token) { this.loading = false; cx.notify(); }
    });
  }

  blockAt(i) {
    return this.blocks.find((b) => b.i === i);
  }

  textOf(b) {
    return this.full.get(b.i) ?? b.text;
  }

  canShowAll() {
    const b = this.focus != null ? this.blockAt(this.focus) : null;
    return Boolean(b && b.truncated && !this.full.has(b.i));
  }

  showAll(cx) {
    const b = this.focus != null ? this.blockAt(this.focus) : null;
    if (!b || !this.canShowAll()) return;
    const token = this.token, item = this.item;
    cx.spawn(async (cx) => {
      try {
        const full = await api.block(item, b.i);
        if (token !== this.token) return;
        this.full.set(b.i, full.text);
      } catch (e) {
        this.note = e.message;
      }
      cx.notify();
    });
  }

  copyMessage(cx) {
    const b = this.focus != null ? this.blockAt(this.focus) : null;
    if (!b) { this.note = "先点一条消息再复制"; return; }
    const finish = (text) => { cx.write_to_clipboard(text); this.note = "已复制"; };
    if (b.truncated && !this.full.has(b.i)) {
      const token = this.token, item = this.item;
      cx.spawn(async (cx) => {
        try {
          const full = await api.block(item, b.i);
          if (token !== this.token) return;
          this.full.set(b.i, full.text);
          finish(full.text);
        } catch (e) { this.note = e.message; }
        cx.notify();
      });
    } else {
      finish(this.textOf(b));
    }
  }

  /** Called by the host with the key reported by the list. */
  click(key, cx) {
    if (key === "earlier") return this.earlier(cx);
    const i = Number(key.slice(1));
    const b = this.blockAt(i);
    if (!b) return;
    this.focus = i;
    this.note = null;
    if (FOLDED.has(b.kind)) {
      if (this.expanded.has(i)) this.expanded.delete(i); else this.expanded.add(i);
    }
  }

  // ---- drawing ----

  render(t, cx, { button }) {
    const item = this.item;
    if (!item) return div().flex_1().min_w(0).flex().items_center().justify_center().text_color(t.dim).child("选择左侧的会话查看内容");
    const m = this.meta ?? {};
    const rows = (this.start > 0 ? 1 : 0) + this.blocks.length;
    const offset = this.start > 0 ? 1 : 0;
    const keyOf = (index) => (offset && index === 0 ? "earlier" : `b${this.blocks[index - offset].i}`);

    if (this.stickBottom && this.blocks.length) {
      this.stickBottom = false;
      this.scroll.scroll_to_bottom();
    }

    const header = v_flex().flex_none().px(16).pt(14).pb(10).gap(6).border_b(1).border_color(t.border)
      .child(h_flex().gap(10).items_center()
        .child(div().flex_1().min_w(0).truncate().text_size(15).font_semibold().text_color(t.foreground).child(item.name))
        .child(button("d-resume", this.actions.running?.(item) ? "跳转到终端" : "恢复会话", (cx) => this.actions.resume?.(cx), false, "solid")))
      .child(h_flex().gap(10).text_size(11).text_color(t.dim).flex_wrap()
        .child(div().child(`${item.engine} · ${m.id || item.id}`))
        .child(div().child(item.time ? `${item.time}（${ago(item.modified_ms)}）` : ""))
        .child(div().min_w(0).truncate().child(m.cwd || item.cwd || "")))
      .child(h_flex().flex_wrap().gap(8).pt(2).items_center()
        .child(button("d-copy", "复制消息", () => this.copyMessage(cx)))
        .child(button("d-all", "显示全部", () => this.showAll(cx), !this.canShowAll()))
        .child(button("d-id", "复制会话 ID", () => { cx.write_to_clipboard(item.id); this.note = "已复制会话 ID"; cx.notify(); }))
        .child(button("d-del", "删除…", (cx) => this.actions.remove?.(item, cx), false, "danger"))
        .child(div().flex_1())
        .child(div().text_size(11).text_color(this.error ? t.ansi1 : t.dim).child(this.error ?? this.note ?? (this.loading ? "加载中…" : `${this.blocks.length}/${this.total} 条`))));

    const body = !this.blocks.length && !this.loading
      ? div().flex_1().flex().items_center().justify_center().text_color(t.dim).child(this.error ? "" : "没有可显示的消息")
      : div().flex_1().min_h(0).relative()
        .child(list("detail-list", rows, keyOf, (index) => {
          if (offset && index === 0) return this.earlierRow(t);
          return this.blockRow(this.blocks[index - offset], t);
        }).track_scroll(this.scroll).on_item_click((key, cx) => { this.click(key, cx); this.onChange(cx); }).size_full())
        .child(Scrollbar.vertical("detail-list").viewport_from_layout().absolute().inset_0());

    return v_flex().flex_1().min_w(0).h_full().child(header).child(body);
  }

  earlierRow(t) {
    return div().w_full().px(16).py(8).child(
      div().px(12).py(6).rounded(6).border_1().border_color(t.border).text_size(12).text_color(t.accent).cursor_pointer().hover((s) => s.bg(t.hover))
        .child(this.loading ? "加载中…" : `加载更早的消息（还有 ${this.start} 条）`),
    );
  }

  blockRow(b, t) {
    const focused = this.focus === b.i;
    const frame = (inner) => div().w_full().px(12).py(4).child(inner);
    if (b.kind === "corrupted") return frame(div().text_size(11).text_color(t.dim).child(b.text));

    if (FOLDED.has(b.kind)) {
      const open = this.expanded.has(b.i);
      const head = h_flex().gap(8).items_center().text_size(12).text_color(t.muted)
        .child(div().w(12).text_color(t.dim).child(open ? "▾" : "▸"))
        .child(div().text_color(t.dim).child(KIND_LABEL[b.kind]))
        .when(!open, (e) => e.child(div().flex_1().min_w(0).truncate().text_color(t.dim).child(firstLine(this.textOf(b)))));
      return frame(
        v_flex().gap(6).px(10).py(6).rounded(6).border_1().border_color(focused ? t.accent : t.border).bg(open ? t.raised : t.popover)
          .child(head)
          .when(open, (e) => e.child(this.pre(this.textOf(b), b, t))),
      );
    }

    const isUser = b.kind === "user";
    return frame(
      v_flex().gap(4).px(12).py(8).rounded(8).border_1().border_color(focused ? t.accent : isUser ? t.border : t.popover).bg(isUser ? t.raised : t.popover)
        .child(div().text_size(11).text_color(isUser ? t.accent : t.dim).child(KIND_LABEL[b.kind]))
        .child(TextView.markdown(`m${b.i}`, this.textOf(b)).text_size(13).text_color(t.foreground))
        .when(b.truncated && !this.full.has(b.i), (e) => e.child(div().text_size(11).text_color(t.dim).child("内容已截断，选中后点上方「显示全部」"))),
    );
  }

  pre(text, b, t) {
    return div().w_full().text_size(11.5).text_color(t.muted).whitespace_normal().child(text)
      .when(b.truncated && !this.full.has(b.i), (e) => e.child(div().pt(4).text_color(t.dim).child("内容已截断，选中后点上方「显示全部」")));
  }
}
