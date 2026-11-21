#!/usr/bin/env bash
# One round of the CTF tournament (docs/BOTS.md): the bench on both
# maps at average and adept, into shots/ctf/rounds/<name>.txt.
#
#   tools/ctf_round.sh r00 [seeds] [seconds]
#
# Built into target/bench so a game running from target/release is left alone.
set -e
name=$1
seeds=${2:-6}
secs=${3:-240}
cd "$(dirname "$0")/.."
# Retried: on a busy machine rustc can run out of memory and die mid-build.
built=0
for try in 1 2 3 4; do
  if CARGO_TARGET_DIR=target/bench cargo build --release -j 1 >/dev/null 2>&1; then built=1; break; fi
  sleep 20
done
[ $built = 1 ] || { echo "build failed"; exit 1; }
mkdir -p shots/ctf/rounds
out=shots/ctf/rounds/$name.txt
: > "$out"
for world in sluice ctfbox; do
  for skill in average adept; do
    ./target/bench/release/lurelia.exe --world $world --ctf $seeds --ctf-seconds $secs --ctf-size 3 --ctf-skill $skill 2>&1 | tail -8 >> "$out"
  done
done
cat "$out"
