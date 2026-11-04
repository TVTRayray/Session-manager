// Bridge to the Rust backend (`sessions-manager api ...`) and to Herdr's tab

import { exec, hc, home, run, terminals } from "herdr";

const LABEL = "sm.session";

const binary = () => `${home()}/.local/bin/sessions-manager`;

async function call(args) {
  let result;
  try {
    result = await exec(binary(), ["api", ...args]);
  } catch (e) {
    throw new Error(`无法启动 sessions-manager（运行 scripts/install-herdr.sh 安装）：${e?.message ?? e}`);
  }
  let data;
  try {
    data = JSON.parse(result.stdout);
  } catch {
    throw new Error(result.stderr.trim().split("\n").pop() || `sessions-manager 退出码 ${result.code}`);
  }
  if (data?.error) throw new Error(data.error);
  return data;
}

export const list = (cached = false) => call(["list", ...(cached ? ["--cached"] : [])]);

export const detail = (item, { before, limit = 50 } = {}) =>
  call(["detail", "--engine", item.engine, "--path", item.path, "--limit", String(limit), ...(before == null ? [] : ["--before", String(before)])]);

export const block = (item, index) => call(["detail", "--engine", item.engine, "--path", item.path, "--block", String(index)]);

export const search = (query, { limit = 50, includeTools = false } = {}) =>
  call(["search", "--query", query, "--limit", String(limit), ...(includeTools ? ["--include-tools"] : [])]);

export const remove = (targets) => call(["delete", "--targets", JSON.stringify(targets)]);

export const sessionKey = (item) => `${item.engine}:${item.id}`;

/** The live terminal that was started for this session by us, if any. */
export function runningTerminal(item) {
  const key = `${item.engine}:${item.resume?.id ?? item.id}`;
  try {
    return terminals().find((t) => t?.labels?.[LABEL] === key && isRunning(t)) ?? null;
  } catch {
    return null;
  }
}

// Agents run inside the user's shell, which outlives them: a terminal whose
// foreground program is back to a shell no longer runs the session.
const SHELLS = new Set(["sh", "bash", "zsh", "fish", "dash", "nu", "pwsh"]);
function isRunning(t) {
  if (t.state !== "live") return false;
  const name = t.foreground?.name;
  return !name || !SHELLS.has(name);
}

/** Keys of every session that currently has a live tagged terminal. */
export function runningKeys() {
  const out = new Set();
  try {
    for (const t of terminals()) {
      const v = t?.labels?.[LABEL];
      if (v && isRunning(t)) out.add(v);
    }
  } catch {}
  return out;
}

const shQuote = (s) => `'${String(s).replace(/'/g, `'\\''`)}'`;

const addressOf = (t) => t.address ?? t.id;

async function doCommand(name, args) {
  const r = await exec("herdr-app", ["do", name, ...Object.entries(args).flatMap(([k, v]) => [`--${k}`, typeof v === "string" ? v : JSON.stringify(v)])]);
  let data = null;
  try { data = JSON.parse(r.stdout); } catch {}
  if (r.code !== 0 || data?.error) throw new Error(data?.error || r.stderr.trim() || `herdr-app do ${name} 失败`);
  return data;
}

/**
 * Opens the session in a new tab (or jumps to the tab already running it).
 * Throws an Error whose message is meant for the user.
 */
export async function resume(item, tabName) {
  const r = item.resume;
  if (!r || !r.argv?.length) throw new Error("这个会话没有可用的恢复命令");
  if (!r.folder) throw new Error("会话缺少工作目录信息，无法恢复");
  if (!r.folder_exists) throw new Error(`工作目录不存在：${r.folder}`);

  const running = runningTerminal(item);
  if (running) {
    run("pane.focus", { terminal: addressOf(running) });
    return { jumped: true };
  }
  // A plain shell in the folder, then `cd` + the agent typed into it: the shell
  // reports its directory (so path-aware mods follow it) and is still sitting
  // in that directory after the agent exits.
  const made = await doCommand("tab.new", { folder: r.folder, name: tabName });
  const terminal = made?.terminal;
  if (!terminal) throw new Error("新 tab 没有返回终端地址");
  try { await hc(["label", terminal, `${LABEL}=${item.engine}:${r.id ?? item.id}`]); } catch {}
  const command = `cd -- ${shQuote(r.folder)} && ${r.argv.map(shQuote).join(" ")}`;
  const sent = await hc(["send", terminal, command]);
  if (sent.code !== 0) throw new Error(sent.stderr.trim() || "无法向新终端发送命令");
  return { jumped: false, terminal };
}
