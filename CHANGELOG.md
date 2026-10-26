# Changelog

## Unreleased

Rivers run between cells, like Civilization V's.

- **Rivers along cell edges:** water still flows from cell to cell, but each river's line now runs along the boundaries between cells, from corner to corner. A river is the line between the two cells on its banks, both of which count as riverside (water, floodplains, harbours).
- **Rivers as borders:** a border river's crossing cost sits on the edge it runs along, so where a state or province border follows a river, it is the river itself, no longer a line beside it that switches banks from cell to cell. Border rivers divide provinces and states at least as often as before (on seeds 1 and 5: 76–78% of their length divides provinces, 49–63% states). Province borders along a river are typed `river` from the river edges themselves.
- **Barriers layer:** border and backbone rivers are drawn on their edges; *Remove barrier* turns off the border role of the rivers along the erased cells.
- **Export:** `rivers.png` and `rivers.geojson` follow the cell edges, and `provinces.png` borders are not warped next to rivers, so borders along rivers line up with the river pixels.
- Hydrology, habitability, states and provinces change for the same seed; projects store four more per-corner river fields (about 17 MB at grid level 8).

## 0.3.1

Mountains now shape temperature as well as rain.

- **Foehn:** air that rains out climbing a range comes down the far side warmer. The lee of high, wet ranges is up to about 4 °C warmer in a month (most in the wet season), fading over a few hundred km; windward slopes and crests are unchanged, and so is rain. New *Foehn warming* layer, `foehn` field and *Foehn ×* setting (0–3, default 1); the status line shows the warming.
- **Mountains block sea air:** mild maritime air stays on the windward side of a range, so the land behind it has hotter summers and colder winters (larger annual range) than the coast before it; windward slopes and summits stay maritime. *Mountains block sea air* setting (0–3, default 1: half the maritime influence lost per 2 km of descent).
- **More rain and snow on windward mountains:** wet air forced up a slope now turns vapour into cloud that rains out over the whole windward slope and crest instead of a thin line at the foot, and sinking air on the far side dries what is left. Rain is also placed on slopes facing the wind at full map detail, not just on the coarser climate grid. Windward mountains get about 1.5–1.9× the rain they would without orographic rain (1.3× before), lee slopes stay dry, and no month exceeds ~2,500 mm (the wettest places reached 18,000–27,000 mm a year before, about 15,000 now). *Orographic rain ×* scales it.
- Climates, and everything after them, change slightly for the same seed.

## 0.3.0

The 0.3.0 line out of beta: everything from 0.3.0-beta.1 to 0.3.0-beta.4, with no changes since 0.3.0-beta.4. For those updating from 0.2.0-beta.2:

- **Stage 4 history:** EU4-style institutions that open each era and spread over roads, railways, ports and airports; ports; tags on the world, nations and states; feudal empires; nations that already exist in the first year, with nation seeds to place them.
- **Finer time:** the Stage 4 clock counts whole months, with steps of 24, 12 and 6 months as the present nears (adjustable).
- **Stage 1 relief:** old, worn mountain clusters from noise, with brushes to add or flatten them.
- **Maps and editing:** a transport map with railway stripes and icons; info tags that choose what the status line shows; a two-column, foldable legend; fresh results no longer hidden by stale ones, and *Clear stale* / *Clear from here* to drop old results.

See the beta entries below for details.

## 0.3.0-beta.4

Edits to early steps show at once: stale results no longer hide fresh ones, and old results can be cleared.

- **Old results no longer hide new ones:** after changing the sketch, plates or seed and re-running only the early steps, the map showed the old eroded elevation (and lakes, rivers) of the stale Hydrology step, so edits seemed to have no effect. Map layers and the status line now read from the newest step that is up to date.
- **Clearing results:** *Clear stale (N)* in the top bar removes every result whose inputs changed; *Clear from here* in a step card removes that step's result and every later one. Edits and parameters stay; the next run rebuilds them. API: `clear_results`.

## 0.3.0-beta.3

Nations that already exist in the first year, and nation seeds to choose them.

- **Nations at the start:** some polities already exist in the first year. Place **nation seeds** (Nations card, *Nation seed*, X) with a name and a size in provinces to choose where they are; *Nations at the start* (12) more are placed by chance in the most populous provinces, spaced apart, each of *Their size* (6) provinces. Seeds are a new override layer, `nation_seeds`, that feeds Stage 4 only.

## 0.3.0-beta.2

A tidier status line and legend.

- **Info tags in the status line:** choose which kinds of cell information are shown (relief, climate, water, land, province, culture, nation, transport), or let *Auto* pick what fits the current map layer; hover the status line for everything. The line no longer runs off the screen.
- **Legend:** long lists use two columns and are no longer cut off after 24 entries; click the title to fold the legend away.

## 0.3.0-beta.1

Institutions, ports, feudal empires and finer time steps for Stage 4; old mountains in Stage 1.

- **Old mountains:** besides the high, sharp ranges of plate boundaries, noise now raises worn, rounded mountain clusters away from them (share of land, height and cluster size are parameters). *Old mountains*, *Scatter old mountains* and *Flatten old mountains* brushes in the relief card add or remove them without touching plate relief; new *Old mountains* layer and `old_relief` field.
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
