# Fantasy World Maker

A desktop world generator, built from `Fantasy World Maker — Proposal.md`. It implements **Stage 1, Make the planet** (roadmap M0–M2), **Stage 2, States and provinces** (M3), **Stage 3, Cultures** (M4) and **Stage 4, Nations and history** (M5), with an optional **AI guide** that steers Stages 3 and 4 toward the history you describe:

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
11. Cultures (an agent simulation of bands of people; cultures, culture groups and their family tree)
12. Nations and history (polities form, expand, fight, colonise and break apart up to a 1910–1920 start date; railways)

Every step is checkpointed and can be viewed on the globe or flat map, and hand-edited, before the next one runs. Output is a Paradox-style PNG package that can be edited in GIMP and imported back in.

Stack, as the proposal recommends: a **Rust simulation core** (`core/`), a **Tauri 2 desktop shell** (`app/src-tauri/`) and a **TypeScript + WebGL2 UI** (`app/src/`). The same core also runs headless through the `worldgen` CLI (`cli/`).

## Download (Windows)

Windows builds are on the [Releases page](https://github.com/Carboxylate652/FantasyWorldBuilder/releases): an installer (`…_x64-setup.exe`) and a portable zip with the app and the `worldgen` CLI. They need 64-bit Windows 10 or 11 with Microsoft Edge WebView2 (the installer fetches it if it is missing). The builds are not code-signed, so SmartScreen may warn on first launch (*More info → Run anyway*).

**Updates:** from 0.1.0-beta.3 on, the app asks GitHub for the newest release at startup. When one is out, a bar under the top bar offers *Install and restart* (the installed app: it downloads the installer, checks its size and SHA-256 against GitHub's digest, starts it and closes; worlds and settings are kept), *What's new* and *Skip this version*. The portable copy and the browser UI offer the download page instead. The version button at the right of the top bar shows the version, checks on demand and holds the settings (check at startup, include betas; betas are on while you run a beta). `worldgen check-update` does the same from the command line, and `--download DIR` fetches the portable zip.

Releases are built by `.github/workflows/release-windows.yml` on a Windows runner when a `v*` tag is pushed, or a commit whose message contains `[release v0.1.0-beta.1]` (the workflow then creates the tag on that commit). Tags with a hyphen (`v0.1.0-beta.1`) become pre-releases.

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

- **Left panel:** the twelve steps in four stages. Each card shows its parameters, a *Run to step N* button, and a status: done, stale (inputs changed) or not yet run. Changing a parameter only marks that step and the ones after it as stale.
- **Stage buttons (top bar):** *Next: Stage N* generates everything up to the end of the first stage that isn't done; *Stage 1* to *Stage 4* generate up to the end of that stage (a check mark shows a finished stage); **Generate all** runs everything that is stale.
- **Opening a card** switches to that step's map layer and brush tools:
  - Sketch: *Land*, *Sea*, *Mountains here*, *Erase hint*, and the scatter brushes *Scatter land*, *Scatter sea* and *Scatter mountains*. A scatter brush breaks its edge up with seeded fractal noise (*Scatter* = amount, *Grain* = feature size), giving ragged coasts, bays, fjords and offshore islands up to one radius beyond the rim. A dashed ring shows that reach. The preview while painting uses the same noise as the core, so what you see is what you get.
  - Plates: *Plate pin*, *Motion arrow* (drag in the direction of motion), *Plate paint*
  - Relief: *Raise*, *Lower*, *Smooth*, *Flatten*, plus **Import heightmap**
  - Biomes: *Biome paint*, *Biome erase*
  - Habitability & barriers: *Add barrier* and *Remove barrier*. State and province borders follow painted barriers; *Remove barrier* also turns off a border river. *Site pin* places a town no model explains.
  - States: *Fertile land* paints fertility from −1 (barren) to +1 (fertile); it adds to habitability, so states and provinces form (smaller where fertile) and people settle where you paint. Without paint, habitability alone decides. *Erase fertility* removes it. *Grow state*: start the stroke inside a state and paint; the land you cover joins that state.
  - Cultures: *Founding band* places a founding people with a number of bands, all of one culture: more bands give a bigger head start, so a large pin on a big continent and a small one on an island give a major and a minor origin. Drag from a pin to move it; *Remove founder* deletes one. Pins replace the random founders (shown as hollow circles); *Pin these* turns the last run's random founders into pins you can then move or resize. *Attraction* paints, from −1 to +1, where people are driven away (a ghost town: capacity down to none, and bands avoid it) or drawn (a metropolis: up to 5× the capacity, and new bands and migrants favour it) during the simulation; unlike *Fertile land* it leaves states and provinces as they are, and unlike a site pin it acts all through the run instead of fixing a final population. The *Attraction* layer shows it.
  - Provinces: *Grow province* (the same, inside one state), plus **Import…** for an edited provinces.png and definition.csv
- **Step by step (Stages 3 and 4):** opening the Cultures or Nations card shows a panel on the map. *Start step by step* begins a live run that you advance with *Step* (a generation, or a few years), *+N years*, *Next era*, *Play* (step after step, map updating) and *To the end*, watching the Cultures or Nations layer change. *Finish and keep* runs to the end and keeps the result as the stage's output; *Restart* starts over (replaying the directives so far); *Stop* leaves the stage as it was.
  - **World** tab: the largest cultures or nations (click a nation to fly to its capital), the largest cities (★ capital, 🚉 station) and the city pins, what is in effect, and the latest events.
  - **City pins** (Stage 4, *City pin* tool or the Steer tab): click a province to make it draw people (+, a boom town; up to +1 a metropolis) or lose them (−, an abandoned city), drag a pin to move it, *Remove city pin* to take it away; pins can also expire after some years. The AI guide uses the same pins, adding, moving and removing them as its story goes on, and can move a nation's capital.
  - **Steer** tab: directives, applied from the next step. Stage 3: *growth* (a fertile or hard age, for places or one culture), *attraction*, *isolate* (no contact or migration across an edge, so the people inside drift into their own culture), *drift* (a culture changes fast: a schism), *contact* (two cultures meet as if alike: they tend to merge), *catastrophe*, *settle* (a migration wave). Stage 4: *aggression*, *expand toward* a province, *war*, *peace*, *stability*, *split* (along a culture, or the outlying part), *found*, *union*, *transfer* (a treaty), *rename*, *railway*, *catastrophe*, *pin add* / *pin move* / *pin remove* (city pins), *move capital*. Places are given as province, state or region ids; hovering the map shows them, with culture and nation ids. A note (why) goes into the chronicle.
  - **AI guide** tab: describe the history you want ("a sea empire in the south that breaks into three kingdoms by 1800"), then *Start guiding*. Each turn the simulation advances (*Years per turn*, or what the model asks for), the model reads a summary of the world and your goal, issues up to *Max directives* directives and writes a line for the chronicle. *Pause after this turn* stops it; you can steer by hand in between. Tokens used are counted.
  - **Providers** (*Settings* in the AI guide tab): *Anthropic* (the Claude API, default model `claude-opus-5-5`), the *Vercel AI Gateway*, *OpenAI*, *OpenRouter*, a *local server* (Ollama, LM Studio) or any other OpenAI-compatible or Anthropic-compatible gateway: choose a preset, then adjust the base URL and model. Keys are saved on this computer only (`%APPDATA%\FantasyWorldMaker\guide.json`, or `~/.config/fantasy-world-maker/guide.json`), never in a world; `ANTHROPIC_API_KEY`, `AI_GATEWAY_API_KEY`, `OPENAI_API_KEY` or `OPENROUTER_API_KEY` work too. *Test* sends one short request. With the Claude API, refusals fall back to another model server-side, and the stable part of each request (instructions and tools) is cached.
  - **Directives are saved with the world** (the *directives* override layer), each with the generation or year it was issued, so re-running the stage, exporting or opening the world elsewhere replays the same history without the model. A live run that is kept gives exactly what a fresh run with the same directives gives. Undo ends a live run.
- **Map editor (Edit map, top bar):** province, state and goods tools that work whatever step is open:
  - *Merge provinces* / *Merge states*: drag from the one to absorb (an island, a sliver) onto the one that absorbs it, even across a strait. A merged province stays in the state of the province it was dropped on.
  - *Province → state*: drag from a province into the state it should join.
  - *Grow province*, *Grow state*: brush the land you cover into the province or state under the stroke's start.
  - *Rename province*, *Rename state*: click and type a name. Culture names never replace it.
  - *Paint goods*: provinces under the brush get a trade good or cash crop, or gain or lose a deposit (copper, gold, silver, iron, coal, salt, oil), before the culture simulation runs.
- **Auto-update** re-runs the edited step after each stroke.
- **User edits win.** Strokes are saved as resolution-independent override layers in lat/lon and are reapplied whenever a step regenerates, including after a seed or grid-level change. Undo and redo cover every edit.
- **Overrides** (top bar, with the number of edits): every override layer with its edit count and the step it feeds, and *Clear* per layer. After Stage 2 it lists the state and province edits that no longer apply (a merge whose end is now sea after a seed change, a rename of a province that is gone) and removes them on request. *Export…* writes the ticked layers to an override bundle (`*.fwm-overrides.json`); *Import…* adds a bundle's layers to this world, or replaces them with *Replace on import*. Bundles carry edits to another seed or project: draw the sketch and fertility once, then try seeds.
- **Political layers:** *Habitability*, *Barriers* (border rivers red, backbone rivers green), *States*, *Regions & continents*, *Provinces*, *Nations* (owner, with borders between nations), *Railways* and *City growth* (Stage 4 attraction: capitals, stations and pins draw people; war devastation drives them away). State borders are drawn as lines, and the *Borders* overlay shows them over any other layer. Hovering shows the province, state, region and continent, with their ids, and the culture and nation.
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
worldgen sweep --out <dir> --seeds 1..20 [--level 7] [--to STEP] [--no-export]
                                                         # seed sweeps; writes <dir>/sweep.csv
worldgen check [--seed N] [--level L]                    # determinism + save/load round trip
worldgen validate <project-dir> [--run] [--json]         # consistency checks; exit code 1 on errors
worldgen info <project-dir> [--json]
worldgen stats <project-dir>                             # zonal climate, winds, rain seasonality
worldgen overrides <project-dir> [list]                  # edits per layer, edits that no longer apply
worldgen overrides <project-dir> clear <layer> | remove <layer> <i,j,...> | prune
worldgen overrides <project-dir> export <file> [--layers a,b]
worldgen overrides <project-dir> import <file> [--layers a,b] [--replace]
worldgen edit <project-dir> --tool TOOL --at LAT,LON[;LAT,LON...] [--value V] [--radius KM] [--name NAME] [--run]
worldgen directive <project-dir> --stage cultures|nations --at N --action NAME [--args JSON] [--note TEXT] [--run]
worldgen directive --list                                # every directive and its arguments
worldgen guide <project-dir> --goal TEXT [--stage nations|cultures] [--years 50] [--max-turns 60] [--max-actions 4]
worldgen guide-setup [--provider anthropic|openai] [--base-url URL] [--model ID] [--key KEY] [--effort medium]
worldgen version
worldgen check-update [--betas | --stable] [--download DIR]
worldgen serve [--port 8765] [--static app/dist] [--project DIR]
```

Layers are sketch, plates, elevation, biomes, barriers, sites, fertility, states, provinces, bands and attraction. Tools for `worldgen edit` use the app's names in snake case (`province_merge`, `fertility_paint`, `band_pin`, `goods_paint`, ...); a drag is two or more `--at` points.

For automated tests: `worldgen validate` checks that province ids and colours are unique, every land province is in an existing state and every state's members and capital agree, the adjacency table has no self-loops, duplicates or missing provinces, the grid's province field and the table match, trade goods and deposits are known names, and cultures point at living cultures in existing groups. Stale steps and edits that no longer apply are warnings. `worldgen sweep` adds a `sweep.csv` row per seed (land share, states, provinces, cultures, groups, population, cash-crop and oil provinces, unapplied edits, validation result, time). The CI workflow (`.github/workflows/ci.yml`) runs the tests, builds the UI and the desktop shell, and runs a CLI smoke test on every push.

Steps are planet, sketch, plates, relief, climate, hydrology, biomes, habitability, states, provinces, cultures and nations; `--to` defaults to nations.

`worldgen guide` runs a guided history without the app: it opens the project, starts the live simulation, plays turns until the end (printing each chronicle line and the directives it applied), keeps the result and saves the project. `worldgen directive` adds a directive by hand (applied before generation or year `--at`), for scripted histories and tests.

A full level-8 world (655,362 cells) generates in about 7 s, of which Stage 2 takes about 2 s. An 8192×4096 export takes about 3.5 s. The culture simulation adds about 10 s at the default 5,000 bands and 400 generations (measured in a cloud container, where Stages 1–2 took about 22 s).

## Project folder

```
world.json            seed, parameters, per-step cache keys and summaries
sketch.json           sketch strokes, plate pins, motion arrows
overrides/*.json      user edit layers: plates, elevation, biomes, barriers, sites, fertility,
                      states, provinces, bands, attraction, directives (Stage 3/4 steering)
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
| `cultures.png` | Reference map: provinces coloured by majority culture (hue = culture group), culture borders dark, group borders black, unsettled land grey (with cultures up to date) |
| `cultures.csv` | Every culture that ever existed: name, group, colour, parent, alive, population, provinces, founding year, end year and fate (merged into another, or died out) — the family tree |
| `culture_groups.csv`, `culture_events.csv` | Groups with their cultures; the dated log of emergences, splits, merges and extinctions |
| `provinces.csv` (Stage 2 columns added) | `trade_good`, `resources` (copper, gold, silver, iron, coal, salt, oil; fish for sea zones), `rain_mm`, `river`, `spring` (0–1) |
| `trade_goods.csv` | Every trade good, deposit and sea resource with its category (staple, cash crop, livestock, forest, mineral, energy, sea), the number of provinces that have it and their area |
| `province_cultures.csv` | Population, majority culture and culture shares (≥ 5%) per province, for Victoria-style pops |
| `nations.png` | Reference map at the start date: provinces coloured by owner, borders between nations dark, unruled land grey, railways dark with white stations (with Stage 4 up to date) |
| `railways.png` | Railways (black) and stations (red) on a grey map |
| `nations.csv` | Every nation that ever existed: name, government (city-state, kingdom, empire), colour, capital, ruling culture, provinces, population, overseas provinces, founding year, end and fate, parent (for breakaways) |
| `nation_events.csv` | The chronicle: foundings, independences, capitals taken, war summaries (who took how many provinces from whom, per half century), colonies, unions, renames, railways, ends |
| `railways.csv` | Each line: owner, opening year, stations and the provinces it runs through |
| `cities.csv` | The 100 largest cities at the start date: owner, population, capital, station, attraction, and the peak population with its year |
| `ruins.csv` | Cities that lost three quarters of their people and never recovered, with their peak and when |
| `province_nations.csv` | Owner, culture after assimilation (with shares), population and railway (track or station) per province at the start date |
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
- **Groundwater and springs:** a share of the rain above 150 mm/yr (*Groundwater recharge*) soaks in and flows downhill underground, keeping 1/e of its water over *Groundwater reach*. It surfaces below steep rises (mountain feet) and on closed basin floors, playas and salt lake shores. In land drier than *Springs matter below* and away from rivers, that is an oasis: its site value (0–1) raises habitability up to *Oasis habitability*, and Stage 2 seeds a small, fertile-sized province around each strong spring. *Site pin* (Habitability card) places a town no model explains: it becomes habitable land and, in the last culture era, holds the population you set. The *Groundwater & springs* layer shows both.
- **Barriers:** crossing cost per cell from ridge crests (cells higher than the two rings around them), high ground, deep desert away from rivers, ice and marsh. Rivers take one of two roles:
  - A river wider than *Border river width* through land at least *Border river habitability* is a border (Rhine).
  - A river through land drier than *Backbone river rainfall* holds its valley together (Nile): it carries no barrier, and states grow cheaply along it.
- **States:**
  - Seeds are placed by Poisson-disc sampling weighted by habitability. The disc radius shrinks on fertile land, so states in river valleys are smaller than in steppe or tundra (about 5× in area by default). The radius is tuned by bisection so the count matches land area ÷ *Mean state area*.
  - All seeds grow together (multi-source Dijkstra, step cost = length × (1 + barrier)), so neighbours meet on ridges, border rivers and deserts. Water can be crossed at *Sea crossing cost*, so islands without their own seed join the state across the shortest strait. Landmasses over *Own state from island size* get their own.
  - Clean-up: disconnected fragments join the neighbour they touch most, and states under the minimum merge into their longest-border neighbour.
  - Regions are a few neighbouring states, by farthest-point seeding and growth over the state graph (land borders and sea links), relaxed four times. Continents come from landmasses over *Continent size*; smaller islands join the nearest.
  - Names are placeholders from a small random "language" per continent; each region speaks a dialect of it (a few sounds and endings swapped), so neighbouring regions sound related but distinct. The Cultures step renames provinces and states in their cultures' languages.
- **Provinces:**
  - Inside each state, seeds are spaced so that province area runs from *Province area, fertile* to *Province area, barren* with habitability. Growth uses the same barrier-aware search at a lower weight, followed by Lloyd relaxation passes.
  - Ice, mountains above *Wasteland above* and land below *Wasteland below habitability* become wasteland provinces when the patch is large enough. They stay inside their state.
  - The sea is split into coastal (within *Coastal sea band* of land), shelf (shallower than *Shelf depth*) and open-ocean zones of band-specific size. Lakes over *Lake province from* become lake provinces; smaller lakes join the land province around them.
  - Each state's capital province (the land province with the state's capital cell) takes the state's name; other land provinces are named in their region's dialect, seas and lakes in the nearest continent's language. No name is used twice.
  - Resources (`core/src/stages/resources.rs`): copper, gold and silver in mountain belts (tectonic stress), iron and some gold in old shields (continental crust with no stress), coal in humid lowland basins, salt in dry basins, oil in sedimentary basins (lowlands on old continental crust, and the salt basins where domes trap it), fish on coastal and shelf seas (richest in cool water), each drawn per province with a chance that grows with its signal. Every land province also gets a trade good: a plantation-era cash crop when its climate fits one and a draw allows (rubber in hot wet jungle; coffee in tropical uplands, 16–25 °C, 350–2,400 m; sugar on hot wet lowlands; tea on humid subtropical hills; cotton on warm lowland plains with 450–1,300 mm of rain; silk and tobacco in warm temperate country), otherwise the staple of its dominant terrain (grain, wine, horses, wool, cattle, wood, furs, spices, fish, stone, metals, dates at desert springs, salt, camels). On the default world about a fifth of land provinces grow a cash crop and about 4% have oil. The *Trade goods* layer shows them; hovering shows the deposits.
  - The step also finds strait crossings and types every border between two provinces (land, river, impassable, coast, lake, sea, strait) with its length and crossing cost.

- **Cultures** (`core/src/stages/cultures.rs`), an agent simulation on the province graph:
  - *Bands* are groups of people with a home province, a population, 24 cultural traits (Axelrod's model, 4 values each) and an empty polity slot for Stage 4. A province holds people in proportion to area × habitability^1.5.
  - *Travel* follows province borders at a cost of length × (1 + barrier); wasteland is slow. Four eras open straits, coastal sailing and the open sea, and widen the travel range.
  - *One generation:* logistic growth; a band whose province is full founds a new band in the nearest unsettled province (at the band limit, a band from a shared province moves there instead); contact with nearby bands, weighted by population and falling off over *Contact distance*; a contact succeeds with probability similarity^*Like seeks like*, and one band then copies a trait of the other; *Conformity*: a band may take the value most of its successful contacts share; random *Drift*; old contacts fade.
  - *Cultures* are found every 10 generations: Louvain community detection (modularity with a resolution, *Culture detail*) on the band graph, weighted by recent contact × similarity². Communities are matched to the previous cultures by population overlap. A split, a merge, or a band's change of culture only counts after it holds for *Checks before a change counts* checks in a row, so cultures don't flicker. Splits, merges and extinctions are logged with their year: the family tree.
  - *Culture groups* are a second, coarser Louvain pass over cultures (*Group detail*).
  - *Settlements beyond the climate:* a province holds people in proportion to area × the mean of habitability^1.5 over its cells, so an oasis counts in full. Later eras add room. From era 2, caravan routes run between the 60 most populous provinces, with dry land costly to cross unless it has a spring or river, and watered stops in dry land gain up to *Caravan stop size*. From era 3, each metal deposit draws *Mining town size* people. From era 4, dry provinces with a river irrigate *Irrigated share* of their land. Site pins reach their population in era 4. The card counts *desert towns*: provinces under 250 mm of rain, with no river, that hold at least 10,000 people and 0.5 per km², by cause.
  - *Names:* the first cultures speak a dialect of their home region's language; a daughter culture starts from a changed copy of its parent's. Land provinces are renamed in their majority culture's language and states take their capital's name; the export uses these names when cultures are up to date.
  - The proposal names Leiden for community detection; this uses Louvain, which is simpler. The persistence rule covers the flicker Leiden's refinement step would reduce.
  - The simulation runs one generation at a time (`CultureSim`), so the same code serves a full run and a live step-by-step run.

- **Nations and history** (`core/src/stages/nations.rs`), on the province graph with a clock in years from *First polities* (default 800) to the *Start date* (default 1914), two years a step:
  - *Ownership is per province:* a border can cut through a state, and a nation's land need not be connected (colonies, exclaves). States are never redrawn.
  - *Formation:* a province with *People to found a polity* and no ruler founds one with a small chance each step (less once ocean shipping has begun), named after it and ruled by its majority culture.
  - *Expansion:* each nation tries to take frontier provinces (*Expansion* attempts per step × its aggression). A target is worth its people and land and costs more across barriers, far from the capital (*Reach*) and among other cultures (*Culture border weight*, half within the culture group). Empty land is settled, and settlers bring the ruler's culture; held land is fought over, won with the odds of the two nations' strength near it (people, reach from the capital, a bonus for defending one's own culture). Barren ice stays unclaimed.
  - *Eras:* gunpowder (cheaper expansion, steadier states), ocean shipping (colonies across the sea within *Colony range*, including weakly held land of much weaker nations), industry (faster growth, railways).
  - *Breakups:* a nation's instability grows with the share of its people of other cultures, its size and its spread from the capital; when it breaks, the provinces of its largest foreign culture secede, or else the part farther from the capital than from its far edge.
  - *Feedback on culture:* each year *Assimilation* of a ruled province's other cultures takes its ruler's culture, so cultures converge within borders over centuries.
  - *Railways:* from the industrial era, each nation links its largest cities (those with at least the founding population) to its network, along the cheapest route over its own land, every few years; stations sit at the ends, every five provinces and in dry land (water stops), and each draws *People per railway station*: railway towns, also in the desert.
  - *Cities rise and fall:* every province has an attraction (−1 to +1) that changes with history. A capital grows into *Capital pull* over *Years to grow a capital* (more for a larger nation), and a moved capital starts over; a conquered province loses *Sack* of its people (four times as much for a capital) and gains *War devastation*, which stops growth and drives people away until it heals (*Recovery per year*), so a city fought over again and again empties; railway stations draw people (+0.15); city pins add their value. Attraction scales how many people a province holds (up to 5×, down to none for a negative pin), and each year *Migration to cities* of a nation's people moves toward its attractive provinces, as far as they have room, more from devastated ones. A city that passes a million people and one that falls below a quarter of its peak (from at least three times the founding population) are logged as *metropolis* and *ruined* events.
  - *Directives* apply in the step whose years contain their date; *war summaries* list who took three or more provinces from whom each half century.
  - On the default world (seed 3, level 6) a run takes about 0.3 s and ends in 1914 with about 170 nations, 92% of land ruled, about 1,000 capitals taken (with the capital moving each time), some 100 independences, 60 colonies, about 500 railway lines, 68 cities that passed a million (the largest capitals reach 2–3.4 M) and 40 cities in ruins; world population grows from about 110 M to 490 M (wars and their devastation cost about a quarter of the growth seen without them).

## Tests

```bash
cargo test --release -p worldcore -p fwm-update -p fwm-guide
```

This covers grid topology, barycentric location, Köppen against real stations (London, Cairo, Singapore, Moscow, Athens), Earth insolation, determinism and the save/load round trip (through Stage 2), stale-step invalidation, plausible climate, the heightmap export/import round trip, and Stage 2 completeness: every land cell in a state and a province, unique ids and colours, state tables consistent with the cells, a capital inside every state, unique province names, a typed border for every pair of neighbours, the provinces.png export/import round trip, and the export clean-up (no provinces without pixels, no X-crossings, under 1% of provinces in pieces; unit tests on small rasters in `export_clean.rs`). For Stage 3: cultures emerge and settle most of the land, the family tree is consistent (parents before daughters, every ended culture merged or died out), province cultures match the cell field, and the culture files are exported; Louvain is unit-tested on small graphs (`community.rs`). Also: override bundles carry edits to another seed (add, replace, layer filter, bad bundles refused without changes), edits that no longer apply are reported with their layer and index, a generated world passes `validate`, cash crops and oil appear in sensible shares, and the goods editor's numbers for old goods are unchanged. For Stage 4: capitals are most of the ten largest cities; a +1 city pin grows a city and a −1 pin empties one; a pin that expires is gone at the end, moving a missing pin has no effect, and a capital move dated between two steps applies in the step that covers its year; nation tables agree with the owner field (every owned province belongs to a living nation, each nation owns its capital and the provinces it lists, ended nations own nothing), railways start and end at stations and open in the industrial era, events are in order, and a live run stepped in uneven chunks with Stage 3 and Stage 4 directives (a plague, a schism, a migration wave, aggression, a peace, a civil war, a rename) gives exactly the same tables and fields as a fresh run with the same directives. Directive schemas are strict and validation rejects unknown actions, wrong types and out-of-range values. The AI guide is tested against a mock provider in both formats (`guide/tests/mock.rs`): the Anthropic request carries the key, version header, cached system prompt, effort and tools (and no fallback outside the Claude API itself), the OpenAI-compatible one a bearer key and function tools; tool calls become directives (an out-of-range one is refused), the chronicle line is stored, and the steered run commits unchanged. The updater (`update/`) is tested on semver ordering (beta.2 < beta.10 < rc.1 < release), parsing GitHub's release list (drafts and non-version tags dropped, digests read) and refusing downloads or pages outside the project.

## Status against the roadmap

- **M0, done:** sphere grid, globe and flat viewer, project file, headless CLI (with `validate`, `overrides`, `edit` and sweep tables for automated tests), override layers (with the Overrides panel, unapplied-edit report and bundles), seeds, and update checks against GitHub releases. Level 8 renders through texture-driven WebGL2 with no vertex buffers. `worldgen check` passes both determinism and save/load identity.
- **M1, done:** sketch tools, plates, boundary relief, hotspots and pins/arrows, with plausible trench, arc and mountain placement.
- **M3, implemented:** habitability and barrier maps with barrier paint, states, regions and continents, provinces with wasteland, lakes and sea zones, state and province paint, the neutral CK3/Vic3-style package (provinces.png, definition.csv, states.csv, regions.csv, adjacencies.csv), and re-import with validation. The proposal's check "an edit made in GIMP passes validation on re-import" is covered by the round-trip test and `validate-provinces`.
- **M4, started:** the culture simulation, cultures and culture groups with a family tree, population per province, culture names, the Cultures card and the Cultures, Culture groups and Population density layers, and the culture files in the export. On seeds 1–3 it settles nearly all land and ends with 44–50 cultures in 17–22 groups (seed 1: 49 cultures in 18 groups after 60 splits, 27 merges and 4 extinctions). The proposal's speed check (5,000 bands, 400 generations in about a minute) is met at about 10 s. Groundwater, springs and site pins, resources and trade goods, and era-gated caravan, mining and irrigation growth are in. The proposal's check, "on an Earth-like test map at least one desert spring or route waypoint far from any river grows a settlement", holds on seeds 1–3: 22, 14 and 17 desert towns (under 250 mm of rain, no river, at least 10,000 people), among them spring towns on every seed; on seed 1 one spring town sits on a caravan route and has a mine (20,500 people at 228 mm of rain). Not yet done: saving the band state so the simulation can continue in Stage 4.
- **M5, implemented:** Stage 4, nations and history: polity formation, expansion and war over the culture map, eras, colonies and exclaves, breakups along culture lines, culture feedback, railways with stations, the Nations card and layers, and the nation files in the export. Stages 3 and 4 run step by step with directives, by hand or from the AI guide, and the directives replay exactly. The proposal's checks: borders mostly follow culture groups and barriers (cultural distance and barriers make expansion costlier, and breakups secede along culture lines); the event log is deterministic, so replaying the stage with the same directives reproduces the same final map (tested); "re-running Stage 4 never changes a locked culture" waits for culture locks (world editor). The Stage 3 band state is not carried into Stage 4: nations work on province populations and culture shares, which is enough for these rules.
- **M6, started:** the map editor in the top bar (merge provinces and states across any distance, move a province to another state, grow, rename, paint trade goods and deposits), fertility paint, founding-band pins, attraction paint, and stage buttons. Still to come from the proposal's world editor: splitting provinces, cultures (merge, split, paint, re-parent, lock), and painting nations and railways by hand (directives cover founding, unions, treaties, renames and new lines in the meantime).
- **M2, implemented:** climate, hydrology and biomes. The proposal's acceptance check, "Earth's real elevation in, Köppen map broadly matches", needs a real Earth heightmap. Import one with `worldgen import-heightmap <project> earth.png --encoding linear --min -11000 --max 8500` (or *Import heightmap…* in the Relief card), then compare `biomes.png`. No Earth data ships with this repo.

## Known limitations and next steps

- The climate is tuned to be plausible rather than exact. Large continental interiors come out dry, and the cold-desert share is higher than Earth's. Parameters in the Climate card adjust this.
- Plate motion is random unless pinned with arrows, so some seeds produce few collision ranges. Use mountain hints or arrows.
- The flat export is equirectangular, with an optional latitude crop (settled in the proposal).
- - Export uses the neutral profile only; game-specific writers (CK3 `default.map`, Vic3 `state_regions` script files) are not written yet.
- `adjacencies.csv` (CK3 layout) lists sea crossings only; every other border, including impassable ones, is in `province_adjacency.csv`.
- At grid level 7 a fertile province is only a few cells; use level 8 (default) or 9 for province maps.
- Validating an export can still report a handful of provinces in several pieces: islands that a channel in the heightmap separates from the rest of their province.
- The province clean-up makes `worldgen export` slower and larger: on the default world at 8192 × 4096 it takes about 28 s instead of 16 s, with a peak of about 800 MB instead of 590 MB.
