# Architecture

## Components

The repository is a Cargo workspace plus a TypeScript front end.

| Path | Crate / package | Role |
| --- | --- | --- |
| `core/` | `worldcore` | The simulation: grid, the twelve steps, edits and overrides, export and import, validation, live simulation sessions, the JSON API (`api.rs`) |
| `cli/` | `worldgen` | Headless command-line tool; also serves the UI over HTTP (`worldgen serve`) |
| `guide/` | `fwm-guide` | The AI guide: talks to an LLM (Anthropic Messages API or any OpenAI-compatible chat endpoint) and turns its tool calls into directives |
| `update/` | `fwm-update` | Update checks against GitHub releases, download with SHA-256 verification |
| `app/src-tauri/` | `worldbuilder-app` | Tauri 2 desktop shell: one `api` command that forwards to `worldcore::api`, plus the installer/updater glue |
| `app/src/` | (npm) | TypeScript + WebGL2 UI: globe and flat map, cards, tools, layers, the step-by-step panel |

The desktop app and the browser UI (through `worldgen serve`) call the same `worldcore::api::handle(cmd, args)`, so every UI action can also be scripted.

### Core modules (`core/src/`)

| Module | Contents |
| --- | --- |
| `grid.rs` | Subdivided icosahedron (N = 10·4ⁿ+2 cells), neighbours, barycentric location. Vertex order is hierarchical: a level-7 grid is a prefix of level 8 |
| `fields.rs` | Typed per-cell arrays (`F32`, `U8`, `U16`, `U32`, …) stored one binary file per field |
| `world.rs` | The `World`: parameters, edits, per-step outputs, cache keys, staleness, save/load |
| `stages/` | One module per step (`sketch`, `plates`, `tectonics`, `climate` + `wind`, `hydrology`, `biomes`, `habitability`, `states` + `partition`, `provinces` + `resources`, `cultures`, `nations` + `nations/transport`, `nations/ports`, `nations/institutions`, `nations/feudal`) and the step registry (`mod.rs`) |
| `edits.rs` | Override layers and how each edit tool is applied when a step regenerates |
| `directives.rs` | Directive type, the action catalogue with strict JSON schemas, validation |
| `api.rs` | JSON command handler, undo/redo, live simulation sessions (`sim_*`) |
| `export.rs`, `export_clean.rs` | Rasterising to equirectangular PNGs, province clean-up, CSV tables, `package.json` |
| `import.rs`, `province_import.rs` | Heightmap import; `provinces.png` + `definition.csv` import with validation |
| `validate.rs` | Consistency checks on a generated world (`worldgen validate`) |
| `community.rs` | Louvain community detection (cultures and culture groups) |
| `names.rs` | Random "languages" and dialects for place and culture names |
| `noise.rs`, `rng.rs`, `hash.rs`, `vec3.rs`, `graph.rs` | Seeded noise, deterministic RNG streams, content hashes, vector maths, graph helpers |

## Data model

- **Cells.** All simulation runs on the sphere grid (default level 8: 655,362 cells of about 780 km², ~28 km apart). Flat images exist only at export. There are no polar or seam artefacts.
- **Fields.** A step writes named fields, one value per cell (for example `elevation`, `temp`, `province`, `owner`). Fields by step: sketch (`land`, `sketch`, `mountain_hint`), plates (`plate`, `crust`, `vel_e`, `vel_n`), tectonics (`elevation`, `boundary`, `ocean_age`, `stress`, `old_relief`), climate (`temp`, `precip`, `wind_u`, `wind_v`, `t_mean`, `t_warm`, `t_cold`, `p_ann`, `continentality`, `current_offset`), hydrology (`elevation`, `water`, `lake`, `water_level`, `discharge`, `drainage_area`, `river`, `river_rank`, `river_width`, `receiver`, `erosion`, `temp_adjust`), biomes (`koppen`, `terrain`, `relief`), habitability (`habitability`, `barrier`, `river_role`, `groundwater`, `site`, `site_kind`, `fertility`), states (`state`, `region`, `continent`), provinces (`province`, `province_kind`, `state`, `trade_good`), cultures (`culture`, `culture_group`, `population`, `attraction`), nations (`owner`, `nation_culture`, `nation_population`, `nation_attraction`, `railway`, `road`, `airport`, `port`, `nation_era`, `institutions`, `institution`, `realm`, `imperial`).
- **Meta.** Each step also returns a JSON summary (`meta`) with its tables: Stage 2 has the province, state, region and adjacency tables; Stage 3 the cultures, groups, events and province shares; Stage 4 the nations, events, railways, stations, roads, airports, ports, institutions, empires and feudal titles, cities, ruins and pins.
- **Province graph.** From Stage 2 on, most work happens on the province graph (a few thousand nodes) rather than on cells. Borders carry a type (land, river, impassable, coast, lake, sea, strait), a length and a barrier cost.

## Steps, staleness and caching

Each step reads the outputs of earlier steps, its own parameters and the override layers that feed it. Its cache key is a hash of exactly those inputs plus a per-step model version (`MODEL_VERSION` in `core/src/stages/mod.rs`). Changing a parameter marks that step and everything after it as stale; nothing earlier is recomputed. When an algorithm changes, bump that step's entry in `MODEL_VERSION` so results saved by older builds are recomputed. A field file changed outside the app fails its content hash on load and the step is recomputed.

## Override layers ("user edits win")

Hand edits are stored as override layers, never baked into results:

| Layer | Fed by | Step |
| --- | --- | --- |
| `sketch` (strokes, plate pins, motion arrows) | Sketch and plate tools | 2–3 |
| `plates`, `elevation`, `biomes` | Plate paint, relief brushes, biome paint | 3, 4, 7 |
| `barriers`, `sites`, `fertility` | Barrier paint, site pins, fertility paint | 8 |
| `states`, `provinces` | Grow, merge, move, rename, goods paint | 9, 10 |
| `bands`, `attraction` | Founding-band pins, attraction paint | 11 |
| `directives` | Step-by-step steering, by hand or by the AI guide | 11, 12 |

Edits are stored in latitude/longitude (strokes, points), not by cell or id, so they replay on any seed or grid level. The price is that an edit can stop applying after a big change (a merge whose end is now sea); such edits are reported (Overrides panel, `worldgen overrides`) and can be pruned. Layers can be exported to an override bundle (`*.fwm-overrides.json`) and imported into another world.

Imported files (an edited heightmap or `provinces.png`) are recorded in `imports.json` and replace that step's result.

## Project folder

```
world.json            seed, parameters, per-step cache keys and summaries (meta)
sketch.json           sketch strokes, plate pins, motion arrows
overrides/*.json      one file per override layer (see above)
imports.json          files that replace a stage result
fields/<step>/*.bin   one little-endian binary file per field
export/               the map package
```

## Determinism

The same seed, parameters, edits and directives always give the same world. Randomness comes from seeded streams (`rng::stream::*`), one per step, so a change in one step never shifts the random numbers of another. Iteration over maps uses ordered containers (`BTreeMap`/`BTreeSet`) or sorted vectors wherever order affects the result. `worldgen check` verifies determinism and the save/load round trip; a test verifies that a live run stepped in uneven chunks with directives equals a fresh run with the same directives.

## Performance (measured in a cloud container)

| Work | Time |
| --- | --- |
| Level-8 world, Stages 1–2 | about 7 s on a desktop (about 22 s in the container) |
| Cultures, 5,000 bands × 400 generations | about 10 s |
| Nations, 800–1949 in 24-, 12- and 6-month steps (level 6) | about 5 s |
| Export 8192 × 4096 | about 28 s with the province clean-up |
