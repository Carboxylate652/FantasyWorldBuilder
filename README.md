# Fantasy World Maker

A desktop world generator, built from `Fantasy World Maker — Proposal.md`. It implements **Stage 1, Make the planet** (roadmap M0–M2), and **Stage 2, States and provinces** (M3):

1. Planet parameters
2. Continent sketch
3. Plates
4. Tectonic relief
5. Climate
6. Hydrology and erosion
7. Biomes
8. Habitability and barriers
9. States (with regions and continents)
10. Provinces (land, wasteland, lakes and sea zones)

Every step is checkpointed and can be viewed on the globe or flat map, and hand-edited, before the next one runs. Output is a Paradox-style PNG package that can be edited in GIMP and imported back in.

Stack, as the proposal recommends: a **Rust simulation core** (`core/`), a **Tauri 2 desktop shell** (`app/src-tauri/`) and a **TypeScript + WebGL2 UI** (`app/src/`). The same core also runs headless through the `worldgen` CLI (`cli/`).

## Run it

Prerequisites: Rust (the GNU toolchain works, MSVC isn't required) and Node 18+.

```bash
cd app && npm install
```

Desktop app (release build, no installer):

```bash
cd app && npx tauri build --no-bundle
```

This produces `target/release/worldbuilder-app.exe`. `npx tauri build` without the flag also builds an NSIS installer; the first time, Tauri downloads NSIS.

Development with hot reload:

```bash
cd app && npx tauri dev
```

The UI also runs in a normal browser through the CLI's HTTP server:

```bash
cargo build --release -p worldgen && (cd app && npx vite build)
./target/release/worldgen serve --static app/dist
```

Then open http://127.0.0.1:8765/. The server only accepts same-origin JSON POSTs, so other websites can't drive it.

## Using the app

- **Left panel:** the seven steps. Each card shows its parameters, a *Run to step N* button, and a status: done, stale (inputs changed) or not yet run. Changing a parameter only marks that step and the ones after it as stale. **Generate all** runs everything that is stale.
- **Opening a card** switches to that step's map layer and brush tools:
  - Sketch: *Land*, *Sea*, *Mountains here*, *Erase hint*, and the scatter brushes *Scatter land*, *Scatter sea* and *Scatter mountains*. A scatter brush breaks its edge up with seeded fractal noise (*Scatter* = amount, *Grain* = feature size), giving ragged coasts, bays, fjords and offshore islands up to one radius beyond the rim. A dashed ring shows that reach. The preview while painting uses the same noise as the core, so what you see is what you get.
  - Plates: *Plate pin*, *Motion arrow* (drag in the direction of motion), *Plate paint*
  - Relief: *Raise*, *Lower*, *Smooth*, *Flatten*, plus **Import heightmap**
  - Biomes: *Biome paint*, *Biome erase*
  - Habitability & barriers: *Add barrier* and *Remove barrier*. State and province borders follow painted barriers; *Remove barrier* also turns off a border river.
  - States: *Grow state*. Start the stroke inside a state and paint; the land you cover joins that state.
  - Provinces: *Grow province* (the same, inside one state), plus **Import…** for an edited provinces.png and definition.csv
- **Auto-update** re-runs the edited step after each stroke.
- **User edits win.** Strokes are saved as resolution-independent override layers in lat/lon and are reapplied whenever a step regenerates, including after a seed or grid-level change. Undo and redo cover every edit.
- **Political layers:** *Habitability*, *Barriers* (border rivers red, backbone rivers green), *States*, *Regions & continents* and *Provinces*. State borders are drawn as lines, and the *Borders* overlay shows them over any other layer. Hovering shows the province, state, region and continent.
- **Navigation:** drag with *Navigate* or right-drag with any tool; scroll to zoom. Top bar: globe/flat toggle, map layers, month selector, overlays (rivers, winds, plate motion, pins) and globe relief exaggeration.
- **Shortcuts:** `Ctrl+Z`/`Ctrl+Y` undo/redo, `Ctrl+S` save, `[` `]` brush size, `G`/`F` globe/flat. Each tool's shortcut letter is shown in its tooltip.
- **Export…** writes the map package, optionally cropped to a latitude band, at up to 32768 px wide.

## CLI

```
worldgen new <project-dir> [--seed N] [--level L]
worldgen run <project-dir> [--to STEP]
worldgen generate --out <dir> [--seed N] [--level L] [--export] [--width W] [--height H]
worldgen export <project-dir> [--out DIR] [--width W] [--height H] [--lat-min A] [--lat-max B]
worldgen import-heightmap <project-dir> <png> [--encoding heightmap16|paradox8|linear] [--min M --max M]
worldgen import-provinces <project-dir> <provinces.png> [--csv definition.csv] [--lat-min A --lat-max B]
worldgen import-provinces <project-dir> --remove
worldgen validate-provinces <provinces.png> [--csv definition.csv]   # checks only
worldgen sweep --out <dir> --seeds 1..20 [--level 7]     # seed sweeps
worldgen check [--seed N] [--level L]                    # determinism + save/load round trip
worldgen stats <project-dir>                             # zonal climate, winds, rain seasonality
worldgen serve [--port 8765] [--static app/dist] [--project DIR]
```

Steps are planet, sketch, plates, relief, climate, hydrology, biomes, habitability, states and provinces; `--to` defaults to provinces.

A full level-8 world (655,362 cells) generates in about 7 s, of which Stage 2 takes about 2 s. An 8192×4096 export takes about 3.5 s.

## Project folder

```
world.json            seed, parameters, per-step cache keys and summaries
sketch.json           sketch strokes, plate pins, motion arrows
overrides/*.json      user edit layers: plates, elevation, biomes, barriers, states, provinces
imports.json          files that replace a stage result (an edited heightmap or provinces.png)
fields/<step>/*.bin   one little-endian binary file per field
export/               the map package
```

Each step is cached under a hash of exactly the inputs it reads, plus a per-step model version (`MODEL_VERSION` in `core/src/stages/mod.rs`). Bump a step's version when its algorithm changes, so results saved by older builds are recomputed. On load, a field file that was changed outside the app fails its content hash, and that step is recomputed.

## Export package

| File | Contents |
| --- | --- |
| `heightmap.png` | 8-bit greyscale. Sea level = 20 by default; 255 = 6,500 m |
| `heightmap16.png` | 16-bit, metres = v / 65535 × 20000 − 11000 (lossless round trip) |
| `terrain.png` | Indexed palette: 18 game terrains, from glacier to jungle, mountains and floodplains |
| `rivers.png` | CK3-style indexed palette: source, merge, width classes 3–11 (each step ×√2 in real width), land 254, water 255 |
| `rivers.geojson` | One LineString per river (source to confluence or mouth) with `width_m` and `discharge_m3s` per vertex, for spline-based renderers (Vic3 style) and GIS tools |
| `biomes.png` | Köppen–Geiger colours |
| `climate.png`, `precipitation.png` | Reference layers for modders |
| `provinces.png` | One unique 24-bit RGB colour per province, no anti-aliasing. Borders are warped by noise (up to a third of a cell) so they don't follow the hexagonal cells; the coastline matches the heightmap pixel for pixel. A clean-up pass keeps provinces in one piece (see below) |
| `definition.csv` | `id;r;g;b;name;x;` with a `0;0;0;0;x;x;` first row (CK3 / Victoria 3) |
| `provinces.csv` | Extra columns per province: kind (land, wasteland, lake, sea), sea band (coastal, shelf, open), state, region, continent, terrain, area, habitability, coastal, centre, neighbours |
| `states.csv` | State key (`STATE_…`), name, region, continent, capital province, area, habitability, province ids and Victoria 3 style `xRRGGBB` colours |
| `regions.csv`, `continents.csv` | The hierarchy above states |
| `adjacencies.csv` | CK3 layout (`From;To;Type;Through;start_x;start_y;stop_x;stop_y;Comment`): sea crossings between provinces on different landmasses, up to *Longest strait crossing* |
| `province_adjacency.csv` | Every border between two provinces: `from;to;type;border_km;barrier;crossing_km`. Types: `land`, `river` (the border runs along a border river), `impassable` (wasteland), `coast` (land–sea), `lake`, `sea`, `strait` (with its crossing width). `barrier` is the mean crossing cost along the border (0 = open) |
| `states.png` | Reference map: states coloured, province borders thin, state borders black |
| `package.json` | Palettes, encodings, file formats, parameters |

Detail noise is added only at export time, which makes coastlines sharper than the grid. It also makes islets and ponds the grid doesn't have, and the noise-warped borders cut a few pixels off their province. Before `provinces.png` is written, a clean-up pass (`core/src/export_clean.rs`):

- turns islets and ponds that hold no grid cell's centre into the surface around them (the heightmap follows), keeping real islands;
- makes every land pixel belong to a land or wasteland province and every sea pixel to a sea zone;
- gives each stray piece of a province to the neighbouring province it touches most;
- gives a province that the noise left under 8 pixels (a one-cell island) a disc of about half a cell;
- breaks up X-crossings where four provinces meet at a pixel corner.

`package.json` reports what it changed under `province_cleanup`. On the default world (seed 1, level 8, 8192 px) this takes provinces in several pieces from 464 to 18, X-crossings from 310 to 0, and provinces with no pixels from 4 to 0. The 18 left are islands of at least a cell, cut off from the rest of their province by a channel in the heightmap; they are kept rather than deleted.

### Re-importing edited provinces

Edit `provinces.png` (and `definition.csv`) with a hard-edged pencil, then use *Import…* in the Provinces card or `worldgen import-provinces`. The files are checked first:

- **Errors (the import is refused):** duplicate colours or ids in the CSV, colours in the image with no CSV row, and anti-aliased edge pixels (a colour that is a blend of two neighbouring provinces).
- **Warnings:** CSV rows with no pixels, provinces in several pieces, provinces under 8 pixels, and X-crossings where four provinces meet at a pixel corner. A fresh export gives none of these except a handful of island provinces in several pieces.

Each cell then takes its province by majority pixel vote. A province joins the state it overlaps most, so existing provinces keep their state. Ids, colours and names come from the CSV; without a CSV, colours are numbered automatically. The latitude band is read from `package.json` next to the image. Exporting and re-importing a package leaves 99.8% of cells in the same province (seed 1, level 8, 8192 px).

## How each step works (short version)

- **Grid:** a subdivided icosahedron with N = 10·4ⁿ+2 cells. Vertex order is hierarchical, so a level-7 grid is a prefix of level 8; the climate solves on level 7 and interpolates up.
- **Plates:**
  - Continental plates are seeded inside landmasses, oceanic plates fill the oceans, and all grow by a noisy weighted flood fill.
  - Some continental plates take adjacent ocean (passive margins); others stop at the coast (active margins).
  - Minor plates form along major boundaries.
  - Each plate turns about an Euler pole; motion arrows set the pole.
- **Relief:**
  - Boundaries are classified by relative motion and crust type: collision, subduction (ocean–continent and ocean–ocean), ridge, rift or transform.
  - Each type has a cross-section profile by distance. Sea floor deepens as 2.6 + 0.35√age km.
  - Hotspot chains trail along plate motion, and ridged noise is scaled by tectonic stress.
  - Sketch fidelity pulls the coast back to the sketch.
- **Climate:**
  - A seasonal Budyko–Sellers energy balance in sin(lat), with ice-albedo feedback and persistent ice sheets. Separate land and ocean heat capacities set seasonal swings, and maritime air is carried downwind.
  - Warm and cold coastal currents and a 6.5 °C/km lapse rate.
  - Three-cell winds with a migrating ITCZ, blended so they never break at the cell edges.
  - Thermal winds (`core/src/stages/wind.rs`), added on top of the belts:
    - Surface-pressure anomalies follow temperature anomalies, using sea-level temperature with the land anomaly amplified by height, so plateaus become summer heat lows. Raw surface pressure (P ~ −h) is deliberately not used: its slope gradients are about 100× too large.
    - The anomalies are smoothed to synoptic scale. Wind then follows a closed-form friction + Coriolis balance: flow straight into lows near the equator, along the isobars at mid-latitudes.
    - This gives monsoon reversals, winter highs over continents and summer heat lows.
  - Mountain blocking turns the upslope part of the wind along the contours.
  - Temperatures are re-solved with the new winds, and low-level convergence adds rain.
  - Winds are stored per month (`wind_u`, `wind_v`, in m/s) and shown by the *Wind speed* layer and the *Wind* overlay. The *Thermal winds ×* and *Mountain blocking* settings in the Climate card control them.
  - Moisture transport with recycling over land, orographic rain, rain shadows, summer convection, and Clausius–Clapeyron scaling.
- **Hydrology:**
  - Isolated below-sea-level water bodies smaller than *Inland sea limit* are basins, not ocean.
  - Rivers cut basin outlets down by *Outlet incision*, so only deeper basins can hold lakes.
  - Each lake fills from its deepest point until open-water evaporation, minus the rain falling on it, balances inflow. A lake that reaches its spill point overflows (fresh). One that doesn't becomes terminal (salt), with dry basin floor around it, or a playa when there is almost no water. Water inside a basin drains to its lake.
  - The climate is then re-solved with lakes as open water (evaporation, lake-effect rain, milder shores) and the lakes are re-balanced. Hydrology's climate fields replace the first pass for the Biomes step.
  - The Hydrology card lists each large lake's inflow, evaporation and outflow.
  - Runoff = P − PET; implicit stream-power plus thermal erosion that keeps the sketched coast.
  - River flux: discharge adds up at every confluence, so merged rivers widen. Rivers crossing arid land lose water to evaporation and seepage (*Channel loss*), so desert rivers can narrow downstream.
  - River width follows hydraulic geometry, w = a·Q^b (default a = 5, b = 0.5): about 110 m for a Seine-sized river and 900 m for a Yangtze-sized one.
  - Routing uses a few tens of metres of seeded micro-relief, so rivers on smooth plains branch and merge instead of running in straight parallel lines. River systems shorter than *Minimum river length* are dropped.
  - Rivers are drawn as ribbons whose thickness follows their real width; hovering shows the width and a class (stream, river, major river, great river).
- **Biomes:** Köppen–Geiger from monthly T and P (Peel et al. rules), then terrains that also use relief, rivers and lakes.
- **Habitability (0–1):** growing season (months above about 6 °C), water (rain, or a river within *River reach*: irrigation), heat, slope, altitude and terrain, plus a bonus near rivers and coasts.
- **Barriers:** crossing cost per cell from ridge crests (cells higher than the two rings around them), high ground, deep desert away from rivers, ice and marsh. Rivers take one of two roles:
  - A river wider than *Border river width* through land at least *Border river habitability* is a border (Rhine).
  - A river through land drier than *Backbone river rainfall* holds its valley together (Nile): it carries no barrier, and states grow cheaply along it.
- **States:**
  - Seeds are placed by Poisson-disc sampling weighted by habitability. The disc radius shrinks on fertile land, so states in river valleys are smaller than in steppe or tundra (about 5× in area by default). The radius is tuned by bisection so the count matches land area ÷ *Mean state area*.
  - All seeds grow together (multi-source Dijkstra, step cost = length × (1 + barrier)), so neighbours meet on ridges, border rivers and deserts. Water can be crossed at *Sea crossing cost*, so islands without their own seed join the state across the shortest strait. Landmasses over *Own state from island size* get their own.
  - Clean-up: disconnected fragments join the neighbour they touch most, and states under the minimum merge into their longest-border neighbour.
  - Regions are a few neighbouring states, by farthest-point seeding and growth over the state graph (land borders and sea links), relaxed four times. Continents come from landmasses over *Continent size*; smaller islands join the nearest.
  - Names are placeholders from a small random "language" per continent; each region speaks a dialect of it (a few sounds and endings swapped), so neighbouring regions sound related but distinct. Stage 3 will replace them.
- **Provinces:**
  - Inside each state, seeds are spaced so that province area runs from *Province area, fertile* to *Province area, barren* with habitability. Growth uses the same barrier-aware search at a lower weight, followed by Lloyd relaxation passes.
  - Ice, mountains above *Wasteland above* and land below *Wasteland below habitability* become wasteland provinces when the patch is large enough. They stay inside their state.
  - The sea is split into coastal (within *Coastal sea band* of land), shelf (shallower than *Shelf depth*) and open-ocean zones of band-specific size. Lakes over *Lake province from* become lake provinces; smaller lakes join the land province around them.
  - Each state's capital province (the land province with the state's capital cell) takes the state's name; other land provinces are named in their region's dialect, seas and lakes in the nearest continent's language. No name is used twice.
  - The step also finds strait crossings and types every border between two provinces (land, river, impassable, coast, lake, sea, strait) with its length and crossing cost.

## Tests

```bash
cargo test --release -p worldcore
```

This covers grid topology, barycentric location, Köppen against real stations (London, Cairo, Singapore, Moscow, Athens), Earth insolation, determinism and the save/load round trip (through Stage 2), stale-step invalidation, plausible climate, the heightmap export/import round trip, and Stage 2 completeness: every land cell in a state and a province, unique ids and colours, state tables consistent with the cells, a capital inside every state, unique province names, a typed border for every pair of neighbours, the provinces.png export/import round trip, and the export clean-up (no provinces without pixels, no X-crossings, under 1% of provinces in pieces; unit tests on small rasters in `export_clean.rs`).

## Status against the roadmap

- **M0, done:** sphere grid, globe and flat viewer, project file, headless CLI, seeds. Level 8 renders through texture-driven WebGL2 with no vertex buffers. `worldgen check` passes both determinism and save/load identity.
- **M1, done:** sketch tools, plates, boundary relief, hotspots and pins/arrows, with plausible trench, arc and mountain placement.
- **M3, implemented:** habitability and barrier maps with barrier paint, states, regions and continents, provinces with wasteland, lakes and sea zones, state and province paint, the neutral CK3/Vic3-style package (provinces.png, definition.csv, states.csv, regions.csv, adjacencies.csv), and re-import with validation. The proposal's check "an edit made in GIMP passes validation on re-import" is covered by the round-trip test and `validate-provinces`.
- **M2, implemented:** climate, hydrology and biomes. The proposal's acceptance check, "Earth's real elevation in, Köppen map broadly matches", needs a real Earth heightmap. Import one with `worldgen import-heightmap <project> earth.png --encoding linear --min -11000 --max 8500` (or *Import heightmap…* in the Relief card), then compare `biomes.png`. No Earth data ships with this repo.

## Known limitations and next steps

- The climate is tuned to be plausible rather than exact. Large continental interiors come out dry, and the cold-desert share is higher than Earth's. Parameters in the Climate card adjust this.
- Plate motion is random unless pinned with arrows, so some seeds produce few collision ranges. Use mountain hints or arrows.
- The flat export is equirectangular, with an optional latitude crop (settled in the proposal).
- Stage 2 names are placeholders until Stage 3 (cultures) generates names from cultural phonologies. Stage 3 isn't started.
- Export uses the neutral profile only; game-specific writers (CK3 `default.map`, Vic3 `state_regions` script files) are not written yet.
- `adjacencies.csv` (CK3 layout) lists sea crossings only; every other border, including impassable ones, is in `province_adjacency.csv`.
- At grid level 7 a fertile province is only a few cells; use level 8 (default) or 9 for province maps.
- Validating an export can still report a handful of provinces in several pieces: islands that a channel in the heightmap separates from the rest of their province.
- The province clean-up makes `worldgen export` slower and larger: on the default world at 8192 × 4096 it takes about 28 s instead of 16 s, with a peak of about 800 MB instead of 590 MB.
