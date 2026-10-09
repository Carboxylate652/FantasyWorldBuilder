# Development history

What each round of work added, oldest first. Commit hashes refer to this repository; user-facing release notes are in [`CHANGELOG.md`](../CHANGELOG.md).

## 1. Stage 1 and Stage 2 — `319b4b3`
The first build from the proposal: a deterministic Rust core, the Tauri 2 shell, the TypeScript + WebGL2 UI and the `worldgen` CLI.
- **Stage 1:** icosphere grid, continent sketch with scatter brushes, plates, tectonic relief, a seasonal energy-balance climate with thermal winds and mountain blocking, hydrology with lake water balance, river flux and width, erosion, Köppen biomes.
- **Stage 2:** habitability and barriers with border and backbone rivers, barrier-aware states with regions and continents, provinces with wasteland, lakes and sea zones, paint tools, the CK3/Vic3-style export with validated re-import.

## 2. Proposal updates — `009f462`, `d34cb89`, `48ee593`
Stage 4 (nations and history) after cultures, settlement sites for desert cities, a unified world editor (M6), per-province nation ownership, exclaves, a 1910–1920 start date, railways with a station/line editor.

## 3. Stage 2 polish — `f16de9c`
The province raster clean-up (pieces 464 → 18, X-crossings 310 → 0), typed borders (`province_adjacency.csv`), regional dialects for names.

## 4. Stage 3: cultures — `727e212`, `de3f03e`
- The band simulation, Louvain cultures and groups, the family tree, culture languages and renaming; the Cultures card, layers and export files.
- Groundwater and springs, site pins, resources and trade goods, caravan/mining/irrigation towns; desert towns on every tested seed.

## 5. First Windows release — `0b52db7`, `32f2d4a` (0.1.0-beta.1)
The release workflow (installer and portable zip), triggered by a `v*` tag or a `[release vX.Y.Z]` commit message.

## 6. Map editor — `824a73c`, `d76a9f8` (0.1.0-beta.2)
Merge provinces and states, province → state, grow, rename, goods paint; fertility paint; founding-band pins; the attraction brush (metropolises and ghost towns); stage buttons.

## 7. Groundwork before Stage 4 — `3342cef` (0.1.0-beta.3)
- The Overrides panel: per-layer summaries, unapplied-edit reports, override bundles.
- The test CLI: `validate`, `overrides`, `edit`, `sweep.csv`, `info --json`; the CI workflow with a CLI smoke test.
- Oil and climate-driven cash crops; `trade_goods.csv`.
- The `fwm-update` crate: update checks against GitHub releases, verified installer download, the update bar.
- Released as 0.1.0-beta.3 (`c85b9c2`) and merged (PR #4, `9a2340d`).

## 8. Stage 4 and steering — `6a48fc2`
- **Nations and history:** formation, expansion and war over the culture map, colonies and exclaves, breakups along culture lines, assimilation, railways, eras; the Nations card, layers and export files.
- **Step by step for Stages 3 and 4:** live simulation sessions (`sim_start`, `sim_step`, `sim_directive`, `sim_commit`, `sim_cancel`), with the culture simulation made steppable.
- **Directives:** a catalogue of strict, validated actions for both stages, saved as an override layer and replayed exactly.
- **The AI guide** (`fwm-guide`): the Claude API (with cached system prompt, effort and server-side refusal fallback) and OpenAI-compatible endpoints (Vercel AI Gateway, OpenAI, OpenRouter, local servers); keys stored per user; `worldgen guide` and `guide-setup`.
- Fixes found along the way: the updater's TLS behind proxies (native certificates, proxy from the environment), loopback requests bypassing the proxy, too many foundings and logged conquests, polar ice nations.

## 9. Cities rise and fall — `47d346d`
Dynamic attraction: capitals grow into metropolises over decades, conquered cities are sacked and devastated (repeatedly fought-over cities empty), stations draw people, bounded migration toward attractive cities; city pins added, moved, removed or expiring (by hand, by directive or by the guide); capital moves; metropolis and ruin events; `cities.csv`, `ruins.csv`, the City growth layer. Directives dated between steps now apply in the step covering their date.

## 10. History to 1949: eras, economy and transport — `a63f795`
Requested: extend the simulation to 1949; the introduction and spread of synthetic fertilizer (1909); a road system like Imperator Rome's, which speeds interaction but costs more to build and keep the better it is; detailed railway lines with a penalty when changing lines at a crossing, and costs for stations, track and upkeep; a new era after about 1919 with cars (highways) and airplanes; eras decided dynamically by a nation's power or wealth.

What was built (details in [nations.md](nations.md)):
- **Start date 1949** by default.
- **Per-nation technology and eras** (early, gunpowder, ocean shipping, industry, fertilizer, motor age, air age), with a target level from the nation's rank in wealth per head and population, raised by neighbours, at most `tech_lead_years` ahead of the calendar. The world era follows the leader.
- **Synthetic fertilizer:** capacity up to ×1.6, phased in over 20 years of a nation's technology; first adopter ~1910.
- **Treasuries:** taxes on people × integration × productivity; the army, roads, railways and airports cost money; spending of full treasuries; bankruptcy closes lines and lets roads decay.
- **Access and integration:** Dijkstra travel costs from each capital over roads, railway lines (12% of km, a transfer penalty at every change of line), sea lanes and flights; integration drives taxes, assimilation, migration and military reach.
- **Roads** by quality (track, paved, highway), built by projects as money allows; decay when unowned.
- **Railway lines** (`nations/transport.rs`): extended at their ends, junctions where they meet, closed in bankruptcy; stations, junctions and track draw people.
- **Airports** in the air age with flight links.
- New directives `build_road`, `airport`, `subsidy`, `tech`; `railway` now costs money.
- Outputs: eras, technology, treasury, integration and infrastructure per nation; roads, stations and airports tables; `road`, `airport`, `nation_era` fields; `transport.png`, `roads.csv`, `stations.csv`, `airports.csv`; UI parameters, the *Roads, rail & air* and *Eras* layers, eras and treasuries in the live panel; guide prompt.
- Tests: the transfer-penalty unit test and `transport_economy_and_eras`.
- Tuning took three designs for technology (two rate models drifted to all-equal or all-behind before the rank-based target model) and several rounds for costs (productivity-scaled costs, a reserve that spends down, an upkeep cap, selective paved roads).

## 11. Documentation — this folder
The `doc/` folder: architecture, pipeline, Stage 4, steering and the AI guide, app guide, CLI and API, export, a generated parameter reference, development recipes, status, future plans and this history.
