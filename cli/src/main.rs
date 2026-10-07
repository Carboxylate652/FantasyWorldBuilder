//! `worldgen` — headless Fantasy World Maker: batch generation, seed sweeps,
//! determinism checks, export, and an HTTP server for the UI.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use worldcore::api::{self, ProgressState, Reply, Session};
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
  worldgen sweep --out <dir> --seeds A..B [--level L] [--width W] [--height H]
  worldgen import-heightmap <project-dir> <png> [--encoding heightmap16|paradox8|linear] [--min M --max M] [--blend B]
  worldgen import-provinces <project-dir> <provinces.png> [--csv definition.csv] [--lat-min A --lat-max B]
  worldgen validate-provinces <provinces.png> [--csv definition.csv]
  worldgen import-provinces <project-dir> --remove
  worldgen check [--seed N] [--level L]      determinism + save/load round-trip check
  worldgen stats <project-dir>               zonal climate, wind and rain-seasonality means
  worldgen info <project-dir>
  worldgen serve [--port 8765] [--static DIR] [--project DIR] [--debug]

STEPS: planet, sketch, plates, relief, climate, hydrology, biomes (Stage 1),
       habitability, states, provinces (Stage 2; default: provinces)
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
            for seed in a..=b {
                let mut p = params_from(&args);
                p.planet.seed = seed;
                p.planet.grid_level = args.num("level", 7);
                let mut w = World::new(p);
                eprintln!("seed {seed}");
                run_world(&mut w, LAST);
                let mut o = export_opts(&args);
                o.width = args.num("width", 2048);
                o.height = args.num("height", 1024);
                let dir = out.join(format!("seed-{seed}"));
                w.save(&dir).unwrap_or_else(|e| die(&e.to_string()));
                export(&mut w, &dir.join("export"), &o, &|_, _| {}).unwrap_or_else(|e| die(&e));
                summary(&w);
            }
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
            println!("seed {} level {}", w.params.planet.seed, w.params.planet.grid_level);
            summary(&w);
        }
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
