#!/usr/bin/env bash
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
BIN="$HERE/target/release/input-probe"
OUT="${OUT:-$HERE/results}"
mkdir -p "$OUT"

run() {
  local name="$1"
  shift
  echo "::group::$name"
  "$@" >"$OUT/$name.jsonl" 2>"$OUT/$name.stderr"
  echo "exit=$?" >>"$OUT/$name.stderr"
  cat "$OUT/$name.jsonl"
  tail -n 40 "$OUT/$name.stderr"
  echo "::endgroup::"
}

case "$(uname -s)" in
Linux)
  export NO_AT_BRIDGE=1
  export WEBKIT_DISABLE_DMABUF_RENDERER=1
  pkg-config --modversion webkit2gtk-4.1 >"$OUT/webkit-version.txt" 2>&1
  XV=(xvfb-run -a -s "-screen 0 1280x1024x24")
  for v in main widget nofocus; do run "x11-input-$v" "${XV[@]}" env GDK_BACKEND=x11 "$BIN" input "$v"; done
  for m in deny allow none; do run "x11-perm-$m" "${XV[@]}" env GDK_BACKEND=x11 "$BIN" perm "$m"; done

  export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp/xdg-$UID}"
  mkdir -p "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"
  Xvfb :77 -screen 0 1600x1200x24 >"$OUT/xvfb-wl.log" 2>&1 &
  sleep 2
  weston --version >"$OUT/weston.log" 2>&1
  DISPLAY=:77 weston --backend=x11-backend.so --use-pixman --socket=wl-probe --width=1280 --height=1024 >>"$OUT/weston.log" 2>&1 &
  sleep 4
  if [ ! -S "$XDG_RUNTIME_DIR/wl-probe" ]; then
    DISPLAY=:77 weston --backend=x11 --renderer=pixman --socket=wl-probe --width=1280 --height=1024 >>"$OUT/weston.log" 2>&1 &
    sleep 4
  fi
  if [ -S "$XDG_RUNTIME_DIR/wl-probe" ]; then
    WL=(env -u DISPLAY GDK_BACKEND=wayland WAYLAND_DISPLAY=wl-probe)
    for v in main widget nofocus; do run "wl-input-$v" "${WL[@]}" "$BIN" input "$v"; done
    for m in deny allow none; do run "wl-perm-$m" "${WL[@]}" "$BIN" perm "$m"; done
  else
    echo "weston socket missing" >>"$OUT/weston.log"
  fi
  ;;
Darwin)
  sw_vers >"$OUT/os-version.txt" 2>&1
  defaults read /System/Library/Frameworks/WebKit.framework/Resources/Info.plist CFBundleVersion >>"$OUT/os-version.txt" 2>&1
  for v in direct window spi tracking; do run "mac-input-$v" "$BIN" input "$v"; done
  for m in deny allow none; do run "mac-perm-$m" "$BIN" perm "$m"; done
  ;;
*)
  for m in deny allow none; do run "win-perm-$m" "$BIN.exe" perm "$m"; done
  ;;
esac
exit 0
