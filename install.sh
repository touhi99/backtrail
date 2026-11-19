#!/bin/sh
set -e
cd "$(dirname "$0")"
for tool in cargo npm; do
  command -v "$tool" >/dev/null || { echo "$tool is required but not installed." >&2; exit 1; }
done
command -v rustc >/dev/null && [ "$(rustc --version | awk '{print $2}' | awk -F. '{print $1*1000+$2}')" -lt 1085 ] \
  && { echo "Rust 1.85 or newer is required (found $(rustc --version))." >&2; exit 1; }
(cd app && npm install && npm run build)
cargo build --release
bin_dir="${BT_BIN_DIR:-$HOME/.local/bin}"
mkdir -p "$bin_dir"
ln -sf "$PWD/target/release/backtrail" "$bin_dir/bt"
echo "Linked $bin_dir/bt"
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
