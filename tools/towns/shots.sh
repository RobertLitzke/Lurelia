#!/bin/sh
# Shots for judging the town generator: a city, a town and a suburb in
# local/worlds/townlab, each from overhead and from a low angle.
# Usage: tools/towns/shots.sh OUTDIR [BINARY]
out=${1:?out dir}; B=${2:-./target/release/lurelia.exe}
mkdir -p "$out"
S="--world townlab --headless --save ${TEMP:-/tmp}/vg_townlab --fresh --style calm --frames 40 --wait 90 --no-hud --hour 16 --weather sun --season summer"
top() { # name x z height view
  $B $S --size 1800x1800 --pos "$2,$4,$3" --look 0,-89.5 --view "$5" --screenshot "$out/$1.png" > "$out/$1.log" 2>&1 || echo "shot $1 failed"
}
low() { # name x y z yaw pitch
  $B $S --size 1600x900 --pos "$2,$3,$4" --look "$5,$6" --view 400 --screenshot "$out/$1_low.png" > /dev/null 2>&1 || echo "low $1 failed"
}
top portmouth 660 1620 760 900 &
top lynford 560 560 480 650 &
top oakfield 1330 1600 520 650 &
wait
low portmouth 662 120 2010 0 -24 &
low lynford 760 120 760 45 -28 &
low oakfield 1520 110 1790 45 -28 &
wait
ls "$out"
