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
async fn api(state: tauri::State<'_, AppState>, cmd: String, args: serde_json::Value) -> Result<Response, String> {
    // FWM_TRACE=<file> logs every command, for diagnosing the UI ↔ core link.
    if let Ok(path) = std::env::var("FWM_TRACE") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{cmd}");
        }
    }
    let session = state.session.clone();
    let progress = state.progress.clone();
    tauri::async_runtime::spawn_blocking(move || match api::handle(&session, &progress, &cmd, args)? {
        Reply::Json(v) => Ok(Response::new(serde_json::to_vec(&v).map_err(|e| e.to_string())?)),
        Reply::Bytes(b) => Ok(Response::new(b)),
    })
    .await
    .map_err(|e| e.to_string())?
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState { session: Arc::new(Mutex::new(Session::default())), progress: Arc::new(Mutex::new(ProgressState::default())) })
        .invoke_handler(tauri::generate_handler![api])
        .run(tauri::generate_context!())
        .expect("error while running Fantasy World Maker");
}
