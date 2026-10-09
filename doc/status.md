# Status: what is implemented and what is not

Measured against the roadmap in the proposal (`Fantasy World Maker — Proposal.md`). Last updated with the 1949 / transport work (commit `a63f795`, branch `master-3sczxa`).

## Roadmap milestones

| Milestone | State | Notes |
| --- | --- | --- |
| **M0 Foundations** | Done | Sphere grid, globe and flat viewer (texture-driven WebGL2, level 8 without vertex buffers), project file, headless CLI with `validate`/`overrides`/`edit`/`sweep`, override layers with bundles and an unapplied-edit report, update checks. `worldgen check` passes determinism and save/load identity |
| **M1 Sketch and tectonics** | Done | Sketch and scatter brushes, plates, boundary relief, hotspots, pins and arrows |
| **M2 Climate, hydrology, biomes** | Implemented | The acceptance check ("Earth's real elevation in, Köppen map broadly matches") needs an Earth heightmap, which is not shipped; import one with `worldgen import-heightmap … --encoding linear --min -11000 --max 8500` and compare `biomes.png` |
| **M3 States, provinces, export/import** | Implemented | Habitability and barriers, states/regions/continents, provinces with wasteland, lakes and sea zones, the neutral CK3/Vic3-style package, re-import with validation (round trip tested) |
| **M4 Culture simulation** | Implemented, one item open | Cultures, groups, family tree, names, springs, site pins, resources and trade goods, era-gated caravan/mining/irrigation towns; 5,000 bands × 400 generations in ~10 s; desert towns on seeds 1–3. Open: saving the band state so Stage 4 continues the same agents |
| **M5 Nations and history** | Implemented, one check open | Formation, expansion, war, colonies, exclaves, breakups, assimilation, per-nation technology and eras to the air age, treasuries, roads, railway lines with junctions, airports, fertilizer, cities rising and falling, city pins; step-by-step runs with directives and the AI guide; exact replay (tested). Open: "re-running Stage 4 never changes a locked culture" waits for culture locks |
| **M6 World editor and polish** | Started | Map editor (merge provinces/states, province → state, grow, rename, goods paint), fertility, founding-band and attraction paint, stage buttons, undo/redo. See below for what is missing |

## Not implemented

### World editor (proposal § World editor)
- **Provinces:** split by a line; mark as wasteland / lake / sea zone.
- **States and regions:** split, regroup states into regions and continents, recolour.
- **Cultures:** merge, split (automatic or by line/lasso/pick), paint, re-parent in the family tree, rename or regenerate names, **lock**.
- **Nations:** paint ownership, merge, release a nation, recolour, lock. (Founding, unions, treaties, renames and capital moves exist as directives in a live run.)
- **Railways:** add/move/delete stations, draw/reroute/delete lines, set opening years, lock lines. (A `railway` directive builds a line in a live run.) Roads and airports have no editor either (directives only).
- **Continue sim** from an edited state, and an inspector panel for the hovered province.

### Simulation
- Stage 3 band state is not carried into Stage 4; nations work on province populations and culture shares.
- **Religions** (suggested addition) are not modelled.
- **Resources and trade goods do not affect Stage 4** — the economy uses people, integration and technology only; coal, iron and oil play no role in industry or the motor age, and there is no trade.
- **Diplomacy** is minimal: no alliances, vassals, personal unions or trade agreements; wars are per-province attempts weighted by strength. There are no explicit world wars, revolutions or decolonisation, although the default start date is 1949.
- Technology is a single number per nation (no separate military, industrial or agricultural tracks). Government type is only by size (city-state, kingdom, empire).
- Transport is per province: a road or station "in a province" is not a path inside it; there are no ports or shipping lanes as built infrastructure (sea lanes are implicit), no canals, no tunnels, no trams or metro.
- No migration between nations (refugees, colonists of other cultures).
- A time-stepped plate-drift mode (supercontinent break-up) is not implemented; the tectonic model is static.

### Export
- Game-specific writers (CK3 `default.map`, Victoria 3 `state_regions`, history files with owners) are not written; only the neutral profile exists.
- `adjacencies.csv` lists sea crossings only; other borders are in `province_adjacency.csv`.
- No GeoJSON/Azgaar-style export of province polygons (rivers have GeoJSON).

## Known limitations

- **Climate** is plausible, not exact: large continental interiors come out dry, and the cold-desert share is higher than Earth's.
- **Plates** move randomly unless pinned with arrows, so some seeds have few collision ranges.
- **Grid level 7** gives fertile provinces only a few cells; use level 8 (default) or 9 for province maps.
- **Export** takes about 28 s and ~800 MB peak at 8192 × 4096 with the province clean-up; a handful of island provinces stay in several pieces (cut by a channel in the heightmap).
- **Stage 4 takes ~3 s** (was 0.3 s before access, roads and rails were added); one step in a live run is a few milliseconds.
- **Stage 4 economy and technology constants are judgement calls** (see [nations.md](nations.md#tuning-notes)), tuned on one world (seed 3, level 6); other worlds may need different *Tax share*, cost or technology settings.
- **Changing the Stage 4 model changes histories:** the same seed gives a different history after this update.

## Not verified

- The AI guide has only been tested against a mock provider; live calls to the Claude API, the Vercel AI Gateway, OpenAI, OpenRouter or local servers were not exercised in automated tests.
- Releases are Windows-only and not code-signed; macOS and Linux builds from source are not part of CI beyond compiling on Ubuntu.
- UI flows are checked by hand and with ad-hoc Playwright scripts, not by tests in CI.

## Repository note

PR #5 (opened from the Claude Code UI) merges `main` into `master` and contains the history up to the 0.1.0-beta.3 merge (`9a2340d`). The Stage 4 work (`6a48fc2`, `47d346d`, `a63f795`) is on branch `master-3sczxa` and is not part of PR #5.
