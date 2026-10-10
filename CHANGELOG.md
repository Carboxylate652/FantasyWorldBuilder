# Changelog

## Unreleased

Institutions, ports, feudal empires and finer time steps for Stage 4.

- **Institutions** (EU4 style, with a touch of westernization): each era opens with an institution — gunpowder, navigation, industrialisation, synthetic fertilizer, motorisation, aviation — born in one province and spreading over land, roads, railways, ports, airports and from capitals. A nation enters the era once most of its provinces have embraced it; rich, developed nations take new ideas up first. Choose each birthplace among highlighted candidates in the step-by-step panel, or let chance decide. Nations far behind a neighbour may reform on its model. New *Institutions* layer, `institutions.csv`, directives *institution birth* and *reform*.
- **Ports:** coastal provinces with a good harbour (calm winds, deep water, shelter, a river mouth, no winter ice) and enough people around become ports once their owner sails the oceans. Sea lanes, colonies, trade income and institutions cross the sea through ports. `ports.csv`, *port* directive.
- **Tags and feudal empires:** tag the world, states or nations (isolationist, expansionist, merchant republic, eternal, holy land, free state, institution cradle, no overseas colonies, slow or fast institutions, frequent wars, stable realms). A *feudal empire* enfeoffs kings over regions and dukes over states, with counties and baronies; its members never fight each other or break apart, and it lasts in every era until you dissolve it — the largest member is elected if the emperor falls; land outside the imperial territory stays outside. New *Realms & empires* layer and `titles.csv`.
- **Finer time steps toward the present:** the Stage 4 clock counts whole months; by default steps are 24 months until 1400, 12 until 1800 and 6 after (adjustable: `months_per_step`, `months_per_step_1`, `months_per_step_2`). Directives carry a year and an optional month (`--month` in `worldgen directive`); `sim_step` takes `months` as well as `years`.
- **Transport map:** roads fill each province by quality, railways show as stripes over them, and stations, junctions, ports and airports are icons over the map.

## 0.2.0-beta.2

Core performance and project reliability improvements.

- **Safer saves and loading:** project files are replaced atomically one file at a time; corrupt edit JSON is reported rather than silently discarded. Cached field paths and lengths are validated.
- **Lower generation overhead:** reuse graph edge geometry, climate grids and culture-search scratch storage; avoid copying large political tables for status updates and allocating per-cell climate transport weights.
- **Lighter undo/redo:** history stores the affected edit component rather than copying every layer for each action.
- **Build compatibility:** replace the approximate polar constant that caused current Clippy to fail.
- **Regression coverage:** add reproducible CLI tests for generation, determinism, save/load, editing, imports, exports and seed sweeps, plus a climate-cache regression test.
- **Windows release packaging:** the installer uses this release's version; releases remain drafts until both the installer and portable ZIP have been uploaded. Release notes include the matching changelog entry.

## 0.2.0-beta.1

Stage 4: nations and history, and history you can steer.

- **Nations and history** (Stage 4): polities form where people are many and grow over the culture map up to a start date (1949 by default: late in the second great war's era, early in the cold war's): they settle empty land, fight over borders, colonise overseas, break apart along culture lines and assimilate their provinces. Borders can cut through states; colonies and exclaves are allowed. Railways link the largest cities in the industrial era, with stations that grow railway towns. New *Nations* and *Railways* layers, a Nations card, and `nations.png`, `railways.png`, `nations.csv`, `nation_events.csv`, `railways.csv` and `province_nations.csv` in the export.
- **Technology and eras per nation** (Stage 4): each nation's technology follows its wealth per head and its size, and spreads from its neighbours, so rich and large nations reach each era first: gunpowder, ocean shipping, industry, synthetic fertilizer (farmland feeds more people, 1909 in our world), the motor age (highways) and the air age (airports). New *Eras* layer.
- **Economy and transport** (Stage 4): nations tax their people into a treasury that pays the army and builds and keeps roads (track, paved road, highway), railway lines and airports; better roads cost more to build and keep, and building costs follow wages. Roads, railways (with a time penalty for changing lines at junctions), sea lanes and flights bind a nation's provinces to its capital: well-connected provinces pay more, assimilate faster, draw migrants, and extend the nation's reach. Nations deep in debt go bankrupt and close lines. New directives *build road*, *airport*, *subsidy* and *tech*; *railway* now costs money. New *Roads, rail & air* layer, `transport.png`, `roads.csv`, `stations.csv` and `airports.csv`, and era, treasury, income, integration and infrastructure columns in `nations.csv`.
- **Step by step** (Stages 3 and 4): a panel on the map runs the simulation a generation or a few years at a time, to the next era or to the end, or plays it while the map updates. *Finish and keep* keeps the history.
- **Directives:** steer a live run (a fertile age, isolation, a schism, a plague, a migration wave; aggression, war, peace, stability, a split, a founding, a union, a treaty, a rename, a railway). They are saved with the world and replay exactly.
- **AI guide:** describe the history you want; an LLM steers the live run turn by turn with the same directives and writes a chronicle. Works with the Claude API, the Vercel AI Gateway, OpenAI, OpenRouter, local servers and other OpenAI- or Anthropic-compatible gateways. Keys stay on your computer, never in a world.
- **Cities rise and fall** (Stage 4): capitals grow into metropolises over decades, conquered cities are sacked and devastated (a city fought over again and again empties), stations draw people, and people migrate toward attractive cities. *City pins* (a map tool in a live run, a directive, and a tool of the AI guide) make boom towns, metropolises or abandoned cities anywhere, and can be moved, removed or expire; capitals can be moved. New *City growth* layer, `cities.csv` and `ruins.csv`, and metropolis and ruin events in the chronicle.
- **CLI:** `worldgen guide` (a guided history without the app), `worldgen guide-setup`, `worldgen directive`; `worldgen validate` checks Stage 4 too.

## 0.1.0-beta.3

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
