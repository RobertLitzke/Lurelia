#!/usr/bin/env bash
# Headless timing helper: run the game offscreen with the given options and print
# a one-line summary from its stats file.
#   tools/timing.sh --size 2560x1440            static view at the spawn point
#   tools/timing.sh --size 2560x1440 --bench    30 s fly-through with live streaming
set -e
cd "$(dirname "$0")/.."
mkdir -p shots
./target/release/lurelia.exe --headless --frames 120 --no-hud "$@" --stats shots/timing.json >/dev/null 2>&1
python - "$*" <<'PY'
import json, sys
d = json.load(open("shots/timing.json"))
line = "p50 %.2f  avg %.2f  max %.2f ms" % (d["frame_ms_p50"], d["frame_ms_avg"], d["frame_ms_max"])
b = d.get("bench")
if b:
    line = "bench p50 %.2f  p95 %.2f  p99 %.2f  max %.2f  over16 %d" % (b["p50_ms"], b["p95_ms"], b["p99_ms"], b["max_ms"], b["over_16ms"])
print(line, " | split", {k: round(v, 2) for k, v in d["split_avg_ms"].items()}, " |", sys.argv[1])
PY
