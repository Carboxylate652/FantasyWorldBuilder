# Parameter reference

Every parameter of the twelve steps, with its default and meaning. This file is generated from `core/src/params.rs` (doc comments and `Default` impls) and the UI labels in `app/src/schema.ts`; regenerate it when parameters change. In `world.json` the parameters live under `params.<section>.<name>`; a missing value takes its default.

## Planet (step 1)

| Name | UI label | Default | Meaning |
| --- | --- | --- | --- |
| `seed` | Seed (0–4294967295) | `1` | — |
| `grid_level` | — | `8` | Icosphere subdivision level (6 = preview, 7 = fast, 8 = default, 9 = high detail). |
| `radius_km` | Radius (km) (1000–20000) | `6371.0` | — |
| `axial_tilt_deg` | Axial tilt (°) (0–90) | `23.4` | — |
| `day_length_h` | Day length (h) (4–240) | `24.0` | Faster rotation narrows the wind cells and the poleward heat transport. |
| `year_length_days` | Year length (days) (30–3000) | `365.25` | — |
| `eccentricity` | Eccentricity (0–0.5) | `0.0167` | — |
| `perihelion_deg` | Perihelion longitude (°) (0–360) | `283.0` | Solar longitude of perihelion, degrees (Earth ≈ 283°, i.e. early January). |
| `solar_constant` | Solar constant (W/m²) (600–2500) | `1361.0` | — |
| `greenhouse` | Greenhouse strength (0.1–8) | `1.0` | 1.0 = Earth. Each doubling adds about 30 W/m² of extra greenhouse forcing. |
| `ocean_fraction` | Target ocean fraction (0.05–0.95) | `0.70` | Used when the continent sketch is generated automatically. |

## Continent sketch (step 2)

| Name | UI label | Default | Meaning |
| --- | --- | --- | --- |
| `coast_roughness` | Coast roughness (0–1.5) | `0.55` | Amplitude of coastline noise added to soft sketch edges (0 = exact brush edges). |
| `coast_scale_km` | Coast feature size (km) (100–3000) | `900.0` | Feature scale of the coastline noise, in km. |
| `auto_continents` | Generated continents (0–12) | `5` | Number of continents when the sketch is generated automatically (0 = noise only). |

## Plates (step 3)

| Name | UI label | Default | Meaning |
| --- | --- | --- | --- |
| `major_plates` | Major plates (2–40) | `12` | — |
| `minor_plates` | Minor plates (0–80) | `20` | — |
| `continental_plate_area_mkm2` | Land per continental plate (M km²) (2–120) | `22.0` | Target land area per continental plate (million km²). |
| `shelf_width_km` | Continental shelf (km) (0–600) | `120.0` | Width of continental shelf/crust beyond the coastline (km). |
| `speed_scale` | Plate speed × (0.1–4) | `1.0` | Scales all plate speeds. |
| `edge_roughness` | Edge roughness (0–1.5) | `0.6` | Irregularity of plate edges (0 = smooth Voronoi, 1 = very ragged). |

## Tectonic relief (step 4)

| Name | UI label | Default | Meaning |
| --- | --- | --- | --- |
| `sketch_fidelity` | Sketch fidelity (0–1) | `0.85` | 0 = pure tectonic result, 1 = the sketched coastline always wins. |
| `mountain_scale` | Mountain height × (0.1–3) | `1.0` | — |
| `detail_scale` | Detail noise × (0–3) | `1.0` | — |
| `hotspots` | Hotspots (0–40) | `8` | — |
| `hint_height_m` | Mountain hint height (m) (0–8000) | `3200.0` | Strength of "mountains here" hint strokes (metres at full brush). |
| `max_influence_km` | Boundary influence (km) (300–4000) | `1600.0` | — |

## Climate (step 5)

| Name | UI label | Default | Meaning |
| --- | --- | --- | --- |
| `climate_level` | — | `7` | Grid level the climate is solved on (capped at the world level). |
| `lapse_rate_c_per_km` | Lapse rate (°C/km) (0–12) | `6.5` | — |
| `global_precip_mm` | Global precipitation (mm/yr) (100–3000) | `1000.0` | Global mean precipitation at Earth temperature (mm/year). |
| `ocean_current_c` | Coastal currents (°C) (0–15) | `5.0` | Strength of warm/cold coastal current offsets (°C at full effect). |
| `maritime_reach_km` | Maritime reach (km) (100–5000) | `1400.0` | How far maritime air reaches inland along the wind (km). |
| `orographic` | Orographic rain × (0–4) | `1.0` | Orographic rain strength. |
| `moisture_passes` | Moisture passes (20–300) | `90` | — |
| `heat_diffusion` | Poleward heat transport (0.1–2) | `0.68` | Heat transport toward the poles (W/m²/K, North's D). |
| `thermal_wind` | Thermal winds × (0–3) | `1.0` | Strength of winds driven by thermal pressure systems (monsoons, continental highs). |
| `mountain_blocking` | Mountain blocking (0–1) | `0.7` | Fraction of upslope wind turned along the contours of steep terrain. |

## Hydrology and erosion (step 6)

| Name | UI label | Default | Meaning |
| --- | --- | --- | --- |
| `river_threshold_m3s` | River threshold (m³/s) (5–5000) | `300.0` | Discharge above which a cell is a river (m³/s). |
| `erosion_passes` | Erosion passes (0–20) | `4` | — |
| `erosion_strength` | Erosion strength (0–5) | `1.0` | — |
| `thermal_passes` | Thermal passes per pass (0–10) | `3` | — |
| `talus_m_per_km` | Talus slope (m/km) (5–300) | `45.0` | Talus slope for thermal erosion (metres of drop per km). |
| `lake_min_depth_m` | Minimum lake depth (m) (1–500) | `30.0` | Basins shallower than this after outlet incision hold no lake (m). |
| `width_coeff` | River width coefficient (0.5–30) | `5.0` | River width w = width_coeff · Q^width_exponent (m, with Q in m³/s). |
| `width_exponent` | River width exponent (0.3–0.7) | `0.5` | River width w = width_coeff · Q^width_exponent (m, with Q in m³/s). |
| `min_river_length_km` | Minimum river length (km) (0–1000) | `120.0` | River systems shorter than this (source to mouth) are not drawn as rivers. |
| `channel_loss` | Channel loss per 100 km (0–0.2) | `0.02` | Share of a river's flow lost per 100 km across fully arid land. |
| `breach_depth_m` | Outlet incision (m) (0–1000) | `150.0` | How far outlet rivers cut into basin rims (m); shallower basins drain completely. |
| `open_water_evap` | Open-water evaporation × (0.5–2.5) | `1.25` | Open-water evaporation as a multiple of land potential evapotranspiration. |
| `inland_sea_max_mkm2` | Inland sea limit (M km²) (0–20) | `1.0` | Below-sea-level water bodies smaller than this (million km²) that are not the main ocean are inland basins whose level follows their water balance. |
| `lake_climate_feedback` | — | `1` | Times the climate is re-solved with lakes as open water (0 = off). |

## Biomes (step 7)

| Name | UI label | Default | Meaning |
| --- | --- | --- | --- |
| `cd_threshold_c` | — | `0.0` | Coldest-month threshold between C and D climates (0 °C or -3 °C). |
| `mountain_elev_m` | Mountain elevation (m) (300–6000) | `1800.0` | — |
| `hill_relief_m` | Hill relief (m) (50–2000) | `300.0` | — |
| `mountain_relief_m` | Mountain relief (m) (100–4000) | `900.0` | — |
| `forest_precip_mm` | Forest precipitation (mm/yr) (100–3000) | `650.0` | Annual precipitation above which temperate land is forest rather than plains. |

## Habitability and barriers (step 8)

| Name | UI label | Default | Meaning |
| --- | --- | --- | --- |
| `river_reach_km` | River reach (km) (5–300) | `60.0` | Distance over which a river still gives water and transport (km). |
| `coast_reach_km` | Coast reach (km) (5–500) | `120.0` | Distance over which the coast still adds to habitability (km). |
| `barrier_ridge` | Ridge crest barrier (0–20) | `4.0` | Barrier strength of ridge crests (cells higher than their surroundings). |
| `barrier_mountain` | High ground barrier (0–20) | `2.5` | Barrier strength of high ground in general. |
| `barrier_river` | Border river barrier (0–20) | `5.0` | Barrier strength of border rivers. |
| `barrier_desert` | Desert barrier (0–20) | `2.0` | Barrier strength of deep desert away from rivers. |
| `barrier_ice` | Ice barrier (0–30) | `10.0` | Barrier strength of ice caps. |
| `barrier_marsh` | Marsh barrier (0–20) | `1.5` | Barrier strength of marsh (jungle counts for 60 %). |
| `river_border_width_m` | Border river width (m) (20–3000) | `300.0` | Rivers at least this wide (m) can act as borders. |
| `river_border_habitability` | Border river habitability (0–1) | `0.3` | Wide rivers through land at least this habitable divide it (Rhine). |
| `backbone_max_precip_mm` | Backbone river rainfall (mm/yr) (0–1500) | `350.0` | Rivers through land drier than this (mm/yr, regional) hold their valley together instead (Nile). |
| `groundwater_recharge` | Groundwater recharge (0–1) | `0.15` | Share of rain above 150 mm/yr that soaks in and becomes groundwater. |
| `groundwater_reach_km` | Groundwater reach (km) (20–3000) | `400.0` | Groundwater flows downhill underground; this far (km) it keeps 1/e of its water. |
| `spring_flux_m3s` | Spring flow for a full oasis (m³/s) (0.05–50) | `1.0` | Spring flow (m³/s) that makes a full oasis. |
| `spring_max_precip_mm` | Springs matter below (mm/yr) (0–1500) | `350.0` | Springs only matter in land drier than this (mm/yr). |
| `spring_habitability` | Oasis habitability (0–1) | `0.7` | Habitability of a full oasis. |

## States (step 9)

| Name | UI label | Default | Meaning |
| --- | --- | --- | --- |
| `state_area_km2` | Mean state area (km²) (10000–2000000) | `150_000.0` | Mean land area per state (km²). |
| `min_state_area_km2` | Minimum state area (km²) (0–500000) | `20_000.0` | States smaller than this merge into the neighbour they share most border with. |
| `habitability_density` | Smaller states on fertile land (0–1) | `0.8` | 0 = equal-sized states, 1 = much smaller states on fertile land. |
| `size_variation` | Size variation (0–2) | `0.35` | Random head start of each seed, as a share of the state radius (size variety). |
| `barrier_weight` | Barrier weight (0–5) | `1.0` | Multiplies every barrier cost. |
| `sea_crossing` | Sea crossing cost (1–50) | `8.0` | Cost of crossing water per km, relative to open land. |
| `island_state_km2` | Own state from island size (km²) (0–1000000) | `40_000.0` | Islands at least this large (km²) become states of their own. |
| `states_per_region` | States per region (1–30) | `6.0` | — |
| `continent_min_mkm2` | Continent size (M km²) (0.1–50) | `2.0` | Landmasses at least this large (million km²) are continents. |

## Provinces (step 10)

| Name | UI label | Default | Meaning |
| --- | --- | --- | --- |
| `province_area_km2` | Province area, fertile (km²) (1000–200000) | `12_000.0` | Province area on fully habitable land (km²). |
| `sparse_province_area_km2` | Province area, barren (km²) (1000–1000000) | `150_000.0` | Province area on barely habitable land (km²). |
| `min_province_area_km2` | Minimum province area (km²) (0–100000) | `2_500.0` | Smaller provinces merge into a neighbour in the same state. |
| `barrier_weight` | Barrier weight (0–5) | `0.4` | Barrier weight inside states (lower than between states). |
| `lloyd_passes` | Relaxation passes (0–8) | `2` | — |
| `wasteland_habitability` | Wasteland below habitability (0–0.5) | `0.04` | Land below this habitability (in large patches) becomes wasteland. |
| `wasteland_elev_m` | Wasteland above (m) (500–9000) | `3200.0` | Mountains above this elevation become wasteland. |
| `wasteland_area_km2` | Wasteland province area (km²) (5000–2000000) | `120_000.0` | Mountains above this elevation become wasteland. |
| `coastal_band_km` | Coastal sea band (km) (20–1000) | `180.0` | Sea within this distance of land is coastal sea. |
| `shelf_depth_m` | Shelf depth (m) (50–6000) | `1500.0` | Sea shallower than this (outside the coastal band) is shelf sea. |
| `coastal_sea_km2` | Coastal sea zone (km²) (2000–1000000) | `45_000.0` | — |
| `shelf_sea_km2` | Shelf sea zone (km²) (5000–3000000) | `160_000.0` | — |
| `open_sea_km2` | Open ocean zone (km²) (20000–10000000) | `900_000.0` | — |
| `lake_province_km2` | Lake province from (km²) (100–500000) | `6_000.0` | Lakes at least this large get their own province. |
| `max_strait_km` | Longest strait crossing (km) (0–1000) | `160.0` | Longest water crossing written to adjacencies.csv (km). |

## Cultures (step 11)

| Name | UI label | Default | Meaning |
| --- | --- | --- | --- |
| `initial_bands` | Founding bands (1–1000) | `20` | Bands placed at the start, weighted by carrying capacity. |
| `max_bands` | Band limit (100–20000) | `5000` | Band count cap (never below the number of provinces that can hold people). |
| `ticks` | Generations (20–2000) | `400` | Simulation length in ticks (one tick = one generation). |
| `years_per_tick` | Years per generation (10–50) | `25.0` | — |
| `traits` | — | `24` | Cultural traits per band (Axelrod features) and values per trait. |
| `trait_values` | — | `4` | — |
| `density_per_km2` | People per km² (fertile land) (0.1–100) | `2.0` | People per km² a fully habitable province supports. |
| `growth_rate` | — | `0.3` | Logistic growth rate per tick. |
| `fission_mutation` | — | `0.03` | Chance per trait that a band founded by fission starts with a changed value. |
| `drift` | Drift per trait (0–0.02) | `0.0005` | Chance per trait and tick of a random change. |
| `contacts_per_tick` | — | `4` | Contacts each band tries per tick. |
| `homophily` | Like seeks like (0–10) | `3.0` | A contact succeeds with probability similarity^homophily: higher values make unlike bands avoid each other, so cultures form sharper borders. |
| `conformity` | Conformity (0–1) | `1.0` | Chance per tick that a band takes, for one trait, the value most of its successful contacts share (conformist transmission). |
| `contact_memory` | — | `0.9` | Contact weight kept from one tick to the next. |
| `contact_scale_km` | Contact distance (km) (20–2000) | `200.0` | Contact falls off as exp(−cost / scale): most contact is within about this distance. |
| `travel_range_km` | — | `[500.0, 800.0, 1200.0, 1800.0]` | Travel range (km of open land) in each of the four eras: the farthest a new band settles or a contact reaches. |
| `era_starts` | — | `[0.25, 0.5, 0.75]` | Start of eras 2–4 (straits, coastal sailing, open sea) as a fraction of the run. |
| `barrier_weight` | Barrier weight (0–5) | `1.0` | Cost multiplier for barriers along a border. |
| `sea_cost` | Sea travel cost (0.1–5) | `0.7` | Cost per km at sea (relative to open land) once sailing is possible. |
| `check_every` | — | `10` | Ticks between culture checks. |
| `persistence` | Checks before a change counts (1–10) | `3` | Checks in a row a split, merge or change of culture must hold. |
| `culture_resolution` | Culture detail (0.5–20) | `4.0` | Community detection resolution for cultures (higher = more, smaller cultures). |
| `group_resolution` | Group detail (0.005–2) | `0.05` | Resolution for culture groups (lower = fewer, larger groups). |
| `min_culture_share` | — | `0.002` | Smallest share of world population a new culture can start with. |
| `caravan_people` | Caravan stop size (people) (0–500000) | `25_000.0` | From the second era: people a watered stop on the busiest caravan route across dry land can hold (less on quieter routes). |
| `mining_people` | Mining town size (people) (0–200000) | `4_000.0` | From the third era: people each metal deposit draws to a mining town. |
| `irrigation_share` | Irrigated share of dry river land (0–1) | `0.3` | From the fourth era: share of a dry province's land that irrigation from its river turns into farmland. |

## Nations and history (step 12)

| Name | UI label | Default | Meaning |
| --- | --- | --- | --- |
| `start_year` | First polities (year) (-5000–1800) | `800` | Year the first polities can form. |
| `start_date` | Start date (year) (1800–2000) | `1949` | Year history stops: the map's start date (default 1949: late in the second great war's era, early in the cold war's; 1910–1920 for a pre-war start). |
| `years_per_step` | Years per step (1–25) | `2` | Years per simulation step. |
| `found_population` | People to found a polity (1000–1000000) | `60_000.0` | People a province needs before a polity can form there. |
| `found_rate` | Founding chance per step (0–0.05) | `0.0015` | Chance per step that a province with that many people and no ruler founds a polity (more people, likelier). |
| `expansion_rate` | Expansion (0–5) | `0.8` | Expansion attempts per nation and step at aggression 1. |
| `barrier_weight` | Barrier weight (0–5) | `1.0` | Cost multiplier for barriers along a border (ridges, rivers, desert). |
| `culture_weight` | Culture border weight (0–10) | `1.5` | Extra cost of taking land of another culture (half for the same group). |
| `reach_km` | Reach from the capital (km) (100–5000) | `700.0` | Distance from the capital (km) at which expansion costs twice as much. |
| `collapse_rate` | Breakups (0–0.2) | `0.01` | Chance per step of a breakup at instability 1 (mixed cultures, size, spread). |
| `assimilation` | Assimilation per year (0–0.02) | `0.0015` | Share of a province's other cultures that takes its ruler's culture per year. |
| `growth` | — | `0.0018` | Population growth per year before and during the industrial era. |
| `industrial_growth` | — | `0.009` | Population growth per year before and during the industrial era. |
| `gunpowder_year` | Gunpowder (technology year) (0–2000) | `1450` | Era technology years: gunpowder states, ocean shipping (overseas colonies), industry (railways, faster growth). Each nation enters an era when its own technology reaches the year, not the calendar. |
| `shipping_year` | Ocean shipping (technology year) (0–2000) | `1500` | Era technology years: gunpowder states, ocean shipping (overseas colonies), industry (railways, faster growth). Each nation enters an era when its own technology reaches the year, not the calendar. |
| `industrial_year` | Industry (technology year) (0–2000) | `1830` | Era technology years: gunpowder states, ocean shipping (overseas colonies), industry (railways, faster growth). Each nation enters an era when its own technology reaches the year, not the calendar. |
| `overseas_km` | Colony range (km) (500–20000) | `9000.0` | Farthest a colony can lie from its nation's coast (km), with ocean shipping. |
| `railway_cities` | Cities linked by rail per nation (2–40) | `8` | Largest cities each nation links by rail. |
| `railway_every_years` | Years between railway projects (1–100) | `8` | Years between a nation's railway projects. |
| `station_people` | People per railway station (0–200000) | `15_000.0` | People a railway station draws (railway towns, also in the desert). |
| `capital_pull` | Capital pull (0–1) | `0.5` | Attraction (0–1) a capital grows into: people move there and it holds more. |
| `capital_years` | Years to grow a capital (1–300) | `60.0` | Years a new capital takes to grow into its full pull. |
| `war_sack` | Sack (share of people lost) (0–0.5) | `0.03` | Share of a conquered province's people lost in the sack (four times for a capital). |
| `war_devastation` | War devastation (0–1) | `0.25` | Attraction lost by a conquered province (devastation: people leave). |
| `recovery` | Recovery per year (0–0.2) | `0.03` | Share of devastation that heals each year. |
| `migration` | Migration to cities per year (0–0.05) | `0.002` | Share of a nation's people that move each year toward its attractive provinces (capitals, stations, city pins), more from devastated land. |
| `fertilizer_year` | Synthetic fertilizer (technology year) (0–2100) | `1909` | Technology years at which synthetic fertilizer, the motor age (highways) and the air age (airports) begin. Like the other era years, a nation reaches them when its own technology does: rich, large and well-connected nations first, others as ideas spread to them. |
| `motor_year` | Motor age (technology year) (0–2100) | `1920` | Technology years at which synthetic fertilizer, the motor age (highways) and the air age (airports) begin. Like the other era years, a nation reaches them when its own technology does: rich, large and well-connected nations first, others as ideas spread to them. |
| `air_year` | Air age (technology year) (0–2100) | `1935` | Technology years at which synthetic fertilizer, the motor age (highways) and the air age (airports) begin. Like the other era years, a nation reaches them when its own technology does: rich, large and well-connected nations first, others as ideas spread to them. |
| `tech_spread` | Technology catch-up (0–0.2) | `0.02` | How much faster a nation catches up per year of technology gap to the level its wealth, size and neighbours allow (0.02: a century behind, three years of progress per year). |
| `tech_lead_years` | Technology lead (years) (0–100) | `10.0` | How far ahead of the calendar the most advanced nation can get (years). |
| `tax` | Tax share (0–0.5) | `0.08` | Share of output collected as taxes (pays the army, roads, railways and airports). |
| `road_cost` | Road cost (0–10) | `1.0` | Multipliers on the cost of building and keeping roads, and railways and airports. |
| `rail_cost` | Railway and airport cost (0–10) | `1.0` | Multipliers on the cost of building and keeping roads, and railways and airports. |
| `transfer_km` | Line change penalty (km) (0–2000) | `150.0` | Time lost changing trains at a junction (km of travel on foot). |
| `fertilizer_boost` | Fertilizer boost (1–4) | `1.6` | How many more people farmland holds once a nation uses synthetic fertilizer. |
| `road_every_years` | Years between road projects (1–100) | `10` | Years between a nation's road projects (and airport projects in the air age). |
