# Export package and re-import

`worldgen export` or *Export…* writes the package to `<project>/export/` (or `--out`). The flat images are equirectangular, 8192 × 4096 by default (up to 32,768 px wide), optionally cropped to a latitude band (`--lat-min`, `--lat-max`; Paradox maps usually leave out the poles). Detail noise is added only at export, so coastlines are sharper than the grid. `package.json` documents every file, palette and encoding and records the parameters.

## Stage 1–2 files

| File | Contents |
| --- | --- |
| `heightmap.png` | 8-bit greyscale; sea level = 20 by default; 255 = 6,500 m |
| `heightmap16.png` | 16-bit; metres = v / 65535 × 20000 − 11000 (lossless round trip) |
| `terrain.png` | Indexed palette of 18 game terrains |
| `rivers.png` | CK3-style indexed palette: source, merge, width classes 3–11 (each ×√2 in real width), land 254, water 255 |
| `rivers.geojson` | One LineString per river with `width_m` and `discharge_m3s` per vertex |
| `biomes.png`, `climate.png`, `precipitation.png` | Reference layers (Köppen colours, temperature, rain) |
| `provinces.png` | One unique 24-bit RGB colour per province, no anti-aliasing; borders noise-warped (up to a third of a cell); the coastline matches the heightmap pixel for pixel |
| `definition.csv` | `id;r;g;b;name;x;` with a `0;0;0;0;x;x;` first row (CK3 / Victoria 3) |
| `provinces.csv` | Kind, sea band, state, region, continent, terrain, area, habitability, coastal, centre, neighbours, trade good, resources, rain, river, spring |
| `states.csv` | `STATE_…` key, name, region, continent, capital province, area, habitability, provinces and `xRRGGBB` colours |
| `regions.csv`, `continents.csv` | The hierarchy above states |
| `adjacencies.csv` | CK3 layout: sea crossings between provinces on different landmasses |
| `province_adjacency.csv` | Every border: `from;to;type;border_km;barrier;crossing_km` (land, river, impassable, coast, lake, sea, strait) |
| `trade_goods.csv` | Every good, deposit and sea resource with its category and the provinces/area that have it |
| `states.png` | Reference map of states with province borders |

## Stage 3 files

| File | Contents |
| --- | --- |
| `cultures.png` | Provinces coloured by majority culture (hue = group), culture borders dark, group borders black |
| `cultures.csv` | Every culture that ever existed: name, group, colour, parent, alive, population, provinces, years, fate (the family tree) |
| `culture_groups.csv`, `culture_events.csv` | Groups; dated emergences, splits, merges, extinctions |
| `province_cultures.csv` | Population, majority culture and shares (≥ 5%) per province |

## Stage 4 files

| File | Contents |
| --- | --- |
| `nations.png` | Owners at the start date, borders dark, unruled land grey, railways dark with white stations |
| `railways.png` | Railways black, stations red, junctions blue |
| `transport.png` | Roads (track light brown, paved dark brown, highway orange), railways with stations and junctions, airports purple, over nation borders |
| `nations.csv` | Every nation that ever existed: name, government, colour, capital, culture, provinces, population, overseas provinces, founded, ended, fate, parent; era, technology, treasury, income, integration, track/paved/highway km, rail km, stations, airports |
| `nation_events.csv` | The chronicle: `year;event;nation;other;province;text` |
| `railways.csv` | Lines: name, builder, opened, closed, km, stations, provinces in order |
| `stations.csv` | Stations with the open lines serving them; `junction` when two or more |
| `roads.csv` | Roads between neighbouring provinces: quality (1 track, 2 paved, 3 highway), year built, km |
| `airports.csv` | Airports: province, owner, year opened |
| `cities.csv` | The 100 largest cities: owner, population, capital, station, attraction, peak and year |
| `ruins.csv` | Cities that lost three quarters of their people and never recovered |
| `province_nations.csv` | Per province: owner, culture after assimilation with shares, population, railway (0–3), best road, airport, integration |

## Province clean-up (`core/src/export_clean.rs`)

Detail noise creates islets and ponds the grid doesn't have, and warped borders cut pixels off their province. Before `provinces.png` is written:

1. islets and ponds holding no grid cell's centre become the surface around them (the heightmap follows);
2. every land pixel belongs to a land or wasteland province, every sea pixel to a sea zone;
3. each stray piece of a province goes to the neighbour it touches most;
4. a province left under 8 pixels gets a disc of about half a cell;
5. X-crossings (four provinces at a pixel corner) are broken up.

On the default world (seed 1, level 8, 8192 px) provinces in several pieces drop from 464 to 18 (islands cut off by a channel, kept on purpose), X-crossings from 310 to 0, provinces with no pixels from 4 to 0. `package.json` reports the numbers under `province_cleanup`.

## Re-importing

- **Heightmap:** *Import heightmap…* (Relief card) or `worldgen import-heightmap` with encoding `heightmap16`, `paradox8` or `linear --min --max`. It replaces the relief step's result; later steps recompute.
- **Provinces:** edit `provinces.png` (and `definition.csv`) with a hard-edged pencil, then *Import…* in the Provinces card or `worldgen import-provinces`. Checked first:
  - errors (refused): duplicate colours or ids, colours with no CSV row, anti-aliased edge pixels;
  - warnings: CSV rows with no pixels, provinces in pieces, provinces under 8 pixels, X-crossings.

  Each cell takes its province by majority pixel vote; a province joins the state it overlaps most. Ids, colours and names come from the CSV (colours are numbered without one); the latitude band is read from `package.json`. Exporting and re-importing leaves 99.8% of cells in the same province. `--remove` drops the import.
