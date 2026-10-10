# The pipeline: Stages 1–3

Stage 4 has its own document: [nations.md](nations.md). Parameter names below are the UI labels; [parameters.md](parameters.md) lists them with their keys and defaults.

## Stage 1 — Make the planet

### 1. Planet parameters
Radius (default 6,371 km), axial tilt (23.4°), day and year length, orbital eccentricity and perihelion, solar constant (1,361 W/m²), greenhouse strength, target ocean fraction, seed and grid level (6 preview · 7 fast · **8 default** · 9 high detail).

### 2. Continent sketch (`stages/sketch.rs`)
- **Input:** your strokes (*Land*, *Sea*, *Mountains here*, *Erase hint*, and the scatter brushes), or, with no strokes, an automatic sketch of *Auto continents* blobs aiming at the target ocean fraction.
- **Scatter brushes** break a stroke's edge with seeded fractal noise (*Scatter* amount, *Grain* size): ragged coasts, bays, fjords and offshore islands up to one radius beyond the rim. The UI preview uses the same noise as the core.
- **Output fields:** `sketch` (soft land value), `land`, `mountain_hint`.

### 3. Plates (`stages/plates.rs`)
- Continental plates are seeded inside landmasses, oceanic plates fill the oceans; all grow by a noisy weighted flood fill. Some continental plates take adjacent ocean (passive margins), others stop at the coast (active margins). Minor plates form along major boundaries.
- Each plate turns about an Euler pole; *Motion arrows* set the pole, *Plate pins* and *Plate paint* override membership.
- **Output fields:** `plate`, `crust`, `vel_e`, `vel_n`.

### 4. Tectonic relief (`stages/tectonics.rs`)
- Boundaries are classified by relative motion and crust type: collision, subduction (ocean–continent, ocean–ocean), ridge, rift, transform. Each type has a cross-section profile by distance from the boundary.
- Sea floor deepens as 2.6 + 0.35·√age km; hotspot chains trail along plate motion; ridged noise is scaled by tectonic stress; *Sketch fidelity* pulls the coast back to the sketch.
- **Old mountains** come from noise, not plates: clusters where low-frequency noise (*Old mountain cluster size*, 700 km) is highest, covering *Old mountains* (10%) of the continental land, up to *Old mountain height* (1,500 m). Their summits are rounded and their valleys cut (inverted ridged noise), like the Appalachians or the Urals, unlike the high, sharp crests of colliding plates. They fade where young ranges already stand, are added on top of the plate relief, and are kept in their own field, so the *Old mountains* brushes add or remove them without touching plate relief.
- The model is static: it builds today's relief from today's plate motion and never moves your continents.
- **Output fields:** `elevation`, `boundary`, `ocean_age`, `stress`, `old_relief` (metres added by old mountains). Relief brushes (*Raise*, *Lower*, *Smooth*, *Flatten*) and *Import heightmap* override it; *Old mountains*, *Scatter old mountains* and *Flatten old mountains* paint or erase the old mountains only.

### 5. Climate (`stages/climate.rs`, `stages/wind.rs`)
- Seasonal Budyko–Sellers energy balance in sin(latitude), 12 months, with ice-albedo feedback and persistent ice sheets; separate land and ocean heat capacities; maritime air carried downwind; warm and cold coastal currents; 6.5 °C/km lapse rate.
- Winds: three-cell belts with a migrating ITCZ (blended so they never break at cell edges), plus thermal winds — surface-pressure anomalies follow temperature anomalies (plateaus become summer heat lows), smoothed to synoptic scale, then a closed-form friction + Coriolis balance. This gives monsoon reversals, continental winter highs and summer heat lows. Mountains turn upslope wind along the contours.
- Moisture transport with recycling over land, orographic rain and rain shadows, summer convection, convergence rain and Clausius–Clapeyron scaling.
- **Output fields:** monthly `temp`, `precip`, `wind_u`, `wind_v`; `t_mean`, `t_warm`, `t_cold`, `p_ann`, `continentality`, `current_offset`.

### 6. Hydrology and erosion (`stages/hydrology.rs`)
- Below-sea-level water bodies smaller than *Inland sea limit* are basins, not ocean. Rivers cut basin outlets by *Outlet incision*.
- Each lake fills from its deepest point until evaporation (minus rain on it) balances inflow: one that reaches its spill point overflows (fresh), one that doesn't is terminal (salt) with dry basin floor or a playa. The climate is then re-solved with lakes as open water.
- Runoff = P − PET; implicit stream-power plus thermal erosion that keeps the sketched coast. Discharge adds up at confluences; arid rivers lose water (*Channel loss*). Width follows w = a·Q^b (default a = 5, b = 0.5: ~110 m for a Seine, ~900 m for a Yangtze). Seeded micro-relief lets plains rivers branch and merge. Rivers shorter than *Minimum river length* are dropped.
- **Output fields:** corrected `elevation`, `water`, `lake`, `water_level`, `discharge`, `drainage_area`, `river`, `river_rank`, `river_width`, `receiver`, `erosion`, `temp_adjust`. The card lists each large lake's water balance.

### 7. Biomes (`stages/biomes.rs`)
Köppen–Geiger classes from monthly temperature and precipitation (Peel et al. rules), then 18 game terrains (glacier to jungle, mountains, floodplains) that also use relief, rivers and lakes. **Output fields:** `koppen`, `terrain`, `relief`. *Biome paint* overrides it.

## Stage 2 — States and provinces

### 8. Habitability and barriers (`stages/habitability.rs`)
- **Habitability (0–1):** growing season (months above ~6 °C), water (rain, or a river within *River reach*: irrigation), heat, slope, altitude, terrain, a bonus near rivers and coasts, plus *Fertile land* paint (−1 to +1).
- **Groundwater and springs:** a share of rain above 150 mm/yr soaks in and flows downhill underground (keeping 1/e over *Groundwater reach*), surfacing below steep rises and on basin floors, playas and salt-lake shores. In dry land away from rivers that is an oasis: its site value raises habitability up to *Oasis habitability*. *Site pins* place towns no model explains, with a target population.
- **Barriers:** crossing cost from ridge crests, high ground, deep desert away from rivers, ice and marsh, plus *Add/Remove barrier* paint. Rivers take one of two roles: a wide river through habitable land is a **border** (Rhine); a river through dry land is a **backbone** (Nile) that holds its valley together.
- **Output fields:** `habitability`, `barrier`, `river_role`, `groundwater`, `site`, `site_kind`, `fertility`.

### 9. States (`stages/states.rs`, `stages/partition.rs`)
- Seeds by Poisson-disc sampling weighted by habitability; the radius shrinks on fertile land (river-valley states ~5× smaller than steppe states), tuned by bisection to land area ÷ *Mean state area*.
- All seeds grow together (multi-source Dijkstra, step cost = length × (1 + barrier)), so neighbours meet on ridges, border rivers and deserts. Water is crossed at *Sea crossing cost*; islands over *Own state from island size* get their own state.
- Clean-up merges fragments and undersized states. Regions (a few neighbouring states, farthest-point seeding, four relaxations) and continents (landmasses over *Continent size*) complete the hierarchy.
- Names come from a random language per continent with a dialect per region.
- **Output fields:** `state`, `region`, `continent`; tables of states, regions, continents.

### 10. Provinces (`stages/provinces.rs`, `stages/resources.rs`)
- Inside each state, seeds are spaced so province area runs from *Province area, fertile* to *Province area, barren*; growth uses the barrier-aware search at a lower weight, then Lloyd relaxation.
- Ice, high mountains and very poor land become **wasteland** provinces (inside their state). The sea is split into coastal, shelf and open-ocean zones; large lakes become lake provinces.
- **Resources:** copper, gold and silver in mountain belts; iron (and some gold) in old shields; coal in humid lowland basins; salt in dry basins; oil in sedimentary basins and salt domes; fish on coastal and shelf seas. **Trade goods:** a plantation-era cash crop where climate fits (rubber, coffee, sugar, tea, cotton, silk, tobacco), otherwise the staple of the dominant terrain. About a fifth of land provinces grow a cash crop and ~4% have oil on the default world.
- Strait crossings are found, and every border between two provinces is typed (land, river, impassable, coast, lake, sea, strait) with its length and crossing cost.
- **Output fields:** `province`, `province_kind`, `state`, `trade_good`; the province, adjacency, state, region and continent tables.

## Stage 3 — Cultures

### 11. Cultures (`stages/cultures.rs`, `community.rs`)
An agent simulation on the province graph, one generation (~25 years) at a time (`CultureSim`), so the same code serves full and step-by-step runs.

- **Bands:** groups of people with a home province, a population, 24 cultural traits (Axelrod's model, 4 values each) and an empty polity slot. A province holds people in proportion to area × habitability^1.5 (averaged over its cells, so oases count).
- **Founders:** random, or *Founding band* pins (a culture of N bands at a point).
- **Travel** follows province borders at cost length × (1 + barrier); four eras open straits, coastal sailing and the open sea and widen the range.
- **One generation:** logistic growth; a full province sends a band to the nearest unsettled province; contacts weighted by population and falling off over *Contact distance*; a contact succeeds with probability similarity^*Like seeks like* and copies a trait; *Conformity* adopts the value most successful contacts share; random *Drift*; contacts fade.
- **Cultures** are found every 10 generations by Louvain community detection (*Culture detail*) on the band graph (recent contact × similarity²), matched to previous cultures by population overlap. A change counts only after it persists for *Checks before a change counts* checks, so cultures don't flicker. Splits, merges and extinctions are logged: the family tree. **Culture groups** are a coarser Louvain pass.
- **Settlements beyond the climate:** from era 2 caravan routes between the 60 most populous provinces give watered stops in dry land up to *Caravan stop size*; from era 3 each metal deposit draws *Mining town size* people; from era 4 dry river provinces irrigate *Irrigated share*. Site pins reach their population in era 4. *Attraction* paint (−1 ghost town … +1 metropolis) scales capacity and steers settlers all through the run.
- **Names:** first cultures speak their region's dialect; daughters start from a changed copy of the parent's. Land provinces and states are renamed in their culture's language.
- **Directives** (step by step): growth, attraction, isolate, drift, contact, catastrophe, settle — see [steering.md](steering.md).
- **Output fields:** `culture`, `culture_group`, `population`, `attraction`; tables of cultures (with parent, fate, years), groups, events and per-province shares.
- Differences from the proposal: Louvain instead of Leiden (the persistence rule covers the flicker Leiden would reduce); homophily and conformity were added because without them neighbours agree on only about half their traits.
