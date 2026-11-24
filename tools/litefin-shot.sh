#!/usr/bin/env bash
# One simulator screenshot of the Jellyfin path against the offline demo library: no server, no
# sign-in, no watch state written anywhere. Boots tests/mock_jf.py --demo on a free port, arms the
# jf triggers with the mock's own placeholder token in a fresh instance root, adds the screen
# triggers given, runs `make sim-macos-shot` at 1920x1080, and once the mock has answered its last
# request (the posters in) and the heartbeat shows a still screen (the 1 fps keepalive at most, or
# a paused player), asks for the frame through the remote FIFO, then stops the simulator and mock.
#
#   tools/litefin-shot.sh <out.png> [trigger[=value] ...]
#   tools/litefin-shot.sh /tmp/shots/home.png heropin=0
#   tools/litefin-shot.sh /tmp/shots/player.png play=102 simvideo clocksink autopause=at=3000
#
# Each trigger becomes $SIM_DIR/nativejelly-<name> holding <value> (empty for a bare flag); the
# trigger list is docs/agent-reference.md's. ratingKeys and the reference shots: docs/litefin-shots.md.
# Environment: SHOT_QUIET_S (2, mock and screen both still), SHOT_AFTER_S (4, earliest capture),
# SHOT_TIMEOUT_S (180) bounds the run, KEEP=1 keeps the instance root. Needs `make demo-library`.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
[ $# -ge 1 ] || { sed -n "9,16p" "$0" >&2; exit 2; }
out=$1
shift
mkdir -p "$(dirname "$out")"
out="$(cd "$(dirname "$out")" && pwd)/$(basename "$out")"  # make runs in $root
rm -f "$out"

# Build first, outside the capture's time bound: a cold lane compiles for minutes.
make -s -C "$root" sim-macos

sim=$(mktemp -d "${TMPDIR:-/tmp}/nj-litefin-shot.XXXXXX")
keep=${KEEP:-}
mock=
cleanup() {
  [ -z "$mock" ] || { kill "$mock" 2>/dev/null || true; wait "$mock" 2>/dev/null || true; }
  if [ -n "$keep" ]; then echo "litefin-shot: instance root kept: $sim" >&2; else rm -rf "$sim"; fi
}
trap cleanup EXIT

python3 "$root/tests/mock_jf.py" --demo --port 0 > "$sim/mock.log" 2>&1 &
mock=$!
port=
for _ in $(seq 900); do  # the demo catalogue takes a few seconds to build
  port=$(sed -n 's|^mock_jf: serving on http://127\.0\.0\.1:\([0-9]*\)$|\1|p' "$sim/mock.log")
  [ -n "$port" ] && break
  kill -0 "$mock" 2>/dev/null || { cat "$sim/mock.log" >&2; exit 1; }
  sleep 0.1
done
[ -n "$port" ] || { echo "litefin-shot: mock_jf did not start" >&2; exit 1; }

arm() { printf '%s' "$2" > "$sim/nativejelly-$1"; }
arm jf ''
arm token mock-jf-token  # mock_jf's placeholder: the mock checks no token, so this is no credential
arm pms-origin "http://127.0.0.1:$port"
arm plextv "http://127.0.0.1:$port"  # nothing leaves the machine
arm stillclock 4000  # free-running animation held still, as make screenshots does
for t in "$@"; do
  name=${t%%=*}
  case $name in '' | */* | .*) echo "litefin-shot: bad trigger name: $t" >&2; exit 2 ;; esac
  case $t in *=*) arm "$name" "${t#*=}" ;; *) arm "$name" '' ;; esac
done

# Not the app's own settled capture (NJ_SHOT_SETTLE): it counts only screen work, so on a loaded
# machine a page still waiting for the mock reads as at rest and is captured empty.
log=$sim/nativejelly-events.log
shot=$sim/frame.png
make -s -C "$root" sim-macos-shot SIM_DIR="$sim" SIM_W=1920 SIM_H=1080 SIM_SHOT="$shot" SIM_FRAME= \
  SIM_PMS=127.0.0.1 SIM_PORT="$port" > "$sim/sim.out" 2>&1 &
run=$!
start=$SECONDS
deadline=$((start + ${SHOT_TIMEOUT_S:-180}))
quiet=$((${SHOT_QUIET_S:-2} * 5))
after=${SHOT_AFTER_S:-4}
seen=
still=0
asked=
while kill -0 "$run" 2>/dev/null && [ $SECONDS -lt $deadline ]; do
  size=$(grep -c '"GET ' "$sim/mock.log" || true)  # GETs only: a paused player still reports progress
  # still: no more than the 1 fps keepalive, or a paused player (its plane presents at 60)
  if [ "$size" = "$seen" ] && grep '^loop=' "$log" 2>/dev/null | tail -1 | grep -qE ' fps=[01] | play=0pm '; then
    still=$((still + 1))
  else
    still=0
  fi
  seen=$size
  if [ -z "$asked" ] && [ $still -ge $quiet ] && [ $((SECONDS - start)) -ge "$after" ] && [ -p "$sim/nativejelly-remote" ]; then
    exec 3<> "$sim/nativejelly-remote"  # read-write: a plain open would block on a FIFO
    printf 'shot\n' >&3
    exec 3>&-
    asked=1
  fi
  [ -n "$asked" ] && grep -q '^shot: wrote' "$log" && break
  sleep 0.2
done
grep -q '^shot: wrote' "$log" 2>/dev/null || echo "litefin-shot: no frame at rest within ${SHOT_TIMEOUT_S:-180}s" >&2
pkill -f "nativejelly-sim 127\.0\.0\.1 $port\$" 2>/dev/null || true  # the port names this run's simulator
wait "$run" 2>/dev/null || true
touch "$log"
[ ! -s "$sim/frame-1.png" ] || mv "$sim/frame-1.png" "$out"
grep -h 'BADTRIGGER\|refused\|gave up' "$log" >&2 || true
misses=$(sed -nE 's/^mock_jf: "(GET|POST|DELETE) ([^ ?]*)[^"]*" 404 .*/\1 \2/p' "$sim/mock.log" | sort -u)
[ -z "$misses" ] || printf 'litefin-shot: mock 404s (an image an item lacks is a 404 on Jellyfin too):\n%s\n' "$misses" >&2
if [ ! -s "$out" ]; then
  keep=1
  echo "litefin-shot: no capture written; simulator output:" >&2
  tail -15 "$sim/sim.out" >&2
  exit 1
fi
echo "$out"
