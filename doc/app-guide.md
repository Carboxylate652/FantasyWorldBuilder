# Using the app

The desktop app (Tauri) and the browser UI (`worldgen serve --static app/dist`, then http://127.0.0.1:8765/) are the same interface.

## Layout

- **Top bar:** New, Open…, Save, Save as…, Export…, Undo/Redo, Globe / Flat map, map layer, month (for monthly layers), *Overrides* (with the number of edits), *Edit map*, stage buttons *Stage 1*–*Stage 4* (a check mark when done), *Next: Stage N* / *Generate all*, and the version button (update settings).
- **Second row:** overlays — Rivers, Borders, Wind, Plate motion, Pins — and globe relief exaggeration.
- **Left panel:** the twelve step cards in four stages. Each card has its parameters, a *Run to step N* button and a status (done, stale, not run) with its run time. Opening a card switches to its map layer and its tools.
- **Tool box** (over the map): the open step's tools, the brush size and the tool's value.
- **Step-by-step panel** (over the map, Cultures and Nations cards): see [steering.md](steering.md). In Stage 4 it shows the current time and step length, stops before an institution is born to let you pick its birthplace among the gold-starred candidates (or *Let chance decide*; uncheck *Ask me where institutions are born* to let chance always decide), and lists institutions, empires and tags in the World tab.
- **Status line** (bottom): what is under the cursor — position, elevation, plate, climate, biome, province, state, region, continent (with ids), culture, nation, railway, road, port or good harbour, airport, era, embraced institutions, and the realm and imperial territory of feudal empires.

Changing a parameter marks that step and the later ones as stale; *Auto-update* re-runs the edited step after each stroke.

## Tools

| Step | Tool (shortcut) | What it does |
| --- | --- | --- |
| any | Navigate (V) | Drag to rotate or pan, scroll to zoom; right-drag navigates with any tool |
| Sketch | Land (L), Sea (S) | Paint land or sea with a soft brush |
| | Mountains here (M), Erase hint | Mountain hint strokes raise ridged relief |
| | Scatter land (N), Scatter sea, Scatter mountains | The same with a fractal edge (*Scatter* amount, *Grain* size): ragged coasts, fjords, offshore islands |
| Plates | Plate pin (P), Motion arrow (A), Plate paint | Force a plate seed, set a plate's motion by dragging, reassign cells |
| Relief | Raise (R), Lower, Smooth, Flatten | Elevation brushes (override layer); *Import heightmap* in the card |
| | Old mountains (W), Scatter old mountains, Flatten old mountains | Add worn, rounded mountains from noise, or remove them; ranges raised by plates stay |
| Biomes | Biome paint (B), Biome erase | Force a game terrain |
| Habitability | Add barrier (K), Remove barrier | Make land costly to cross (borders follow), or remove barriers including border rivers |
| | Site pin (T) | A town no model explains, with a target population |
| States | Fertile land (I), Erase fertility | Paint fertility −1…+1 (adds to habitability; more, smaller states and provinces where fertile) |
| | Grow state (E) | Start inside a state and paint: the land joins it |
| Provinces | Grow province (O) | The same inside one state; *Import…* for an edited provinces.png |
| Cultures | Founding band (J), Remove founder | Place founding peoples (number of bands); drag to move; *Pin these* turns random founders into pins |
| | Attraction (H), Erase attraction | Ghost town (−1) … metropolis (+1) during the culture simulation |
| Nations | City pin (Y), Remove city pin | In a live Stage 4 run: a boom town or an abandoned city; drag to move |

**Map editor** (*Edit map*, any step): Merge provinces (U), Merge states (drag from the one absorbed onto the absorber, even across a strait), Province → state, Grow province / Grow state, Rename province / Rename state, Paint goods (Q: a trade good, cash crop or deposit, before the culture simulation).

**Shortcuts:** `Ctrl+Z` / `Ctrl+Y` undo/redo, `Ctrl+S` save, `[` `]` brush size, `G` / `F` globe / flat.

## Map layers

| Group | Layers |
| --- | --- |
| Planet | Sketch, Plates, Crust & plate motion, Plate boundaries, Elevation, Tectonic stress, Old mountains (height added by noise mountains), Sea-floor age |
| Climate | Temperature, Precipitation, Wind speed (monthly), Continentality, Coastal currents |
| Water | Rivers & lakes, Erosion / deposition |
| Biomes | Köppen climate, Game terrain |
| Political | Habitability, Groundwater & springs, Barriers, States, Regions & continents, Provinces, Trade goods |
| Cultures | Cultures, Culture groups, Attraction, Population density |
| Nations | Nations, Railways (track, station, junction), Transport (roads fill each province by quality, railways are grey stripes over them, and stations ■, junctions ◆, ports ⚓ and airports ✈ are icons over the map; good harbours without a port are tinted), Eras (each nation's era), Institutions (institutions embraced, the newest one spreading), Realms & empires (feudal empires in their emperor's colour, imperial territory tinted, borders between realms), City growth (Stage 4 attraction) |

## Overrides panel

Lists every override layer with its edit count and the step it feeds, with *Clear* per layer. After Stage 2 it lists state and province edits that no longer apply (and removes them on request). *Export…* writes the ticked layers to a bundle (`*.fwm-overrides.json`); *Import…* adds a bundle's layers, or replaces them with *Replace on import*. Bundles carry edits to another seed or project: draw the sketch and fertility once, then try seeds.

## Export

*Export…* writes the map package (see [export-import.md](export-import.md)), optionally cropped to a latitude band, up to 32,768 px wide.

## Updates

From 0.1.0-beta.3 the app asks GitHub for the newest release at startup. When one is out, a bar offers *Install and restart* (the installed app downloads the installer, checks its size and SHA-256 against GitHub's digest, starts it and closes; worlds and settings are kept), *What's new* and *Skip this version*. The portable copy and the browser UI offer the download page. The version button checks on demand and holds the settings (check at startup, include betas).
