// Prevent a console window flashing on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use tether_core::{
    discovery::{run_cascade, CascadeEvent, CascadeOptions},
    pairing::state::{PairingState, PairingUiEvent},
    store::Store,
};
use tokio::sync::{mpsc, Mutex};

/// Tauri-managed shared state. Wraps the pairing state machine so all
/// commands talk to the same instance.
struct AppState {
    pairing: Mutex<Arc<PairingState>>,
    store: Mutex<Arc<Store>>,
}

#[tauri::command]
async fn start_pairing(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<(), String> {
    let pairing = state.pairing.lock().await.clone();
    let (tx, mut rx) = mpsc::channel::<PairingUiEvent>(32);

    // UI bridge: forward every state-machine event to the frontend
    // as a `pairing` event. The frontend listens with a single
    // listen("pairing", ...) call and renders accordingly.
    tokio::spawn(async move {
        while let Some(evt) = rx.recv().await {
            if let Err(e) = app.emit("pairing", &evt) {
                tracing::warn!("emit pairing event failed: {e}");
            }
        }
    });

    pairing.start(tx).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn confirm(state: State<'_, AppState>) -> Result<(), String> {
    let pairing = state.pairing.lock().await.clone();
    pairing.user_confirm().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn mismatch(state: State<'_, AppState>) -> Result<(), String> {
    let pairing = state.pairing.lock().await.clone();
    pairing.user_mismatch().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn open_manual_entry(state: State<'_, AppState>) -> Result<(), String> {
    let pairing = state.pairing.lock().await.clone();
    pairing.open_manual_entry().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn submit_manual(
    state: State<'_, AppState>,
    address: String,
    pin: String,
) -> Result<(), String> {
    let pairing = state.pairing.lock().await.clone();
    pairing.submit_manual(address, pin).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn reset_pairing(state: State<'_, AppState>) -> Result<(), String> {
    let pairing = state.pairing.lock().await.clone();
    pairing.reset().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_paired(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let store = state.store.lock().await.clone();
    let pairs = store.list_paired().await.map_err(|e| e.to_string())?;
    serde_json::to_value(pairs).map_err(|e| e.to_string())
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tether_core=info,tether_desktop=info".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let store = tokio::runtime::Handle::try_current()
                .map(|h| h.block_on(Store::open_default()))
                .unwrap_or_else(|_| {
                    tauri::async_runtime::block_on(Store::open_default())
                })
                .expect("failed to open store");
            let store = Arc::new(store);

            let options = CascadeOptions::default();
            let _ = handle; // reserved for future use
            let pairing = Arc::new(PairingState::new(
                Arc::clone(&store),
                options,
            ));

            app.manage(AppState {
                pairing: Mutex::new(pairing),
                store: Mutex::new(store),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_pairing,
            confirm,
            mismatch,
            open_manual_entry,
            submit_manual,
            reset_pairing,
            list_paired,
        ])
        .run(tauri::generate_context!())
        .expect("Tether desktop failed to launch");
}

// Wire CascadeEvent through to the Tauri event channel for completeness
// (used by integration tests that drive run_cascade directly without
// the pairing state machine wrapper).
#[allow(dead_code)]
fn forward_cascade_event(app: &tauri::AppHandle, evt: CascadeEvent) {
    let _ = app.emit("cascade", &evt);
}
