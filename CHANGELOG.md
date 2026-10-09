# Changelog

## 0.1.0-beta.3 (unreleased)

Groundwork before Stage 4.

- **Updates from GitHub**: the app checks for a newer release at startup and offers *Install and restart* (download checked against GitHub's SHA-256 digest), *What's new* or *Skip this version*. The version button in the top bar checks on demand and holds the settings (check at startup, include betas). `worldgen check-update` does the same on the command line.
- **Overrides panel** (top bar): every override layer with its edits and the step it feeds; clear a layer; see and remove the state and province edits that no longer apply after a seed or sketch change; export layers to an override bundle and import them into another world or seed.
- **Resources and trade goods for the 1910s**: cash crops by climate (cotton, sugar, coffee, tea, tobacco, rubber, silk) and oil in sedimentary and salt basins, in the goods editor too. `trade_goods.csv` in the export summarises every good.
- **Headless CLI for testing**: `worldgen validate` (consistency checks, exit code 1 on errors), `worldgen overrides` (list, clear, remove, prune, export, import), `worldgen edit` (add a stroke), `worldgen sweep` writes `sweep.csv`, `info --json`, `version`. A CI workflow runs the tests and a CLI smoke test on every push.

## 0.1.0-beta.2

Shaping the world by hand.

- **Map editor** (*Edit map* in the top bar): merge provinces or states by dragging one onto another (islands across straits included), move a province to another state, grow provinces and states, rename provinces and states, and paint trade goods or add/remove deposits.
- **Fertility paint** (States card): fertile or barren land, added to habitability; states, provinces and people follow it.
- **Founding-band pins** (Cultures card): choose where the first peoples start and how many bands each has, for major and minor origins. Drag to move, erase to remove, or pin the random founders to adjust them.
- **Attraction paint** (Cultures card): metropolises and ghost towns in the culture simulation, without changing states or provinces.
- **Stage buttons**: *Next: Stage N*, *Stage 1/2/3* and *Generate all* (which now runs through Stage 3).

## 0.1.0-beta.1

First Windows build: Stage 1 (planet: sketch, plates, relief, climate, rivers, biomes), Stage 2 (states and provinces, with a clean Paradox-style export and re-import), and Stage 3 (cultures, culture groups and their family tree; springs, desert towns, resources and trade goods).
