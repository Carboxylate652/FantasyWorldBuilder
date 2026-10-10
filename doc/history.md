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

## 11. Documentation and 0.2.0-beta.1 — `46c76da`, PR #6
The `doc/` folder: architecture, pipeline, Stage 4, steering and the AI guide, app guide, CLI and API, export, a generated parameter reference, development recipes, status, future plans and this history. Merged into `main` with the Stage 4 work (PR #6) and released as 0.2.0-beta.1.

## 12. Core performance and reliability — PR #8 (0.2.0-beta.2)
By another contributor: atomic project saves, cached grid geometry and climate grids, lighter undo/redo, a CLI regression script (`cli/tests/smoke.sh`) and release packaging checks (see `doc/core-review-changes.md`).

## 13. Institutions, ports, tags and feudal empires (unreleased)
Requested: ports in good harbours after the ocean-shipping era that carry sea transport; EU4-style institutions with a touch of westernization, born in a province (chosen among highlighted candidates or by chance) and spreading over roads, rail, ports and airports, with eras entered once most provinces embrace them; rail, ports and airports shown apart from roads (rail as stripes, stations, ports and airports as icons); time steps that shorten toward the present (adjustable); tags on states, nations and the world for guided histories, above all a lasting feudal empire of kings, dukes, counts and barons, with territory outside it.

What was built (details in [nations.md](nations.md)):
- **Time:** an integer clock in months (the year is the count divided by 12) with a step schedule (24 months, 12 from 1400, 6 from 1800), directives dated by year and month, timers for projects and summaries.
- **Ports** (`nations/ports.rs`): harbour quality from winds, depth, shelter, river mouths and winter ice; ports opening with trade and population; sea lanes only between ports; colonies sailing from ports; port trade income and upkeep; *port* directive.
- **Institutions** (`nations/institutions.rs`): six institutions with candidate birthplaces fitted to each idea; spread over land by road quality, coastal sailing, rail lines, port lanes, airports and capitals; absorption by development; eras gated by embraced share; reforms on a neighbour's model; *institution birth*, *reform* directives; the `tech` directive embraces and can give birth. Tuning went through spread rates that left continents untouched (before coastal sailing) or swept the world in a decade (before absorption).
- **Tags and feudal empires** (`nations/feudal.rs`): eleven tags; empires of kingdoms (regions) and duchies (states) with county and barony titles, internal peace, joint defence, tribute, election, dissolution, imperial territory.
- **UI:** the birthplace panel with gold-starred candidates, *Ask me where institutions are born*, drop-down choices in the Steer form, the Transport layer (road fill, rail stripes, icons), the Institutions and Realms layers, step length and fractional years in the panel.
- **Outputs:** `port`, `institutions`, `institution`, `realm`, `imperial` fields; `ports.csv`, `institutions.csv`, `titles.csv`; new columns in `nations.csv` and `province_nations.csv`; ports in `transport.png`.
- **Tests:** the live-replay test covers tags, an empire, a birthplace choice and a directive dated to a month; a new test chooses a birthplace through the live API and checks ports, births and the step schedule.

## 14. Old mountains (unreleased)
Requested: small and minor mountain clusters from noise beside the high plate-boundary ranges (sharp and high from plates, old and blunt from noise), with brushes to add and remove only the noise mountains.

What was built: an `old_mountains` pass in `stages/tectonics.rs` (low-frequency cluster noise thresholded to a share of continental land, rounded massifs with incised valleys, fading near young ranges and the coast) added on top of the plate relief and stored as `old_relief`; the `old_mountain` and `old_mountain_erase` tools (elevation override layer) and their UI brushes; the *Old mountains* layer; a test that old mountains are the only difference from a world without them and that the brushes paint and erase them while plate stress stays the same. A first try with a four-octave cluster mask gave scattered pimples; two octaves at 700 km give coherent clusters.
