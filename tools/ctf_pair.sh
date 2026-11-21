#!/usr/bin/env bash
# One paired round of the CTF tournament: the bench with change NAME on and
# with it off (CTF_OFF=NAME), from the same build, side by side.
#
#   tools/ctf_pair.sh r06 overwatch [seeds] [seconds]
#
# Writes shots/ctf/rounds/<round>_on.txt and <round>_off.txt. Built into
# target/bench so a game running from target/release is left alone.
set -e
name=$1
change=$2
seeds=${3:-10}
secs=${4:-240}
cd "$(dirname "$0")/.."
built=${SKIP_BUILD:-0}
for try in 1 2 3 4; do
  [ $built = 1 ] && break
  if CARGO_TARGET_DIR=target/bench cargo build --release -j 1 >/dev/null 2>&1; then built=1; break; fi
  sleep 20
done
[ $built = 1 ] || { echo "build failed"; exit 1; }
mkdir -p shots/ctf/rounds
run() {
  out=$1
  : > "$out"
  for world in sluice ctfbox; do
    for skill in average adept; do
      ./target/bench/release/lurelia.exe --world $world --ctf $seeds --ctf-seconds $secs --ctf-size 3 --ctf-skill $skill 2>&1 | tail -8 >> "$out"
    done
  done
}
# Changes not yet judged stay off on both sides: PENDING=a,b tools/ctf_pair.sh ...
CTF_OFF=${PENDING:-none} run shots/ctf/rounds/${name}_on.txt &
CTF_OFF=${PENDING:-none},$change run shots/ctf/rounds/${name}_off.txt &
wait
python tools/ctf_score.py ${name}_off ${name}_on
