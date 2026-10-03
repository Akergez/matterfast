#!/usr/bin/env bash
# Play every scenario in crates/matterfast/tests/ui/ into the real application
# and fail if one of them does not come out the way it says it should.
#
#   build-aux/ui-tests.sh                 on the display you are sitting at
#   build-aux/ui-tests.sh --headless      under a private headless sway
#   build-aux/ui-tests.sh [--headless] switcher settings   only these
#
# A scenario is a MATTERFAST_SCRIPT (crates/matterfast/src/ui/script.rs) kept
# in a file, one step per line, with what it needs said in its header:
#
#   # mode: demo | server     sample data, or a fresh matterfast-testserver
#   # size: 1320x840          the window it was written against
#   # server: MM_HISTORY=150  what that server is started with, if anything
#
# It passes when the application reaches the script's `quit` and exits with
# status 0: an `expect:` step that does not hold exits with 1, a crash exits
# with something else, and a script that never gets to its end runs into the
# timeout.
#
# Clicks are coordinates inside the window, so they only mean something at
# the size the scenario names. A tiling compositor gives a window the size it
# likes, not the one asked for; that is what --headless is for, and it is how
# CI runs these. Without it, scenarios that click may fail on your desktop
# while the keyboard-only ones still hold.
#
# Every run gets its own empty XDG directories, so a scenario that changes a
# setting does not change yours, and no scenario sees what another left.
set -Eeuo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$root"

scenarios_dir=crates/matterfast/tests/ui
app=${MATTERFAST_BIN:-target/debug/matterfast}
server=${MATTERFAST_TESTSERVER_BIN:-target/debug/matterfast-testserver}
timeout_s=${UI_TEST_TIMEOUT:-90}

headless=false
if [[ ${1:-} == --headless ]]; then
  shift
  headless=true
  runtime=$(mktemp -d)
  chmod 700 -- "$runtime"
  export XDG_RUNTIME_DIR=$runtime
  unset DISPLAY WAYLAND_DISPLAY
  # sway on wlroots' headless backend. It tiles, which here is the point: with
  # no borders and no bar the one window is exactly as large as the output,
  # and the output can be resized per scenario. (weston's headless backend
  # has no wl_seat at all, and the toolkit will not start without one.)
  # pixman, so the compositor needs no GPU; the application draws itself
  # through whatever Vulkan driver there is — lavapipe in CI.
  # --unsupported-gpu: sway refuses to start when the kernel has Nvidia's
  # module loaded, and a container sees its host's modules. Nothing here
  # touches the GPU, so the refusal is about a driver that is never used.
  printf '%s\n' 'default_border none' 'default_floating_border none' \
    'focus_follows_mouse no' > "$runtime/sway.conf"
  WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman \
    sway --unsupported-gpu -c "$runtime/sway.conf" >"$runtime/sway.log" 2>&1 &
  compositor_pid=$!
  for _ in $(seq 100); do
    socket=$(find "$runtime" -maxdepth 1 -name 'wayland-*' ! -name '*.lock' -print -quit)
    [[ -n $socket ]] && break
    sleep 0.1
  done
  [[ -n ${socket:-} ]] || { cat "$runtime/sway.log" >&2; exit 1; }
  export WAYLAND_DISPLAY=${socket##*/}
  SWAYSOCK=$(find "$runtime" -maxdepth 1 -name 'sway-ipc.*' -print -quit)
  export SWAYSOCK
fi

[[ -x $app && -x $server ]] || cargo build -p matterfast -p matterfast-testserver

if (( $# )); then
  files=()
  for name in "$@"; do files+=("$scenarios_dir/$name.script"); done
else
  files=("$scenarios_dir"/*.script)
fi

work=$(mktemp -d)
server_pid=
cleanup() {
  [[ -z $server_pid ]] || kill "$server_pid" 2>/dev/null || true
  [[ -z ${compositor_pid:-} ]] || kill "$compositor_pid" 2>/dev/null || true
  rm -rf -- "$work" ${runtime:+"$runtime"}
}
trap cleanup EXIT

header() { sed -n "s/^# *$2: *//p" "$1" | head -n1; }

failed=()
for file in "${files[@]}"; do
  name=$(basename -- "$file" .script)
  [[ -f $file ]] || { echo "no such scenario: $name" >&2; exit 2; }
  mode=$(header "$file" mode)
  size=$(header "$file" size)
  # The steps: every line that is not blank or a comment, joined by `;`.
  script=$(grep -v -E '^\s*(#|$)' "$file" | paste -sd ';')

  home=$work/$name
  mkdir -p -- "$home"
  env=(
    XDG_CONFIG_HOME="$home/config" XDG_CACHE_HOME="$home/cache" XDG_DATA_HOME="$home/data"
    MATTERFAST_NON_UNIQUE=1 MATTERFAST_SIZE="${size:-1320x840}" MATTERFAST_SCRIPT="$script"
  )
  case $mode in
    demo) env+=(MATTERFAST_DEMO=1) ;;
    server)
      # A fresh server for each scenario: it is stateful, and one scenario's
      # posts must not be another's surprise.
      # Unquoted on purpose: the header is a list of NAME=value words.
      # shellcheck disable=SC2046
      env $(header "$file" server) "$server" >"$home/server.log" 2>&1 &
      server_pid=$!
      for _ in $(seq 50); do
        (exec 3<>/dev/tcp/127.0.0.1/8065) 2>/dev/null && break
        sleep 0.1
      done
      env+=(MATTERFAST_SERVER=http://127.0.0.1:8065 MATTERFAST_USER=anton MATTERFAST_PASSWORD=test)
      # The test server is also a fake of Zed's extension registry, so a
      # scenario that installs a theme does not depend on somebody else's API.
      env+=(MATTERFAST_THEMES_API=http://127.0.0.1:8065/zed)
      ;;
    *) echo "$name: header must say '# mode: demo' or '# mode: server'" >&2; exit 2 ;;
  esac

  $headless && swaymsg -q output HEADLESS-1 resolution "${size:-1320x840}"

  status=0
  env "${env[@]}" timeout "$timeout_s" "$app" >"$home/app.log" 2>&1 || status=$?

  if [[ -n $server_pid ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
    server_pid=
  fi

  if (( status == 0 )); then
    echo "ok    $name"
  else
    case $status in
      124) why="did not finish in ${timeout_s}s" ;;
      1)   why=$(grep -m1 '^script: expected' "$home/app.log" || echo 'exit status 1') ;;
      *)   why="exit status $status" ;;
    esac
    echo "FAIL  $name — $why"
    grep -E 'panicked|ERROR' "$home/app.log" | head -n 5 | sed 's/^/        /' || true
    failed+=("$name")
  fi
done

echo
if (( ${#failed[@]} )); then
  echo "${#failed[@]} of ${#files[@]} failed: ${failed[*]}"
  exit 1
fi
echo "all ${#files[@]} passed"
