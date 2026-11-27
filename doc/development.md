# Development

## Building

Prerequisites: Rust (stable; the GNU toolchain is enough on Windows) and Node 18+.

```bash
cd app && npm install                      # once
cd app && npx tauri dev                     # desktop app with hot reload
cd app && npx tauri build --no-bundle       # release exe (target/release/worldbuilder-app[.exe])
cd app && npx tauri build                   # also the NSIS installer
cargo build --release -p worldgen           # CLI
(cd app && npx vite build) && ./target/release/worldgen serve --static app/dist   # browser UI
```

## Tests

```bash
cargo test --release -p worldcore -p fwm-update -p fwm-guide
(cd app && npx tsc --noEmit)               # UI type check
```

- **Unit tests** (in the modules): Louvain (`community.rs`), province raster clean-up (`export_clean.rs`), directive schemas and validation (`directives.rs`), line transfers and travel costs (`stages/nations/transport.rs`), the updater's semver order, release parsing and download checks (`update/`), the guide's tools, brief and key masking (`guide/`).
- **Pipeline tests** (`core/tests/pipeline.rs`, small worlds): grid topology and location; Köppen against real stations; insolation; determinism and save/load; staleness; climate plausibility; heightmap round trip; Stage 2 completeness and the provinces.png round trip; cultures and the family tree; override bundles and unapplied edits; validation; cash crops and oil; the goods editor; attraction; springs and site pins; nations consistency; cities rising and falling with pins; transport, economy and eras (`transport_economy_and_eras`); live runs with directives equal fresh runs.
- **Guide mock test** (`guide/tests/mock.rs`): a local mock provider in both formats.
- **Determinism:** `worldgen check`.
- **Earth check** (`core/examples/earth_check.rs`): the climate step on Earth's real terrain, scored against 133 places. See *Tuning the climate* below.
- **Consistency:** `worldgen validate <project> --run`.

## Continuous integration and releases

- `.github/workflows/ci.yml` (every push): builds the UI, runs the three crates' tests, builds the CLI and runs a smoke test (generate seed 3 level 5 to nations, an edit, a directive, an override export, `validate --run`, `overrides`, an export), and checks that the desktop shell compiles.
- `.github/workflows/release-windows.yml`: builds the Windows installer and portable zip on a Windows runner and publishes a GitHub release. It runs on a `v*` tag, or on a commit whose message contains `[release vX.Y.Z]` (the workflow creates the tag; tag pushes may be blocked in some environments). Versions with a hyphen (`v0.2.0-beta.1`) become pre-releases. Record user-facing changes in `CHANGELOG.md` first.

## Conventions

- **Determinism first.** Use the step's RNG stream; iterate ordered containers where order matters; never depend on hash-map order or wall-clock time in simulation code.
- **Bump `MODEL_VERSION`** (`core/src/stages/mod.rs`) for a step whenever its algorithm or outputs change; otherwise saved projects keep stale cached results.
- **Parameters** have `#[serde(default)]`, so old projects load with new defaults. Document each field with a `///` comment — `doc/tools/gen_parameters.py` turns them into [parameters.md](parameters.md).
- **User edits win:** new hand edits go into an override layer in lat/lon, not into results.
- **Directives must replay:** any steering action is a directive applied inside `apply_directives` at the start of a step.
- Comments and docs: plain, short sentences; README and these docs describe behaviour, the code comments explain why.

## Recipes

### Add a parameter
1. Add the field with a `///` doc comment to the step's struct in `core/src/params.rs` and a default in its `Default` impl.
2. Use it in the step; bump the step's `MODEL_VERSION` if defaults change results.
3. Add a `p('<group>', '<key>', 'Label', min, max, step, 'hint')` line to the card in `app/src/schema.ts`.
4. Regenerate the reference: `python3 doc/tools/gen_parameters.py` (from the repository root).

### Add a Stage 3/4 directive
1. Add an `ActionSpec` in `core/src/directives.rs` (strict schema: every property required, optional ones nullable).
2. Handle it in `apply_directives` of `stages/cultures.rs` or `stages/nations.rs`; set `ok = false` when it can't apply.
3. Add a test that applies it and checks the effect; the UI form and the guide's tools pick it up automatically from the schema.
4. Mention it in the guide's system prompt (`guide/src/lib.rs`) if the model needs context, and in [steering.md](steering.md).

### Add a map layer
1. Write the field in the step (`f.put(...)`) and in its empty output.
2. Add a `LayerDef` and a `case` in `colorize` (`app/src/layers.ts`), the layer's step in `stepOfLayer` (`app/src/main.ts`) and, if useful, a hover line in `describe`.

### Add a step
Add a variant to `Step` and `STEPS`, a parameter struct, a module under `stages/`, its inputs to the cache key, a `MODEL_VERSION` entry, a card in `schema.ts`, export and validation as needed. This changes `N_STEPS` and the project format; prefer extending an existing step when possible.

### Tuning the climate
The climate constants are tuned on Earth: real terrain in, and the result compared with what Earth's climate is. No Earth data ships with the repository, so build the heightmap once. Best is NOAA's ETOPO 2022 (60 arc-second surface GeoTIFF from ncei.noaa.gov), converted with GDAL:

```bash
gdal_translate -of PNG -ot UInt16 -scale -11000 9000 0 65535 -outsize 4096 2048 \
  ETOPO_2022_v1_60s_N90W180_surface.tif earth16.png
cargo run --release -p worldcore --example earth_check -- earth16.png out/ 7
```

Without GDAL, a rougher map (NASA Blue Marble land heights, made-up sea floor) comes from an npm package:

```bash
npm pack three-globe && tar xzf three-globe-*.tgz
python3 doc/tools/make_earth_heightmap.py package/example/img/earth-topology.png package/example/img/earth-water.png earth16.png
```

Scores depend on the map (an 8-bit map loses land below about 20 m to the sea, the Ganges delta among it), so compare runs on the same file.

`out/report.txt` starts with the scores: January and July temperature errors at 133 places, the typical factor by which annual rain is off, the share of places in the right Köppen group and class, errors of the zonal means, and the Köppen shares of all land beside Earth's. Below is one line per place. The maps (`koppen.png`, `p_ann.png`, `p_jan.png` and `p_jul.png` with wind arrows, `t_jan.png`, `t_jul.png`, `cont.png`) show where the errors are. `FWM_PROBE="Moscow,Delhi"` prints those places month by month. Pass `-` for the heightmap and a seed as the fourth argument to run a generated world (then only the Köppen land shares mean anything).

With the current model on ETOPO at level 7: January error 4.3 °C and July 3.2 °C (mean absolute), rain off by a typical factor of 1.9, 71% of places in the right Köppen group (36% in the exact class), zonal temperature error 2.2 °C, land shares A 15 / B 33 / C 20 / D 18 / E 13 (Earth about 19 / 30 / 14 / 24 / 13). Compare runs station by station, not only by the totals: a change can raise the totals while breaking a whole region. Check both a change's Earth scores and a few generated seeds: a constant that helps Earth can still give odd worlds elsewhere. Bump the climate entry of `MODEL_VERSION` with any change.

### Tuning Stage 4
Run a world once to nations, then re-run only Stage 4 with changed parameters: drop the `nations` entry from `world.json`'s `steps` (or bump `MODEL_VERSION`) and `worldgen run <project> --to nations`. `worldgen info <project> --json` and the `nations` meta (eras, transport, leader, counts) are the quickest summary.
