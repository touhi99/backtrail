#!/bin/sh
# Builds the web app and the binary, then links the binary as `bt` in $BT_BIN_DIR (default ~/.local/bin).
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
# A login agent keeps running the old binary until it is restarted.
if launchctl list 2>/dev/null | grep -q dev.backtrail.serve; then
  launchctl kickstart -k "gui/$(id -u)/dev.backtrail.serve" && echo "Restarted dev.backtrail.serve"
fi
case ":$PATH:" in *":$bin_dir:"*) ;; *) echo "Add $bin_dir to PATH to run bt from any folder." ;; esac
