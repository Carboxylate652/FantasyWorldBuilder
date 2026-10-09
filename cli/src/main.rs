//! `worldgen` — headless Fantasy World Maker: batch generation, seed sweeps,
//! determinism checks, export, and an HTTP server for the UI.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use worldcore::api::{self, ProgressState, Reply, Session};
use worldcore::edits::{EditLayer, Stroke, Tool};
use worldcore::export::{export, ExportOptions};
use worldcore::params::WorldParams;
use worldcore::stages::{Step, LAST, STEPS};
use worldcore::World;

const USAGE: &str = "\
worldgen — Fantasy World Maker headless CLI

USAGE:
  worldgen new <project-dir> [--seed N] [--level L]
  worldgen run <project-dir> [--to STEP]
  worldgen export <project-dir> [--out DIR] [--width W] [--height H] [--lat-min A] [--lat-max B]
  worldgen generate --out <project-dir> [--seed N] [--level L] [--to STEP] [--export] [--width W] [--height H]
  worldgen sweep --out <dir> --seeds A..B [--level L] [--to STEP] [--no-export] [--width W] [--height H]
                                             also writes <dir>/sweep.csv (one row of key numbers per seed)
  worldgen import-heightmap <project-dir> <png> [--encoding heightmap16|paradox8|linear] [--min M --max M] [--blend B]
  worldgen import-provinces <project-dir> <provinces.png> [--csv definition.csv] [--lat-min A --lat-max B]
  worldgen validate-provinces <provinces.png> [--csv definition.csv]
  worldgen import-provinces <project-dir> --remove
  worldgen check [--seed N] [--level L]      determinism + save/load round-trip check
  worldgen stats <project-dir>               zonal climate, wind and rain-seasonality means
  worldgen info <project-dir> [--json]
  worldgen validate <project-dir> [--run] [--json]
                                             consistency checks (tables, fields, goods, cultures,
                                             override edits); exit code 1 on errors
  worldgen overrides <project-dir> [list]    override layers: edits per layer, edits that no longer apply
  worldgen overrides <project-dir> clear <layer>
  worldgen overrides <project-dir> remove <layer> <index,index,...>
  worldgen overrides <project-dir> prune     remove every edit that no longer applies
  worldgen overrides <project-dir> export <file> [--layers a,b]
  worldgen overrides <project-dir> import <file> [--layers a,b] [--replace]
  worldgen edit <project-dir> --tool TOOL --at LAT,LON[;LAT,LON...] [--value V] [--radius KM]
                [--strength S] [--hardness H] [--name NAME] [--run]
                                             add one override stroke (tools as in the app:
                                             province_merge, fertility_paint, band_pin, ...)
  worldgen directive <project-dir> --stage cultures|nations --at N --action NAME [--args JSON] [--note TEXT] [--run]
                                             add a directive by hand (applies before generation N or
                                             year N); `worldgen directive --list` shows the actions
  worldgen guide <project-dir> --goal TEXT [--stage nations|cultures] [--years 50] [--max-turns 60]
                [--max-actions 4]          AI-guided history: an LLM steers the live simulation
                                             toward the goal turn by turn, then the run is committed
  worldgen guide-setup [--provider anthropic|openai] [--base-url URL] [--model ID] [--key KEY]
                [--effort low|medium|high] guide provider settings (saved outside projects; keys
                                             can also come from ANTHROPIC_API_KEY, OPENAI_API_KEY,
                                             AI_GATEWAY_API_KEY or OPENROUTER_API_KEY)
  worldgen version
  worldgen check-update [--betas | --stable] [--download DIR]
                                             compare with the newest GitHub release; --download
                                             fetches its portable zip (size and SHA-256 checked)
  worldgen serve [--port 8765] [--static DIR] [--project DIR] [--debug]

LAYERS: sketch, plates, elevation, biomes, barriers, sites, fertility, states,
        provinces, bands, attraction

STEPS: planet, sketch, plates, relief, climate, hydrology, biomes (Stage 1),
       habitability, states, provinces (Stage 2), cultures (Stage 3), nations (Stage 4; default: nations)
";

struct Args {
    pos: Vec<String>,
    opts: Vec<(String, Option<String>)>,
}

impl Args {
    fn parse() -> Args {
        let mut pos = Vec::new();
        let mut opts = Vec::new();
        let raw: Vec<String> = std::env::args().skip(1).collect();
        let mut i = 0;
        while i < raw.len() {
            let a = &raw[i];
            if let Some(k) = a.strip_prefix("--") {
                let v = raw.get(i + 1).filter(|v| !v.starts_with("--")).cloned();
                if v.is_some() {
                    i += 1;
                }
                opts.push((k.to_string(), v));
            } else {
                pos.push(a.clone());
            }
            i += 1;
        }
        Args { pos, opts }
    }
    fn get(&self, k: &str) -> Option<&str> {
        self.opts.iter().find(|(n, _)| n == k).and_then(|(_, v)| v.as_deref())
    }
    fn flag(&self, k: &str) -> bool {
        self.opts.iter().any(|(n, _)| n == k)
    }
    fn num<T: std::str::FromStr>(&self, k: &str, d: T) -> T {
        self.get(k).and_then(|v| v.parse().ok()).unwrap_or(d)
    }
}

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1)
}

fn progress_printer() -> impl Fn(Step, f32, &str) + Sync {
    let last = Mutex::new(String::new());
    move |s: Step, f: f32, m: &str| {
        let line = format!("  [{:<9}] {:>3.0}%  {}", s.key(), f * 100.0, m);
        let mut l = last.lock().unwrap();
        if *l != line {
            eprintln!("{line}");
            *l = line;
        }
    }
}

fn params_from(args: &Args) -> WorldParams {
    let mut p = WorldParams::default();
    p.planet.seed = args.num("seed", p.planet.seed);
    p.planet.grid_level = args.num("level", p.planet.grid_level);
    p
}

fn export_opts(args: &Args) -> ExportOptions {
    let d = ExportOptions::default();
    ExportOptions {
        width: args.num("width", d.width),
        height: args.num("height", d.height),
        lat_min: args.num("lat-min", d.lat_min),
        lat_max: args.num("lat-max", d.lat_max),
        ..d
    }
}

fn step_arg(args: &Args) -> Step {
    let k = args.get("to").unwrap_or(LAST.key());
    Step::from_key(k).unwrap_or_else(|| die(&format!("unknown step `{k}`")))
}

fn run_world(w: &mut World, to: Step) {
    let t0 = Instant::now();
    let p = progress_printer();
    let ran = w.run_to(to, &p);
    for st in w.status() {
        if ran.iter().any(|r| r.key() == st.key) {
            eprintln!("  {:<22} {:>7} ms", st.title, st.millis);
        }
    }
    eprintln!("  total {:.1} s", t0.elapsed().as_secs_f64());
}

fn summary(w: &World) {
    for st in w.status() {
        let m = &st.meta;
        if m.is_null() {
            println!("  {:<22} {:<6}", st.title, st.state);
            continue;
        }
        let extra = match st.key {
            "planet" => format!("{} cells, {:.0} km spacing", m["cells"], m["spacing_km"].as_f64().unwrap_or(0.0)),
            "sketch" => format!("land {:.1}%", m["land_fraction"].as_f64().unwrap_or(0.0) * 100.0),
            "plates" => format!("{} plates ({} continental), {} landmasses", m["plate_count"], m["continental_plates"], m["landmasses"]),
            "relief" => format!("elevation {:.0}..{:.0} m", m["min_elevation_m"].as_f64().unwrap_or(0.0), m["max_elevation_m"].as_f64().unwrap_or(0.0)),
            "climate" => format!("global mean {:.1} °C, {:.0} mm/yr", m["global_mean_temp_c"].as_f64().unwrap_or(0.0), m["global_precip_mm"].as_f64().unwrap_or(0.0)),
            "hydrology" => format!("{} river cells, {} lakes ({} endorheic)", m["river_cells"], m["lakes"], m["endorheic_lakes"]),
            "biomes" => {
                let mut v: Vec<(String, f64)> = m["koppen_share"].as_object().map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_f64().unwrap_or(0.0))).collect()).unwrap_or_default();
                v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                v.iter().take(6).map(|(k, s)| format!("{k} {:.0}%", s * 100.0)).collect::<Vec<_>>().join(", ")
            }
            "habitability" => format!(
                "mean {:.2}, {:.0}% of land habitable, border rivers {:.0} km, backbone rivers {:.0} km",
                m["mean_habitability"].as_f64().unwrap_or(0.0),
                m["habitable_share"].as_f64().unwrap_or(0.0) * 100.0,
                m["border_river_km"].as_f64().unwrap_or(0.0),
                m["backbone_river_km"].as_f64().unwrap_or(0.0)
            ),
            "states" => format!(
                "{} states (target {}), {} regions, {} continents, mean {:.0}k km², {} islands attached",
                m["states"], m["target_states"], m["regions"], m["continents"], m["mean_area_km2"].as_f64().unwrap_or(0.0) / 1000.0, m["attached_islands"]
            ),
            "provinces" => format!(
                "{} provinces: {} land (median {:.0}k km²), {} wasteland, {} lakes, {} sea; {} straits",
                m["provinces"], m["land"], m["median_land_km2"].as_f64().unwrap_or(0.0) / 1000.0, m["wasteland"], m["lakes"], m["sea"], m["straits"]
            ),
            "cultures" => format!(
                "{} cultures in {} groups ({} ever: {} splits, {} merged, {} extinct), {} bands, {:.1} M people over {} years",
                m["cultures"], m["groups"], m["cultures_ever"], m["splits"], m["merged"], m["extinct"], m["bands"],
                m["population"].as_f64().unwrap_or(0.0) / 1e6, m["years"]
            ),
            "nations" => format!(
                "{} nations in {} ({} ever), {:.0}% of land ruled, {} conquests, {} independences, {} colonies, {} railway lines, {} airports; {} leads ({})",
                m["nations"], m["start_date"], m["nations_ever"], m["ruled_share"].as_f64().unwrap_or(0.0) * 100.0,
                m["conquests"], m["independences"], m["colonies"], m["railways"], m["transport"]["airports"],
                m["leader"]["name"].as_str().unwrap_or("no one"), m["era"].as_str().unwrap_or("")
            ),
            _ => String::new(),
        };
        println!("  {:<22} {:<6} {}", st.title, st.state, extra);
    }
}

fn main() {
    let args = Args::parse();
    let Some(cmd) = args.pos.first().cloned() else {
        print!("{USAGE}");
        return;
    };
    match cmd.as_str() {
        "new" => {
            let dir = PathBuf::from(args.pos.get(1).unwrap_or_else(|| die("missing project dir")));
            let w = World::new(params_from(&args));
            w.save(&dir).unwrap_or_else(|e| die(&e.to_string()));
            println!("created {}", dir.display());
        }
        "run" => {
            let dir = PathBuf::from(args.pos.get(1).unwrap_or_else(|| die("missing project dir")));
            let mut w = World::load(&dir).unwrap_or_else(|e| die(&e));
            run_world(&mut w, step_arg(&args));
            w.save(&dir).unwrap_or_else(|e| die(&e.to_string()));
            summary(&w);
        }
        "export" => {
            let dir = PathBuf::from(args.pos.get(1).unwrap_or_else(|| die("missing project dir")));
            let mut w = World::load(&dir).unwrap_or_else(|e| die(&e));
            run_world(&mut w, LAST);
            let out = args.get("out").map(PathBuf::from).unwrap_or_else(|| dir.join("export"));
            let r = export(&mut w, &out, &export_opts(&args), &|f, m| eprint!("\r  export {:>3.0}% {:<30}", f * 100.0, m)).unwrap_or_else(|e| die(&e));
            eprintln!();
            println!("exported {} files to {} in {} ms", r.files.len(), r.dir, r.millis);
        }
        "generate" => {
            let dir = PathBuf::from(args.get("out").unwrap_or_else(|| die("missing --out")));
            let mut w = World::new(params_from(&args));
            run_world(&mut w, step_arg(&args));
            w.save(&dir).unwrap_or_else(|e| die(&e.to_string()));
            if args.flag("export") {
                let r = export(&mut w, &dir.join("export"), &export_opts(&args), &|_, _| {}).unwrap_or_else(|e| die(&e));
                println!("exported {} files in {} ms", r.files.len(), r.millis);
            }
            summary(&w);
        }
        "sweep" => {
            let out = PathBuf::from(args.get("out").unwrap_or_else(|| die("missing --out")));
            let seeds = args.get("seeds").unwrap_or("1..4");
            let (a, b) = seeds.split_once("..").unwrap_or_else(|| die("--seeds A..B"));
            let (a, b): (u64, u64) = (a.parse().unwrap_or(1), b.parse().unwrap_or(4));
            let to = step_arg(&args);
            let mut csv = String::from("seed;level;land_pct;states;provinces;land_provinces;cultures;culture_groups;population_m;cash_crop_provinces;oil_provinces;unapplied_edits;validation;seconds\n");
            for seed in a..=b {
                let mut p = params_from(&args);
                p.planet.seed = seed;
                p.planet.grid_level = args.num("level", 7);
                let level = p.planet.grid_level;
                let mut w = World::new(p);
                eprintln!("seed {seed}");
                let t0 = Instant::now();
                run_world(&mut w, to);
                let secs = t0.elapsed().as_secs_f64();
                let dir = out.join(format!("seed-{seed}"));
                w.save(&dir).unwrap_or_else(|e| die(&e.to_string()));
                if !args.flag("no-export") {
                    let mut o = export_opts(&args);
                    o.width = args.num("width", 2048);
                    o.height = args.num("height", 1024);
                    export(&mut w, &dir.join("export"), &o, &|_, _| {}).unwrap_or_else(|e| die(&e));
                }
                summary(&w);
                let rep = worldcore::validate::validate(&mut w);
                let m = |st: Step, k: &str| w.meta(st).map(|m| m[k].clone()).unwrap_or(serde_json::Value::Null);
                let num = |v: serde_json::Value| v.as_f64().map_or(String::new(), |x| format!("{x}"));
                let provs = w.meta(Step::Provinces).and_then(|m| m["table"]["provinces"].as_array().cloned()).unwrap_or_default();
                let crops = provs.iter().filter(|p| p["trade_good"].as_str().is_some_and(|g| worldcore::stages::resources::category(g, "trade good") == "cash crop")).count();
                let oil = provs.iter().filter(|p| p["resources"].as_array().is_some_and(|r| r.iter().any(|x| x == "oil"))).count();
                let unapplied = worldcore::api::overrides_report(&w)["unapplied"].as_array().map_or(0, |a| a.len());
                csv += &format!(
                    "{seed};{level};{:.1};{};{};{};{};{};{};{crops};{oil};{unapplied};{};{secs:.1}\n",
                    m(Step::Sketch, "land_fraction").as_f64().unwrap_or(0.0) * 100.0,
                    num(m(Step::States, "states")), num(m(Step::Provinces, "provinces")), num(m(Step::Provinces, "land")),
                    num(m(Step::Cultures, "cultures")), num(m(Step::Cultures, "groups")),
                    m(Step::Cultures, "population").as_f64().map_or(String::new(), |x| format!("{:.2}", x / 1e6)),
                    if rep.ok { "ok" } else { "errors" },
                );
                std::fs::create_dir_all(&out).ok();
                std::fs::write(out.join("sweep.csv"), &csv).unwrap_or_else(|e| die(&e.to_string()));
            }
            println!("wrote {}", out.join("sweep.csv").display());
        }
        "import-heightmap" => {
            use worldcore::edits::HeightEncoding;
            let dir = PathBuf::from(args.pos.get(1).unwrap_or_else(|| die("missing project dir")));
            let png = args.pos.get(2).unwrap_or_else(|| die("missing heightmap png"));
            let mut w = World::load(&dir).unwrap_or_else(|e| die(&e));
            let enc = match args.get("encoding").unwrap_or("heightmap16") {
                "heightmap16" => HeightEncoding::Heightmap16,
                "paradox8" => HeightEncoding::Paradox8 { sea_level_value: args.num("sea-level", 20.0), max_elevation_m: args.num("max", 6500.0), max_depth_m: args.num("depth", 6000.0) },
                "linear" => HeightEncoding::Linear { min_m: args.num("min", -11000.0), max_m: args.num("max", 9000.0) },
                e => die(&format!("unknown encoding `{e}` (heightmap16, paradox8, linear)")),
            };
            let imp = worldcore::import::describe(png, enc, args.num("lat-min", -90.0), args.num("lat-max", 90.0), args.num("blend", 1.0)).unwrap_or_else(|e| die(&e));
            w.edits.imports.elevation = Some(imp);
            run_world(&mut w, step_arg(&args));
            w.save(&dir).unwrap_or_else(|e| die(&e.to_string()));
            summary(&w);
        }
        "import-provinces" => {
            let dir = PathBuf::from(args.pos.get(1).unwrap_or_else(|| die("missing project dir")));
            let mut w = World::load(&dir).unwrap_or_else(|e| die(&e));
            if args.flag("remove") {
                w.edits.imports.provinces = None;
            } else {
                let png = args.pos.get(2).unwrap_or_else(|| die("missing provinces png"));
                let (imp, rep) = worldcore::province_import::describe(png, args.get("csv"), args.get("lat-min").and_then(|v| v.parse().ok()), args.get("lat-max").and_then(|v| v.parse().ok()))
                    .unwrap_or_else(|e| die(&e));
                print_report(&rep);
                if !rep.ok {
                    die("import refused; fix the errors above");
                }
                w.edits.imports.provinces = Some(imp);
            }
            run_world(&mut w, step_arg(&args));
            w.save(&dir).unwrap_or_else(|e| die(&e.to_string()));
            summary(&w);
        }
        "validate-provinces" => {
            let png = args.pos.get(1).unwrap_or_else(|| die("missing provinces png"));
            let img = worldcore::province_import::read_png(png).unwrap_or_else(|e| die(&e));
            let defs = args.get("csv").map(|c| worldcore::province_import::read_definition(c).unwrap_or_else(|e| die(&e)));
            let (rep, _) = worldcore::province_import::validate(&img, defs.as_deref(), args.num("min-pixels", 8));
            print_report(&rep);
            if !rep.ok {
                std::process::exit(1);
            }
        }
        "check" => check(&args),
        "info" => {
            let dir = PathBuf::from(args.pos.get(1).unwrap_or_else(|| die("missing project dir")));
            let w = World::load(&dir).unwrap_or_else(|e| die(&e));
            if args.flag("json") {
                let steps: Vec<serde_json::Value> = w.status().iter().map(|st| serde_json::json!({ "step": st.key, "state": st.state, "millis": st.millis })).collect();
                let out = serde_json::json!({ "seed": w.params.planet.seed, "level": w.params.planet.grid_level, "steps": steps, "overrides": worldcore::api::overrides_report(&w) });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                println!("seed {} level {}", w.params.planet.seed, w.params.planet.grid_level);
                summary(&w);
                let total = worldcore::api::overrides_report(&w)["total"].as_u64().unwrap_or(0);
                if total > 0 {
                    println!("  {total} override edits (worldgen overrides {} for details)", dir.display());
                }
            }
        }
        "validate" => {
            let dir = PathBuf::from(args.pos.get(1).unwrap_or_else(|| die("missing project dir")));
            let mut w = World::load(&dir).unwrap_or_else(|e| die(&e));
            if args.flag("run") {
                run_world(&mut w, LAST);
                w.save(&dir).unwrap_or_else(|e| die(&e.to_string()));
            }
            let rep = worldcore::validate::validate(&mut w);
            if args.flag("json") {
                println!("{}", serde_json::to_string_pretty(&rep).unwrap());
            } else {
                for c in &rep.checks {
                    println!("  check    {c}");
                }
                for e in &rep.errors {
                    println!("  error    {e}");
                }
                for wn in &rep.warnings {
                    println!("  warning  {wn}");
                }
                println!("{}: {} checks, {} errors, {} warnings", if rep.ok { "PASS" } else { "FAIL" }, rep.checks.len(), rep.errors.len(), rep.warnings.len());
            }
            if !rep.ok {
                std::process::exit(1);
            }
        }
        "overrides" => overrides(&args),
        "edit" => edit(&args),
        "version" => println!("worldgen {}", fwm_update::current_version()),
        "guide" => guide(&args),
        "directive" => {
            if args.flag("list") {
                for a in worldcore::directives::actions() {
                    println!("{:<9} {:<14} {}\n{:<24} args: {}", a.stage, a.name, a.description, "", a.schema["properties"]);
                }
                return;
            }
            let dir = PathBuf::from(args.pos.get(1).unwrap_or_else(|| die("missing project dir")));
            let mut w = World::load(&dir).unwrap_or_else(|e| die(&e));
            let d = worldcore::directives::Directive {
                stage: args.get("stage").unwrap_or_else(|| die("missing --stage")).to_string(),
                at: args.num("at", i64::MIN).max(i64::MIN + 1),
                action: args.get("action").unwrap_or_else(|| die("missing --action")).to_string(),
                args: serde_json::from_str(args.get("args").unwrap_or("{}")).unwrap_or_else(|e| die(&format!("--args: {e}"))),
                note: args.get("note").unwrap_or("").to_string(),
                by: "user".into(),
            };
            if args.get("at").is_none() {
                die("missing --at (generation for cultures, year for nations)");
            }
            worldcore::directives::validate(&d).unwrap_or_else(|e| die(&e));
            w.edits.overrides.directives.push(d);
            println!("{} directives", w.edits.overrides.directives.len());
            if args.flag("run") {
                run_world(&mut w, LAST);
                summary(&w);
            }
            w.save(&dir).unwrap_or_else(|e| die(&e.to_string()));
        }
        "guide-setup" => {
            let session = Mutex::new(Session::default());
            let progress = Arc::new(Mutex::new(ProgressState::default()));
            let mut set = serde_json::Map::new();
            for (flag, key) in [("provider", "provider"), ("base-url", "base_url"), ("model", "model"), ("key", "api_key"), ("effort", "effort")] {
                if let Some(v) = args.get(flag) {
                    set.insert(key.into(), serde_json::json!(v));
                }
            }
            let cmd = if set.is_empty() { "guide_config_get" } else { "guide_config_set" };
            let r = fwm_guide::handle(&session, &progress, cmd, &serde_json::Value::Object(set)).unwrap().unwrap_or_else(|e| die(&e));
            println!("{}", serde_json::to_string_pretty(&r).unwrap());
        }
        "check-update" => check_update(&args),
        "serve" => serve(&args),
        "stats" => {
            let dir = PathBuf::from(args.pos.get(1).unwrap_or_else(|| die("missing project dir")));
            let w = World::load(&dir).unwrap_or_else(|e| die(&e));
            stats(w);
        }
        _ => {
            print!("{USAGE}");
            std::process::exit(2);
        }
    }
}

/// Zonal means of climate fields over land and sea, for tuning against Earth.
fn stats(mut w: World) {
    use worldcore::fields::Field;
    let g = w.grid();
    let get = |w: &World, n: &str| -> Vec<f32> {
        match w.field(n) {
            Some((f, _, _)) => f.as_f32(),
            None => vec![0.0; g.len()],
        }
    };
    let elev = get(&w, "elevation");
    let (tm, tw, tc, pa) = (get(&w, "t_mean"), get(&w, "t_warm"), get(&w, "t_cold"), get(&w, "p_ann"));
    let temp = match w.field("temp") {
        Some((Field::F32(v), _, _)) => v.clone(),
        _ => vec![0.0; 12 * g.len()],
    };
    let n = g.len();
    println!(" lat  | land: Tmean  Tjan  Tjul  Tcold Twarm  P(mm) | sea: Tmean  P(mm) | land%");
    for b in 0..18 {
        let (lo, hi) = (-90.0 + b as f64 * 10.0, -80.0 + b as f64 * 10.0);
        let mut acc = [0.0f64; 9];
        for i in 0..n {
            let la = g.lat[i].to_degrees();
            if la < lo || la >= hi {
                continue;
            }
            let a = g.area[i];
            if elev[i] > 0.0 {
                acc[0] += a;
                acc[1] += a * tm[i] as f64;
                acc[2] += a * temp[i] as f64;
                acc[3] += a * temp[6 * n + i] as f64;
                acc[4] += a * tc[i] as f64;
                acc[5] += a * tw[i] as f64;
                acc[6] += a * pa[i] as f64;
            } else {
                acc[7] += a;
                acc[8] += a * tm[i] as f64;
                acc[2 + 0] += 0.0;
            }
        }
        let sea_p: f64 = (0..n).filter(|&i| elev[i] <= 0.0 && (lo..hi).contains(&g.lat[i].to_degrees())).map(|i| g.area[i] * pa[i] as f64).sum();
        let l = acc[0].max(1e-12);
        let s = acc[7].max(1e-12);
        println!(
            "{:>4.0}  |      {:>5.1} {:>5.1} {:>5.1} {:>5.1} {:>5.1} {:>6.0} |      {:>5.1} {:>6.0} | {:>4.0}",
            lo + 5.0, acc[1] / l, acc[2] / l, acc[3] / l, acc[4] / l, acc[5] / l, acc[6] / l, acc[8] / s, sea_p / s, 100.0 * acc[0] / (acc[0] + acc[7])
        );
    }
    // Winds and seasonality over land.
    let wu = get(&w, "wind_u");
    let wv = get(&w, "wind_v");
    let pr = match w.field("precip") {
        Some((Field::F32(v), _, _)) => v.clone(),
        _ => vec![0.0; 12 * n],
    };
    if wu.len() == 12 * n {
        println!("\n lat  | land wind m/s: |Jan| |Jul|  v(Jan) v(Jul) | summer/winter rain");
        for b in 0..18 {
            let (lo, hi) = (-90.0 + b as f64 * 10.0, -80.0 + b as f64 * 10.0);
            let mut acc = [0.0f64; 6];
            for i in 0..n {
                let la = g.lat[i].to_degrees();
                if la < lo || la >= hi || elev[i] <= 0.0 {
                    continue;
                }
                let a = g.area[i];
                let (j, l) = (i, 6 * n + i);
                acc[0] += a;
                acc[1] += a * (wu[j].hypot(wv[j])) as f64;
                acc[2] += a * (wu[l].hypot(wv[l])) as f64;
                acc[3] += a * wv[j] as f64;
                acc[4] += a * wv[l] as f64;
                // Summer = Apr–Sep in the north, Oct–Mar in the south.
                let (mut s, mut wi) = (0.0, 0.0);
                for m in 0..12 {
                    let summer = (3..9).contains(&m) == (la >= 0.0);
                    if summer { s += pr[m * n + i] as f64 } else { wi += pr[m * n + i] as f64 }
                }
                acc[5] += a * (s + 1.0) / (wi + 1.0);
            }
            let l = acc[0].max(1e-12);
            if acc[0] > 0.0 {
                println!("{:>4.0}  |            {:>5.1} {:>5.1}  {:>6.1} {:>6.1} | {:>5.2}", lo + 5.0, acc[1] / l, acc[2] / l, acc[3] / l, acc[4] / l, acc[5] / l);
            }
        }
    }
    if let Some(m) = w.meta(Step::Biomes) {
        println!("koppen: {}", m["koppen_share"]);
    }
    if let Some(m) = w.meta(Step::Relief) {
        println!("boundaries km: {}", m["boundary_km"]);
    }
}

fn print_report(r: &worldcore::province_import::Report) {
    println!("{}×{} px, {} provinces: {}", r.width, r.height, r.provinces, if r.ok { "OK" } else { "ERRORS" });
    for e in &r.errors {
        println!("  error:   {e}");
    }
    for w in &r.warnings {
        println!("  warning: {w}");
    }
}

fn layer_arg(k: &str) -> EditLayer {
    EditLayer::from_key(k).unwrap_or_else(|| die(&format!("unknown layer `{k}` (see LAYERS in worldgen help)")))
}

fn layers_opt(args: &Args) -> Option<Vec<EditLayer>> {
    args.get("layers").map(|v| v.split(',').map(|k| layer_arg(k.trim())).collect())
}

/// `worldgen overrides`: inspect and manage the override layers of a project.
fn overrides(args: &Args) {
    let dir = PathBuf::from(args.pos.get(1).unwrap_or_else(|| die("missing project dir")));
    let mut w = World::load(&dir).unwrap_or_else(|e| die(&e));
    let sub = args.pos.get(2).map(String::as_str).unwrap_or("list");
    let save = |w: &World| w.save(&dir).unwrap_or_else(|e| die(&e.to_string()));
    match sub {
        "list" => {
            let r = api::overrides_report(&w);
            println!("  {:<12} {:>6}  {:<13} layer", "key", "edits", "first step");
            for l in r["layers"].as_array().into_iter().flatten() {
                println!("  {:<12} {:>6}  {:<13} {}", l["layer"].as_str().unwrap_or(""), l["edits"].as_u64().unwrap_or(0), l["step"].as_str().unwrap_or(""), l["title"].as_str().unwrap_or(""));
            }
            let un = r["unapplied"].as_array().cloned().unwrap_or_default();
            if un.is_empty() {
                println!("all edits apply (as of the last up-to-date run of states and provinces)");
            } else {
                println!("{} edits no longer apply:", un.len());
                for u in &un {
                    println!("  {} #{} {} at {}: {}", u["layer"].as_str().unwrap_or(""), u["index"], u["tool"].as_str().unwrap_or(""), u["at"], u["reason"].as_str().unwrap_or(""));
                }
            }
        }
        "clear" => {
            let l = layer_arg(args.pos.get(3).unwrap_or_else(|| die("missing layer")));
            let n = w.edits.count(l);
            w.edits.clear_layer(l);
            save(&w);
            println!("cleared {n} edits from {}", l.key());
        }
        "remove" => {
            let l = layer_arg(args.pos.get(3).unwrap_or_else(|| die("missing layer")));
            let idx: Vec<usize> = args.pos.get(4).unwrap_or_else(|| die("missing indices")).split(',').map(|x| x.trim().parse().unwrap_or_else(|_| die("indices are numbers"))).collect();
            let n = w.edits.remove_strokes(l, &idx);
            save(&w);
            println!("removed {n} edits from {}", l.key());
        }
        "prune" => {
            run_world(&mut w, Step::Provinces);
            let r = api::overrides_report(&w);
            let mut by_layer: std::collections::BTreeMap<String, Vec<usize>> = Default::default();
            for u in r["unapplied"].as_array().into_iter().flatten() {
                by_layer.entry(u["layer"].as_str().unwrap_or("").into()).or_default().push(u["index"].as_u64().unwrap_or(0) as usize);
            }
            let mut n = 0;
            for (k, idx) in &by_layer {
                n += w.edits.remove_strokes(layer_arg(k), idx);
            }
            save(&w);
            println!("removed {n} edits that no longer apply");
        }
        "export" => {
            let file = args.pos.get(3).unwrap_or_else(|| die("missing file"));
            let layers = layers_opt(args).unwrap_or_else(|| EditLayer::ALL.into_iter().filter(|&l| w.edits.count(l) > 0).collect());
            let b = w.edits.bundle(&layers, w.params.planet.seed);
            std::fs::write(file, serde_json::to_string_pretty(&b).unwrap()).unwrap_or_else(|e| die(&format!("{file}: {e}")));
            println!("wrote {} layers to {file}: {}", layers.len(), layers.iter().map(|l| format!("{} ({})", l.key(), w.edits.count(*l))).collect::<Vec<_>>().join(", "));
        }
        "import" => {
            let file = args.pos.get(3).unwrap_or_else(|| die("missing file"));
            let text = std::fs::read_to_string(file).unwrap_or_else(|e| die(&format!("{file}: {e}")));
            let b: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| die(&format!("{file}: {e}")));
            let only = layers_opt(args);
            let taken = w.edits.apply_bundle(&b, only.as_deref(), args.flag("replace")).unwrap_or_else(|e| die(&e));
            save(&w);
            println!(
                "{} {}",
                if args.flag("replace") { "replaced" } else { "added" },
                taken.iter().map(|(l, n)| format!("{} ({n})", l.key())).collect::<Vec<_>>().join(", ")
            );
        }
        _ => die(&format!("unknown overrides command `{sub}` (list, clear, remove, prune, export, import)")),
    }
}

/// `worldgen edit`: add one override stroke, as a click or drag in the app would.
fn edit(args: &Args) {
    let dir = PathBuf::from(args.pos.get(1).unwrap_or_else(|| die("missing project dir")));
    let mut w = World::load(&dir).unwrap_or_else(|e| die(&e));
    let tool: Tool = serde_json::from_value(serde_json::json!(args.get("tool").unwrap_or_else(|| die("missing --tool")))).unwrap_or_else(|_| die("unknown --tool (snake_case names as in the app, e.g. province_merge)"));
    let points: Vec<[f64; 2]> = args
        .get("at")
        .unwrap_or_else(|| die("missing --at LAT,LON[;LAT,LON...]"))
        .split(';')
        .map(|p| {
            let (a, b) = p.split_once(',').unwrap_or_else(|| die("--at points are LAT,LON"));
            let (la, lo): (f64, f64) = (a.trim().parse().unwrap_or_else(|_| die("bad latitude")), b.trim().parse().unwrap_or_else(|_| die("bad longitude")));
            if !(-90.0..=90.0).contains(&la) || !(-180.0..=360.0).contains(&lo) {
                die("--at is out of range");
            }
            [la, lo]
        })
        .collect();
    let d = Stroke::default();
    let st = Stroke {
        tool,
        points,
        value: args.num("value", d.value),
        radius_km: args.num("radius", d.radius_km),
        strength: args.num("strength", d.strength),
        hardness: args.num("hardness", d.hardness),
        name: args.get("name").unwrap_or("").to_string(),
        ..d
    };
    let layer = tool.layer();
    w.edits.add_stroke(st);
    println!("added a {} edit to layer {} ({} edits)", args.get("tool").unwrap_or(""), layer.key(), w.edits.count(layer));
    if args.flag("run") {
        run_world(&mut w, LAST);
        summary(&w);
    }
    w.save(&dir).unwrap_or_else(|e| die(&e.to_string()));
}

/// `worldgen guide`: AI-guided history on a project, saved when done.
fn guide(args: &Args) {
    let dir = args.pos.get(1).unwrap_or_else(|| die("missing project dir"));
    let goal = args.get("goal").unwrap_or_else(|| die("missing --goal \"the history you want\""));
    let stage = args.get("stage").unwrap_or("nations");
    let session = Mutex::new(Session::default());
    let progress = Arc::new(Mutex::new(ProgressState::default()));
    let call = |cmd: &str, a: serde_json::Value| match api::handle(&session, &progress, cmd, a) {
        Ok(Reply::Json(v)) => v,
        Ok(_) => serde_json::Value::Null,
        Err(e) => die(&format!("{cmd}: {e}")),
    };
    call("open", serde_json::json!({ "path": dir }));
    eprintln!("bringing earlier steps up to date and starting the {stage} simulation…");
    call("sim_start", serde_json::json!({ "stage": stage }));
    let mut years = args.num("years", 50.0);
    let max_turns: usize = args.num("max-turns", 60);
    let max_actions: u64 = args.num("max-actions", 4);
    let (mut tin, mut tout) = (0u64, 0u64);
    for k in 0..max_turns {
        // The first turn looks at the start; later turns advance first.
        let r = fwm_guide::handle(&session, &progress, "guide_turn", &serde_json::json!({ "goal": goal, "years": if k == 0 { 0.0 } else { years }, "max_actions": max_actions }))
            .unwrap()
            .unwrap_or_else(|e| die(&e));
        tin += r["input_tokens"].as_u64().unwrap_or(0);
        tout += r["output_tokens"].as_u64().unwrap_or(0);
        if r["done"] == true {
            break;
        }
        println!("[{}] {}", r["position"], r["narration"].as_str().unwrap_or(""));
        for a in r["actions"].as_array().into_iter().flatten() {
            println!("    {} {} {}", if a["ok"] == true { "+" } else { "x" }, a["action"].as_str().unwrap_or(""), if a["ok"] == true { a["args"].to_string() } else { a["error"].as_str().unwrap_or("").to_string() });
        }
        if let Some(y) = r["next_years"].as_f64() {
            years = y.clamp(5.0, 1000.0);
        }
    }
    let st = call("sim_commit", serde_json::json!({}));
    call("save", serde_json::json!({ "path": dir }));
    eprintln!("committed {} ({} tokens in, {} out); saved {dir}", st["committed"].as_str().unwrap_or(stage), tin, tout);
}

/// `worldgen check-update`: compare with the newest GitHub release.
fn check_update(args: &Args) {
    let pre = if args.flag("betas") { Some(true) } else if args.flag("stable") { Some(false) } else { None };
    let c = fwm_update::check(pre).unwrap_or_else(|e| die(&e));
    println!("this build: {}", c.current);
    match &c.latest {
        None => println!("no {}release on GitHub yet", if c.include_prereleases { "" } else { "stable " }),
        Some(l) => {
            println!("newest {}release: {} ({}) {}", if c.include_prereleases { "" } else { "stable " }, l.version, l.published_at, l.url);
            println!("{}", if c.update_available { "an update is available" } else { "up to date" });
        }
    }
    if let (Some(dir), Some(l)) = (args.get("download"), &c.latest) {
        let a = l.portable.as_ref().unwrap_or_else(|| die("that release has no portable zip"));
        let shown = std::sync::atomic::AtomicU64::new(u64::MAX);
        let path = fwm_update::download(a, std::path::Path::new(dir), &|d, t| {
            // One line per 10%.
            let step = if t > 0 { d * 10 / t } else { d >> 20 };
            if shown.swap(step, std::sync::atomic::Ordering::Relaxed) != step {
                eprintln!("  {:>5.1} / {:.1} MB", d as f64 / 1e6, t as f64 / 1e6);
            }
        })
        .unwrap_or_else(|e| die(&e));
        println!("downloaded {}{}", path.display(), if a.sha256.is_some() { " (SHA-256 verified)" } else { "" });
    }
}

/// M0 check: the same seed gives the same world, and save/load is lossless.
fn check(args: &Args) {
    let mut p = params_from(args);
    p.planet.grid_level = args.num("level", 6);
    eprintln!("run A");
    let mut a = World::new(p.clone());
    run_world(&mut a, LAST);
    eprintln!("run B");
    let mut b = World::new(p.clone());
    run_world(&mut b, LAST);
    let (fa, fb) = (a.fingerprint(), b.fingerprint());
    let det_ok = fa == fb;
    for ((na, ha), (_, hb)) in fa.iter().zip(&fb) {
        if ha != hb {
            println!("  MISMATCH {na}: {ha} vs {hb}");
        }
    }
    println!("determinism: {} ({} fields)", if det_ok { "PASS" } else { "FAIL" }, fa.len());

    let dir = std::env::temp_dir().join(format!("worldgen-check-{}", std::process::id()));
    a.save(&dir).unwrap_or_else(|e| die(&e.to_string()));
    let c = World::load(&dir).unwrap_or_else(|e| die(&e));
    let rt_ok = c.fingerprint() == fa && c.params == a.params && c.edits == a.edits && STEPS.iter().all(|&s| c.is_fresh(s));
    println!("save/load round trip: {}", if rt_ok { "PASS" } else { "FAIL" });
    let _ = std::fs::remove_dir_all(&dir);
    if !(det_ok && rt_ok) {
        std::process::exit(1);
    }
}

fn serve(args: &Args) {
    let port: u16 = args.num("port", 8765);
    let static_dir = args.get("static").map(PathBuf::from);
    let debug = args.flag("debug");
    let mut session = Session::default();
    if let Some(p) = args.get("project") {
        let w = World::load(std::path::Path::new(p)).unwrap_or_else(|e| die(&e));
        session.world = w;
        session.path = Some(PathBuf::from(p));
    }
    let session = Arc::new(Mutex::new(session));
    let progress = Arc::new(Mutex::new(ProgressState::default()));
    let server = tiny_http::Server::http(("127.0.0.1", port)).unwrap_or_else(|e| die(&e.to_string()));
    println!("worldgen serving on http://127.0.0.1:{port}/");
    for mut req in server.incoming_requests() {
        let session = session.clone();
        let progress = progress.clone();
        let static_dir = static_dir.clone();
        let debug = debug;
        std::thread::spawn(move || {
            // Only this server's own pages and the Vite dev server may call the API.
            // A JSON content type is required, which forces a CORS preflight for any
            // cross-site request, so other websites cannot drive the local engine.
            let header = |name: &str| req.headers().iter().find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|h| h.value.as_str().to_string());
            let origin = header("Origin");
            let allowed = [format!("http://127.0.0.1:{port}"), format!("http://localhost:{port}"), "http://localhost:5173".into(), "http://127.0.0.1:5173".into()];
            let origin_ok = origin.as_ref().map_or(true, |o| allowed.contains(o));
            let cors: Vec<tiny_http::Header> = match origin.as_ref().filter(|_| origin_ok) {
                Some(o) => vec![
                    tiny_http::Header::from_bytes("Access-Control-Allow-Origin", o.as_bytes()).unwrap(),
                    tiny_http::Header::from_bytes("Access-Control-Allow-Headers", "content-type").unwrap(),
                    tiny_http::Header::from_bytes("Vary", "Origin").unwrap(),
                ],
                None => vec![],
            };
            let respond = |req: tiny_http::Request, status: u16, ctype: &str, data: Vec<u8>, cors: &[tiny_http::Header]| {
                let mut r = tiny_http::Response::from_data(data).with_status_code(status);
                r.add_header(tiny_http::Header::from_bytes("Content-Type", ctype).unwrap());
                for h in cors {
                    r.add_header(h.clone());
                }
                let _ = req.respond(r);
            };
            let url = req.url().to_string();
            if req.method() == &tiny_http::Method::Options {
                respond(req, if origin_ok { 204 } else { 403 }, "text/plain", vec![], &cors);
                return;
            }
            let is_post_json = req.method() == &tiny_http::Method::Post
                && header("Content-Type").is_some_and(|c| c.starts_with("application/json"));
            // Debug aid (--debug only): the UI can upload a canvas snapshot to <temp>/fwm-debug/.
            if let Some(name) = url.strip_prefix("/debug/upload/") {
                if !debug || !origin_ok {
                    respond(req, 403, "text/plain", b"forbidden".to_vec(), &cors);
                    return;
                }
                let name: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-').collect();
                let dir = std::env::temp_dir().join("fwm-debug");
                let _ = std::fs::create_dir_all(&dir);
                let mut body = Vec::new();
                let _ = req.as_reader().read_to_end(&mut body);
                let ok = std::fs::write(dir.join(&name), &body).is_ok();
                let msg = dir.join(&name).display().to_string().into_bytes();
                respond(req, if ok { 200 } else { 500 }, "text/plain", msg, &cors);
                return;
            }
            if let Some(cmd) = url.strip_prefix("/api/") {
                if !origin_ok || !is_post_json {
                    respond(req, 403, "text/plain", b"API calls must be same-origin JSON POSTs".to_vec(), &cors);
                    return;
                }
                let cmd = cmd.to_string();
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                let a: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
                if cmd == "update_install" {
                    respond(req, 400, "text/plain; charset=utf-8", b"Installing updates works in the desktop app; download the new release from GitHub instead".to_vec(), &cors);
                    return;
                }
                if cmd.starts_with("guide_") {
                    match fwm_guide::handle(&session, &progress, &cmd, &a).unwrap_or_else(|| Err(format!("unknown command `{cmd}`"))) {
                        Ok(v) => respond(req, 200, "application/json", serde_json::to_vec(&v).unwrap(), &cors),
                        Err(e) => respond(req, 400, "text/plain; charset=utf-8", e.into_bytes(), &cors),
                    }
                    return;
                }
                if let Some(r) = fwm_update::handle(&cmd, &a) {
                    match r {
                        Ok(v) => respond(req, 200, "application/json", serde_json::to_vec(&v).unwrap(), &cors),
                        Err(e) => respond(req, 400, "text/plain; charset=utf-8", e.into_bytes(), &cors),
                    }
                    return;
                }
                match api::handle(&session, &progress, &cmd, a) {
                    Ok(Reply::Json(v)) => respond(req, 200, "application/json", serde_json::to_vec(&v).unwrap(), &cors),
                    Ok(Reply::Bytes(b)) => respond(req, 200, "application/octet-stream", b, &cors),
                    Err(e) => respond(req, 400, "text/plain; charset=utf-8", e.into_bytes(), &cors),
                }
                return;
            }
            // Static UI files.
            if let Some(root) = static_dir {
                let rel = url.split('?').next().unwrap_or("/").trim_start_matches('/');
                let rel = if rel.is_empty() { "index.html" } else { rel };
                if !rel.contains("..") {
                    if let Ok(data) = std::fs::read(root.join(rel)) {
                        let ctype = match rel.rsplit('.').next() {
                            Some("html") => "text/html; charset=utf-8",
                            Some("js") => "text/javascript",
                            Some("css") => "text/css",
                            Some("svg") => "image/svg+xml",
                            Some("png") => "image/png",
                            Some("json") => "application/json",
                            _ => "application/octet-stream",
                        };
                        let mut r = tiny_http::Response::from_data(data);
                        r.add_header(tiny_http::Header::from_bytes("Content-Type", ctype).unwrap());
                        let _ = req.respond(r);
                        return;
                    }
                }
            }
            let _ = req.respond(tiny_http::Response::from_string("not found").with_status_code(404));
        });
    }
}
