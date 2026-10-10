# Future plans

Suggestions, in rough order of value for effort. Each says why and where it would go.

## 1. Housekeeping (short)

- **Bring `master` up to date with `main`:** PR #5 merged `main` into `master` only up to 0.1.0-beta.3; the Stage 4 work and 0.2.0-beta.1 are on `main`.
- **CI:** add `cargo clippy` (warnings allowed at first), `npx tsc --noEmit`, and a short Playwright run of the UI (open, generate a small world, step Stage 4, switch layers) using the pre-installed Chromium.
- **Statistical regression tests** for Stage 4 on two or three seeds: ranges for nations, ruled share, population, era spread, first-industry year, transport totals — so tuning changes are caught.

## 2. Finish the world editor (M6)

The biggest gap against the proposal.

- **Nations editor:** paint ownership, set capital, merge, release, recolour, lock. Store as an override layer applied at the start date (or as directives at a chosen year, which already replay).
- **Railway and road editor:** draw lines between stations snapped to the province graph, delete, reroute, opening year, lock. A locked line would be inserted into `NationSim` when its year comes, so automatic projects build around it.
- **Culture editor and locks:** merge, split, paint, re-parent, rename, lock — then the open M5 check (a locked culture survives re-running Stage 4).
- **Province split** by a line, and marking provinces as wasteland/lake/sea.
- **Continue sim** from the edited state.

## 3. Deeper Stage 4

- **Resources in the economy:** coal and iron raise industrial productivity and speed railways; oil matters from the motor age; cash crops and trade goods add income in colonies. The data already exists per province (`trade_good`, `resources`).
- **Trade:** trade routes between nations along roads, rails and port lanes (ports exist; port levels and capacity would come with them), trade goods as income; canals as rare projects through narrow isthmuses.
- **Diplomacy:** alliances (wars drag in allies), vassals outside feudal empires, personal unions, peace treaties with province transfers; directives for each.
- **Feudal depth:** vassals that change liege, rebel, inherit or are elected; empires that form by themselves (a dominant nation of a culture group); counties and baronies held by their own lords; a feudal-titles editor.
- **More institutions:** ideas without an era of their own (printing press, enlightenment, global trade) that raise productivity, stability or assimilation; random delays and events at birth.
- **Era events for a 1949 start:** optional world-war phases (industrial great powers at war over a span of years, with devastation and border changes), decolonisation after the air age (overseas provinces of other cultures gain independence), revolutions in bankrupt or unstable nations.
- **Technology tracks:** split technology into agriculture, industry and military so a nation can be rich but weak, or the reverse; make *Tech* directives target a track.
- **Government types** by era, size and stability (tribe, kingdom, empire, republic, colony, protectorate), exported in `nations.csv`.
- **Religions** with the Stage 3 engine (second trait vector, slower clock), adopted by nations; religious borders and conversions.
- **Migration between nations:** refugees from wars and devastation, colonists overseas carrying their culture.
- **Carry Stage 3 bands into Stage 4** (the polity slot), so nations form from band clusters as the proposal describes.

## 4. Game-ready exports

- **Victoria 3:** `state_regions`, history files with owners, pops from culture shares and population, buildings from railways and cities.
- **CK3:** `default.map`, landed titles from states/regions, a history at a chosen year.
- **Hearts of Iron IV:** a natural fit for the 1949 start — `railways.txt` (line levels from road/rail quality), supply nodes at junctions, airbases from airports, owners and cores from the nation and culture maps.
- **GeoJSON / Azgaar-style JSON** for provinces, states and nations, for GIS tools and other generators.

## 5. AI guide

- **Memory across turns:** a short running summary of what the guide intended, so it follows a long plan without the full history.
- **Cost display and limits** per guided run (tokens are already counted).
- **Goal check:** at the end, ask the model how close the history came to the goal and propose directives for a second pass.
- **Integration test against a real provider**, run manually or with a secret in CI.

## 6. Performance

- Parallelise per-nation access (Dijkstra) and the road/rail route searches with `rayon`; update access incrementally after small changes.
- Reduce export memory (stream the raster in tiles) and time for the province clean-up.

## 7. Planet

- A time-stepped plate-drift mode (supercontinent break-up), as an optional history before Stage 1's relief.
- Climate refinements for continental interiors (more moisture recycling, seasonal lows) and a reference comparison against real Earth data in a test.

## 8. Distribution

- macOS and Linux builds in the release workflow; code signing for Windows.
