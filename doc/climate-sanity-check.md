# Climate Sanity Check & Earth Diagnostic Report

**Snapshot:** commit `652e5d0` (`climate/earth-refine`, `MODEL_VERSION[Step::Climate] = 8`), compared against baseline `90c2429` (`origin/master-3sczxa`).
**Test input:** `core/examples/earth16.png` (kept on branch `climate/earth-refine`, not in the main line) — NOAA ETOPO 2022 (`ETOPO_2022_v1_60s_N90W180_surface.tif`) converted to `Heightmap16` (`-11000..9000 m`), received as `1024 × 512` 8-bit RGBA (`487 KB`, decoded by `HeightEncoding::Heightmap16` at ~78 m vertical quantization), evaluated with `core/examples/earth_check.rs`.

This document records the current climate state for sanity checking, diagnoses the remaining regional discrepancies against observed Earth normals, and outlines **general-physics** (planet-agnostic) design changes for the next tuning pass. No simulation code is modified here.

---

## 1. Current Earth Report Summary

### Aggregate Metrics (`earth_check`, 133 stations)

| Metric | Baseline `90c2429` (L7) | Current `652e5d0` (L7) | Current `652e5d0` (L8) | Observed Earth |
| :--- | ---: | ---: | ---: | ---: |
| Annual precip factor (bias) | `x2.12` (`x1.17`) | **`x1.95`** (`x0.94`) | **`x1.91`** (`x0.92`) | `x1.00` |
| Köppen group / exact match | `64%` / `20%` | **`68%` / `31%`** | **`67%` / `29%`** | — |
| January temp MAE (bias) | `4.0 °C` (`+0.8`) | `4.2 °C` (`+1.3`) | `4.1 °C` (`+1.3`) | — |
| July temp MAE (bias) | `3.0 °C` (`-1.7`) | `3.1 °C` (`-1.9`) | `3.0 °C` (`-2.0`) | — |
| Zonal P factor / T MAE | `x1.74` / `1.8 °C` | **`x1.67`** / `2.2 °C` | **`x1.67`** / `2.2 °C` | — |
| Köppen land shares `A/B/C/D/E` | `19/26/23/20/12` | `17/33/20/16/14` | `17/32/20/16/14` | `19/30/14/24/13` |
| Generated fantasy seed 1 (L7) | — | `A 19 / B 30 / C 17 / D 16 / E 18` | — | `19/30/14/24/13` |

### Zonal Means (Level 7, `652e5d0`)

| Lat band | T sim (°C) | T obs (°C) | T err | P sim (mm) | P obs (mm) | Land P (mm) |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: |
| `-90..-80` | -44.1 | -37.0 | -7.1 | 3 | 80 | 3 |
| `-80..-70` | -33.0 | -20.0 | -13.0 | 52 | 200 | 43 |
| `-70..-60` | -10.8 | -8.5 | -2.3 | 259 | 600 | 123 |
| `-60..-50` | 1.9 | 1.2 | +0.7 | 724 | 1000 | 731 |
| `-50..-40` | 10.8 | 8.9 | +1.9 | 1149 | 1050 | 1067 |
| `-40..-30` | 17.9 | 15.2 | +2.7 | 832 | 850 | 565 |
| `-30..-20` | 22.2 | 20.6 | +1.6 | 529 | 700 | 657 |
| `-20..-10` | 24.0 | 24.1 | -0.1 | 1296 | 1150 | 1203 |
| `-10..0` | 25.5 | 25.8 | -0.3 | 2085 | 1500 | 1943 |
| `0..10` | 25.6 | 26.5 | -0.9 | 2073 | 2000 | 2065 |
| `10..20` | 24.4 | 26.0 | -1.6 | 1289 | 1100 | 1258 |
| `20..30` | 22.3 | 22.9 | -0.6 | 562 | 650 | 525 |
| `30..40` | 16.9 | 17.2 | -0.3 | 808 | 800 | 570 |
| `40..50` | 11.9 | 10.0 | +1.9 | 972 | 900 | 609 |
| `50..60` | 4.1 | 2.3 | +1.8 | 578 | 800 | 420 |
| `60..70` | -7.1 | -6.0 | -1.1 | 224 | 500 | 203 |
| `70..80` | -13.5 | -14.0 | +0.5 | 125 | 250 | 90 |
| `80..90` | -16.0 | -18.0 | +2.0 | 110 | 150 | 96 |

### What Improved Strongly vs `90c2429`

- **Humid subtropical east sides:** Wuhan `240 → 886 mm` (real `1300`), Dallas `86 → 997 mm` (`950`), Houston `397 → 1171 mm` (`1250`), Atlanta `1136 → 1243 mm` (`1270`), St Louis `404 → 650 mm` (`1040`).
- **West coasts & Mediterranean:** Madrid `1159 → 905 mm` (`420`), Rome `2135 → 1326 mm` (`800`), Algiers `1272 → 610 mm` (`600`, exact `Csa`), Casablanca `1393 → 594 mm` (`420`, exact `Csa`), Cape Town `1089 → 531 mm` (`515`, exact `Csb`), Vancouver `2726 → 1156 mm` (`1200`), San Francisco `1829 → 1089 mm` (`600`, exact `Csb`), Los Angeles `2000 → 1089 mm` (`380`).
- **Monsoon-adjacent deserts (partial fix):** Alice Springs `1257 → 297 mm` (`280`, exact `BWh`), Riyadh `982 → 416 mm` (`100`), Khartoum `774 → 468 mm` (`120`), Cairo `809 → 406 mm` (`25`), Baghdad `247 → 80 mm` (`150`, exact `BWh`).
- **Coldest continental winters:** Yakutsk Jan `-21.4 → -37.6 °C` (real `-38.6`), Verkhoyansk Jan `-17.9 → -32.0 °C` (`-45.0`), Utqiagvik `-15.7 → -25.8 °C` (`-25.0`), Vostok Jan `-28.4 → -33.8 °C` (`-32.0`), South Pole Jan `-23.7 → -28.9 °C` (`-28.0`).
- **Spatial coherence:** Eliminated the ~65-day EBM solstice lag (removing spurious `Dsa`/`Dsb` speckles across Eurasia and Canada) and hill-slope rain speckle (pre-blocking synoptic convergence + mixing-ratio moisture transport).

---

## 2. Urgent Problems & Root-Cause Diagnosis

Each diagnosis below is backed by monthly `FWM_PROBE` diagnostics at level 7 (`T` in °C, `P` in mm/month, `u/v` in m/s) and exact references to [climate.rs](../core/src/stages/climate.rs) and [wind.rs](../core/src/stages/wind.rs).

### Problem 1: Too Arid Middle of South America (Paraguay / Gran Chaco / Bolivia / N. Argentina)

#### Observed vs Simulated

| Station | Real Köppen / P (mm) | Sim Köppen / P (mm) | Jan T (real / sim) | Jul T (real / sim) |
| :--- | :--- | :--- | :--- | :--- |
| Asuncion (`25.3°S`) | `Cfa` / `1400` | **`BWh` / `115`** (`x0.09`) | `28.0` / `29.9` | `18.0` / `12.8` |
| Buenos Aires (`34.6°S`) | `Cfa` / `1200` | `BSk` / `547` (`x0.46`) | `25.0` / `25.9` | `11.0` / `7.8` |
| Cordoba AR (`31.4°S`) | `Cwa` / `800` | `Csa` / `675` (`x0.85`) | `24.0` / `26.8` | `11.0` / `10.3` |
| Sao Paulo (`23.6°S`) | `Cfa` / `1450` | `Cwa` / `3020` (`x2.07`) | `22.5` / `24.1` | `16.0` / `13.5` |

Monthly probe at **Asuncion** (`cont 0.74`):
```text
P  Jan..Dec:  31  37  26   2   0   0   0   0   0   1   5  13   (sum 115 mm)
u  Jan..Dec: -6.4 -6.7 -6.4 -4.2 -0.9 +1.6 +2.7 +1.9 -0.9 -4.6 -6.8 -6.7
v  Jan..Dec:  0.0 +1.3 +3.4 +3.7 +2.7 +1.2 +0.5 +1.0 +2.2 +2.3 +0.5 -0.3
```
Monthly probe at **Buenos Aires** (`cont 0.44`):
```text
P  Jan..Dec: 167  87  22   2   6   7   8   8   7   6  88 140   (sum 547 mm)
u  Jan..Dec: -6.1 -4.9 -1.4 +2.5 +6.0 +8.6 +9.4 +8.7 +6.2 +1.9 -1.8 -4.9
```

#### Root Causes in Code
1. **Pure easterlies are penalized almost as heavily as equatorward west-coast flow (`climate.rs:1197`, `1205`):**
   - At Asuncion (`25.3°S`), summer winds are strong onshore easterlies (`u = -6.7 m/s`) with slightly equatorward `v` (`+0.0..+3.4 m/s`, so `poleward = v * lat.signum() <= 0`).
   - Because `sub_wind = (0.80 - 3.2 * poleward).clamp(0.0, 1.0)` assigns **`sub_wind = 0.80..1.0`** whenever `poleward <= 0` and `u_east < 0`, both `subside` (`climate.rs:1199`) and `tsub` (`climate.rs:1205`) stay strongly active over subtropical eastern/central South America even in midsummer.
2. **No synoptic barrier-parallel low-level jet along the Andes (`wind.rs:241`):**
   - On real Earth, easterly trade winds impinging on the Andes deflect poleward (southward, `v < 0`) along the eastern foothills as the South American Low-Level Jet (SALLJ), channeling Amazon moisture straight into Bolivia, Paraguay, and northern Argentina.
   - In `wind.rs`, `mountain_blocking` only rotates winds *on* steep sloped cells; it does not build the mesoscale windward pressure ridge along a meridional cordillera that drives poleward flow across the adjacent lowland basin (`57°–64°W`).
3. **Excessive coastal rainout on the Brazilian Highlands + self-reinforcing dry-ground lock-in (`climate.rs:1221–1245`, `1380`):**
   - Sao Paulo receives `3020 mm` (`2.1×` real), wringing out Atlantic moisture on the Serra do Mar / Brazilian Highlands. Air descending westward into the Paraná basin hits `shadow` + `dissolve` + `tsub` + `subside` + `sub_dry`.
   - Once rainfall dips, `dry_v → 1.0` activates `parched = 1 - DRY_GROUND_CAP * dry_v` (`climate.rs:1232`), halves land recycling `back` (`climate.rs:1251`), and cuts `cond_eff` (`climate.rs:1243`), locking the interior basin into a permanent `BWh` desert.
4. **Winter Andean rain shadow starves Buenos Aires:**
   - May–Sep westerlies (`u = +6..+9.4 m/s`) descend the Andes into the Pampas (`6–8 mm/month`), while `STORM_SPREAD` (`climate.rs:1327`) does not bring enough South Atlantic cyclonic moisture onshore from the east during winter.

#### General-Physics Suggestions
- **Distinguish warm eastern-margin onshore easterlies from cold western-margin subsidence:** Gate the strong baseline subsidence penalty (`sub_wind`) toward **equatorward flow (`poleward < 0`) and cold-current / western-coast air**, rather than imposing `0.80` subsidence on pure onshore easterlies (`poleward ≈ 0`) arriving from a warm western-boundary ocean (`cur >= 0` or humid continental basin).
- **Synoptic barrier deflection for meridional cordilleras:** Add a smoothed upwind topographic pressure perturbation (or Coriolis-directed barrier jet term along the windward foothills of high ranges in [wind.rs](../core/src/stages/wind.rs)) so tropical easterlies blocked by a north–south range naturally channel poleward into the subtropical interior (fixing both the SALLJ in South America and strengthening the Great Plains LLJ in North America).
- **Temper coastal escarpment over-depletion and runaway `dry_v` lock-in:** Cap per-pass windward depletion on moderate coastal plateaus (`< 1500 m`) and soften the `dry_v` recycling cut so moist basins upwind (Amazon) can still sustain inland convection rather than collapsing to `0 mm/month`.

---

### Problem 2: Too Humid Horn of Africa (Somalia / Ogaden / East Kenya)

#### Observed vs Simulated

| Station | Real Köppen / P (mm) | Sim Köppen / P (mm) | Jan T (real / sim) | Jul T (real / sim) |
| :--- | :--- | :--- | :--- | :--- |
| Mogadishu (`2.1°N`) | `BSh` / `430` | **`Af` / `2767`** (`x6.19`) | `27.0` / `25.4` | `26.0` / `24.6` |
| Dar es Salaam (`6.8°S`) | `Aw` / `1100` | `Af` / `3376` (`x3.03`) | `28.0` / `27.0` | `24.0` / `23.6` |
| Nairobi (`1.3°S`) | `Cwb` / `900` | `Cfb` / `2100` (`x2.30`) | `19.5` / `17.7` | `16.5` / `14.4` |
| Addis Ababa (`9.0°N`) | `Cwb` / `1150` | `Cfb` / `1570` (`x1.36`) | `16.0` / `12.3` | `16.0` / `13.2` |

Monthly probe at **Mogadishu** (`cont 0.17`):
```text
P  Jan..Dec: 181 203 288 311 259 205 192 212 260 238 223 194   (sum 2767 mm)
u  Jan..Dec: -0.3 -1.8 -2.6 +0.1 +3.8 +5.6 +5.8 +4.8 +3.1 +1.3 +0.5 +0.3
v  Jan..Dec: -4.6 -3.7 -1.7 +0.3 +0.5 -0.1 -0.2 +0.5 +0.8 -0.6 -3.0 -4.4
```

#### Root Causes in Code
1. **No wind-driven coastal Ekman upwelling (`climate.rs:692`):**
   - `raw_current_c` computes cold currents purely from static east-vs-west basin geometry peaked at `±24°` (`gauss(a, 24.0, 16.0)`).
   - Near the equator (`2°N`) on the *western* side of the Indian Ocean (`s < 0`), Somalia receives a **warm** current (`cold_coast = 1.0`), missing the real cold **Somali Upwelling** driven by the Jun–Sep SW monsoon jet blowing offshore/parallel along the coast.
2. **Large-scale low-level divergence is far too weak as a rain suppressor (`climate.rs:1166–1167`):**
   - During both the NE monsoon (Dec–Feb, `v = -4.6 m/s`, coast-parallel dry Arabian air) and SW monsoon (Jun–Sep, `u = +5.8 m/s`, accelerating offshore toward the Indian heat low), low-level air over the Horn of Africa **diverges** (`conv[i] < 0`).
   - Currently, negative `conv` only multiplies `lm` by `1 + 0.6 * (-0.7) = 0.58` and does **not** feed into `subside`, `convect`, `cond_eff`, or `sub_dry`. In warm equatorial air (`showery = 1.0`, `convect = 2.2`, `v ≈ sat`), even `0.58×` still dumps `190–260 mm/month`.
3. **Westerly monsoon uncapping fires on offshore winds (`climate.rs:1192–1197`, `1205–1207`):**
   - In May–Sep at Mogadishu, `u = +3.8..+5.8 m/s` is **offshore** wind blowing *away* from the Ethiopian/Kenyan highlands out to sea — yet `u_east > 0` sets `monsoon_w = 1.0` and `monsoon_flow = 1.0`, stripping all trade-belt capping.
4. **Isotropic `EDDY_MIX` across land–sea edges re-humidifies offshore-wind coasts (`climate.rs:1098`):**
   - Even when wind blows dry air offshore (`u > 0` at Somalia, `v` coast-parallel at Lima/Namibia), symmetric eddy diffusion (`EDDY_MIX = 0.05`) diffuses saturated ocean air backward onto coastal land cells against the wind.
5. **Equatorial East Africa lacks rain-shadow drying downwind of the Ethiopian/Kenyan highlands:**
   - Nairobi (`2100 mm`) and the Congo-to-Somalia belt keep `w ≈ sat` year-round because the `u > 0` flow across Equatorial Africa loses too little moisture crossing the Rift/Ethiopian highlands.

#### General-Physics Suggestions
- **Dynamic wind-driven coastal Ekman upwelling (replaces/augments static `subtropical` cold current):**
  - On every ocean cell adjacent to land, compute the offshore coast normal $\hat{n}_{\text{off}}$ (from $\nabla(\text{sea\_frac})$).
  - Ekman transport is directed $90^\circ$ right of the wind in NH and $90^\circ$ left in SH ($\vec{E} \propto -\text{sign}(\text{lat})\, \hat{k} \times \vec{u}$), plus direct offshore wind-driven divergence ($\vec{u} \cdot \hat{n}_{\text{off}} > 0$).
  - Where $\vec{E} \cdot \hat{n}_{\text{off}} > 0$ (or strong offshore acceleration occurs), cool `current_c` seasonally and reduce `cold_coast`. On Earth this single physical rule simultaneously creates the **Somali Cold Current**, **Omani/Arabian upwelling**, **Peruvian/Humboldt**, **Benguela**, **Canary**, and **California** upwelling zones — and generalizes to any fantasy coastline orientation.
- **Couple synoptic low-level divergence (`conv < 0`) directly to subsidence:**
  - Treat large-scale low-level divergence `div = (-conv[i] / 3.0e-6).clamp(0.0, 1.0)` as genuine vertical subsidence (`w < 0`), adding it into the stability cap (`subside_eff = (subside + 0.65 * div).min(0.95)`) so accelerating monsoon outflows and coast-parallel divergent jets (Horn of Africa, Arabia, NE Brazil) suppress `convect`, `cond_eff`, and entrain dry air via `sub_dry`.
- **Gate `monsoon_w` / `monsoon_flow` to onshore or inland-converging flow, and damp upwind ocean-to-land eddy diffusion:**
  - Do not uncap `tsub`/`subside` when strong wind is blowing *offshore* out of a continent.
  - Scale `eddy` across land–sea edges down when wind blows from land to sea so offshore-wind coasts do not absorb ocean moisture upwind.

---

### Problem 3: Sahara & Arabian Peninsula — Still Partially `BSh` Instead of `BWh`/`BWk` (and Cool Summer Bias)

#### Observed vs Simulated

| Station | Real Köppen / P (mm) | Sim Köppen / P (mm) | Jul T (real / sim / err) | Main Wet Months (sim) |
| :--- | :--- | :--- | :--- | :--- |
| Riyadh (`24.7°N`) | `BWh` / `100` | **`BSh` / `416`** (`x3.64`) | `36.5` / `23.6` (**`-12.9 °C`**) | May–Sep (`59–81 mm/mo`) |
| Khartoum (`15.6°N`) | `BWh` / `120` | **`BSh` / `468`** (`x3.48`) | `32.5` / `23.9` (**`-8.6 °C`**) | May–Sep (`59–97 mm/mo`) |
| Timbuktu (`16.8°N`) | `BWh` / `200` | **`BSh` / `557`** (`x2.62`) | `32.5` / `24.5` (**`-8.0 °C`**) | May–Sep (`72–131 mm/mo`) |
| Cairo (`30.0°N`) | `BWh` / `25` | **`BSh` / `406`** (`x9.46`) | `28.5` / `25.2` (`-3.3 °C`) | Nov–Mar (`45–71 mm/mo`), Jul (`39`) |
| Tripoli (`32.9°N`) | `BSh` / `330` | `BSh` / `650` (`x1.91`) | `28.0` / `25.7` (`-2.3 °C`) | Winter Mediterranean |
| Baghdad (`33.3°N`) | `BWh` / `150` | `BWh` / `80` (`x0.59`) | `35.0` / `32.1` (`-2.9 °C`) | Dry all year |

Monthly probes show the **exact coupled failure mode**:
```text
Riyadh:   T  14.8 17.6 22.0 24.3 | 23.6 23.8 23.6 23.0 21.8 | 19.9 17.7 14.9
          P     2    2    7   21 |   59   78   73   81   61 |   23    8    1
          v  +0.1 -0.0  0.0 +1.2 | +1.5 +1.3 +0.9 -1.2 -2.6 | -2.5 -1.5 -0.4

Timbuktu: T  20.1 23.1 27.0 30.1 | 25.9 24.9 24.5 24.8 24.8 | 23.6 22.1 19.9
          P     0    0    1    6 |   83  107  131  122   72 |   31    3    0
          u  -4.8 -5.2 -4.7 -1.9 | +1.5 +4.1 +4.1 +2.5 -2.1 | -5.7 -6.5 -5.6
          v  -2.8 -3.8 -4.5 -2.5 | +1.7 +3.9 +3.9 +2.6 -0.3 | -2.6 -3.4 -3.0

Cairo:    P    67   71   52    5 |    8   25   39   24    6 |    4   45   60
```

#### Root Causes in Code
1. **Weak poleward wind (`v ≈ +1.2..+1.5 m/s`) disables subtropical subsidence over Arabia (`climate.rs:1197`):**
   - At Riyadh in Apr–Jul, light southerly inflow (`v = +1.2..+1.5 m/s`, `poleward ≈ 0.17`) from the Arabian Sea / Persian Gulf cuts `sub_wind = (0.80 - 3.2 * poleward)` from `0.80` down to **`0.25`**!
   - That removes the Hadley subsidence lid over central Arabia right in midsummer, letting `60–80 mm/month` fall from May to September (`352 mm` in 5 summer months).
2. **Sahel / West African monsoon reaches ~4–5° too far north (`climate.rs:479`, `1192–1209`):**
   - At Timbuktu (`16.8°N`) and Khartoum (`15.6°N`), `RAIN_BAND_FOLLOW = 0.45` plus `monsoon_w` / `monsoon_flow` uncap `15°–18°N` too aggressively. Notice how Timbuktu hits **`30.1 °C` in April** (when dry), then plunges to **`24.5 °C` in July** once `100–130 mm/month` of monsoon rain flips `dry_v` from `1.0 → 0.0`!
3. **Coupled rain → `dry_v` → cooling → lower Köppen `BWh` threshold feedback (`climate.rs:88–91`, `976`):**
   - As soon as summer rain exceeds ~40 mm/month, `dry_v` drops to `0`, flipping `heat` from `+5.5 °C` (`DRY_HEAT_MEAN_C + DRY_HEAT_SWING_C`) to `-1.2 °C` (`-WET_LAND_COOL_C`), a **`-6.7 °C` cooling cliff**.
   - Even when completely dry, `DRY_HEAT_MEAN_C = 1.5` + `DRY_HEAT_SWING_C = 4.0` underestimates cloud-free subtropical desert summer heating (real Sahara/Arabia sit `+8..+10 °C` above the zonal land mean because cloud fraction is ~5% vs ~50%).
   - Lower simulated `T_ann` directly lowers the Köppen aridity threshold (`20 * T_ann + ...`), turning borderline stations from `BWh` into `BSh` even faster.
4. **Excessive winter Mediterranean rain on North Africa (`Cairo`, `Tripoli`):**
   - Cairo receives `295 mm` in Nov–Mar (`45–71 mm/month`) because winter westerlies (`u = +7.3 m/s`) pick up too much moisture over the narrow eastern Mediterranean (`evap` floor `0.35 + 0.65 * open_sea` at `climate.rs:1249`) and the winter storm track (`storm_center = belts.ferrel - 13.0`, width `14°/20°`) extends too far south into `30°N`.

#### General-Physics Suggestions
- **Raise the poleward threshold for breaking subtropical subsidence (and gate by oceanic western-boundary warmth/humidity):**
  - Instead of `0.80 - 3.2 * poleward` (which lets a gentle `1.5 m/s` thermal-low breeze break the subsidence lid over Arabia), require a **stronger sustained poleward moisture flux** (e.g., `smoothstep(0.15, 0.55, poleward)` weighted by incoming air humidity / open-ocean maritime fraction, so dry desert breezes or narrow-gulf air do not break the subtropical high).
- **Strengthen desert subsidence drying (`sub_dry`) & narrow the tropical rain band poleward of `12°–15°` over dry interiors:**
  - Keep `local_itcz` / monsoon uncapping coupled to actual moisture supply (open-ocean fetch vs narrow gulfs like the Red Sea / Persian Gulf, where `open_sea ≈ 0` should reduce `evap` floor from `0.35` down to `~0.15–0.20`). That simultaneously cuts Cairo's winter rain, Riyadh's Persian Gulf / Red Sea moisture, and Khartoum's Red Sea moisture!
- **Increase cloud-free dry-ground summer heating & reduce the `dry_v` cliff:**
  - Raise `DRY_HEAT_SWING_C` (e.g., `4.0 → 6.5–7.5 °C`, `DRY_HEAT_MEAN_C` `1.5 → 2.5 °C`), scaled smoothly by low-cloud / subtropical insolation (`smoothstep(5.0, 35.0, abs_lat)` or clear-sky fraction `1 - rh`), so hyper-arid deserts reach realistic `34–38 °C` summer temperatures and accurate Köppen `BWh` thresholds.
- **Tighten winter Mediterranean storm-track skirt (`30°–33°N`):**
  - Lower the narrow-sea evaporation floor (`0.35 → 0.15–0.20`) and slightly narrow the equatorward tail of `STORM_RAIN` / `STORM_MIX` in winter so `30°N` desert coasts (Cairo, Libya) receive `< 100 mm/yr` while `38°–52°N` Europe stays well-watered.

---

## 3. Secondary Discrepancies Observed in Current Run

While less urgent than the three regions above, these discrepancies also stand out in the level-7/8 reports and share underlying physics with them:

### 4. Northern India / Ganges Plain Monsoon Gap

| Station | Real Köppen / P (mm) | Sim Köppen / P (mm) | Notes |
| :--- | :--- | :--- | :--- |
| Delhi (`28.6°N`) | `Cwa` / `800` | `BWh` / `80` (`x0.12`) | Strong Jun–Jul `u = +5.2 m/s`, yet only `14–25 mm/mo` |
| Kolkata (`22.6°N`) | `Aw` / `1750` | `BSh` / `413` (`x0.24`) | Strong onshore `u = +6.0, v = +4.3 m/s`, yet only `42–62 mm/mo` |
| Dhaka (`23.8°N`) | `Aw` / `2100` | `BSh` / `538` (`x0.26`) | Blocked in pre-monsoon dryness trap |
| Mumbai (`19.1°N`) | `Am` / `2200` | `BSh` / `704` (`x0.33`) | Peaks at `200 mm/mo` in Jul vs real `~800 mm/mo` |
| Chongqing (`29.6°N`) | `Cfa` / `1100` | `BWh` / `219` (`x0.21`) | Sichuan basin rain-shadowed & trapped in `dry_v` |
| Hanoi (`21.0°N`) | `Cwa` / `1700` | `BSh` / `453` (`x0.27`) | Too much subsidence/trade cap at `21°N` |

- **Root cause:** In `dryness()` (`climate.rs:1380`), `rain = 0.5*m + 0.3*(m-1) + 0.2*(m-2)` carries 2 months of **pre-monsoon dry-season memory** (April–May drought) into June–July. Combined with `parched` (`DRY_GROUND_CAP = 0.45`), `cond_eff` (`1 - 0.55 * dry_v`), and `moist_gate` (`1 - 0.85 * dry_v`), a strongly seasonal monsoon region that is bone-dry in March–May gets **self-suppressed** right as the June monsoon arrives!
- **Suggestion:** Decouple **atmospheric rain suppression (`parched`, `conv_eff`, `moist_gate`)** from multi-month soil dryness memory — use the *current month's* simulated moisture/convergence (or only suppress when *both* the ground is dry *and* incoming air is dry/continental), keeping multi-month memory solely for surface soil-water evaporation (`soil_water`). That cleanly separates **monsoon climates** (dry spring → wet summer onshore monsoon: India, South China, Indochina) from **perennial deserts** (dry air year-round: Sahara, Arabia).

### 5. Peruvian / Atacama Coast (`Lima`)

| Station | Real Köppen / P (mm) | Sim Köppen / P (mm) | Jan–Mar Sim P (mm/mo) |
| :--- | :--- | :--- | :--- |
| Lima (`12.1°S`, cell `1370 m`) | `BWh` / `10` | `Aw` / `794` (`x27.1`) | `158, 154, 103` |
| Antofagasta (`23.7°S`) | `BWk` / `2` | `BWh` / `121` (`x6.4`) | Dry overall, slight summer leak |

- **Root cause:** At `12°S` in January (`cz = -7.9°`), Lima lies only `4°` from the zonal ITCZ (`subside = 0`), while `FWM_PROBE` shows massive equatorward flow (`v = +10 m/s`, `u = -5..-9 m/s`). Two things cause the `794 mm`: (1) static cold currents (`gauss(a, 24, 16)`) are too weak at `12°S`, and (2) `eddy` (`(-dh / 1200.0).exp()`) leaks Amazon vapor westward across the narrow 4 km Andes crest onto Lima's `1370 m` coastal slope cell.
- **Suggestion:** The **wind-driven Ekman upwelling** fix from Problem 2 directly strengthens the cold Humboldt Current up to `5°S`, while tightening `eddy` terrain damping (`1200 m → 600 m`) and letting strong equatorward coastal jets (`poleward < -0.5`) suppress `lm`/`convect` even within `15°` of the equator will dry Lima to `< 50 mm`.

### 6. Subpolar North Atlantic Winter Cold Bias

| Station | Real Jan (°C) | Sim Jan (°C) | Err |
| :--- | ---: | ---: | ---: |
| Reykjavik (`64.1°N`) | `0.0` | `-19.2` | `-19.2 °C` |
| Nuuk (`64.2°N`) | `-8.0` | `-19.7` | `-11.7 °C` |
| Murmansk (`69.0°N`) | `-10.0` | `-23.6` | `-13.6 °C` |
| Svalbard (`78.2°N`) | `-12.0` | `-27.9` | `-15.9 °C` |

- **Root cause:** `raw_current_c` (`climate.rs:686`) sets sign `s = (dw2 - de2) / (dw2 + de2)` from local east/west coast distance. Because Greenland sits immediately west of Iceland (`dw` tiny, `de` large to Norway), `s < 0`, assigning **cold** current water right around Iceland and freezing the Irminger/Norwegian Sea margin in winter (producing `-5.9 m/s` northerly winds off the ice pack).
- **Suggestion:** Compute subpolar warm drift by advecting the mid-latitude western-boundary warm current poleward and eastward across the open ocean basin along the westerlies (using a basin-width scale or filtering out small islands/narrow straits in the `de/dw` march), so the warm drift doesn't flip sign immediately east of Greenland.

### 7. East Asian Winter Mild Bias

| Station | Real Jan (°C) | Sim Jan (°C) | Err |
| :--- | ---: | ---: | ---: |
| Vladivostok (`43.1°N`) | `-12.0` | `+3.0` | `+15.0 °C` |
| Harbin (`45.8°N`) | `-17.5` | `-3.3` | `+14.2 °C` |
| Seoul (`37.6°N`) | `-2.0` | `+11.7` | `+13.7 °C` |
| Wuhan (`30.6°N`) | `+4.0` | `+12.0` | `+8.0 °C` |

- **Root cause:** `FWM_PROBE` at Vladivostok shows winter winds blowing **southwesterly** (`u = +7.4, v = +0.4 m/s`) instead of **northwesterly** (`v < 0`) out of the Siberian High. In `wind.rs:42`, `COLD_HIGH = 0.5` halves continental winter high pressure, so the zonal westerly belt's poleward component (`v = +0.3 * w` in `belt_wind`, `climate.rs:470`) overpowers Siberian High outflow and pulls mild mid-latitude air into Manchuria/Korea.
- **Suggestion:** Slightly raise `COLD_HIGH` (`0.5 → 0.65–0.75`) or reduce the belt westerlies' built-in poleward tilt (`0.3 * w → 0.15 * w`) on winter continental interiors so high-latitude winter anticyclones drive realistic cold NW surges toward eastern continental coasts (Manchuria, Korea, Northeast US/Montreal).

### 8. Residual Mediterranean & West-Coast Winter Rain Excess

| Station | Real P (mm) | Sim P (mm) | Ratio |
| :--- | ---: | ---: | ---: |
| Madrid (`40.4°N`) | `420` | `905` | `x2.10` |
| Rome (`41.9°N`) | `800` | `1326` | `x1.64` |
| Athens (`38.0°N`) | `400` | `1041` | `x2.53` |
| Los Angeles (`34.1°N`) | `380` | `1089` | `x2.77` |
| Santiago (`33.5°S`) | `330` | `1362` | `x3.95` |

- **Root cause:** Lowering narrow-sea evaporation (`open_sea` floor `0.35 → 0.18`), narrowing the winter storm-track equatorward skirt, and adding wind-driven coastal upwelling (Problem 2) will also cut excess winter/spring rain on the Mediterranean, Southern California, and Central Chile without starving Pacific Northwest or Western Europe.

---

## 4. Recommended Implementation Order (Next Climate Pass)

To avoid regressions across coupled variables, tackle the fixes in four focused, independently testable steps (evaluating both `earth_check` at L7/L8 and fantasy seeds 1–3 after each step):

1. **Wind-driven coastal Ekman upwelling + synoptic divergence subsidence** (`climate.rs`):
   - Directly solves **Horn of Africa** (Problem 2) and **Lima / Peru** (Problem 5), and helps **California / Chile / Arabia**.
2. **Refine subtropical subsidence (`sub_wind`), narrow-sea `evap`, and desert `DRY_HEAT`** (`climate.rs`):
   - Require stronger humid poleward flow to break subsidence, lower narrow-sea evaporation floor (`0.35 → 0.18`), and strengthen clear-sky desert summer heating.
   - Directly solves **Sahara & Arabia `BWh` + summer temperatures** (Problem 3) and **Cairo / Mediterranean excess** (Problem 8).
3. **Decouple monsoon atmospheric gating from multi-month soil dryness + warm-margin easterly uncapping + cordillera barrier jet** (`climate.rs`, `wind.rs`):
   - Directly solves **Middle of South America** (Problem 1) and **Northern India / South China monsoon gap** (Problem 4).
4. **High-latitude winter refinements (`COLD_HIGH`, `belt_wind` poleward tilt, subpolar basin current)** (`wind.rs`, `climate.rs`):
   - Improves **East Asian winter cold** (Problem 7) and **North Atlantic subpolar winter warmth** (Problem 6).

---

## 5. Results of the Follow-up Pass (`master-3sczxa`)

**Snapshot:** commits `84d5ce1`, `ea94ad8`, `35a7024` and the commit adding this section (`MODEL_VERSION[Step::Climate] = 12`), compared against `652e5d0` (the snapshot above) on the same `earth16.png`. Every change was tested station by station against the previous run (`report.txt` of both runs), not only on the totals; generated seeds 1–3 were checked after each step.

### Aggregate Metrics

| Metric | `652e5d0` L7 | Now L7 | `652e5d0` L8 | Now L8 |
| :--- | ---: | ---: | ---: | ---: |
| Köppen group / exact match | `68%` / `31%` | `68%` / **`37%`** | `67%` / `29%` | **`68%`** / **`32%`** |
| Annual precip factor (bias) | `x1.95` (`x0.94`) | `x1.90` (`x0.94`) | `x1.91` (`x0.92`) | `x1.88` (`x0.94`) |
| January temp MAE (bias) | `4.2 °C` (`+1.3`) | `4.4 °C` (`+1.6`) | `4.1 °C` (`+1.3`) | `4.3 °C` (`+1.6`) |
| July temp MAE (bias) | `3.1 °C` (`-1.9`) | `3.2 °C` (`-1.2`) | `3.0 °C` (`-2.0`) | `3.1 °C` (`-1.3`) |
| Zonal T MAE / P factor | `2.2 °C` / `x1.67` | `2.3 °C` / `x1.59` | `2.2 °C` / `x1.67` | `2.3 °C` / `x1.59` |
| Köppen land shares `A/B/C/D/E` | `17/33/20/16/14` | `14/36/19/18/13` | `17/32/20/16/14` | `14/35/19/18/13` |

Rain improved by more than `x1.5` at 9 stations and got worse at 9 (level 8: 12 and 9). The totals move little; the gains are regional.

### Regional Results (Level 7; P in mm, T error January / July in °C)

| Station | Real P | `652e5d0` | Now | Note |
| :--- | ---: | :--- | :--- | :--- |
| Mumbai | 2200 | `BSh` 704, -4.9 / -5.8 | `Aw` 1271, -3.5 / -1.8 | monsoon regression fixed (cold currents) |
| Hanoi | 1700 | `BSh` 453, +1.6 / -4.2 | `Cwa` 1015, +1.4 / -2.8 | same |
| Dhaka | 2100 | `BSh` 538, -2.2 / -6.3 | `Cwa` 1256, -0.5 / -2.1 | cell is sea in the 8-bit map |
| Reykjavik | 850 | `ET` 293, -19.2 / -5.2 | `Dfc` 405, -11.1 / -1.0 | back to the `90c2429` level (-10.8) |
| Murmansk | 480 | 108, -13.6 / -1.8 | 142, -12.1 / -0.2 | still 7 °C colder than `90c2429` |
| Rome | 800 | 1326, +2.7 / -3.9 | 958, +2.0 / -5.0 | enclosed-sea evaporation, seasonal convection |
| Athens | 400 | 1041, +3.7 / -3.8 | 709, +3.5 / -1.6 | same |
| Cairo | 25 | `BSh` 406 | `BWh` 271 | cell is coastal: the Nile delta is sea in the 8-bit map (Alexandria: ~190) |
| Riyadh | 100 | `BSh` 416, +0.3 / -12.9 | `BWh` 142, -0.2 / -7.9 | rain fixed; July still too cool |
| Khartoum | 120 | `BSh` 468, -3.7 / -8.6 | `BWh` 338, -3.9 / -8.4 | rainy-season heat still missing |
| Timbuktu | 200 | `BSh` 557 | `BWh` 308 | |
| Lima | 10 | `Aw` 794 | `BSh` 546 | Peru upwelling, sinking over cold water |
| Los Angeles | 380 | 1089 | 808 | |
| Delhi | 800 | `BWh` 80 | `BWh` 67 | unchanged: summer air comes from the Thar |
| Asuncion | 1400 | `BWh` 115 | `BWh` 64 | worse: no low-level jet |
| Mogadishu | 430 | `Af` 2767 | `Af` 2090 | still far too wet |
| Chongqing | 1100 | `BWh` 219, July +0.4 | `BWh` 72, July +4.5 | worse |
| Winnipeg | 520 | January +10.2 | January +9.7 | trade-off kept (see below) |

### What Was Changed (General Physics)

1. **Ocean currents from wind and basin.** The cold side of the coastal currents came from the east/west geometry alone, so the Arabian Sea and Bay of Bengal got California-like cold water, and `652e5d0` carried it downwind. Cold water now comes only from **wind-driven upwelling**: offshore Ekman transport from each month's winds (right of the wind in the north, left in the south), on open-ocean coasts (share of sea within ~1500 km), below ~45°, damped under subtropical western-boundary currents. The warm side is **gyre-based**: western-boundary currents, a subpolar drift warmest toward the east of each basin, a narrow cold current down subpolar west coasts. **Sea ice** is computed from the final currents instead of the first zonal guess. Narrow-sea evaporation uses the 1500-km openness instead of 400-km smoothing (every coastal strip had been evaporating at ~40%).
2. **Sinking air.** Low-level divergence adds subsidence only over cold upwelled water and not for poleward or monsoon flow. Continental air beyond a monsoon trough sinks and forms no cloud (`FAR_CAP`), unless moist air streams on poleward (the Meiyu front). Cloud under a subsidence inversion rains at a fifth of the rate.
3. **Deserts.** Dry ground heats more (`DRY_HEAT_MEAN_C` 2.5, `DRY_HEAT_SWING_C` 6.5), and evaporation demand climbs steeply above 18 °C, so a hot month with 30–80 mm of rain no longer counts as wet ground.
4. **Mediterranean.** Seas joined to the ocean only through narrow straits (sea cells > 120 km from land form pieces; pieces under 2% of the planet) moisten the air at 0.15 of the open ocean's rate: at levels 7–8 the Mediterranean, Black, Red and Caspian Seas, the Persian Gulf, the Baltic and Hudson Bay, while the Gulf of Mexico, the Bay of Bengal and the Gulf of Tonkin stay open. Land showers follow the warm season (all year only in the hot tropics), so a 16 °C winter no longer convects.

### Which Diagnoses in Sections 2–3 Held

| Diagnosis | Result |
| :--- | :--- |
| Problem 4: soil-dryness memory chokes the monsoon | **Did not hold.** Separating it changed every station by a few mm; reverted. The cause was the cold currents (above). |
| Problem 2: weak divergence-subsidence at the Horn of Africa | **Did not hold.** The model has no strong divergence there; Mogadishu did not move. The Somali jet is missing (a barrier-jet problem). |
| Problems 2 and 5: wind-driven Ekman upwelling | **Held.** It is the core of the monsoon fix and helps Lima, California and Chile. |
| Problem 3: subsidence breaks too easily; desert heat too weak | **Partly.** Lid and heat changes dried Arabia and the Sahel; Riyadh is still 8 °C cool in July. Requiring maritime air for the poleward break was tried and dropped (Dallas and Wuhan collapsed). |
| Problem 1: cordillera barrier jet | **Not reached.** A synoptic barrier term in `wind.rs` did not change the winds at Asuncion or Mogadishu and cost 3 points of Köppen match; reverted. |
| Problem 6: subpolar current sign flip near Greenland | **Held.** Fixed by the gyre currents. |
| Problem 7: `COLD_HIGH` / westerly tilt | **Did not hold.** Neither moved the interiors; their mild winters come from the wider winter westerlies (`belt_coord`), a trade-off with Europe (narrower belts make Moscow 16 °C too cold). |
| Problem 8: narrow-sea evaporation, storm-track skirt | **Partly.** Narrowing the storm track's equatorward side did almost nothing; the enclosed-sea rule (by strait width, not by openness, which could not tell the Mediterranean from the Bay of Bengal) and seasonal convection fixed most of it. |

### Still Wrong

- **No low-level jets:** Delhi and the Ganges plain, the Chaco (Asuncion) and the Horn of Africa.
- **Desert heat:** Riyadh's July (-7.9 °C), the Sahel's rainy season (Khartoum, Timbuktu -8 °C). Even dry months in Arabia are 4–6 °C cool; more desert heat would overheat Alice Springs.
- **Interiors in winter:** North America, Kazakhstan and Manchuria 9–15 °C too mild (zonal belts cannot make continental east sides colder than west sides).
- **High latitudes:** Murmansk and Arkhangelsk colder than `90c2429`; McMurdo's cell is treated as bare land, not ice.
- **New regressions:** Chongqing and Asuncion drier, Antofagasta wetter.

### Data Notes

- The 8-bit `earth16.png` turns land below ~20 m into sea: the Nile delta (so "Cairo" is on the coast), the Ganges delta (Kolkata, Dhaka), the Red River delta (Hanoi), Shanghai, the Netherlands. These stations partly measure the data, not the model. A 16-bit file avoids this (`doc/development.md`, *Tuning the climate*).
- At level 6 the climate grid is too coarse to find the Mediterranean's strait.
