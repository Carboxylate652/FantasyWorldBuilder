#!/usr/bin/env bash
# Build with `cargo build -p worldgen`, then:
# bash cli/tests/smoke.sh /absolute/path/to/worldgen [grid-level]
set -euo pipefail

bin=${1:?Pass the built worldgen binary as the first argument}
bin=$(realpath "$bin")
level=${2:-5}
command -v jq >/dev/null
command -v rg >/dev/null
test -x "$bin"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/fwb-cli-smoke.XXXXXX")
project="$scratch/world"
printf 'CLI test artifacts: %s\n' "$scratch"

run() {
    local name=$1
    shift
    printf 'Testing %s ... ' "$name"
    if "$bin" "$@" >"$scratch/$name.stdout" 2>"$scratch/$name.stderr"; then
        printf 'PASS\n'
    else
        printf 'FAIL\n'
        tail -30 "$scratch/$name.stderr"
        return 1
    fi
}

run version version
run check check --seed 7 --level "$level"
rg -q 'determinism: PASS' "$scratch/check.stdout"
rg -q 'save/load round trip: PASS' "$scratch/check.stdout"
run new new "$project" --seed 7 --level "$level"
run generate-stages run "$project"
run info info "$project" --json
jq -e '.seed == 7 and (.steps | length == 12) and all(.steps[]; .state == "done")' "$scratch/info.stdout" >/dev/null
run validate validate "$project" --json
jq -e '.ok and (.errors | length == 0)' "$scratch/validate.stdout" >/dev/null
jq -S '[.steps[] | {step, hash, millis, fields}]' "$project/world.json" > "$scratch/manifest-before.json"
run cached-run run "$project"
jq -S '[.steps[] | {step, hash, millis, fields}]' "$project/world.json" > "$scratch/manifest-after.json"
cmp "$scratch/manifest-before.json" "$scratch/manifest-after.json"
run stats stats "$project"
run edit edit "$project" --tool raise --at '12,25;14,35' --value 100 --radius 400 --run
run validate-edited validate "$project" --json
jq -e '.ok' "$scratch/validate-edited.stdout" >/dev/null
run overrides-export overrides "$project" export "$scratch/edits.json" --layers elevation
cp "$project/overrides/elevation.json" "$scratch/elevation-before.json"
run overrides-clear overrides "$project" clear elevation
jq -e 'length == 0' "$project/overrides/elevation.json" >/dev/null
run overrides-import overrides "$project" import "$scratch/edits.json" --replace
cmp "$scratch/elevation-before.json" "$project/overrides/elevation.json"
run overrides-remove overrides "$project" remove elevation 0
jq -e 'length == 0' "$project/overrides/elevation.json" >/dev/null
run regenerate run "$project"
run export export "$project" --out "$scratch/export" --width 1024 --height 512
for name in heightmap16.png provinces.png definition.csv package.json; do
    test -s "$scratch/export/$name"
done
run validate-provinces validate-provinces "$scratch/export/provinces.png" --csv "$scratch/export/definition.csv"
run import-heightmap import-heightmap "$project" "$scratch/export/heightmap16.png" --to relief
run import-provinces import-provinces "$project" "$scratch/export/provinces.png" --csv "$scratch/export/definition.csv" --to provinces
run remove-import import-provinces "$project" --remove --to provinces
run validate-imported validate "$project" --run --json
jq -e '.ok' "$scratch/validate-imported.stdout" >/dev/null
run sweep sweep --out "$scratch/sweep" --seeds 11..12 --level "$level" --no-export
test "$(wc -l < "$scratch/sweep/sweep.csv")" -eq 3
run directive-list directive --list
cp "$project/overrides/barriers.json" "$scratch/barriers-before.json"
printf 'invalid json' > "$project/overrides/barriers.json"
if "$bin" info "$project" >"$scratch/corrupt.stdout" 2>"$scratch/corrupt.stderr"; then
    printf 'FAIL: corrupt project was accepted\n'
    exit 1
fi
rg -q 'barriers.json' "$scratch/corrupt.stderr"
cp "$scratch/barriers-before.json" "$project/overrides/barriers.json"
printf 'PASS: CLI smoke tests; artifacts retained in %s\n' "$scratch"
