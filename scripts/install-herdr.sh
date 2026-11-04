#!/bin/sh
# Builds the sessions-manager binary, installs it to ~/.local/bin and copies the
# Herdr mods into the app's mods folder.
#
#   scripts/install-herdr.sh              build + install binary + copy mods
#   scripts/install-herdr.sh --mods-only  copy mods only (no cargo build)
#   scripts/install-herdr.sh --bind-keys  also add ctrl-shift-s / ctrl-shift-f (and mod:sessions ctrl-shift-f) to keys.json
#
# HERDR_APP_HOME selects another Herdr home (default ~/.config/herdr-app).
set -eu

repo=$(cd "$(dirname "$0")/.." && pwd)
app_home=${HERDR_APP_HOME:-$HOME/.config/herdr-app}
bin_dir=${SESSIONS_MANAGER_BIN_DIR:-$HOME/.local/bin}

build=1
bind_keys=0
for arg in "$@"; do
  case "$arg" in
    --mods-only) build=0 ;;
    --bind-keys) bind_keys=1 ;;
    -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

if [ "$build" = 1 ]; then
  (cd "$repo" && cargo build --release)
  mkdir -p "$bin_dir"
  # Write to a temp name first so a running copy is replaced atomically.
  cp "$repo/target/release/sessions-manager" "$bin_dir/sessions-manager.new"
  mv "$bin_dir/sessions-manager.new" "$bin_dir/sessions-manager"
  echo "installed $bin_dir/sessions-manager"
fi

mkdir -p "$app_home/mods"
for mod in sessions session-search; do
  src="$repo/herdr/mods/$mod"
  dest="$app_home/mods/$mod"
  mkdir -p "$dest"
  # Drop files that no longer exist in the repo (the app's own .d.ts / jsconfig stay).
  for old in "$dest"/*.js "$dest"/*.mjs; do
    [ -e "$old" ] || continue
    [ -e "$src/$(basename "$old")" ] || rm -f "$old"
  done
  # Only the mod's own sources; tests stay in the repo.
  for file in "$src"/*.js "$src"/*.json; do
    [ -e "$file" ] || continue
    cp "$file" "$dest/"
  done
  echo "copied mod $mod -> $dest"
done

if [ "$bind_keys" = 1 ]; then
  if ! command -v node >/dev/null 2>&1; then
    echo "node not found; add the keys by hand (see herdr/README.md)" >&2
    exit 1
  fi
  KEYS_FILE="$app_home/keys.json" node -e '
    const fs = require("fs");
    const file = process.env.KEYS_FILE;
    const keys = fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, "utf8")) : {};
    keys.global = keys.global || {};
    const want = { "ctrl-shift-s": "mod.open:sessions", "ctrl-shift-f": "mod.open:session-search" };
    for (const [k, v] of Object.entries(want)) {
      if (keys.global[k] && keys.global[k] !== v) { console.error(`keys.json: ${k} already bound to ${keys.global[k]}, left alone`); continue; }
      keys.global[k] = v;
    }
    keys["mod:sessions"] = keys["mod:sessions"] || {};
    if (!keys["mod:sessions"]["ctrl-shift-f"]) keys["mod:sessions"]["ctrl-shift-f"] = "sessions.search";
    fs.writeFileSync(file, JSON.stringify(keys, null, 2) + "\n");
    console.log("updated " + file);
  '
fi

echo "check with: herdr-app check $app_home/mods/sessions"
