#!/bin/bash
export PATH="$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
bt="$(command -v backtrail || command -v bt)"
if [ -z "$bt" ]; then
  [ "$1" = brief ] && echo "Backtrail plugin: the backtrail binary is not installed. Clone https://github.com/touhi99/backtrail and run BT_HOOKS=0 ./install.sh."
  exit 0
fi
case "$1" in
  brief) exec "$bt" brief ;;
  end)
    real="$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$bt")"
    exec "$(dirname "$real")/../../skill/session-end.sh"
    ;;
esac
