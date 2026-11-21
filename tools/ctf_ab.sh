#!/usr/bin/env bash
# Role changes against the baseline team (docs/BOTS.md). Each argument
# is a variant: a comma-separated set of `brain::TWEAKS` names switched on for
# one side, the other side playing the same team brain without them. All
# variants run at once, from one build, each over four settings (Sluice and
# ctfbox, average and adept).
#
#   tools/ctf_ab.sh none attack1 carry1,carry2
#
# Writes shots/ctf/roles/<variant>.txt and prints the scores. SEEDS (20) and
# SECS (240) set the size, SEED0 (0) the first seed; SKIP_BUILD=1 reuses the
# last build. TAG is added to each results file's name.
set -e
seeds=${SEEDS:-20}
secs=${SECS:-240}
cd "$(dirname "$0")/.."
built=${SKIP_BUILD:-0}
for try in 1 2 3 4; do
  [ $built = 1 ] && break
  if CARGO_TARGET_DIR=target/bench cargo build --release -j 1 >/dev/null 2>&1; then built=1; break; fi
  sleep 20
done
[ $built = 1 ] || { echo "build failed"; exit 1; }
mkdir -p shots/ctf/roles
# A copy to run, so the next build can land while these play.
exe=target/bench/ab_$$.exe
cp target/bench/release/lurelia.exe $exe
run() {
  out=shots/ctf/roles/$1${TAG}.txt
  : > "$out"
  for world in sluice ctfbox; do
    for skill in average adept; do
      CTF_SEED0=${SEED0:-0} CTF_AB=$1 ./$exe --world $world --ctf $seeds --ctf-seconds $secs --ctf-size 3 --ctf-skill $skill 2>&1 | tail -4 >> "$out"
    done
  done
}
for v in "$@"; do run "$v" & done
wait
rm -f $exe
python tools/ctf_ab.py $(for v in "$@"; do printf '%s ' "$v${TAG}"; done)
