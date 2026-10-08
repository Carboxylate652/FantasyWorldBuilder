use worldcore::edits::{HeightEncoding, Stroke, Tool};
use worldcore::export::{export, ExportOptions};
use worldcore::grid::{cell_count, Grid};
use worldcore::params::WorldParams;
use worldcore::stages::{biomes::koppen, biomes::KOPPEN, climate::insolation, Step, LAST, STEPS};
use worldcore::vec3::Vec3;
use worldcore::World;

fn small_params(seed: u64) -> WorldParams {
    let mut p = WorldParams::default();
    p.planet.seed = seed;
    p.planet.grid_level = 5;
    p.climate.climate_level = 5;
    p
}

#[test]
fn grid_counts_and_topology() {
    for level in 0..=5 {
        let g = Grid::new(level);
        assert_eq!(g.len(), cell_count(level));
        let mut pent = 0;
        for i in 0..g.len() {
            let d = g.neighbors(i).len();
            assert!(d == 5 || d == 6, "cell {i} has {d} neighbours");
            if d == 5 {
                pent += 1;
            }
        }
        assert_eq!(pent, 12);
        let total: f64 = g.area.iter().sum();
        assert!((total - 4.0 * std::f64::consts::PI).abs() < 1e-9);
    }
}

#[test]
fn hierarchical_vertex_order() {
    let (a, b) = (Grid::new(3), Grid::new(4));
    for i in 0..a.len() {
        assert!((a.pos[i] - b.pos[i]).len() < 1e-12);
    }
}

#[test]
fn locate_returns_containing_triangle() {
    let g = Grid::new(4);
    for k in 0..500 {
        let p = Vec3::from_lat_lon_deg(-89.0 + (k as f64 * 37.3) % 178.0, -180.0 + (k as f64 * 71.9) % 360.0);
        let (v, tri, w) = g.locate(p, None);
        assert!((w.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        assert!(tri.contains(&(v as u32)));
        let interp = (g.pos[tri[0] as usize] * w[0] + g.pos[tri[1] as usize] * w[1] + g.pos[tri[2] as usize] * w[2]).normalized();
        assert!(interp.angle_to(p) < g.spacing * 0.05, "barycentric point far from query");
    }
}

#[test]
fn koppen_matches_real_stations() {
    let cases: [(&str, [f64; 12], [f64; 12], bool); 5] = [
        ("Cfb", [5.2, 5.3, 7.6, 9.9, 13.3, 16.5, 18.7, 18.5, 15.7, 12.0, 8.0, 5.5], [55., 41., 42., 44., 49., 45., 45., 50., 49., 69., 59., 55.], true), // London
        ("BWh", [14.0, 15.0, 18.0, 21.5, 25.0, 27.5, 28.5, 28.5, 26.5, 24.0, 19.5, 15.5], [5., 4., 3., 1., 0., 0., 0., 0., 0., 1., 3., 6.], true), // Cairo
        ("Af", [26.5, 27.1, 27.5, 27.9, 28.3, 28.3, 27.9, 27.9, 27.6, 27.6, 27.0, 26.5], [234., 115., 170., 166., 171., 130., 159., 176., 169., 194., 256., 288.], true), // Singapore
        ("Dfb", [-6.2, -5.9, -0.7, 6.7, 13.2, 17.0, 19.2, 17.0, 11.3, 5.6, -0.4, -4.4], [52., 41., 35., 37., 49., 80., 85., 82., 68., 71., 55., 52.], true), // Moscow
        ("Csa", [11.0, 12.0, 14.0, 17.0, 21.0, 25.5, 28.5, 28.5, 25.0, 20.5, 15.5, 12.0], [70., 75., 56., 50., 30., 12., 6., 13., 60., 104., 109., 86.], true), // Athens-like
    ];
    for (want, t, p, north) in cases {
        let got = KOPPEN[koppen(&t, &p, north, 0.0) as usize].0;
        assert_eq!(got, want);
    }
}

#[test]
fn insolation_is_earth_like() {
    let p = WorldParams::default();
    let annual = |lat: f64| (0..365).map(|d| insolation(&p, lat.to_radians(), (d as f64 + 0.5) / 365.0)).sum::<f64>() / 365.0;
    assert!((annual(0.0) - 418.0).abs() < 10.0, "equator {}", annual(0.0));
    assert!((annual(89.9) - 173.0).abs() < 10.0, "pole {}", annual(89.9));
}

#[test]
fn deterministic_and_round_trips() {
    let mut a = World::new(small_params(7));
    a.edits.add_stroke(Stroke { tool: Tool::Land, radius_km: 900.0, points: vec![[10.0, 20.0], [15.0, 40.0]], ..Default::default() });
    a.edits.add_stroke(Stroke { tool: Tool::Mountain, radius_km: 400.0, points: vec![[12.0, 25.0], [14.0, 35.0]], ..Default::default() });
    let mut b = World::new(a.params.clone());
    b.edits = a.edits.clone();
    a.run_to(LAST, &|_, _, _| {});
    b.run_to(LAST, &|_, _, _| {});
    assert_eq!(a.fingerprint(), b.fingerprint());

    let dir = std::env::temp_dir().join(format!("fwm-test-{}", std::process::id()));
    a.save(&dir).unwrap();
    let c = World::load(&dir).unwrap();
    assert_eq!(c.fingerprint(), a.fingerprint());
    assert_eq!(c.edits, a.edits);
    assert!(STEPS.iter().all(|&s| c.is_fresh(s)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edits_only_invalidate_later_steps() {
    let mut w = World::new(small_params(3));
    w.run_to(Step::Biomes, &|_, _, _| {});
    w.params.hydrology.river_threshold_m3s = 50.0;
    let st = w.status();
    assert!(st[..5].iter().all(|s| s.state == "done"));
    assert!(st[5..7].iter().all(|s| s.state == "stale"));
    assert!(st[7..].iter().all(|s| s.state == "empty"));
    let ran = w.run_to(Step::Biomes, &|_, _, _| {});
    assert_eq!(ran, vec![Step::Hydrology, Step::Biomes]);
}

#[test]
fn climate_is_plausible() {
    let mut w = World::new(small_params(11));
    w.run_to(Step::Climate, &|_, _, _| {});
    let m = w.meta(Step::Climate).unwrap();
    let t = m["global_mean_temp_c"].as_f64().unwrap();
    assert!((5.0..25.0).contains(&t), "global mean {t}");
    let zonal = m["zonal"].as_array().unwrap();
    let eq = zonal.iter().find(|z| z["lat"].as_f64() == Some(0.0)).unwrap()["t_annual"].as_f64().unwrap();
    let pole = zonal.iter().find(|z| z["lat"].as_f64() == Some(90.0)).unwrap()["t_annual"].as_f64().unwrap();
    assert!(eq > 20.0 && pole < -5.0, "equator {eq}, pole {pole}");
}

#[test]
fn heightmap_export_import_round_trip() {
    let mut w = World::new(small_params(5));
    w.run_to(Step::Relief, &|_, _, _| {});
    let dir = std::env::temp_dir().join(format!("fwm-hm-{}", std::process::id()));
    let opts = ExportOptions { width: 1024, height: 512, detail_noise: false, ..Default::default() };
    export(&mut w, &dir, &opts, &|_, _| {}).unwrap();
    let before = match w.field("elevation") {
        Some((worldcore::fields::Field::F32(v), _, _)) => v.clone(),
        _ => panic!(),
    };
    let path = dir.join("heightmap16.png").display().to_string();
    w.edits.imports.elevation = Some(worldcore::import::describe(&path, HeightEncoding::Heightmap16, -90.0, 90.0, 1.0).unwrap());
    assert!(!w.is_fresh(Step::Relief));
    w.run_to(Step::Relief, &|_, _, _| {});
    let after = match w.field("elevation") {
        Some((worldcore::fields::Field::F32(v), _, _)) => v.clone(),
        _ => panic!(),
    };
    let mean_err = before.iter().zip(&after).map(|(a, b)| (a - b).abs() as f64).sum::<f64>() / before.len() as f64;
    assert!(mean_err < 60.0, "mean elevation error {mean_err} m");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Reference values for app/src/noise.ts (the UI's port of the noise); keep in sync.
#[test]
fn scatter_noise_reference_values() {
    let n = worldcore::noise::Noise::new(123456789, worldcore::rng::stream::SCATTER_BRUSH);
    let pts = [Vec3::new(0.6, 0.0, 0.8), Vec3::new(-0.36, 0.48, -0.8), Vec3::new(0.0, -1.0, 0.0)];
    let got: Vec<f64> = pts.iter().map(|&p| n.fbm(p, 25.484, 5)).collect();
    println!("NOISE_REF {got:?}");
    assert!(got.iter().all(|v| v.abs() <= 1.2));
}

/// Stage 3: cultures emerge and settle the land, the family tree is consistent
/// (parents existed before their daughters, ended cultures have a fate),
/// provinces carry cultures and populations, and the export writes the tables.
#[test]
fn cultures_emerge_and_are_consistent() {
    use worldcore::fields::Field;
    let mut p = WorldParams::default();
    p.planet.seed = 4;
    p.planet.grid_level = 6;
    p.climate.climate_level = 6;
    p.cultures.ticks = 200;
    p.cultures.max_bands = 600;
    let mut w = World::new(p);
    w.run_to(LAST, &|_, _, _| {});
    let m = w.meta(Step::Cultures).unwrap();
    let t = &m["table"];
    let cultures = t["cultures"].as_array().unwrap();
    let alive: Vec<&serde_json::Value> = cultures.iter().filter(|c| c["alive"] == true).collect();
    assert!(alive.len() >= 3, "only {} cultures", alive.len());
    assert!(m["population"].as_f64().unwrap() > 0.5 * m["capacity"].as_f64().unwrap(), "land barely settled: {m}");
    let by_id: std::collections::HashMap<u64, &serde_json::Value> = cultures.iter().map(|c| (c["id"].as_u64().unwrap(), c)).collect();
    let groups = t["groups"].as_array().unwrap().len() as u64;
    for c in cultures {
        let parent = c["parent"].as_u64().unwrap();
        if parent > 0 {
            assert!(by_id[&parent]["founded_year"].as_f64().unwrap() < c["founded_year"].as_f64().unwrap(), "culture {} older than its parent", c["id"]);
        }
        if c["alive"] == true {
            assert!(c["population"].as_f64().unwrap() > 0.0);
            assert!((1..=groups).contains(&c["group"].as_u64().unwrap()));
        } else {
            assert!(c["fate"] == "merged" || c["fate"] == "extinct");
            assert!(c["ended_year"].as_f64().unwrap() >= c["founded_year"].as_f64().unwrap());
        }
    }
    for e in t["events"].as_array().unwrap() {
        assert!(by_id.contains_key(&e["culture"].as_u64().unwrap()));
    }
    // Province cultures agree with the cell field and are living cultures.
    let cult = match w.field("culture") { Some((Field::U16(v), _, _)) => v.clone(), _ => panic!("culture field") };
    let prov = match w.field("province") { Some((Field::U32(v), _, _)) => v.clone(), _ => panic!("province field") };
    let pc: std::collections::HashMap<u64, u64> = t["provinces"].as_array().unwrap().iter().map(|p| (p["id"].as_u64().unwrap(), p["culture"].as_u64().unwrap())).collect();
    for i in 0..cult.len() {
        if let Some(&c) = pc.get(&(prov[i] as u64)) {
            assert_eq!(cult[i] as u64, c);
            if c > 0 {
                assert_eq!(by_id[&c]["alive"], true);
            }
        }
    }
    let dir = std::env::temp_dir().join(format!("fwm-cult-{}", std::process::id()));
    let opts = ExportOptions { width: 1024, height: 512, ..Default::default() };
    let r = export(&mut w, &dir, &opts, &|_, _| {}).unwrap();
    for f in ["cultures.png", "cultures.csv", "culture_groups.csv", "culture_events.csv", "province_cultures.csv"] {
        assert!(r.files.iter().any(|x| x == f), "{f} not exported");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Settlement sites and resources: springs only appear in dry land, land
/// provinces get trade goods and some deposits, and a site pin in a desert
/// grows a town of about the population it asks for.
#[test]
fn springs_resources_and_site_pins() {
    use worldcore::edits::{Stroke, Tool};
    use worldcore::fields::Field;
    let mut p = WorldParams::default();
    p.planet.seed = 4;
    p.planet.grid_level = 6;
    p.climate.climate_level = 6;
    p.cultures.ticks = 200;
    let mut w = World::new(p.clone());
    w.run_to(LAST, &|_, _, _| {});
    let f32f = |w: &World, n: &str| match w.field(n) { Some((Field::F32(v), _, _)) => v.clone(), _ => panic!("{n}") };
    let site = f32f(&w, "site");
    let rain = f32f(&w, "p_ann");
    for i in 0..site.len() {
        if site[i] > 0.0 {
            assert!(rain[i] < p.habitability.spring_max_precip_mm as f32, "spring in wet land at cell {i}");
        }
    }
    let provs = w.meta(Step::Provinces).unwrap()["table"]["provinces"].as_array().unwrap().clone();
    let land: Vec<&serde_json::Value> = provs.iter().filter(|p| p["kind"] == "land").collect();
    assert!(land.iter().all(|p| p["trade_good"].is_string()));
    let with_deposits = land.iter().filter(|p| !p["resources"].as_array().unwrap().is_empty()).count();
    assert!(with_deposits > 0 && with_deposits < land.len(), "{with_deposits} of {} land provinces have deposits", land.len());
    assert!(w.meta(Step::Cultures).unwrap()["desert_towns"].is_u64());

    // Pin a town in the driest land province that has people around.
    let dry = land.iter().filter(|p| !p["river"].as_bool().unwrap()).min_by(|a, b| a["rain_mm"].as_f64().partial_cmp(&b["rain_mm"].as_f64()).unwrap()).unwrap();
    let (lat, lon) = (dry["center"][0].as_f64().unwrap(), dry["center"][1].as_f64().unwrap());
    let pid = dry["id"].as_u64().unwrap();
    w.edits.add_stroke(Stroke { tool: Tool::SitePin, value: 40_000.0, points: vec![[lat, lon]], ..Default::default() });
    assert!(!w.is_fresh(Step::Habitability));
    w.run_to(LAST, &|_, _, _| {});
    let prov_field = match w.field("province") { Some((Field::U32(v), _, _)) => v.clone(), _ => panic!() };
    // Provinces are regenerated around the pin: find the one holding it now.
    let g = w.grid();
    let cell = g.nearest(worldcore::vec3::Vec3::from_lat_lon_deg(lat, lon), None);
    let now = prov_field[cell] as u64;
    let c = &w.meta(Step::Cultures).unwrap()["table"]["provinces"];
    let pop = c.as_array().unwrap().iter().find(|p| p["id"].as_u64() == Some(now)).map(|p| p["population"].as_f64().unwrap()).unwrap();
    assert!(pop >= 0.8 * 40_000.0, "pinned town in province {now} (was {pid}) holds only {pop}");
}

/// Stage 2: every land cell is in a state and a province, ids and colours are
/// unique, state tables agree with the cells, and an exported provinces.png
/// imports back to (almost) the same cells.
#[test]
fn states_and_provinces_round_trip() {
    use worldcore::fields::Field;
    let mut p = WorldParams::default();
    p.planet.seed = 4;
    p.planet.grid_level = 6;
    p.climate.climate_level = 6;
    let mut w = World::new(p);
    w.run_to(LAST, &|_, _, _| {});
    let get_u8 = |w: &World, n: &str| match w.field(n) { Some((Field::U8(v), _, _)) => v.clone(), _ => panic!("{n}") };
    let water = get_u8(&w, "water");
    let kind = get_u8(&w, "province_kind");
    let state = match w.field("state") { Some((Field::U16(v), Step::Provinces, _)) => v.clone(), _ => panic!("state") };
    let prov = match w.field("province") { Some((Field::U32(v), _, _)) => v.clone(), _ => panic!("province") };
    for i in 0..prov.len() {
        assert!(prov[i] > 0, "cell {i} has no province");
        if water[i] == 0 {
            assert!(state[i] > 0 && kind[i] <= 1, "land cell {i}: state {} kind {}", state[i], kind[i]);
        }
        if water[i] == 1 {
            assert_eq!(kind[i], 3, "ocean cell {i} is not in a sea zone");
        }
    }
    let t = &w.meta(Step::Provinces).unwrap()["table"];
    let provs = t["provinces"].as_array().unwrap();
    let mut colors = std::collections::HashSet::new();
    let mut ids = std::collections::HashSet::new();
    for p in provs {
        assert!(ids.insert(p["id"].as_u64().unwrap()));
        assert!(colors.insert(p["color"].to_string()), "duplicate colour {}", p["color"]);
    }
    let by_id: std::collections::HashMap<u64, &serde_json::Value> = provs.iter().map(|p| (p["id"].as_u64().unwrap(), p)).collect();
    for s in t["states"].as_array().unwrap() {
        for pid in s["provinces"].as_array().unwrap() {
            let p = provs.iter().find(|p| &p["id"] == pid).unwrap();
            assert_eq!(p["state"], s["id"]);
        }
        // Every state has a capital in it; a land capital carries the state's name.
        let cap = by_id[&s["capital_province"].as_u64().unwrap()];
        assert_eq!(cap["state"], s["id"], "capital of state {} is outside it", s["id"]);
        if cap["kind"] == "land" {
            assert_eq!(cap["name"], s["name"]);
        }
    }
    let mut names = std::collections::HashSet::new();
    for p in provs {
        assert!(names.insert(p["name"].as_str().unwrap()), "province name {} used twice", p["name"]);
    }

    // Every pair of neighbouring provinces has one typed border, and the type fits the kinds.
    let adj = t["adjacency"].as_array().unwrap();
    let mut borders = std::collections::HashMap::new();
    for a in adj {
        let (f, to, ty) = (a["from"].as_u64().unwrap(), a["to"].as_u64().unwrap(), a["type"].as_str().unwrap());
        assert!(f < to);
        assert!(borders.insert((f, to, ty == "strait"), ty).is_none(), "border {f}-{to} listed twice");
        let kinds = [by_id[&f]["kind"].as_str().unwrap(), by_id[&to]["kind"].as_str().unwrap()];
        let land = |k: &str| k == "land" || k == "wasteland";
        let ok = match ty {
            "land" | "river" => kinds == ["land", "land"],
            "impassable" => land(kinds[0]) && land(kinds[1]) && kinds.contains(&"wasteland"),
            "coast" => kinds.contains(&"sea") && kinds.iter().any(|k| land(k)),
            "lake" => kinds.contains(&"lake"),
            "sea" => kinds == ["sea", "sea"],
            "strait" => true,
            _ => false,
        };
        assert!(ok, "border {f}-{to} of type {ty} between {kinds:?}");
    }
    for p in provs {
        let a = p["id"].as_u64().unwrap();
        for b in p["neighbors"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()) {
            assert!(borders.contains_key(&(a.min(b), a.max(b), false)), "neighbours {a} and {b} have no border");
        }
    }

    let n_provs = provs.len();

    let dir = std::env::temp_dir().join(format!("fwm-prov-{}", std::process::id()));
    let opts = ExportOptions { width: 2048, height: 1024, ..Default::default() };
    export(&mut w, &dir, &opts, &|_, _| {}).unwrap();
    let png = dir.join("provinces.png").display().to_string();
    let csv = dir.join("definition.csv").display().to_string();
    let (imp, rep) = worldcore::province_import::describe(&png, Some(&csv), None, None).unwrap();
    assert!(rep.ok, "{:?}", rep.errors);
    // The export clean-up leaves every province some pixels, no X-crossings and
    // (almost) no provinces in pieces.
    for warning in &rep.warnings {
        assert!(!warning.contains("no pixels") && !warning.contains("X-crossings"), "{warning}");
    }
    let pkg: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("package.json")).unwrap()).unwrap();
    let split = pkg["province_cleanup"]["split_provinces"].as_u64().unwrap() as usize;
    assert!(split * 100 <= n_provs, "{split} of {n_provs} provinces are in pieces");
    w.edits.imports.provinces = Some(imp);
    assert!(!w.is_fresh(Step::Provinces) && w.is_fresh(Step::States));
    w.run_to(LAST, &|_, _, _| {});
    let back = match w.field("province") { Some((Field::U32(v), _, _)) => v.clone(), _ => panic!() };
    let same = prov.iter().zip(&back).filter(|(a, b)| a == b).count() as f64 / prov.len() as f64;
    assert!(same > 0.98, "only {:.1}% of cells kept their province", same * 100.0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Map editor and pre-culture edits: fertility paint, province and state
/// merges, moving a province to another state, renames, the goods editor and
/// founding-band pins (add, move, erase).
#[test]
fn map_editor_and_founders() {
    use worldcore::edits::{Stroke, Tool};
    use worldcore::fields::Field;
    let mut p = WorldParams::default();
    p.planet.seed = 4;
    p.planet.grid_level = 6;
    p.climate.climate_level = 6;
    p.cultures.ticks = 60;
    let mut w = World::new(p);
    w.run_to(Step::Provinces, &|_, _, _| {});
    let table = w.meta(Step::Provinces).unwrap()["table"].clone();
    let land: Vec<serde_json::Value> = table["provinces"].as_array().unwrap().iter().filter(|p| p["kind"] == "land").cloned().collect();
    let ll = |p: &serde_json::Value| [p["center"][0].as_f64().unwrap(), p["center"][1].as_f64().unwrap()];
    let n_prov = table["provinces"].as_array().unwrap().len();
    let n_states = table["states"].as_array().unwrap().len();
    // Two land provinces of different states, and a third in yet another state.
    let a = &land[0];
    let b = land.iter().find(|p| p["state"] != a["state"]).unwrap();
    let c = land.iter().find(|p| p["state"] != a["state"] && p["state"] != b["state"]).unwrap();
    let pt = |q: &serde_json::Value| ll(q);
    let stroke = |tool: Tool, points: Vec<[f64; 2]>, value: f64, name: &str| Stroke { tool, points, value, radius_km: 300.0, name: name.into(), ..Default::default() };
    w.edits.add_stroke(stroke(Tool::FertilityPaint, vec![pt(c)], 1.0, ""));
    w.edits.add_stroke(stroke(Tool::StateMerge, vec![pt(b), pt(c)], 0.0, ""));
    w.edits.add_stroke(stroke(Tool::ProvinceMerge, vec![pt(a), pt(b)], 0.0, ""));
    w.edits.add_stroke(stroke(Tool::RenameProvince, vec![pt(b)], 0.0, "Testburg"));
    w.edits.add_stroke(stroke(Tool::RenameState, vec![pt(b)], 0.0, "Testland"));
    w.edits.add_stroke(stroke(Tool::GoodsPaint, vec![pt(b)], 2.0, ""));
    w.edits.add_stroke(stroke(Tool::GoodsPaint, vec![pt(b)], 101.0, ""));
    // Founders: two pins, then move the first and erase the second.
    let far = land.iter().max_by(|x, y| {
        let d = |q: &serde_json::Value| (q["center"][0].as_f64().unwrap() - a["center"][0].as_f64().unwrap()).abs();
        d(x).partial_cmp(&d(y)).unwrap()
    }).unwrap();
    w.edits.add_stroke(stroke(Tool::BandPin, vec![pt(a)], 5.0, ""));
    w.edits.add_stroke(stroke(Tool::BandPin, vec![pt(far)], 1.0, ""));
    w.edits.add_stroke(stroke(Tool::BandPin, vec![pt(a), pt(c)], 5.0, ""));
    assert_eq!(w.edits.overrides.bands.len(), 2, "dragging a pin moves it");
    assert_eq!(w.edits.overrides.bands[0].points[0], pt(c));
    w.run_to(LAST, &|_, _, _| {});

    let hab = match w.field("habitability") { Some((Field::F32(v), _, _)) => v.clone(), _ => panic!() };
    let g = w.grid();
    let cell = |q: [f64; 2]| g.nearest(worldcore::vec3::Vec3::from_lat_lon_deg(q[0], q[1]), None);
    assert!(hab[cell(pt(c))] > 0.99, "fertility paint raises habitability");
    let prov = match w.field("province") { Some((Field::U32(v), _, _)) => v.clone(), _ => panic!() };
    let state = match w.field("state") { Some((Field::U16(v), Step::Provinces, _)) => v.clone(), _ => panic!() };
    assert_eq!(prov[cell(pt(a))], prov[cell(pt(b))], "province merge");
    assert_eq!(state[cell(pt(b))], state[cell(pt(c))], "state merge");
    let t = &w.meta(Step::Provinces).unwrap()["table"];
    // (Counts are not compared: fertility paint reshapes provinces and states.)
    let _ = (n_prov, n_states);
    let merged = t["provinces"].as_array().unwrap().iter().find(|q| q["id"].as_u64() == Some(prov[cell(pt(b))] as u64)).unwrap();
    assert_eq!(merged["name"], "Testburg");
    assert_eq!(merged["trade_good"], "wine");
    assert!(merged["resources"].as_array().unwrap().iter().any(|r| r == "copper"));
    let st = t["states"].as_array().unwrap().iter().find(|s| s["id"].as_u64() == Some(state[cell(pt(b))] as u64)).unwrap();
    assert_eq!(st["name"], "Testland");
    // Culture names keep the editor's names.
    let mut named = t.clone();
    worldcore::stages::cultures::apply_names(&mut named, &w.meta(Step::Cultures).unwrap()["table"]);
    assert!(named["provinces"].as_array().unwrap().iter().any(|q| q["name"] == "Testburg"));
    assert!(named["states"].as_array().unwrap().iter().any(|s| s["name"] == "Testland"));
    let founders = w.meta(Step::Cultures).unwrap()["founders"].as_array().unwrap().clone();
    assert_eq!(founders.len(), 2);
    assert!(founders.iter().all(|f| f["pinned"] == true));
    assert_eq!(founders[0]["bands"], 5);
    // Erase the second pin: one founding people left.
    w.edits.add_stroke(stroke(Tool::BandErase, vec![pt(far)], 0.0, ""));
    assert_eq!(w.edits.overrides.bands.len(), 1);
}

/// Attraction paint: a metropolis province ends with more people than without
/// it, a ghost-town province with almost none, and states and provinces stay
/// the same (attraction acts on the culture simulation only).
#[test]
fn attraction_makes_metropolis_and_ghost_town() {
    use worldcore::edits::{Stroke, Tool};
    let mut p = WorldParams::default();
    p.planet.seed = 4;
    p.planet.grid_level = 6;
    p.climate.climate_level = 6;
    p.cultures.ticks = 150;
    let mut w = World::new(p);
    w.run_to(LAST, &|_, _, _| {});
    let pop = |w: &World| -> std::collections::HashMap<u64, f64> {
        w.meta(Step::Cultures).unwrap()["table"]["provinces"].as_array().unwrap().iter().map(|p| (p["id"].as_u64().unwrap(), p["population"].as_f64().unwrap())).collect()
    };
    let before = pop(&w);
    let provs = w.meta(Step::Provinces).unwrap()["table"]["provinces"].as_array().unwrap().clone();
    // Two settled land provinces far apart.
    let settled: Vec<&serde_json::Value> = provs.iter().filter(|p| p["kind"] == "land" && before.get(&p["id"].as_u64().unwrap()).copied().unwrap_or(0.0) > 1000.0).collect();
    let (a, b) = (settled[0], settled[settled.len() / 2]);
    let ll = |p: &serde_json::Value| [p["center"][0].as_f64().unwrap(), p["center"][1].as_f64().unwrap()];
    let brush = |v: f64, at: [f64; 2]| Stroke { tool: Tool::Attraction, value: v, radius_km: 400.0, hardness: 0.9, points: vec![at], ..Default::default() };
    w.edits.add_stroke(brush(1.0, ll(a)));
    w.edits.add_stroke(brush(-1.0, ll(b)));
    assert!(w.is_fresh(Step::Provinces) && !w.is_fresh(Step::Cultures), "attraction only invalidates cultures");
    w.run_to(LAST, &|_, _, _| {});
    let after = pop(&w);
    let (ia, ib) = (a["id"].as_u64().unwrap(), b["id"].as_u64().unwrap());
    assert!(after[&ia] > 1.5 * before[&ia], "metropolis: {} -> {}", before[&ia], after[&ia]);
    assert!(after[&ib] < 0.2 * before[&ib], "ghost town: {} -> {}", before[&ib], after[&ib]);
}
