# Command line and API

## `worldgen` (cli/)

```
worldgen new <project-dir> [--seed N] [--level L]
worldgen run <project-dir> [--to STEP]
worldgen generate --out <dir> [--seed N] [--level L] [--to STEP] [--export] [--width W] [--height H]
worldgen export <project-dir> [--out DIR] [--width W] [--height H] [--lat-min A] [--lat-max B]
worldgen import-heightmap <project-dir> <png> [--encoding heightmap16|paradox8|linear] [--min M --max M]
worldgen import-provinces <project-dir> <provinces.png> [--csv definition.csv] [--lat-min A --lat-max B]
worldgen import-provinces <project-dir> --remove
worldgen validate-provinces <provinces.png> [--csv definition.csv]
worldgen sweep --out <dir> --seeds 1..20 [--level 7] [--to STEP] [--no-export]
worldgen check [--seed N] [--level L]
worldgen validate <project-dir> [--run] [--json]
worldgen info <project-dir> [--json]
worldgen stats <project-dir>
worldgen overrides <project-dir> [list]
worldgen overrides <project-dir> clear <layer> | remove <layer> <i,j,...> | prune
worldgen overrides <project-dir> export <file> [--layers a,b]
worldgen overrides <project-dir> import <file> [--layers a,b] [--replace]
worldgen edit <project-dir> --tool TOOL --at LAT,LON[;LAT,LON...] [--value V] [--radius KM] [--name NAME] [--run]
worldgen directive <project-dir> --stage cultures|nations --at N --action NAME [--args JSON] [--note TEXT] [--run]
worldgen directive --list
worldgen guide <project-dir> --goal TEXT [--stage nations|cultures] [--years 50] [--max-turns 60] [--max-actions 4]
worldgen guide-setup [--provider anthropic|openai] [--base-url URL] [--model ID] [--key KEY] [--effort medium]
worldgen version
worldgen check-update [--betas | --stable] [--download DIR]
worldgen serve [--port 8765] [--static app/dist] [--project DIR]
```

| Command | Purpose |
| --- | --- |
| `new`, `run`, `generate` | Create a project, bring steps up to date (`--to` defaults to `nations`), or both in one go with optional export. Steps: planet, sketch, plates, relief, climate, hydrology, biomes, habitability, states, provinces, cultures, nations |
| `export` | Write the map package |
| `import-heightmap`, `import-provinces`, `validate-provinces` | Replace a stage result with an edited file (checked first) |
| `sweep` | Generate many seeds; writes `sweep.csv` (land share, states, provinces, cultures, groups, population, cash-crop and oil provinces, unapplied edits, validation, time) |
| `check` | Determinism and save/load identity |
| `validate` | Consistency checks (exit code 1 on errors; stale steps and unapplied edits are warnings) |
| `info`, `stats` | Step summaries (one line per step, Stage 4 with era and transport); zonal climate statistics |
| `overrides` | List, clear, remove, prune, export and import override layers |
| `edit` | Apply an app tool by name in snake case (`province_merge`, `fertility_paint`, `band_pin`, `goods_paint`, …); a drag is two or more `--at` points |
| `directive` | Add a directive at generation/year `--at`; `--list` prints every action and its argument schema |
| `guide`, `guide-setup` | A guided history without the app; store the provider settings |
| `check-update` | Look for a newer GitHub release; `--download` fetches the portable zip |
| `serve` | Serve the UI and the JSON API on localhost |

`validate` checks: unique province ids and colours; land provinces in existing states, state members and capitals agreeing; adjacency without self-loops, duplicates or missing provinces; the province field matching the table; known trade goods and deposits; cultures pointing at living cultures in existing groups; for Stage 4, owners alive, nation province counts and capitals, and the owner field matching the table; and that every override edit still applies.

## JSON API (`core/src/api.rs`)

The desktop shell (Tauri command `api`) and `worldgen serve` (POST `/api/<cmd>` with a JSON body; same-origin only) call `worldcore::api::handle`. Commands starting with `guide_` go to `fwm-guide`; `update_*` and `app_version` go to `fwm-update` (in the desktop shell and in `worldgen serve`).

| Command | Purpose |
| --- | --- |
| `status` | The world's state: steps (done, stale, timings, summaries), parameters, edits |
| `grid` | Grid geometry for the renderer |
| `field` | A field's values (binary; `month` for monthly fields) |
| `wind` | Monthly wind vectors for the overlay |
| `probe` | Everything known about a cell (status line) |
| `set_params` | Change parameters (marks steps stale) |
| `add_stroke`, `add_pin`, `add_arrow` | Add an edit to an override layer |
| `clear_layer`, `overrides`, `remove_edits` | Manage override layers; list unapplied edits |
| `export_overrides`, `import_overrides` | Override bundles |
| `set_auto_base` | Base for the automatic sketch |
| `import_heightmap`, `import_provinces`, `validate_provinces` | Imports |
| `political` | The political tables (provinces, states, regions, continents, cultures, nations, events, railways, pins) for the UI |
| `run` | Run steps up to a target (progress is reported while running) |
| `new`, `open`, `save` | Projects |
| `export` | Write the map package |
| `fingerprint` | A hash of the world (tests) |
| `sim_actions`, `sim_start`, `sim_step`, `sim_state`, `sim_directive`, `sim_commit`, `sim_cancel` | Live runs (see [steering.md](steering.md)) |
| `guide_presets`, `guide_config_get`, `guide_config_set`, `guide_test`, `guide_turn` | The AI guide |

Undo history stores the affected edit layer, pins, arrows, auto-base flag or imports,
rather than copying every edit. Undo and redo exchange that saved state with the
current state. Empty clears and invalid removals do not add history entries.

## CLI regression smoke test

Build the CLI and run the end-to-end test (requires Bash, `jq` and `rg`):

```sh
cargo build -p worldgen
bash cli/tests/smoke.sh "$PWD/target/debug/worldgen" 5
```

The script checks generation through nations, determinism and save/load identity,
validation, cached reruns, editing and override bundles, PNG/CSV exports, heightmap
and province imports, seed sweeps, directive discovery, and rejection of corrupt
project JSON. It retains generated projects and command logs in a fresh temporary
directory, printed at the start and end. See [the change and test report](core-review-changes.md).
