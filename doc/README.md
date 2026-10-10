# Fantasy World Maker — documentation

Fantasy World Maker is a desktop world generator. It builds a planet from a rough continent sketch (plate tectonics, climate, rivers, biomes), divides the land into states and provinces along natural barriers, grows cultures with an agent simulation, and then runs a history of nations from the first polities (year 800 by default) up to a start date (1949 by default). Every step can be inspected and hand-edited, and the result is exported as a Paradox-style map package (PNG layers and CSV tables) that can be edited in an image editor and imported back.

The top-level [`README.md`](../README.md) is the quick start (download, build, run). This folder holds the detailed documentation.

| Document | What it covers |
| --- | --- |
| [overview.md](overview.md) | Architecture: crates, the sphere grid, steps and caching, override layers, the project folder, determinism |
| [pipeline.md](pipeline.md) | Stages 1–3 step by step: what each step reads, computes and writes |
| [nations.md](nations.md) | Stage 4 in depth: formation, war, technology and eras, economy, access, roads, railway lines, airports, fertilizer, cities, events |
| [steering.md](steering.md) | Step-by-step runs, directives (every action), and the AI guide (providers, requests, keys) |
| [app-guide.md](app-guide.md) | Using the desktop app: cards, tools, map editor, layers, overrides, updates, shortcuts |
| [cli-and-api.md](cli-and-api.md) | The `worldgen` command-line tool and the JSON API shared by the app and the HTTP server |
| [export-import.md](export-import.md) | The export package file by file, the province clean-up, and re-importing edited files |
| [parameters.md](parameters.md) | Every parameter with its default (generated from the source by `tools/gen_parameters.py`) |
| [development.md](development.md) | Building, testing, CI and releases; how to add a step, a parameter, a directive or a map layer |
| [status.md](status.md) | What is implemented against the proposal's roadmap, what is not, known limitations |
| [future-plans.md](future-plans.md) | Suggested next steps, in rough priority order |
| [history.md](history.md) | Development history: what each round of work added |

The design document this project is built from is [`Fantasy World Maker — Proposal.md`](../Fantasy%20World%20Maker%20—%20Proposal.md); its "As built" notes record where the implementation differs from the plan. User-facing changes per release are in [`CHANGELOG.md`](../CHANGELOG.md).

## The twelve steps at a glance

| Stage | Step | Produces |
| --- | --- | --- |
| 1 · Make the planet | 1 Planet parameters | Radius, tilt, day and year length, solar constant, seed |
| | 2 Continent sketch | Land/sea mask and mountain hints from your strokes (or an automatic sketch) |
| | 3 Plates | Plates with crust type and motion |
| | 4 Tectonic relief | Elevation from plate boundaries, hotspots and noise; old mountains from noise; sea-floor age; stress |
| | 5 Climate | Monthly temperature, precipitation and winds |
| | 6 Hydrology and erosion | Rivers with discharge and width, lakes (fresh and salt), eroded elevation |
| | 7 Biomes | Köppen–Geiger classes and 18 game terrains |
| 2 · States and provinces | 8 Habitability and barriers | Habitability (0–1), crossing cost, border and backbone rivers, springs, site pins |
| | 9 States | States, regions and continents with names |
| | 10 Provinces | Land, wasteland, lake and sea provinces; trade goods and deposits; typed borders |
| 3 · Cultures | 11 Cultures | Bands of people, cultures and culture groups with a family tree, population per province |
| 4 · Nations and history | 12 Nations and history | Nations with owners per province, technology and eras, treasuries, roads, railway lines, airports, cities, a dated chronicle |
