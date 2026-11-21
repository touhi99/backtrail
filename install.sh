#!/bin/sh
set -e
cd "$(dirname "$0")"
for tool in cargo node npm gh; do
  command -v "$tool" >/dev/null || { echo "$tool is required but not installed." >&2; exit 1; }
done
command -v rustc >/dev/null && [ "$(rustc --version | awk '{print $2}' | awk -F. '{print $1*1000+$2}')" -lt 1088 ] \
  && { echo "Rust 1.88 or newer is required (found $(rustc --version))." >&2; exit 1; }
[ "$(node --version | tr -d v | cut -d. -f1)" -lt 22 ] \
  && { echo "Node.js 22 or newer is required (found $(node --version))." >&2; exit 1; }
gh auth status >/dev/null 2>&1 || { echo "gh is not logged in: run gh auth login, then ./install.sh again." >&2; exit 1; }
command -v ollama >/dev/null || {
  echo "Ollama is required for embeddings: install it from https://ollama.com, then run ./install.sh again." >&2
  exit 1
}
ollama list 2>/dev/null | grep -q '^embeddinggemma-2:270m' || ollama pull embeddinggemma-2:270m || {
  echo "Could not pull embeddinggemma-2:270m. Start Ollama and run ./install.sh again." >&2
  exit 1
}
(cd app && npm install && npm run build)
cargo build --release
bin_dir="${BT_BIN_DIR:-$HOME/.local/bin}"
mkdir -p "$bin_dir"
for name in backtrail bt; do ln -sf "$PWD/target/release/backtrail" "$bin_dir/$name"; done
echo "Linked $bin_dir/backtrail and $bin_dir/bt"
bt_bin="$PWD/target/release/backtrail"
harnesses="${BT_HARNESSES:-}"
if [ -z "$harnesses" ]; then
  found=""
  command -v claude >/dev/null && found="$found,claude"
  command -v gemini >/dev/null && found="$found,gemini"
  command -v codex >/dev/null && found="$found,codex"
  command -v agy >/dev/null && found="$found,antigravity"
  found="${found#,}"
  harnesses="$found"
  if [ -t 0 ]; then
    printf 'Coding agents to connect (claude, gemini, codex, antigravity; comma-separated, or none) [%s]: ' "${found:-none}"
    read -r answer
    harnesses="${answer:-$found}"
  fi
fi
harnesses=$(printf '%s' "$harnesses" | tr -d ' ')
if [ -n "$harnesses" ] && [ "$harnesses" != none ]; then
  [ "${BT_HOOKS:-1}" = 0 ] || "$bt_bin" hooks install --harness "$harnesses"
  first="${harnesses%%,*}"
  reader="${BT_AGENT:-}"
  if [ -z "$reader" ] && [ -t 0 ] && [ "$first" != "$harnesses" ]; then
    printf 'Which one reads sessions for decisions [%s]: ' "$first"
    read -r reader
  fi
  "$bt_bin" agent "${reader:-$first}"
fi
if [ "$(uname)" = Darwin ] && [ -z "$BT_NO_AGENT" ]; then
  label=dev.backtrail.serve
  data="${BACKTRAIL_HOME:-$HOME/Library/Application Support/Backtrail}"
  plist="$HOME/Library/LaunchAgents/$label.plist"
  env_block=""
  [ -n "$BACKTRAIL_HOME" ] && env_block="    <key>EnvironmentVariables</key>
    <dict><key>BACKTRAIL_HOME</key><string>$BACKTRAIL_HOME</string></dict>"
  mkdir -p "$data/logs" "$HOME/Library/LaunchAgents"
  new_plist=$(cat <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>$label</string>
    <key>ProgramArguments</key>
    <array>
        <string>$PWD/target/release/backtrail</string>
        <string>serve</string>
    </array>
    <key>WorkingDirectory</key>
    <string>$PWD</string>
$env_block
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ThrottleInterval</key>
    <integer>10</integer>
    <key>StandardOutPath</key>
    <string>$data/logs/serve.log</string>
    <key>StandardErrorPath</key>
    <string>$data/logs/serve.log</string>
</dict>
</plist>
PLIST
)
  if [ -f "$plist" ] && [ "$(cat "$plist")" = "$new_plist" ] && launchctl print "gui/$(id -u)/$label" >/dev/null 2>&1; then
    launchctl kickstart -k "gui/$(id -u)/$label" && echo "Restarted $label"
  else
    launchctl bootout "gui/$(id -u)/$label" 2>/dev/null || true
    printf '%s\n' "$new_plist" > "$plist"
    launchctl bootstrap "gui/$(id -u)" "$plist" && echo "Installed login agent $label ($plist)"
  fi
  echo "The server starts at http://127.0.0.1:8799 (port: config.json key \"port\")."
  echo "If macOS asks whether backtrail may access a folder, allow it; the server waits for the answer."
fi
if [ "$(uname)" = Darwin ] && [ -z "$BT_NO_MAC_APP" ] && command -v swift >/dev/null; then
  mac/build.sh && echo "Mac app: $PWD/mac/Backtrail.app"
fi
case ":$PATH:" in *":$bin_dir:"*) ;; *) echo "Add $bin_dir to PATH to run bt from any folder." ;; esac
