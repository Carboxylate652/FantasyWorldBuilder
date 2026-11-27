//! Earth check: runs the climate step on Earth's real terrain and scores the
//! result against climate normals of 133 places and observed zonal means.
//!
//!   cargo run --release -p worldcore --example earth_check -- <earth_heightmap16.png | -> <out-dir> [grid-level] [seed]
//!
//! The heightmap is an equirectangular 16-bit PNG in the app's heightmap16
//! encoding (metres = v / 65535 · 20000 − 11000); doc/development.md says how
//! to build one (no Earth data ships with the repository). `-` runs a
//! generated world instead (only the Köppen land shares mean anything then).
//! It writes report.txt, stations.json and maps (koppen, p_ann, p_jan, p_jul
//! with wind arrows, t_jan, t_jul, cont) to the out-dir.
//! FWM_PROBE="Moscow,Delhi" also prints those places month by month.
//!
//! The station values were compiled by hand as approximate climate normals
//! (about ±1 °C, ±15% rain); they are a yardstick, not a dataset.
use worldcore::edits::{ElevationImport, HeightEncoding};
use worldcore::fields::Field;
use worldcore::params::WorldParams;
use worldcore::stages::biomes::{koppen, KOPPEN};
use worldcore::stages::Step;
use worldcore::vec3::Vec3;
use worldcore::World;

fn f32s(w: &World, n: &str) -> Vec<f32> {
    match w.field(n).unwrap_or_else(|| panic!("{n}")).0 {
        Field::F32(v) => v.clone(),
        _ => panic!("{n} type"),
    }
}

/// name, lat, lon, Köppen, January °C, July °C, annual precipitation mm
const ST: &[(&str, f64, f64, &str, f64, f64, f64)] = &[
    ("London", 51.5, -0.1, "Cfb", 5.5, 18.7, 600.), ("Paris", 48.9, 2.35, "Cfb", 4.9, 20.5, 640.), ("Berlin", 52.5, 13.4, "Cfb", 0.6, 19.5, 570.),
    ("Madrid", 40.4, -3.7, "Csa", 6.3, 25.6, 420.), ("Lisbon", 38.7, -9.1, "Csa", 11.6, 23.5, 720.), ("Rome", 41.9, 12.5, "Csa", 7.5, 25.0, 800.),
    ("Athens", 38.0, 23.7, "Csa", 10.0, 28.5, 400.), ("Istanbul", 41.0, 29.0, "Csa", 6.0, 24.0, 820.), ("Stockholm", 59.3, 18.1, "Dfb", -1.6, 18.8, 530.),
    ("Oslo", 59.9, 10.75, "Dfb", -3.0, 17.5, 800.), ("Helsinki", 60.2, 24.9, "Dfb", -4.0, 18.0, 650.), ("Reykjavik", 64.1, -21.9, "Cfc", 0.0, 11.5, 850.),
    ("Warsaw", 52.2, 21.0, "Dfb", -1.8, 19.2, 520.), ("Kyiv", 50.45, 30.5, "Dfb", -3.5, 20.5, 620.), ("Moscow", 55.75, 37.6, "Dfb", -6.5, 19.2, 700.),
    ("Murmansk", 69.0, 33.1, "Dfc", -10.0, 13.0, 480.), ("Arkhangelsk", 64.5, 40.5, "Dfc", -12.5, 16.3, 600.), ("Ankara", 39.9, 32.85, "BSk", 0.5, 24.0, 400.),
    ("Novosibirsk", 55.0, 82.9, "Dfb", -16.5, 19.5, 430.), ("Omsk", 55.0, 73.4, "Dfb", -16.5, 19.5, 400.), ("Astana", 51.2, 71.4, "Dfb", -14.5, 20.8, 320.),
    ("Irkutsk", 52.3, 104.3, "Dwc", -17.8, 18.3, 470.), ("Yakutsk", 62.0, 129.7, "Dfd", -38.6, 19.5, 240.), ("Verkhoyansk", 67.55, 133.4, "Dfd", -45.0, 16.5, 180.),
    ("Tiksi", 71.6, 128.9, "ET", -30.0, 8.0, 320.), ("Vladivostok", 43.1, 131.9, "Dwb", -12.0, 19.5, 840.), ("Harbin", 45.75, 126.6, "Dwa", -17.5, 23.0, 520.),
    ("Ulaanbaatar", 47.9, 106.9, "Dwc", -21.5, 18.0, 260.), ("Beijing", 39.9, 116.4, "Dwa", -3.0, 26.5, 570.), ("Seoul", 37.57, 127.0, "Dwa", -2.0, 25.0, 1400.),
    ("Tokyo", 35.7, 139.7, "Cfa", 5.5, 25.5, 1500.), ("Shanghai", 31.2, 121.5, "Cfa", 4.5, 28.5, 1200.), ("Wuhan", 30.6, 114.3, "Cfa", 4.0, 29.0, 1300.),
    ("Chongqing", 29.56, 106.55, "Cfa", 8.0, 28.5, 1100.), ("Guangzhou", 23.1, 113.3, "Cwa", 13.9, 28.9, 1750.), ("Hanoi", 21.0, 105.85, "Cwa", 16.5, 29.5, 1700.),
    ("Kunming", 25.0, 102.7, "Cwb", 9.0, 20.3, 980.), ("Lhasa", 29.65, 91.1, "Cwb", -1.0, 16.5, 430.), ("Urumqi", 43.8, 87.6, "BSk", -12.5, 24.0, 290.),
    ("Kashgar", 39.5, 76.0, "BWk", -5.0, 26.0, 65.), ("Lanzhou", 36.05, 103.8, "BSk", -4.5, 22.5, 320.), ("Tashkent", 41.3, 69.25, "Csa", 1.5, 27.8, 440.),
    ("Kabul", 34.5, 69.2, "BSk", -2.0, 25.0, 310.), ("Tehran", 35.7, 51.4, "BSk", 4.0, 30.5, 230.), ("Baghdad", 33.3, 44.4, "BWh", 9.5, 35.0, 150.),
    ("Riyadh", 24.7, 46.7, "BWh", 14.5, 36.5, 100.), ("Karachi", 24.9, 67.0, "BWh", 18.5, 30.5, 200.), ("Delhi", 28.6, 77.2, "Cwa", 14.0, 31.5, 800.),
    ("Mumbai", 19.1, 72.9, "Am", 24.5, 27.8, 2200.), ("Kolkata", 22.57, 88.36, "Aw", 20.0, 29.5, 1750.), ("Chennai", 13.08, 80.27, "Aw", 25.0, 31.0, 1400.),
    ("Dhaka", 23.8, 90.4, "Aw", 19.0, 29.0, 2100.), ("Colombo", 6.9, 79.86, "Af", 27.0, 28.0, 2400.), ("Bangkok", 13.75, 100.5, "Aw", 27.0, 29.5, 1500.),
    ("Singapore", 1.35, 103.8, "Af", 26.5, 28.0, 2340.), ("Jakarta", -6.2, 106.8, "Am", 26.5, 27.5, 1800.), ("Manila", 14.6, 121.0, "Aw", 26.0, 28.5, 2000.),
    ("Cairo", 30.0, 31.2, "BWh", 14.0, 28.5, 25.), ("Algiers", 36.75, 3.05, "Csa", 11.5, 26.0, 600.), ("Casablanca", 33.57, -7.6, "Csa", 13.0, 23.0, 420.),
    ("Tripoli", 32.9, 13.2, "BSh", 13.0, 28.0, 330.), ("Timbuktu", 16.8, -3.0, "BWh", 22.0, 32.5, 200.), ("Niamey", 13.5, 2.1, "BSh", 24.5, 29.0, 550.),
    ("Khartoum", 15.6, 32.5, "BWh", 23.5, 32.5, 120.), ("Dakar", 14.7, -17.0, "BSh", 21.0, 27.5, 400.), ("Lagos", 6.5, 3.4, "Aw", 27.5, 25.5, 1700.),
    ("Abuja", 9.07, 7.4, "Aw", 26.0, 25.0, 1400.), ("Yaounde", 3.87, 11.5, "Aw", 24.5, 23.0, 1600.), ("Kinshasa", -4.3, 15.3, "Aw", 26.5, 23.0, 1400.),
    ("Kisangani", 0.5, 25.2, "Af", 25.0, 24.5, 1700.), ("Addis Ababa", 9.0, 38.75, "Cwb", 16.0, 16.0, 1150.), ("Nairobi", -1.3, 36.8, "Cwb", 19.5, 16.5, 900.),
    ("Mogadishu", 2.05, 45.3, "BSh", 27.0, 26.0, 430.), ("Dar es Salaam", -6.8, 39.3, "Aw", 28.0, 24.0, 1100.), ("Lusaka", -15.4, 28.3, "Cwa", 21.5, 15.0, 830.),
    ("Luanda", -8.84, 13.23, "BSh", 26.5, 20.5, 400.), ("Windhoek", -22.56, 17.08, "BSh", 23.5, 13.5, 360.), ("Johannesburg", -26.2, 28.0, "Cwb", 20.0, 10.5, 710.),
    ("Cape Town", -33.9, 18.4, "Csb", 21.0, 12.5, 515.), ("Antananarivo", -18.9, 47.5, "Cwb", 21.0, 14.5, 1350.),
    ("Darwin", -12.46, 130.84, "Aw", 28.5, 25.0, 1700.), ("Alice Springs", -23.7, 133.9, "BWh", 29.0, 12.0, 280.), ("Perth", -31.95, 115.86, "Csa", 24.5, 13.0, 730.),
    ("Adelaide", -34.93, 138.6, "Csa", 23.0, 11.5, 540.), ("Melbourne", -37.8, 145.0, "Cfb", 20.5, 10.0, 650.), ("Sydney", -33.9, 151.2, "Cfa", 23.0, 13.0, 1200.),
    ("Brisbane", -27.47, 153.0, "Cfa", 25.5, 15.5, 1100.), ("Auckland", -36.85, 174.76, "Cfb", 19.5, 11.0, 1200.), ("Port Moresby", -9.44, 147.18, "Aw", 27.5, 26.0, 1000.),
    ("Anchorage", 61.2, -149.9, "Dfc", -8.5, 15.0, 410.), ("Fairbanks", 64.84, -147.7, "Dfc", -22.0, 17.0, 280.), ("Utqiagvik", 71.3, -156.8, "ET", -25.0, 5.0, 115.),
    ("Vancouver", 49.25, -123.1, "Csb", 4.0, 18.0, 1200.), ("Seattle", 47.6, -122.3, "Csb", 5.5, 19.0, 950.), ("San Francisco", 37.77, -122.42, "Csb", 10.5, 15.0, 600.),
    ("Los Angeles", 34.05, -118.25, "Csb", 14.5, 22.5, 380.), ("Phoenix", 33.45, -112.07, "BWh", 13.5, 35.0, 200.), ("Salt Lake City", 40.76, -111.9, "BSk", -1.0, 26.5, 400.),
    ("Denver", 39.7, -105.0, "BSk", -0.5, 23.5, 400.), ("Edmonton", 53.5, -113.5, "Dfb", -10.5, 17.5, 450.), ("Winnipeg", 49.9, -97.1, "Dfb", -16.5, 19.5, 520.),
    ("Churchill", 58.77, -94.17, "Dfc", -26.0, 12.5, 430.), ("Iqaluit", 63.75, -68.5, "ET", -27.0, 8.0, 400.), ("Chicago", 41.9, -87.6, "Dfa", -4.0, 24.0, 950.),
    ("St Louis", 38.63, -90.2, "Cfa", 0.0, 27.0, 1040.), ("Dallas", 32.8, -96.8, "Cfa", 8.0, 30.0, 950.), ("Houston", 29.76, -95.37, "Cfa", 12.0, 29.0, 1250.),
    ("Atlanta", 33.75, -84.4, "Cfa", 6.5, 27.0, 1270.), ("Miami", 25.8, -80.4, "Am", 20.0, 29.0, 1550.), ("New York", 40.7, -74.0, "Cfa", 0.5, 25.0, 1200.),
    ("Montreal", 45.5, -73.57, "Dfb", -10.0, 21.0, 1000.), ("Mexico City", 19.43, -99.13, "Cwb", 14.0, 17.5, 820.), ("Havana", 23.1, -82.4, "Aw", 22.0, 28.0, 1200.),
    ("Bogota", 4.7, -74.07, "Cfb", 13.0, 13.0, 1000.), ("Quito", -0.2, -78.5, "Cfb", 14.0, 14.0, 1100.), ("Manaus", -3.1, -60.0, "Af", 26.5, 27.0, 2300.),
    ("Belem", -1.45, -48.5, "Af", 26.0, 26.5, 2900.), ("Brasilia", -15.8, -47.9, "Aw", 21.5, 19.0, 1500.), ("Sao Paulo", -23.55, -46.63, "Cfa", 22.5, 16.0, 1450.),
    ("Asuncion", -25.3, -57.6, "Cfa", 28.0, 18.0, 1400.), ("Buenos Aires", -34.6, -58.4, "Cfa", 25.0, 11.0, 1200.), ("Cordoba AR", -31.4, -64.2, "Cwa", 24.0, 11.0, 800.),
    ("Lima", -12.05, -77.04, "BWh", 23.0, 17.0, 10.), ("La Paz", -16.5, -68.15, "Cwc", 9.5, 5.5, 600.), ("Antofagasta", -23.65, -70.4, "BWk", 20.0, 14.0, 2.),
    ("Santiago", -33.45, -70.67, "Csb", 21.0, 8.5, 330.), ("Punta Arenas", -53.16, -70.9, "Cfc", 10.5, 1.5, 400.),
    ("Nuuk", 64.2, -51.7, "ET", -8.0, 7.0, 750.), ("Svalbard", 78.2, 15.6, "ET", -12.0, 6.5, 200.), ("Greenland summit", 72.6, -38.5, "EF", -41.0, -12.0, 250.),
    ("McMurdo", -77.85, 166.67, "EF", -3.0, -26.0, 200.), ("Vostok", -78.5, 106.8, "EF", -32.0, -67.0, 20.), ("South Pole", -89.5, 0.0, "EF", -28.0, -60.0, 5.),
];

/// Real heights (m) of the places that are not near sea level; the rest count
/// as 100 m. Simulated temperatures are moved to the real height at the
/// free-air lapse rate, since a climate cell seldom sits at a city's height.
const HIGH: &[(&str, f64)] = &[
    ("Tehran", 1200.), ("Salt Lake City", 1300.), ("Denver", 1600.), ("Lhasa", 3650.), ("Mexico City", 2240.), ("Bogota", 2550.), ("Quito", 2850.),
    ("La Paz", 3650.), ("Addis Ababa", 2355.), ("Nairobi", 1795.), ("Johannesburg", 1750.), ("Kabul", 1800.), ("Kunming", 1890.), ("Ulaanbaatar", 1350.),
    ("Lanzhou", 1520.), ("Urumqi", 800.), ("Kashgar", 1290.), ("Ankara", 890.), ("Madrid", 660.), ("Windhoek", 1700.), ("Antananarivo", 1280.),
    ("Brasilia", 1100.), ("Sao Paulo", 760.), ("Lusaka", 1280.), ("Santiago", 520.), ("Alice Springs", 550.), ("Riyadh", 610.), ("Tashkent", 450.),
    ("Astana", 350.), ("Irkutsk", 450.), ("Edmonton", 670.), ("Greenland summit", 3200.), ("Vostok", 3490.), ("South Pole", 2835.), ("Phoenix", 340.),
    ("Cordoba AR", 400.), ("Omsk", 90.), ("Novosibirsk", 150.), ("Yakutsk", 100.), ("Verkhoyansk", 130.), ("Winnipeg", 240.), ("Chicago", 180.),
    ("Dallas", 150.), ("Atlanta", 300.), ("Moscow", 150.), ("Kyiv", 170.), ("Harbin", 150.), ("Delhi", 215.), ("Timbuktu", 260.), ("Niamey", 220.),
    ("Khartoum", 380.), ("Abuja", 480.), ("Yaounde", 730.), ("Kinshasa", 300.), ("Kisangani", 400.), ("Manaus", 60.), ("Asuncion", 100.),
    ("Chongqing", 250.), ("Fairbanks", 135.), ("Urumqi", 800.), ("Baghdad", 35.), ("St Louis", 150.), ("Montreal", 40.), ("Wuhan", 25.),
];

/// Observed zonal means in 10° bands from 90°S to 90°N: annual surface air
/// temperature (°C, all surfaces) and precipitation (mm/year).
const ZT: [f64; 18] = [-37.0, -20.0, -8.5, 1.2, 8.9, 15.2, 20.6, 24.1, 25.8, 26.5, 26.0, 22.9, 17.2, 10.0, 2.3, -6.0, -14.0, -18.0];
const ZP: [f64; 18] = [80., 200., 600., 1000., 1050., 850., 700., 1150., 1500., 2000., 1100., 650., 800., 900., 800., 500., 250., 150.];

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (hm, out) = (&a[1], &a[2]);
    let level: u32 = a.get(3).map(|s| s.parse().unwrap()).unwrap_or(7);
    std::fs::create_dir_all(out).unwrap();
    let mut p = WorldParams::default();
    p.planet.grid_level = level;
    // "-" instead of a heightmap: a generated world (seed as the fourth
    // argument), for a look at its maps; the station scores then mean nothing.
    if hm == "-" {
        p.planet.seed = a.get(4).map(|s| s.parse().unwrap()).unwrap_or(1);
    }
    let mut w = World::new(p);
    if hm != "-" {
        w.edits.imports.elevation = Some(ElevationImport { path: hm.clone(), encoding: HeightEncoding::Heightmap16, lat_min: -90.0, lat_max: 90.0, blend: 1.0, content_hash: String::new() });
    }
    let t0 = std::time::Instant::now();
    w.run_to(Step::Relief, &|_, _, _| {});
    let t1 = std::time::Instant::now();
    w.run_to(Step::Climate, &|_, _, _| {});
    let climate_s = t1.elapsed().as_secs_f64();
    let _ = t0;
    let g = w.grid();
    let n = g.len();
    let temp = f32s(&w, "temp");
    let precip = f32s(&w, "precip");
    let (wu, wv) = (f32s(&w, "wind_u"), f32s(&w, "wind_v"));
    let e = f32s(&w, "elevation");
    let sea: Vec<bool> = e.iter().map(|&x| x <= 0.0).collect();
    let tm = |i: usize, m: usize| temp[m * n + i] as f64;
    let pa = |i: usize| (0..12).map(|m| precip[m * n + i] as f64).sum::<f64>();
    let kop = |i: usize| -> &'static str {
        let t: [f64; 12] = std::array::from_fn(|m| tm(i, m));
        let pr: [f64; 12] = std::array::from_fn(|m| precip[m * n + i] as f64);
        KOPPEN[koppen(&t, &pr, g.lat[i] >= 0.0, 0.0) as usize].0
    };

    // Stations.
    let cont_f = f32s(&w, "continentality");
    let mut rep = String::new();
    rep += &format!("{:<17} {:>4} {:>4} | {:>6} {:>6} {:>5} | {:>6} {:>6} {:>5} | {:>6} {:>6} {:>5} | {:>5}\n", "station", "real", "sim", "Jan", "sim", "err", "Jul", "sim", "err", "P", "sim", "x", "elev");
    let (mut bj, mut bl, mut aj, mut al, mut lp, mut lpb, mut grp, mut exact) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0, 0);
    let mut rows = Vec::new();
    for &(nm, la, lo, kr, tj, tl, pr) in ST {
        let mut c = g.nearest(Vec3::from_lat_lon_deg(la, lo), None);
        if sea[c] {
            // Nearest land cell within two rings.
            let pc = Vec3::from_lat_lon_deg(la, lo);
            let mut best: Option<(usize, f64)> = None;
            for &j in g.neighbors(c) {
                for &k in std::iter::once(&j).chain(g.neighbors(j as usize)) {
                    let k = k as usize;
                    if !sea[k] {
                        let d = g.pos[k].angle_to(pc);
                        if best.is_none_or(|b| d < b.1) {
                            best = Some((k, d));
                        }
                    }
                }
            }
            if let Some((k, _)) = best {
                c = k;
            }
        }
        let real_h = HIGH.iter().find(|x| x.0 == nm).map_or(100.0, |x| x.1);
        let shift = 0.0065 * (e[c].max(0.0) as f64 - real_h);
        let ks = if sea[c] {
            "sea"
        } else {
            let t: [f64; 12] = std::array::from_fn(|m| tm(c, m) + shift);
            let pr: [f64; 12] = std::array::from_fn(|m| precip[m * n + c] as f64);
            KOPPEN[koppen(&t, &pr, g.lat[c] >= 0.0, 0.0) as usize].0
        };
        let (sj, sl, sp) = (tm(c, 0) + shift, tm(c, 6) + shift, pa(c));
        bj += sj - tj;
        bl += sl - tl;
        aj += (sj - tj).abs();
        al += (sl - tl).abs();
        let lf = ((sp + 20.0) / (pr + 20.0)).ln();
        lp += lf.abs();
        lpb += lf;
        if ks.chars().next() == kr.chars().next() {
            grp += 1;
        }
        if ks == kr {
            exact += 1;
        }
        if std::env::var("FWM_PROBE").is_ok_and(|v| v.split(',').any(|x| x == nm)) {
            println!("{nm} (cell elev {:.0} m, cont {:.2})", e[c], cont_f[c]);
            println!("   T  {}", (0..12).map(|m| format!("{:6.1}", tm(c, m) + shift)).collect::<String>());
            println!("   P  {}", (0..12).map(|m| format!("{:6.0}", precip[m * n + c])).collect::<String>());
            println!("   u  {}", (0..12).map(|m| format!("{:6.1}", wu[m * n + c])).collect::<String>());
            println!("   v  {}", (0..12).map(|m| format!("{:6.1}", wv[m * n + c])).collect::<String>());
        }
        rep += &format!("{:<17} {:>4} {:>4} | {:>6.1} {:>6.1} {:>+5.1} | {:>6.1} {:>6.1} {:>+5.1} | {:>6.0} {:>6.0} {:>5.2} | {:>5.0}\n", nm, kr, ks, tj, sj, sj - tj, tl, sl, sl - tl, pr, sp, (sp + 20.0) / (pr + 20.0), e[c]);
        rows.push(serde_json::json!({"name": nm, "lat": la, "lon": lo, "k_real": kr, "k_sim": ks, "jan": sj, "jul": sl, "p": sp, "djan": sj - tj, "djul": sl - tl, "pf": (sp + 20.0) / (pr + 20.0), "cont": cont_f[c], "elev": e[c]}));
    }
    let ns = ST.len() as f64;
    let summary = format!(
        "stations {}: Jan bias {:+.1} MAE {:.1} | Jul bias {:+.1} MAE {:.1} | P factor x{:.2} (bias x{:.2}) | Köppen group {:.0}% exact {:.0}%",
        ST.len(), bj / ns, aj / ns, bl / ns, al / ns, (lp / ns).exp(), (lpb / ns).exp(), 100.0 * grp as f64 / ns, 100.0 * exact as f64 / ns
    );

    // Zonal means.
    let mut zon = String::from("lat band     T sim  T obs   err |  P sim  P obs | T sea  T land | land P\n");
    let (mut zt_mae, mut zp_lf) = (0.0, 0.0);
    for b in 0..18 {
        let (lo, hi) = (-90.0 + 10.0 * b as f64, -80.0 + 10.0 * b as f64);
        let (mut ar, mut ta, mut pp, mut als, mut ts, mut all, mut tl, mut pl) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        for i in 0..n {
            let la = g.lat[i].to_degrees();
            if la < lo || la >= hi {
                continue;
            }
            let t = (0..12).map(|m| tm(i, m)).sum::<f64>() / 12.0;
            ar += g.area[i];
            ta += g.area[i] * t;
            pp += g.area[i] * pa(i);
            if sea[i] {
                als += g.area[i];
                ts += g.area[i] * t;
            } else {
                all += g.area[i];
                tl += g.area[i] * t;
                pl += g.area[i] * pa(i);
            }
        }
        let f = |s: f64, a: f64| if a > 0.0 { format!("{:6.1}", s / a) } else { "     -".into() };
        zt_mae += (ta / ar - ZT[b]).abs() / 18.0;
        zp_lf += ((pp / ar) / ZP[b]).ln().abs() / 18.0;
        zon += &format!("{:>4.0}..{:<4.0} {:>7.1} {:>6.1} {:>+5.1} | {:>6.0} {:>6.0} | {} {} | {}\n", lo, hi, ta / ar, ZT[b], ta / ar - ZT[b], pp / ar, ZP[b], f(ts, als), f(tl, all), f(pl, all));
    }
    let zsum = format!("zonal: T MAE {:.1} °C, P factor x{:.2}; climate step {:.1} s at level {}", zt_mae, zp_lf.exp(), climate_s, level);

    // Land Köppen shares.
    let mut cls = std::collections::BTreeMap::<&str, f64>::new();
    let mut land_a = 0.0;
    let kcell: Vec<&str> = (0..n).map(|i| if sea[i] { "sea" } else { kop(i) }).collect();
    for i in 0..n {
        if !sea[i] {
            land_a += g.area[i];
            *cls.entry(kcell[i]).or_default() += g.area[i];
        }
    }
    let mut grp_s = std::collections::BTreeMap::<char, f64>::new();
    for (c, a) in &cls {
        *grp_s.entry(c.chars().next().unwrap()).or_default() += a / land_a;
    }
    let ksum = format!("Köppen land shares: {} (Earth about A 19, B 30, C 14, D 24, E 13)", grp_s.iter().map(|(g, s)| format!("{g} {:.0}", s * 100.0)).collect::<Vec<_>>().join(", "));

    // The energy-balance model's own profile.
    let mut ebm = String::from("\nEBM profile: lat  annual | ocean Jan Jul | land Jan Jul\n");
    if let Some(z) = w.meta(Step::Climate).and_then(|m| m["zonal"].as_array().cloned()) {
        for r in z.iter().step_by(2) {
            let f = |k: &str| r[k].as_f64().unwrap_or(0.0);
            ebm += &format!("  {:>5.0} {:>7.1} | {:>6.1} {:>6.1} | {:>6.1} {:>6.1}\n", f("lat"), f("t_annual"), f("t_ocean_jan"), f("t_ocean_jul"), f("t_land_jan"), f("t_land_jul"));
        }
    }
    println!("{summary}\n{zsum}\n{ksum}");
    std::fs::write(format!("{out}/report.txt"), format!("{summary}\n{zsum}\n{ksum}\n\n{rep}\n{zon}{ebm}")).unwrap();
    std::fs::write(format!("{out}/stations.json"), serde_json::to_string(&rows).unwrap()).unwrap();

    // Maps (equirectangular).
    let (wd, ht) = (1440usize, 720usize);
    let mut cell = vec![0usize; wd * ht];
    let mut hint = None;
    for y in 0..ht {
        for x in 0..wd {
            let c = g.nearest(Vec3::from_lat_lon_deg(90.0 - (y as f64 + 0.5) * 180.0 / ht as f64, -180.0 + (x as f64 + 0.5) * 360.0 / wd as f64), hint);
            hint = Some(c);
            cell[y * wd + x] = c;
        }
    }
    let save = |name: &str, img: &[u8]| {
        let f = std::fs::File::create(format!("{out}/{name}.png")).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(f), wd as u32, ht as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().unwrap().write_image_data(img).unwrap();
    };
    let coast = |img: &mut [u8]| {
        for y in 1..ht - 1 {
            for x in 0..wd {
                let s = sea[cell[y * wd + x]];
                if s != sea[cell[y * wd + (x + 1) % wd]] || s != sea[cell[(y + 1) * wd + x]] {
                    img[(y * wd + x) * 3..(y * wd + x) * 3 + 3].copy_from_slice(&[20, 20, 20]);
                }
            }
        }
    };
    // Köppen.
    let mut img = vec![0u8; wd * ht * 3];
    for (k, &c) in cell.iter().enumerate() {
        let col = if sea[c] { [24u8, 52, 96] } else { KOPPEN.iter().find(|x| x.0 == kcell[c]).unwrap().1 };
        img[k * 3..k * 3 + 3].copy_from_slice(&col);
    }
    save("koppen", &img);
    // Precipitation (log scale) with wind arrows; temperature.
    let ramp = |s: f64, stops: &[(f64, [f64; 3])]| -> [u8; 3] {
        let s = s.clamp(0.0, 1.0);
        let mut col = stops[0].1;
        for w2 in stops.windows(2) {
            if s >= w2[0].0 && s <= w2[1].0 {
                let u = (s - w2[0].0) / (w2[1].0 - w2[0].0);
                for q in 0..3 {
                    col[q] = w2[0].1[q] + u * (w2[1].1[q] - w2[0].1[q]);
                }
            }
        }
        [col[0] as u8, col[1] as u8, col[2] as u8]
    };
    let pstops = [(0.0, [235.0, 220.0, 170.0]), (0.38, [200.0, 200.0, 90.0]), (0.62, [70.0, 170.0, 70.0]), (0.8, [20.0, 120.0, 160.0]), (1.0, [30.0, 40.0, 150.0])];
    let tstops = [(0.0, [60.0, 0.0, 120.0]), (0.25, [40.0, 90.0, 220.0]), (0.45, [150.0, 220.0, 240.0]), (0.55, [240.0, 240.0, 170.0]), (0.75, [240.0, 140.0, 40.0]), (1.0, [150.0, 0.0, 0.0])];
    let pmap = |name: &str, months: &[usize], arrows: bool| {
        let mut img = vec![0u8; wd * ht * 3];
        let scale = 12.0 / months.len() as f64;
        for (k, &c) in cell.iter().enumerate() {
            let v: f64 = months.iter().map(|&m| precip[m * n + c] as f64).sum::<f64>() * scale;
            img[k * 3..k * 3 + 3].copy_from_slice(&ramp((v.max(10.0).log10() - 1.0) / 2.6, &pstops));
        }
        coast(&mut img);
        if arrows {
            // Arrows from each grid point downwind; a square marks the tail.
            let mut put = |x: f64, y: f64, col: [u8; 3]| {
                let (xi, yi) = ((x.round() as i64).rem_euclid(wd as i64) as usize, (y.round() as i64).clamp(0, ht as i64 - 1) as usize);
                img[(yi * wd + xi) * 3..(yi * wd + xi) * 3 + 3].copy_from_slice(&col);
            };
            for gy in (10..ht - 5).step_by(20) {
                for gx in (10..wd).step_by(20) {
                    let c = cell[gy * wd + gx];
                    let (u, v) = (months.iter().map(|&m| wu[m * n + c] as f64).sum::<f64>() / months.len() as f64, months.iter().map(|&m| wv[m * n + c] as f64).sum::<f64>() / months.len() as f64);
                    let sp = (u * u + v * v).sqrt();
                    if sp < 0.3 {
                        continue;
                    }
                    // 2 px per m/s, at most 18 px.
                    let len = (2.0 * sp).min(18.0);
                    let (dx, dy) = (u / sp, -v / sp);
                    let (x1, y1) = (gx as f64 + dx * len, gy as f64 + dy * len);
                    for s in 0..=(len as usize * 2) {
                        let t = s as f64 / (len * 2.0);
                        put(gx as f64 + dx * len * t, gy as f64 + dy * len * t, [0, 0, 0]);
                    }
                    // Head.
                    for (hx, hy) in [(-dx - dy, -dy + dx), (-dx + dy, -dy - dx)] {
                        for s in 0..=5 {
                            put(x1 + hx * 0.7 * s as f64, y1 + hy * 0.7 * s as f64, [0, 0, 0]);
                        }
                    }
                    put(gx as f64, gy as f64, [255, 255, 255]);
                }
            }
        }
        save(name, &img);
    };
    {
        let cont = f32s(&w, "continentality");
        let mut img = vec![0u8; wd * ht * 3];
        for (k, &c) in cell.iter().enumerate() {
            let v = (cont[c].clamp(0.0, 1.0) * 255.0) as u8;
            img[k * 3..k * 3 + 3].copy_from_slice(&[v, (v as f32 * 0.8) as u8, 255 - v]);
        }
        coast(&mut img);
        save("cont", &img);
    }
    pmap("p_ann", &(0..12).collect::<Vec<_>>(), false);
    pmap("p_jan", &[11, 0, 1], true);
    pmap("p_jul", &[5, 6, 7], true);
    for (name, m) in [("t_jan", 0usize), ("t_jul", 6usize)] {
        let mut img = vec![0u8; wd * ht * 3];
        for (k, &c) in cell.iter().enumerate() {
            img[k * 3..k * 3 + 3].copy_from_slice(&ramp((tm(c, m) + 45.0) / 85.0, &tstops));
        }
        coast(&mut img);
        save(name, &img);
    }
}
