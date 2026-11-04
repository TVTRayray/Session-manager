// Launcher: opens the Sessions panel in full-text mode (`mod.open` cannot pass
// props, so this tiny mod does it). Bound to ctrl-shift-f in keys.json.

import { View, div } from "gpui-kit";
import { props, openOverlay, closeOverlay } from "herdr";

export default class SessionSearch extends View {
  init() {
    const overlay = props()?.overlay;
    if (!overlay) return; // not opened as a dialog (e.g. `herdr-app check`): nothing to replace
    openOverlay({ mod: "sessions", kind: "dialog", width: 0.92, height: 0.9, props: { scope: "text" } });
    closeOverlay(overlay);
  }

  render() {
    return div();
  }
}
