// Fantasy World Maker desktop shell: hosts the TypeScript UI and forwards
// every UI command to the Rust simulation core.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};
use tauri::ipc::Response;
use worldcore::api::{self, ProgressState, Reply, Session};

struct AppState {
    session: Arc<Mutex<Session>>,
    progress: Arc<Mutex<ProgressState>>,
}

/// Single entry point: JSON arguments in, JSON or raw bytes out. Long commands
/// run on a blocking thread so `progress` can be polled meanwhile.
#[tauri::command]
async fn api(app: tauri::AppHandle, state: tauri::State<'_, AppState>, cmd: String, args: serde_json::Value) -> Result<Response, String> {
    // FWM_TRACE=<file> logs every command, for diagnosing the UI ↔ core link.
    if let Ok(path) = std::env::var("FWM_TRACE") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{cmd}");
        }
    }
    let session = state.session.clone();
    let progress = state.progress.clone();
    if cmd == "update_install" {
        return tauri::async_runtime::spawn_blocking(move || install_update(&app, &session, &progress, &args)).await.map_err(|e| e.to_string())?;
    }
    if cmd.starts_with("guide_") {
        return tauri::async_runtime::spawn_blocking(move || match fwm_guide::handle(&session, &progress, &cmd, &args) {
            Some(r) => Ok(Response::new(serde_json::to_vec(&r?).map_err(|e| e.to_string())?)),
            None => Err(format!("unknown command `{cmd}`")),
        })
        .await
        .map_err(|e| e.to_string())?;
    }
    if cmd.starts_with("update_") || cmd == "app_version" {
        return tauri::async_runtime::spawn_blocking(move || match fwm_update::handle(&cmd, &args) {
            Some(r) => Ok(Response::new(serde_json::to_vec(&r?).map_err(|e| e.to_string())?)),
            None => Err(format!("unknown command `{cmd}`")),
        })
        .await
        .map_err(|e| e.to_string())?;
    }
    tauri::async_runtime::spawn_blocking(move || match api::handle(&session, &progress, &cmd, args)? {
        Reply::Json(v) => Ok(Response::new(serde_json::to_vec(&v).map_err(|e| e.to_string())?)),
        Reply::Bytes(b) => Ok(Response::new(b)),
    })
    .await
    .map_err(|e| e.to_string())?
}

struct RunningGuard(Arc<Mutex<ProgressState>>);
impl Drop for RunningGuard {
    fn drop(&mut self) {
        if let Ok(mut p) = self.0.lock() {
            p.running = false;
        }
    }
}

/// Download the installer of the newest release (size and SHA-256 checked),
/// start it and quit, so it can replace this program. Progress is reported
/// through the shared progress state (task "update").
fn install_update(app: &tauri::AppHandle, session: &Arc<Mutex<Session>>, progress: &Arc<Mutex<ProgressState>>, args: &serde_json::Value) -> Result<Response, String> {
    // Reject if the session is actively in use or has unsaved edits.
    {
        let s = match session.try_lock() {
            Ok(s) => s,
            Err(std::sync::TryLockError::WouldBlock) => {
                return Err("another operation is currently using the project; wait for it to finish before updating".into());
            }
            Err(std::sync::TryLockError::Poisoned(_)) => return Err("session lock poisoned".into()),
        };
        if s.dirty {
            return Err("save or discard your unsaved project changes before installing an update".into());
        }
    }
    // Atomically claim the progress slot so concurrent tasks cannot overlap.
    {
        let mut p = progress.lock().map_err(|_| "progress lock poisoned".to_string())?;
        if p.running {
            return Err(format!("another task ({}) is already running", p.task));
        }
        *p = ProgressState { running: true, task: "update".into(), step: "update".into(), frac: 0.0, msg: "Checking GitHub".into() };
    }
    let _running_guard = RunningGuard(progress.clone());
    let set = |f: f32, msg: String| {
        if let Ok(mut p) = progress.lock() {
            *p = ProgressState { running: true, task: "update".into(), step: "update".into(), frac: f, msg };
        }
    };
    let version = (|| {
        let c = fwm_update::check(args["include_prereleases"].as_bool())?;
        let rel = c.latest.filter(|_| c.update_available).ok_or("no newer release to install")?;
        let asset = rel.installer.ok_or("the newest release has no installer")?;
        let path = fwm_update::download(&asset, &fwm_update::download_dir(), &|d, t| {
            set(if t > 0 { d as f32 / t as f32 } else { 0.0 }, format!("Downloading {} ({:.1} of {:.1} MB)", rel.version, d as f64 / 1e6, t as f64 / 1e6))
        })?;
        // Ensure no session mutation/save is mid-flight before launching installer and exiting.
        let s = session.lock().map_err(|_| "session lock poisoned".to_string())?;
        if s.dirty {
            return Err("save or discard your unsaved project changes before installing an update".into());
        }
        set(1.0, "Starting the installer".into());
        fwm_update::launch_installer(&path)?;
        drop(s);
        Ok::<_, String>(rel.version)
    })()?;
    drop(_running_guard);
    // Give the UI a moment to show the message, then make way for the installer.
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(800));
        app.exit(0);
    });
    Ok(Response::new(serde_json::to_vec(&serde_json::json!({ "installing": version })).map_err(|e| e.to_string())?))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState { session: Arc::new(Mutex::new(Session::default())), progress: Arc::new(Mutex::new(ProgressState::default())) })
        .invoke_handler(tauri::generate_handler![api])
        .run(tauri::generate_context!())
        .expect("error while running Fantasy World Maker");
}
