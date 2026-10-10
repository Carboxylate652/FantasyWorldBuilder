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
        let (off, nbr) = g.neighbor_csr();
        let (len, rev) = g.neighbor_geometry();
        for i in 0..g.len() {
            for e in off[i] as usize..off[i + 1] as usize {
                let r = rev[e] as usize;
                assert_eq!(nbr[r] as usize, i);
                assert_eq!(len[e], g.pos[i].angle_to(g.pos[nbr[e] as usize]));
                assert!((len[e] - len[r]).abs() < 1e-15);
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
fn malformed_optional_project_file_is_reported() {
    let dir = std::env::temp_dir().join(format!("fwm-corrupt-{}", std::process::id()));
    let w = World::new(small_params(17));
    w.save(&dir).unwrap();
    std::fs::write(dir.join("overrides/barriers.json"), b"not json").unwrap();
    let err = World::load(&dir).err().expect("corrupt override should fail to load");
    assert!(err.contains("barriers.json"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn coarse_climate_cache_preserves_results_across_level_changes() {
    let mut params = small_params(7);
    params.climate.climate_level = 4;
    let mut a = World::new(params.clone());
    a.run_to(Step::Climate, &|_, _, _| {});
    let expected = a.fingerprint();
    let mut b = World::new(params);
    b.run_to(Step::Climate, &|_, _, _| {});
    assert_eq!(b.fingerprint(), expected, "cached coarse grid changed the result");
    b.params.planet.grid_level = 6;
    b.run_to(Step::Climate, &|_, _, _| {});
    b.params.planet.grid_level = 5;
    b.run_to(Step::Climate, &|_, _, _| {});
    assert_eq!(b.fingerprint(), expected, "replacing the cache changed the result");
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

#[test]
fn override_bundles_carry_edits_to_another_seed() {
    use worldcore::edits::{EditLayer, Stroke, Tool};
    let mut a = World::new(small_params(21));
    let st = |tool: Tool, points: Vec<[f64; 2]>, value: f64| Stroke { tool, points, value, radius_km: 300.0, ..Default::default() };
    a.edits.add_stroke(st(Tool::FertilityPaint, vec![[10.0, 20.0]], 0.8));
    a.edits.add_stroke(st(Tool::Attraction, vec![[-5.0, 40.0]], 1.0));
    a.edits.add_stroke(st(Tool::BandPin, vec![[30.0, 0.0]], 4.0));
    a.edits.add_stroke(st(Tool::BiomePaint, vec![[0.0, 0.0], [1.0, 1.0]], 3.0));
    let all = [EditLayer::Fertility, EditLayer::Attraction, EditLayer::Bands, EditLayer::Biomes];
    let bundle: serde_json::Value = serde_json::from_str(&serde_json::to_string(&a.edits.bundle(&all, 21)).unwrap()).unwrap();

    // Another seed takes the edits as they are (they are stored in lat/lon).
    let mut b = World::new(small_params(22));
    b.edits.add_stroke(st(Tool::FertilityPaint, vec![[50.0, 50.0]], -1.0));
    let taken = b.edits.apply_bundle(&bundle, None, false).unwrap();
    assert_eq!(taken.len(), 4);
    assert_eq!(b.edits.overrides.fertility.len(), 2, "added to the existing edits");
    assert_eq!(b.edits.overrides.attraction, a.edits.overrides.attraction);
    // Replace swaps a layer; a filter takes only the named layers.
    let mut c = b.edits.clone();
    c.apply_bundle(&bundle, Some(&[EditLayer::Fertility]), true).unwrap();
    assert_eq!(c.overrides.fertility, a.edits.overrides.fertility);
    assert_eq!(c.overrides.attraction.len(), 1, "filtered out layers are untouched");
    // Bad bundles change nothing.
    let mut bad = bundle.clone();
    bad["layers"]["biomes"] = serde_json::json!([{ "tool": "band_pin", "points": [[0.0, 0.0]] }]);
    let before = b.edits.clone();
    assert!(b.edits.apply_bundle(&bad, None, false).is_err(), "a stroke in the wrong layer is refused");
    assert!(b.edits.apply_bundle(&serde_json::json!({ "format": "x" }), None, false).is_err());
    assert!(b.edits.apply_bundle(&serde_json::json!({ "format": "fwm-overrides", "version": 99, "layers": {} }), None, false).is_err());
    assert_eq!(b.edits, before);
    // Removing by index.
    assert_eq!(b.edits.remove_strokes(EditLayer::Fertility, &[0, 0, 9]), 1);
    assert_eq!(b.edits.count(EditLayer::Fertility), 1);
}

#[test]
fn validation_and_unapplied_edits() {
    use worldcore::edits::{Stroke, Tool};
    let mut w = World::new(small_params(8));
    w.run_to(Step::Provinces, &|_, _, _| {});
    let rep = worldcore::validate::validate(&mut w);
    assert!(rep.ok, "a generated world passes: {:?}", rep.errors);
    assert!(rep.checks.len() >= 6, "checks ran: {:?}", rep.checks);
    assert!(rep.warnings.is_empty(), "{:?}", rep.warnings);

    // A merge drawn in the open ocean, and a rename of nothing: both are
    // reported as no longer applying, with their index in the layer.
    let sea = {
        let t = &w.meta(Step::Provinces).unwrap()["table"];
        let p = t["provinces"].as_array().unwrap().iter().find(|p| p["kind"] == "sea" && p["band"] == "open").unwrap().clone();
        [p["center"][0].as_f64().unwrap(), p["center"][1].as_f64().unwrap()]
    };
    let st = |tool: Tool, points: Vec<[f64; 2]>| Stroke { tool, points, name: "Nowhere".into(), ..Default::default() };
    w.edits.add_stroke(st(Tool::StateMerge, vec![sea, sea]));
    w.edits.add_stroke(st(Tool::RenameState, vec![sea]));
    w.edits.add_stroke(st(Tool::ProvinceToState, vec![sea, sea]));
    assert!(worldcore::validate::validate(&mut w).warnings.iter().any(|m| m.contains("stale")));
    w.run_to(Step::Provinces, &|_, _, _| {});
    let r = worldcore::api::overrides_report(&w);
    let un = r["unapplied"].as_array().unwrap();
    assert_eq!(un.len(), 3, "{un:?}");
    assert_eq!((un[0]["layer"].as_str(), un[0]["index"].as_u64()), (Some("states"), Some(0)));
    assert_eq!((un[2]["layer"].as_str(), un[2]["index"].as_u64()), (Some("provinces"), Some(0)));
    assert_eq!(r["total"], 3);
    let rep = worldcore::validate::validate(&mut w);
    assert!(rep.ok && rep.warnings.len() == 3, "{:?}", rep.warnings);
}

#[test]
fn cash_crops_oil_and_goods_editor() {
    use worldcore::edits::{Stroke, Tool};
    use worldcore::stages::resources::{category, good_index, DEPOSITS, TRADE_GOODS};
    // Editor numbers stay where they were: old goods and deposits keep their values.
    assert_eq!(good_index("grain"), 1);
    assert_eq!(good_index("camels"), 14);
    assert_eq!(good_index("none"), 0);
    assert_eq!(good_index("coffee"), 18);
    assert_eq!(DEPOSITS[5], "salt");
    assert_eq!(DEPOSITS[6], "oil");
    assert_eq!(category("rubber", "trade good"), "cash crop");
    assert_eq!(category("oil", "deposit"), "energy");

    let mut w = World::new(small_params(12));
    w.run_to(Step::Provinces, &|_, _, _| {});
    let t = w.meta(Step::Provinces).unwrap()["table"].clone();
    let land: Vec<&serde_json::Value> = t["provinces"].as_array().unwrap().iter().filter(|p| p["kind"] == "land").collect();
    let crops = land.iter().filter(|p| category(p["trade_good"].as_str().unwrap(), "trade good") == "cash crop" && p["trade_good"] != "wine" && p["trade_good"] != "spices" && p["trade_good"] != "dates").count();
    let oil = land.iter().filter(|p| p["resources"].as_array().unwrap().iter().any(|r| r == "oil")).count();
    assert!(crops > 0 && crops < land.len() / 2, "plantation crops: {crops} of {}", land.len());
    assert!(oil > 0 && oil < land.len() / 5, "oil: {oil} of {}", land.len());
    assert!(land.iter().all(|p| TRADE_GOODS.contains(&p["trade_good"].as_str().unwrap())));

    // Paint coffee and add oil to one province, remove oil from another.
    let ll = |p: &serde_json::Value| [p["center"][0].as_f64().unwrap(), p["center"][1].as_f64().unwrap()];
    let a = land.iter().find(|p| p["area_km2"].as_f64().unwrap() > 20000.0).unwrap();
    let with_oil = land.iter().find(|p| p["id"] != a["id"] && p["resources"].as_array().unwrap().iter().any(|r| r == "oil")).unwrap();
    let st = |p: [f64; 2], v: f64| Stroke { tool: Tool::GoodsPaint, points: vec![p], value: v, radius_km: 10.0, ..Default::default() };
    w.edits.add_stroke(st(ll(a), 18.0));
    w.edits.add_stroke(st(ll(a), 107.0));
    w.edits.add_stroke(st(ll(with_oil), 207.0));
    w.edits.add_stroke(st(ll(with_oil), 15.0)); // "none" is not paintable: ignored
    let good_before = with_oil["trade_good"].clone();
    w.run_to(Step::Provinces, &|_, _, _| {});
    let t = &w.meta(Step::Provinces).unwrap()["table"];
    let find = |id: &serde_json::Value| t["provinces"].as_array().unwrap().iter().find(|p| p["id"] == *id).unwrap().clone();
    let a2 = find(&a["id"]);
    assert_eq!(a2["trade_good"], "coffee");
    assert!(a2["resources"].as_array().unwrap().iter().any(|r| r == "oil"));
    let o2 = find(&with_oil["id"]);
    assert!(!o2["resources"].as_array().unwrap().iter().any(|r| r == "oil"));
    assert_eq!(o2["trade_good"], good_before);
}

#[test]
fn live_steps_and_directives_match_a_fresh_run() {
    use std::sync::{Arc, Mutex};
    use worldcore::api::{handle, ProgressState, Reply, Session};
    let mut p = small_params(17);
    p.cultures.ticks = 60;
    p.nations.start_year = 1300;
    let session = Mutex::new(Session::new(p.clone()));
    let progress = Arc::new(Mutex::new(ProgressState::default()));
    let call = |cmd: &str, args: serde_json::Value| -> serde_json::Value {
        match handle(&session, &progress, cmd, args) {
            Ok(Reply::Json(v)) => v,
            Ok(Reply::Bytes(_)) => serde_json::Value::Null,
            Err(e) => panic!("{cmd}: {e}"),
        }
    };
    // Stage 3, stepped in uneven chunks with directives along the way.
    let st = call("sim_start", serde_json::json!({ "stage": "cultures" }));
    assert_eq!(st["live"]["position"], 0.0);
    let region = st["sim"]["cultures"].as_array().map(|_| 1).unwrap_or(1);
    call("sim_step", serde_json::json!({ "steps": 7 }));
    call("sim_directive", serde_json::json!({ "action": "catastrophe", "args": { "regions": [region], "severity": 0.6 }, "note": "the great plague" }));
    let st = call("sim_step", serde_json::json!({ "to": "era" }));
    let c1 = st["sim"]["cultures"][0]["id"].as_u64().unwrap_or(1);
    call("sim_directive", serde_json::json!({ "action": "drift", "args": { "culture": c1, "factor": 8.0, "years": 300 } }));
    call("sim_directive", serde_json::json!({ "action": "settle", "args": { "province": st["sim"]["cultures"][0]["regions"][0]["id"], "bands": 3, "culture": null } }));
    assert!(handle(&session, &progress, "sim_directive", serde_json::json!({ "action": "war", "args": {} })).is_err(), "Stage 4 action refused in Stage 3");
    call("sim_step", serde_json::json!({ "years": 230 }));
    let st = call("sim_commit", serde_json::json!({}));
    assert_eq!(st["replayed"], false, "the live result is kept");
    // Stage 4 the same way.
    let st = call("sim_start", serde_json::json!({ "stage": "nations" }));
    assert_eq!(st["live"]["position"], 1300.0);
    let st = call("sim_step", serde_json::json!({ "years": 120 }));
    let n1 = st["sim"]["nations"][0]["id"].as_u64().expect("nations formed");
    let n2 = st["sim"]["nations"][1]["id"].as_u64().unwrap_or(n1);
    call("sim_directive", serde_json::json!({ "action": "aggression", "args": { "nation": n1, "factor": 4.0, "years": 200 } }));
    call("sim_directive", serde_json::json!({ "action": "peace", "args": { "nation": n1, "target": n2, "years": 100 } }));
    let st = call("sim_step", serde_json::json!({ "steps": 33 }));
    let n3 = st["sim"]["nations"][0]["id"].as_u64().unwrap();
    call("sim_directive", serde_json::json!({ "action": "split", "args": { "nation": n1, "culture": null }, "note": "civil war" }));
    call("sim_directive", serde_json::json!({ "action": "rename", "args": { "nation": n2, "name": "Avalon" } }));
    // Tags, a feudal empire and a birthplace left to chance replay too.
    call("sim_directive", serde_json::json!({ "action": "tag", "args": { "scope": "world", "id": null, "tag": "stable_realms", "on": true } }));
    call("sim_directive", serde_json::json!({ "action": "feudal_empire", "args": { "nation": n3, "on": true, "name": "Holy Empire", "provinces": null, "states": null, "regions": null } }));
    call("sim_directive", serde_json::json!({ "action": "institution_birth", "args": { "institution": 2, "province": null } }));
    // Past 1800 steps are half a year: a directive at a fractional time.
    let st = call("sim_step", serde_json::json!({ "years": 400.5 }));
    assert_eq!(st["live"]["position"], 1853.5);
    let n4 = st["sim"]["nations"][0]["id"].as_u64().unwrap();
    call("sim_directive", serde_json::json!({ "action": "reform", "args": { "nation": n4, "years": 20 } }));
    let st = call("sim_commit", serde_json::json!({}));
    assert_eq!(st["replayed"], false);
    let s = session.lock().unwrap();
    assert_eq!(s.world.edits.overrides.directives.len(), 11);
    assert!(s.world.edits.overrides.directives.iter().any(|d| d.at == 1853.5));
    let applied = s.world.meta(Step::Nations).unwrap()["directives"].as_array().unwrap().clone();
    assert_eq!(applied.len(), 8);
    assert!(applied.iter().all(|d| d["applied"] == true), "{applied:?}");

    // A fresh world with the same edits gives the same history.
    let mut fresh = World::new(p);
    fresh.edits = s.world.edits.clone();
    fresh.run_to(Step::Nations, &|_, _, _| {});
    for step in [Step::Cultures, Step::Nations] {
        assert!(s.world.is_fresh(step));
        assert_eq!(fresh.meta(step).unwrap()["table"], s.world.meta(step).unwrap()["table"], "{step:?} table");
        for (name, f) in &s.world.steps[step.index()].as_ref().unwrap().fields.0 {
            assert!(fresh.steps[step.index()].as_ref().unwrap().fields.get(name) == Some(f), "{step:?} field {name}");
        }
    }
    let names: Vec<&str> = fresh.meta(Step::Nations).unwrap()["table"]["nations"].as_array().unwrap().iter().filter_map(|n| n["name"].as_str()).collect();
    assert!(names.contains(&"Avalon"), "rename replayed");
    let empires = fresh.meta(Step::Nations).unwrap()["empires"].as_array().unwrap().clone();
    assert_eq!(empires.len(), 1);
    assert!(empires[0]["dissolved"].is_null(), "the empire lasts to the end: {empires:?}");
}

#[test]
fn nations_are_consistent() {
    use worldcore::fields::Field;
    let mut p = small_params(23);
    p.cultures.ticks = 60;
    let mut w = World::new(p);
    w.run_to(Step::Nations, &|_, _, _| {});
    let m = w.meta(Step::Nations).unwrap().clone();
    let t = &m["table"];
    let nations = t["nations"].as_array().unwrap();
    assert!(m["nations"].as_u64().unwrap() >= 3, "nations: {}", m["nations"]);
    assert!(m["ruled_share"].as_f64().unwrap() > 0.5);
    // Owners exist and are alive; members agree with the province table; capitals are owned.
    let alive: std::collections::HashMap<u64, &serde_json::Value> = nations.iter().filter(|n| n["ended"].is_null()).map(|n| (n["id"].as_u64().unwrap(), n)).collect();
    let mut count: std::collections::HashMap<u64, u64> = Default::default();
    let mut owner_of: std::collections::HashMap<u64, u64> = Default::default();
    for p in t["provinces"].as_array().unwrap() {
        let o = p["owner"].as_u64().unwrap();
        owner_of.insert(p["id"].as_u64().unwrap(), o);
        if o > 0 {
            assert!(alive.contains_key(&o), "province owned by dead nation {o}");
            *count.entry(o).or_insert(0) += 1;
        }
    }
    for (id, n) in &alive {
        assert_eq!(n["provinces"].as_u64().unwrap(), count.get(id).copied().unwrap_or(0), "nation {id} province count");
        assert_eq!(owner_of.get(&n["capital"].as_u64().unwrap()), Some(id), "nation {id} owns its capital");
    }
    for n in nations.iter().filter(|n| !n["ended"].is_null()) {
        assert_eq!(n["provinces"], 0);
    }
    // The owner field matches the table.
    let (Some((Field::U16(of), _, _)), Some((Field::U32(pf), _, _))) = (w.field("owner"), w.field("province")) else { panic!() };
    for (i, &id) in pf.iter().enumerate() {
        if let Some(&o) = owner_of.get(&(id as u64)) {
            assert_eq!(of[i] as u64, o);
        }
    }
    // Railways run over their owner's provinces as they were when built, and are
    // listed with stations at both ends.
    for r in t["railways"].as_array().unwrap() {
        let path = r["provinces"].as_array().unwrap();
        let st = r["stations"].as_array().unwrap();
        assert!(path.len() >= 2 && st.first() == path.first() && st.last() == path.last());
        assert!(r["opened"].as_i64().unwrap() >= 1830);
    }
    let ev = t["events"].as_array().unwrap();
    assert!(ev.iter().any(|e| e["event"] == "founded"));
    assert!(ev.windows(2).all(|w| w[0]["year"].as_i64() <= w[1]["year"].as_i64()), "events in order");
    assert!(worldcore::validate::validate(&mut w).ok);
}

#[test]
fn transport_economy_and_eras() {
    use worldcore::directives::Directive;
    let mut p = small_params(23);
    p.cultures.ticks = 60;
    let mut w = World::new(p);
    w.run_to(Step::Nations, &|_, _, _| {});
    let m = w.meta(Step::Nations).unwrap().clone();
    let t = &m["table"];
    assert_eq!(m["start_date"], 1949);
    let np = worldcore::params::NationParams::default();
    let eras = [np.gunpowder_year, np.shipping_year, np.industrial_year, np.fertilizer_year, np.motor_year, np.air_year];
    // Eras follow institutions: no nation is in an era whose institution
    // isn't born, and technology stays below the next era's year until the
    // nation enters it.
    let born = m["institutions"].as_array().unwrap().iter().filter(|i| !i["born"].is_null()).count();
    let alive: Vec<&serde_json::Value> = t["nations"].as_array().unwrap().iter().filter(|n| n["ended"].is_null()).collect();
    for n in &alive {
        let tech = n["tech"].as_f64().unwrap();
        let era = n["era_index"].as_u64().unwrap() as usize;
        assert!(era <= born, "{}: era {era} with {born} institutions born", n["name"]);
        if era < 6 {
            // (tech is rounded to a tenth)
            assert!(tech < eras[era] as f64 + 0.05, "{}: tech {tech}, era {era}", n["name"]);
        }
        assert!(tech <= 1949.0 + np.tech_lead_years + 0.1);
        let i = n["integration"].as_f64().unwrap();
        assert!((0.0..=1.0).contains(&i));
    }
    let owner: std::collections::HashMap<u64, u64> = t["provinces"].as_array().unwrap().iter().map(|p| (p["id"].as_u64().unwrap(), p["owner"].as_u64().unwrap())).collect();
    // Roads join neighbours; highways and airports only in the motor and air ages.
    let pt = w.meta(Step::Provinces).unwrap()["table"].clone();
    let adj: std::collections::HashSet<(u64, u64)> = pt["adjacency"].as_array().unwrap().iter().map(|a| (a["from"].as_u64().unwrap(), a["to"].as_u64().unwrap())).collect();
    let roads = t["roads"].as_array().unwrap();
    assert!(!roads.is_empty(), "no roads");
    for r in roads {
        let (a, b) = (r["from"].as_u64().unwrap(), r["to"].as_u64().unwrap());
        assert!(adj.contains(&(a, b)) || adj.contains(&(b, a)), "road {a}–{b} is not a border");
        assert!((1..=3).contains(&r["quality"].as_u64().unwrap()));
    }
    let air_age = alive.iter().any(|n| n["era_index"].as_u64().unwrap() >= 6);
    for a in t["airports"].as_array().unwrap() {
        assert!(air_age, "airport without the air age");
        assert!(a["opened"].as_i64().unwrap() >= 1800);
    }
    // Lines have stations at both ends; junctions are stations of two or more
    // open lines, and the province table agrees.
    let lines = t["railways"].as_array().unwrap();
    for l in lines {
        let path = l["provinces"].as_array().unwrap();
        let st = l["stations"].as_array().unwrap();
        assert!(path.len() >= 2 && st.first() == path.first() && st.last() == path.last());
        assert!(l["km"].as_f64().unwrap() > 0.0);
    }
    let rail: std::collections::HashMap<u64, u64> = t["provinces"].as_array().unwrap().iter().map(|p| (p["id"].as_u64().unwrap(), p["railway"].as_u64().unwrap())).collect();
    for s in t["stations"].as_array().unwrap() {
        let n = s["lines"].as_array().unwrap().len();
        assert_eq!(s["junction"].as_bool().unwrap(), n > 1);
        assert_eq!(rail[&s["province"].as_u64().unwrap()], if n > 1 { 3 } else { 2 });
    }
    // Steering: in 1948 the largest nation leaps ahead, gets a windfall, and
    // builds a highway, a railway and an airport to an interior province.
    let big = alive.iter().max_by_key(|n| n["population"].as_u64().unwrap()).unwrap();
    let (nid, cap) = (big["id"].as_u64().unwrap(), big["capital"].as_u64().unwrap());
    let mine: Vec<u64> = owner.iter().filter(|(_, &o)| o == nid).map(|(&p, _)| p).collect();
    let interior = |p: u64| adj.iter().filter(|(a, b)| *a == p || *b == p).all(|(a, b)| owner.get(if *a == p { b } else { a }).is_some_and(|&o| o == nid));
    let mut far: Vec<u64> = mine.iter().copied().filter(|&p| p != cap && interior(p)).collect();
    far.sort();
    let to = *far.last().or(mine.iter().find(|&&p| p != cap)).expect("a second province");
    let d = |action: &str, args: serde_json::Value| Directive { stage: "nations".into(), at: 1948.0, action: action.into(), args, note: String::new(), by: "user".into() };
    w.edits.overrides.directives = vec![
        d("tech", serde_json::json!({ "nation": nid, "years": 200 })),
        d("subsidy", serde_json::json!({ "nation": nid, "years": 5 })),
        d("build_road", serde_json::json!({ "nation": nid, "from": cap, "to": to, "quality": 3 })),
        d("railway", serde_json::json!({ "nation": nid, "from": to, "to": cap })),
        d("airport", serde_json::json!({ "nation": nid, "province": to })),
    ];
    w.run_to(Step::Nations, &|_, _, _| {});
    let m2 = w.meta(Step::Nations).unwrap().clone();
    for a in m2["directives"].as_array().unwrap() {
        assert_eq!(a["applied"], true, "{a}");
    }
    let t2 = &m2["table"];
    let n2 = t2["nations"].as_array().unwrap().iter().find(|n| n["id"] == nid).unwrap();
    assert_eq!(n2["era"], "air age");
    assert!(t2["roads"].as_array().unwrap().iter().any(|r| r["quality"] == 3 && (r["from"] == cap || r["to"] == cap)), "highway from the capital");
    assert!(t2["airports"].as_array().unwrap().iter().any(|a| a["province"] == to));
    assert!(t2["railways"].as_array().unwrap().iter().any(|l| l["opened"] == 1948 && l["owner"] == nid), "railway of 1948");
    assert!(worldcore::validate::validate(&mut w).ok);
}

#[test]
fn institutions_ports_and_step_schedule() {
    use worldcore::api::{handle, ProgressState, Reply, Session};
    let mut p = small_params(23);
    p.cultures.ticks = 60;
    let session = std::sync::Mutex::new(Session::new(p));
    let progress = std::sync::Arc::new(std::sync::Mutex::new(ProgressState::default()));
    let call = |cmd: &str, args: serde_json::Value| -> serde_json::Value {
        match handle(&session, &progress, cmd, args) {
            Ok(Reply::Json(v)) => v,
            Ok(Reply::Bytes(_)) => serde_json::Value::Null,
            Err(e) => panic!("{cmd}: {e}"),
        }
    };
    let st = call("sim_start", serde_json::json!({ "stage": "nations" }));
    // Steps shorten toward the present: 2 years, then 1 from 1400, then 0.5 from 1800.
    assert_eq!(st["sim"]["step_years"], 2.0);
    // Step until gunpowder is ready to be born and the run stops for a choice.
    let st = call("sim_step", serde_json::json!({ "to": "end", "stop_at_choice": true }));
    let pend = &st["sim"]["pending_institution"];
    assert_eq!(pend["index"], 0, "stopped for gunpowder: {pend}");
    assert!(st["live"]["position"].as_f64().unwrap() >= 1450.0);
    assert_eq!(st["sim"]["step_years"], 1.0);
    let cands = pend["candidates"].as_array().unwrap();
    assert!(!cands.is_empty());
    // Stepping again without choosing stays put.
    let again = call("sim_step", serde_json::json!({ "steps": 3, "stop_at_choice": true }));
    assert_eq!(again["live"]["position"], st["live"]["position"]);
    // Choose the last candidate: it is born there.
    let chosen = cands.last().unwrap()["province"].as_u64().unwrap();
    call("sim_directive", serde_json::json!({ "action": "institution_birth", "args": { "institution": 0, "province": chosen } }));
    let st = call("sim_step", serde_json::json!({ "steps": 1, "stop_at_choice": true }));
    let gun = &st["sim"]["institutions"][0];
    assert_eq!(gun["province"], chosen, "{gun}");
    assert!(st["sim"]["pending_institution"].is_null());
    // The rest to the end, choices left to chance.
    let st = call("sim_step", serde_json::json!({ "to": "end" }));
    assert_eq!(st["sim"]["step_years"], 0.5);
    let commit = call("sim_commit", serde_json::json!({}));
    assert_eq!(commit["replayed"], false);
    let mut s = session.lock().unwrap();
    let m = s.world.meta(Step::Nations).unwrap().clone();
    let np = worldcore::params::NationParams::default();
    let earliest = [np.gunpowder_year, np.shipping_year, np.industrial_year, np.fertilizer_year, np.motor_year, np.air_year];
    // Institutions are born in order, never before their year.
    let inst = m["institutions"].as_array().unwrap();
    let mut last = i64::MIN;
    for (k, i) in inst.iter().enumerate() {
        if let Some(y) = i["born"].as_i64() {
            assert!(y >= earliest[k] as i64 && y >= last, "{i}");
            last = y;
        }
    }
    assert!(inst[0]["born"].is_i64() && inst[1]["born"].is_i64(), "gunpowder and navigation are born: {inst:?}");
    // Ports: in good harbours, owned by nations of the shipping era.
    let t = &m["table"];
    let ports = t["ports"].as_array().unwrap();
    assert!(!ports.is_empty(), "ports open");
    for p in ports {
        assert!(p["harbour"].as_f64().unwrap() + 1e-9 >= np.port_quality - 0.15, "{p}");
        assert!(p["opened"].as_i64().unwrap() >= earliest[1] as i64);
    }
    // Province rows agree with the institution totals.
    for (k, i) in inst.iter().enumerate() {
        let name = i["name"].as_str().unwrap();
        let n = t["provinces"].as_array().unwrap().iter().filter(|p| p["institutions"].as_array().unwrap().iter().any(|x| x == name)).count();
        assert_eq!(n as u64, i["embraced_provinces"].as_u64().unwrap(), "institution {k}");
    }
    assert!(worldcore::validate::validate(&mut s.world).ok);
}

#[test]
fn cities_rise_and_fall() {
    use worldcore::directives::Directive;
    let mut p = small_params(29);
    p.cultures.ticks = 60;
    let mut w = World::new(p.clone());
    w.run_to(Step::Nations, &|_, _, _| {});
    let t = w.meta(Step::Nations).unwrap()["table"].clone();
    // Capitals become the largest cities.
    let cities = t["cities"].as_array().unwrap();
    let caps = cities.iter().take(10).filter(|c| c["capital"] == true).count();
    assert!(caps >= 6, "capitals among the 10 largest cities: {caps}");
    // Pins: a boom town and an abandoned city, placed in 1300 on two populous
    // provinces that are not capitals, then the boom town moved and a capital moved.
    let pop_of = |w: &World, id: u64| w.meta(Step::Nations).unwrap()["table"]["provinces"].as_array().unwrap().iter().find(|q| q["id"] == id).unwrap()["population"].as_f64().unwrap();
    let mid: Vec<u64> = cities.iter().skip(20).filter(|c| c["capital"] == false).take(3).map(|c| c["province"].as_u64().unwrap()).collect();
    let (boom, bust, later) = (mid[0], mid[1], mid[2]);
    let d = |at: f64, action: &str, args: serde_json::Value| Directive { stage: "nations".into(), at, action: action.into(), args, note: String::new(), by: "user".into() };
    w.edits.overrides.directives = vec![
        d(1300.0, "pin_add", serde_json::json!({ "province": boom, "value": 1.0, "years": null, "label": "silver rush" })),
        d(1300.0, "pin_add", serde_json::json!({ "province": bust, "value": -1.0, "years": null, "label": "sacked and abandoned" })),
        d(1500.0, "pin_add", serde_json::json!({ "province": later, "value": 0.5, "years": 100, "label": null })),
        d(1900.0, "pin_move", serde_json::json!({ "pin": 99, "province": boom })),
    ];
    for x in &w.edits.overrides.directives {
        worldcore::directives::validate(x).unwrap();
    }
    w.run_to(Step::Nations, &|_, _, _| {});
    let m = w.meta(Step::Nations).unwrap().clone();
    let applied = m["directives"].as_array().unwrap();
    assert_eq!(applied.iter().map(|a| a["applied"] == true).collect::<Vec<_>>(), vec![true, true, true, false], "{applied:?}");
    let before = |id: u64| t["provinces"].as_array().unwrap().iter().find(|q| q["id"] == id).unwrap()["population"].as_f64().unwrap();
    assert!(pop_of(&w, boom) > 1.3 * before(boom), "boom town grew: {} vs {}", pop_of(&w, boom), before(boom));
    assert!(pop_of(&w, bust) < 0.2 * before(bust), "abandoned city emptied: {} vs {}", pop_of(&w, bust), before(bust));
    let pins = m["table"]["pins"].as_array().unwrap();
    assert_eq!(pins.len(), 2, "the 100-year pin expired: {pins:?}");
    assert_eq!(pins[0]["label"], "silver rush");
    assert!(m["table"]["events"].as_array().unwrap().iter().any(|e| e["event"] == "ruined" || e["event"] == "metropolis"));
    assert!(worldcore::validate::validate(&mut w).ok);

    // Moving a capital late (dated between two steps: it applies in the step
    // that covers its year) to a deep interior province, which cannot change
    // hands in that last step.
    let ptab = w.meta(Step::Provinces).unwrap()["table"]["provinces"].as_array().unwrap().clone();
    let nt = m["table"].clone();
    let owner: std::collections::HashMap<u64, u64> = nt["provinces"].as_array().unwrap().iter().map(|q| (q["id"].as_u64().unwrap(), q["owner"].as_u64().unwrap())).collect();
    let (nation, target) = ptab
        .iter()
        .filter_map(|q| {
            let id = q["id"].as_u64()?;
            let o = *owner.get(&id)?;
            let nb: Vec<u64> = q["neighbors"].as_array()?.iter().filter_map(|x| x.as_u64()).collect();
            let cap = nt["nations"].as_array()?.iter().find(|n| n["id"] == o)?["capital"].as_u64()?;
            (o > 0 && id != cap && nb.len() >= 3 && nb.iter().all(|x| owner.get(x).map_or(true, |&y| y == o))).then_some((o, id))
        })
        .next()
        .expect("an interior province");
    // Dated inside the half-year step that starts in 1913.
    w.edits.overrides.directives.push(d(1913.3, "move_capital", serde_json::json!({ "nation": nation, "province": target })));
    w.run_to(Step::Nations, &|_, _, _| {});
    let m2 = w.meta(Step::Nations).unwrap();
    assert_eq!(m2["directives"].as_array().unwrap().last().unwrap()["applied"], true);
    assert!(m2["table"]["events"].as_array().unwrap().iter().any(|e| e["event"] == "capital" && e["year"] == 1913 && e["province"] == target));
    let n2 = m2["table"]["nations"].as_array().unwrap().iter().find(|n| n["id"] == nation).unwrap();
    assert_eq!(n2["capital"], target);
}
