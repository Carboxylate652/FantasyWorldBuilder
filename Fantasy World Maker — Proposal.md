# Fantasy World Maker — Proposal

Oct 4, 2026 · @Yunseok Shin

## Overview

The proposal: build a desktop world generator that goes from planet to provinces to cultures to nations in four stages. Each stage can be inspected, hand-edited and locked before the next stage runs, and a single world editor at the end lets you change any layer the way in-game editors do. The output is a Paradox-style map package (PNG layers plus `definition.csv`), so you can keep working on it in GIMP, Photoshop or a map editor and load it back in.

**Goals**

- An Earth-sized planet (radius 6,371 km by default, adjustable) built from a rough continent sketch, with relief shaped by plate tectonics rather than noise alone.
- A simple physically based climate (energy balance by latitude), turned into biomes.
- States drawn along natural barriers such as ridges, major rivers, straits and deserts, then split into provinces whose size follows how habitable the land is.
- Cultures that emerge from an agent simulation.
- Nations and a short history that grow on top of the finished cultures.
- A world editor, like the in-game editors of HOI4 or Victoria 3, for provinces, states, cultures and nations in one place.
- Round-trip editing: export, change it in an external tool, re-import, then re-run the later stages.

**Non-goals for v1**

- Fine local geography such as coastline detail below about 10 km. The data model leaves room for it later.
- Full general circulation or ocean-current physics.
- A playable grand-strategy game. Stage 4 produces a starting map and a history log, not a running game with diplomacy or battles.
- Anything after the start date. History ends at a start date between 1910 and 1920, so later causes of growth (big dams of the 1930s, legal gambling, tourism, oil wealth) are not simulated; cities that owe their size to them are placed by hand (see *Settlement sites*).

**Principles**

1. *Deterministic:* the same seed and inputs always give the same world.
2. *User edits win:* manual edits are stored as override layers, so regenerating a stage never wipes them.
3. *Every stage can be swapped:* each stage reads and writes plain data, so you can replace one with an imported file.

## Architecture

The simulation runs on a geodesic sphere grid, and flat PNGs are produced only at export time. Because the grid is spherical, there are no polar distortion or seam problems during simulation. The flat map is just a projection of the result.

&#91;embedded content: generation pipeline · 4 stages, override layers, export\]

Changing an earlier stage only marks the later stages as stale. The override layer is reapplied each time, and the exported package can be edited outside the app and loaded back in.

**Recommended stack:** a Rust simulation core, a Tauri desktop shell, and a TypeScript UI using WebGL2 for both the globe and the flat map. The work is CPU-heavy (hundreds of thousands of cells, graph searches, thousands of agents), so it suits Rust with multithreading. A web UI makes the editors cheap to build.

| Option | Strengths | Weaknesses |
| --- | --- | --- |
| Rust core + Tauri + TS/WebGL (recommended) | Fast and multithreaded; small installer; easy UI work | Two languages to maintain |
| Godot 4 + C# | One tool, built-in rendering and input | Building complex editor UIs is slower; harder to test the sim headless |
| Python + NumPy/Numba + web UI | Fastest to prototype | Agent sim and interactive editing get slow; hard to distribute |

**Grid resolution.** The grid is a subdivided icosahedron with N = 10 × 4^n + 2 cells: hexagons plus 12 pentagons. On an Earth-sized planet:

| Level n | Cells | Area per cell (km²) | Spacing (km) | Use |
| --- | --- | --- | --- | --- |
| 6 | 40,962 | 12,450 | \~112 | Live preview while sketching |
| 7 | 163,842 | 3,110 | \~56 | Climate, culture sim |
| 8 | 655,362 | 780 | \~28 | Default for terrain and provinces |
| 9 | 2,621,442 | 195 | \~14 | Optional high detail |

For scale, EU4 has about 4,900 provinces. At level 8 that is roughly 130 cells per province, which is enough to give provinces believable shapes.

**Data per cell** (stored as one array per field): plate ID, crust type, elevation, flow direction, river discharge, mean / warmest / coldest-month temperature, precipitation, biome, habitability, state ID, province ID and culture ID.

**Export rasterisation.** Grid fields are resampled into an equirectangular raster (default 8192 × 4096). Detail noise is added only at this step, so coastlines look sharper than the grid resolution. Paradox maps usually leave out the poles, so the export can be cropped to a latitude band (e.g. 70°S–75°N).

**Project folder:** `world.json` (seed and parameters), one binary file per field, `overrides/` (user edit layers), `export/` (the Paradox package). Each stage is cached under a hash of its inputs. Changing Stage 1 marks the later stages as stale, and your overrides are reapplied wherever they still fit.

## Stage 1 — Make the planet

Stage 1 runs as seven steps, with a checkpoint after each one. Your sketch fixes where the land is, plates explain why the land looks the way it does, and climate and rivers come after that. Every step can be viewed on the globe and edited before you move on.

1. **Planet parameters:** radius, axial tilt (default 23.4°), day length, solar constant (default 1,361 W/m²), greenhouse strength, target ocean fraction, seed.
2. **Continent sketch:** paint land and sea on the globe or a flat map with a soft brush. Optional extra strokes: “mountains here” hints, plate boundary pins, plate motion arrows.
3. **Plates:** seed one or more continental plates inside each sketched landmass and fill the oceans with oceanic plates (default about 12 major and 20 minor). Plates grow by weighted random flood fill, which gives them irregular edges. A large continent that gets two plates also gets a suture mountain belt where they meet, like the Urals.
4. **Tectonic relief:** each plate rotates about a random (or drawn) Euler pole, and the speed of every cell follows from that rotation. Boundaries are classified by the relative motion of the two sides (table below). Each boundary type has a cross-section profile, applied by distance from the boundary. Ridged fBm noise, scaled by tectonic stress, adds detail. A “sketch fidelity” slider decides how strongly the coastline you drew overrides the tectonic result.
5. **Climate:** an energy-balance estimator by latitude (next subsection).
6. **Hydrology and erosion:** priority-flood fills depressions to make lakes. Water flows downhill, and flow accumulation is weighted by precipitation. Cells above a discharge threshold become rivers, ranked by size. A few passes of stream-power and thermal erosion carve valleys and soften slopes. This step depends on precipitation, so it runs after climate.
7. **Biomes:** Köppen classes from monthly temperature and precipitation, mapped to a smaller set of game terrains.

### Plate boundaries and the relief they create

| Boundary | Relief generated | Earth example |
| --- | --- | --- |
| Continent–continent, converging | Broad high mountain belt, with a plateau behind it | Himalaya, Tibet |
| Ocean–continent, converging | Trench offshore, volcanic range along the coast | Andes, Peru–Chile Trench |
| Ocean–ocean, converging | Trench and a curved volcanic island arc | Japan, Mariana Trench |
| Ocean, diverging | Mid-ocean ridge; the sea floor deepens with age (about 2.6 + 0.35√age km) | Mid-Atlantic Ridge |
| Continent, diverging | Rift valley, lakes, flanking highlands | East African Rift |
| Transform | Fault lines, small relief | San Andreas |
| Hotspot (inside a plate) | Island or volcano chain trailing along the plate's motion | Hawaii–Emperor chain |

The tectonic model is static: it builds today's relief from today's plate motions, so it never moves your sketched continents. A time-stepped drift mode (supercontinent breakup) could come later as a separate history feature.

### Climate estimator

This follows the idea of the [geometrian climate simulator](https://space.geometrian.com/calcs/climate-sim.php): quick, ad-hoc formulas tuned until Earth's biomes come out right, with no fluid dynamics solved. That page lists some limits worth fixing here. It has no seasons or axial tilt, works out temperature cell by cell with no smoothing, and its wind breaks up near ±60°. Köppen classes need monthly values, so this model adds seasons.

1. **Insolation:** daily mean sunlight by latitude for each of 12 months, from the solar constant, tilt and orbit.
2. **Zonal temperature:** a one-dimensional Budyko–Sellers energy balance by latitude. It balances absorbed sunlight against outgoing heat and heat spreading toward the poles, with an ice-albedo feedback. It is solved month by month until stable.
3. **Local temperature:** lapse rate of 6.5 °C per km of altitude. Inland areas have bigger seasonal swings depending on distance from the sea. Simple ocean-current offsets make west coasts cooler and east coasts warmer in the subtropics.
4. **Winds:** the three circulation cells (trades, westerlies, polar easterlies), with their latitudes scaled by rotation speed. The tropical rain belt (ITCZ) moves with the seasons. Cell edges are blended so the wind does not break at ±30° and ±60°.
5. **Moisture:** evaporation over the sea rises with temperature. Moisture is carried along the wind for a set number of passes. Air forced uphill drops rain, which leaves rain shadows behind mountains. The ITCZ gets extra rain and the dry belt around 30° gets less.
6. **Outputs:** monthly temperature and precipitation, plus annual summaries, smoothed over neighbouring cells.

## Stage 2 — States and provinces

States grow outward from seeds, and crossing a natural barrier is made expensive, so state borders end up on ridges, rivers and straits. Each state is then cut into provinces, with more and smaller provinces where the land supports more people.

### Inputs computed per cell

- **Habitability (0–1):** combines temperature, precipitation, nearness to rivers and coast, slope and biome. It sets how many provinces a state gets and, later, where agents live.
- **Site value:** extra habitability at points the climate score misses (next subsection). It is added to habitability before states are seeded.
- **Barrier cost** for each edge between cells: ridge crests (high cells that are higher than their neighbours), major rivers (by discharge rank), straits and open sea, deserts, ice and marsh. The barrier map is shown as its own layer, and you can paint on it to add or remove barriers by hand.

### Settlement sites (cities in the desert)

Habitability from climate alone puts nobody in a desert unless a river runs through it (the Nile case, already handled by backbone rivers). Real desert cities exist for other reasons, and most of them can be derived from data the pipeline already has:

| Cause | Earth example | Built from | Where |
| --- | --- | --- | --- |
| Springs and groundwater | Las Vegas (named for its spring-fed meadows), Siwa, oases of the Sahara | New `groundwater` field in hydrology: recharge from rain on nearby uplands, flowing downhill under the surface and surfacing at basin floors and mountain feet | Stage 2, static |
| Route waypoints | Palmyra, Timbuktu, Silk Road oases | Crossing value: how many shortest paths between fertile regions pass through a cell, times the cost of the desert around it | Stage 3, emerges from band travel |
| Mineral wealth | Kalgoorlie, Chuquicamata | Resource deposits (see *Suggested additions*) | Stage 3, after the era that unlocks mining |
| Large-scale water works | Phoenix (canals, Roosevelt Dam 1911) | Era-gated: a river within reach of a dry basin can irrigate it | Stage 3, later eras |
| Railway stops | Las Vegas (founded 1905 as a railway town at its springs) | Industrial era: route waypoints with water get a bonus once railways exist | Stage 4, industrial era |
| Things no model explains, or that come after the start date | Las Vegas as a gambling city, Dubai | A **site pin** placed by hand: a point with a target population | Override layer |

The static part adds a site value that is large in a few cells and zero elsewhere, so a spring in the desert gets a small, dense cluster of provinces, not a wide fertile region. The dynamic parts raise the carrying capacity of those cells during the culture simulation, so a desert city can appear, grow and fade with the eras.

### States

1. Place seeds by Poisson-disc sampling weighted by habitability. The target count comes from land area (default about one state per 150,000 km², adjustable).
2. Grow all seeds together with a multi-source Dijkstra search over the cell graph. Edge cost = 1 + barrier cost, so neighbouring states meet on barriers, much like watershed segmentation.
3. Clean up: merge states below a minimum size, remove enclaves, and attach small islands to the closest state across the shortest strait, or make large islands their own state.
4. Group states into regions and regions into continents (from landmasses), giving the usual Paradox hierarchy.

**Rivers can play two roles.** A big river can be a border (the Rhine) or the backbone of one country (the Nile). A setting decides this by river rank and by how much of a basin is habitable. Desert rivers and short rivers hold their basin together, while long rivers through fertile land act as borders.

### Provinces

- The number of provinces in a state grows with its total habitability, within minimum and maximum size limits.
- Provinces are seeded by weighted Poisson sampling and then grown with the same barrier-aware Dijkstra search, at lower barrier weights. Two or three Lloyd relaxation passes inside the state make the shapes compact.
- Peaks, ice caps and deep desert become impassable wasteland provinces.
- Sea zones are split by depth band (coastal, shelf, open ocean). Large lakes become lake provinces.

### Paradox-style package

Export goes through **profiles**, but no specific game is the target: the Paradox layout is a guideline. The default profile is neutral and follows CK3 and Victoria 3 conventions; profiles for real games are optional extras. The common files are:

| File | Contents |
| --- | --- |
| `provinces.png` | One unique 24-bit RGB colour per province, no anti-aliasing |
| `definition.csv` | Semicolon-separated rows of ID, R, G, B and name, as in CK3 and Victoria 3, plus optional extra columns (e.g. land/sea/lake, terrain) |
| `heightmap.png` | 8-bit greyscale, sea-level value set by the profile |
| `rivers.png` | Indexed palette; width comes from discharge rank; source, merge and split markers |
| `terrain.png` | Indexed palette mapped from biomes |
| `states.csv`, `regions.csv` | Neutral hierarchy files, shaped like Victoria 3 state regions and CK3 title tiers |
| `adjacencies.csv` | Strait crossings between provinces that do not touch |
| `climate.png`, `biomes.png` | Reference layers for modders |

### Re-import and validation

Edited `provinces.png` and `definition.csv` files are read back and checked before anything changes:

- [ ] Duplicate colours, colours missing from the CSV, and CSV rows with no pixels
- [ ] Blended edge colours left by anti-aliasing brushes
- [ ] Provinces split into disconnected pieces, and provinces below a minimum pixel count
- [ ] “X-crossings”, where four provinces meet at one pixel corner

After that, each cell takes its province by majority pixel vote, and the adjacency graph is rebuilt. Existing provinces keep their state. New provinces join the state they overlap most.

## Stage 3 — Cultures

Cultures are not assigned. Population bands spread over the provinces, meet each other, and become more alike through contact while drifting apart through random change. Wherever contact stays rare for long enough, the bands are split into separate cultures. You can watch the run on the map, as in WorldBox, then fix the result in an editor.

### Agents

The simulation uses 500–5,000 **bands**. A band is an abstract group of people with:

- a home province and a population;
- a trait vector (default 24 traits with a few values each), which works like Axelrod's model of cultural spread;
- a phonology (sound inventory and syllable rules) used to generate names;
- an empty **polity** slot. Stage 3 never fills it; it is there so Stage 4 can continue the same simulation without changing the band layout or the save format.

### One tick (default one generation, about 25 years)

1. **Growth:** population grows logistically toward the province's carrying capacity, which comes from habitability plus the site value. Route, mining and water-works bonuses (see *Settlement sites*) switch on with the eras.
2. **Fission and migration:** a band over capacity splits in two. The new band moves to the nearby province with the best habitability after subtracting travel cost. It inherits its parent's traits with small mutations.
3. **Interaction:** each band picks partners within its travel range. The chance of contact falls off with travel cost and rises with similarity. When two bands interact, one copies a trait the other has. Each contact adds weight to a contact graph, and old weight fades over time.
4. **Drift:** each trait has a small random chance of changing every tick.
5. **Eras:** travel range grows over time. Crossing the sea becomes possible at a set era, so cultures that were split apart can meet and assimilate again.

### Detecting cultural spheres

- Every few ticks, build a graph of bands. Edge weight = faded contact weight × trait similarity.
- Run Leiden community detection at two resolutions: the coarse one gives **culture groups**, the fine one gives **cultures**.
- Match the new communities to the previous ones by overlap, so cultures keep their identity between checks. A split or merge only counts after it holds for several checks in a row, which stops cultures from flickering.
- Each split and merge is logged, which builds a **family tree** of cultures with dates.
- A province's culture is the majority by population. Minority shares are kept too, which helps with Victoria-style pops.

### Names

Each culture's phonology drifts along with its traits, and a daughter culture starts from a changed copy of its parent's. Names for cultures, provinces and states are generated from these phonologies, so related cultures have related-sounding names.

Export adds `cultures.csv` (ID, name, group, colour, parent) and per-province culture shares to the package. Cultures are edited in the world editor (below).

## Stage 4 — Nations and history

Nations come after cultures, and read them as input. The finished culture map is the ground they grow on, so a nation forms inside a culture group first and spreads outward, and its borders tend to follow the cultural and natural ones already in the map.

- **Polity formation:** from a set era, dense and well-connected clusters of bands found polities. The polity slot on each band is filled, and a capital province is picked by population and site value.
- **Timeline:** Stage 4 runs on its own clock in years, not generations, from polity formation up to a **start date** chosen between 1910 and 1920 (default 1914). Eras unlock tools along the way: gunpowder states, ocean shipping, then railways and industrial mining in the last century before the start date.
- **Ownership is per province:** a nation's border can cut through a Stage 2 state, so one state can be split between several nations, like Victoria 3's split states. States stay as geographic units and are never redrawn by Stage 4.
- **Expansion and collapse:** polities grow over the province graph at a cost that rises with barriers, distance from the capital and cultural distance. Large, culturally mixed polities have a chance to break apart along culture lines.
- **Feedback on culture:** while Stage 4 runs, contact inside one polity counts for more, so cultures slowly converge within borders. This is the only way Stage 4 changes culture, and locked cultures are exempt.
- **Output:** owner per province at a chosen start date, a capital per nation, `nations.csv` (ID, name, primary culture, colour, capital) and a dated event log of foundings, conquests and splits for lore writing.

Keeping this order makes each stage simple to re-run: changing a nation never re-runs cultures, and changing cultures marks Stage 4 as stale like any other stage.

## World editor

All hand editing comes together in one editor at the end, in the style of the in-game map editors of Paradox games. It works on any layer, reads the current map and writes only to the override layer, so re-running a stage keeps every edit. The Stage 2 brushes (barrier, grow state, grow province) stay where they are as quick tools and move into the editor as well.

| Layer | Tools |
| --- | --- |
| Provinces | Paint, split by line, merge, move to another state, rename, mark as wasteland / lake / sea zone |
| States, regions | Paint, merge, split, re-group into regions and continents, rename, recolour |
| Sites | Place, move or remove site pins with a target population |
| Cultures | Merge (traits averaged by population, or as siblings under one group); split automatically (community detection into 2–n parts) or by line, lasso or province pick; paint; re-parent in the family tree; rename or regenerate names from the phonology; lock |
| Nations | Paint ownership, set capital, merge, release a nation from another, rename, recolour, lock |
| All | Undo and redo; inspector panel for the hovered province; **continue sim** from the edited state, so edits become the new starting point |

## Suggested additions

These go beyond the brief. Each one reuses data the pipeline already produces, so it costs little to add.

| Addition | Why it helps | Built from | Suggested phase |
| --- | --- | --- | --- |
| Override layers | Regenerating never wipes manual work, and edits stay valid when you change the seed | Edit log keyed by cell and ID | Core, M0 |
| Game-ready export profiles (CK3, Vic3) | Only if you later want to load the world in a game | Neutral package + per-game writers | M3 |
| Religions | Same mechanism as cultures, on a slower clock, spread along trade and contact; Stage 4 polities can adopt one | Stage 3 engine, second trait vector | M5 |
| Resources and trade goods | Ore near mountain belts and old shields, grain on fertile plains, fish on shelves; also drives mining towns in *Settlement sites* | Tectonics, biome, hydrology | M4 |
| Population and development per province | A ready-made starting value for each province | Agent populations | M4 |
| Culture family tree export | Lore writing, flavour text | Split and merge event log | M4 |
| GeoJSON / Azgaar-style JSON export | Use with GIS tools and other generators | Province polygons | After M6 |
| Headless CLI | Batch generation, seed sweeps, automated tests | Rust core | M0 |

## Roadmap

There are seven milestones, each ending in something you can use and a pass/fail check. Durations will be set once the stack and the target game are decided.

1. **M0 — Foundations:** sphere grid, globe and flat-map viewer, project file, headless CLI, seeds. *Done when:* a 655,362-cell globe renders smoothly and save/load gives back the identical project.
2. **M1 — Sketch and tectonics:** sketch tools, plates, boundary relief, hotspots. *Done when:* an Earth-like sketch puts trenches, island arcs and mountain belts in plausible places.
3. **M2 — Climate, hydrology, biomes:** *Done when:* given Earth's real elevation data as input, the Köppen map broadly matches the real one. This is the same check the geometrian tool uses.
4. **M3 — States, provinces, export/import:** *Done when:* the exported package matches the CK3/Vic3-style layout, and an edit made in GIMP passes validation on re-import. Groundwater and the static site value land here as an addition to the habitability step.
5. **M4 — Culture simulation:** includes the era-gated site bonuses and resources. *Done when:* 5,000 bands run 400 ticks in about a minute and give stable culture groups, and on an Earth-like test map at least one desert spring or route waypoint far from any river grows a settlement.
6. **M5 — Nations and history:** *Done when:* a run gives nations whose borders mostly follow culture groups and barriers, the event log replays to the same final map, and re-running Stage 4 never changes a locked culture.
7. **M6 — World editor and polish:** one editor for provinces, states, sites, cultures and nations; undo; continue sim. *Done when:* every layer can be edited, every edit survives re-running all four stages, and the edited world exports and re-imports cleanly.

## Risks

| Risk | Mitigation |
| --- | --- |
| The sketch and the tectonics disagree | Plates are seeded from the sketch; a fidelity slider; mountain hint strokes |
| Climate tuning eats time | Check against Earth and accept “plausible”, not exact |
| The package drifts from what modding tools expect | Treat Paradox as a guideline and keep the default package close to CK3/Vic3 conventions |
| Culture borders flicker between checks | Matching by overlap, and splits only count once they persist |
| Too slow at level 9 | Default to level 8; run the culture sim on the province graph, not on cells |

## Open questions

- Which Paradox game is the first export target? Resolved: no game target. The package is a guideline that follows CK3 and Victoria 3 conventions for the `definition.csv` layout, map size and heightmap.
- Is the recommended stack (Rust + Tauri + TypeScript) OK, or do you prefer one engine such as Godot? Resolved: Rust + Tauri + TypeScript, as built in M0–M3.
- Flat export projection: equirectangular, or something like Miller that is kinder at high latitudes? Resolved: equirectangular with an optional latitude crop, as built.
- Will nations or history come after cultures? Resolved: yes, as Stage 4. Bands carry an empty polity slot from M4 so the same simulation can continue into nations.
- Can a nation's border cut through a Stage 2 state? Resolved: yes. Ownership is stored per province, and a state can be split between nations.
- How far should history run? Resolved: up to a start date between 1910 and 1920 (default 1914), so the exported map is an early-20th-century start in the spirit of Victoria 3's late game or HOI4's opening.
