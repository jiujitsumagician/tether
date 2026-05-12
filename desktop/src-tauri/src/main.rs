// Prevent a console window flashing on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use tether_core::{
    discovery::CascadeOptions,
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

/// Spawn an elevated PowerShell that adds the Windows Firewall rules
/// for the TLS listener (TCP/31415) + UDP discovery (31413). The
/// user gets a UAC prompt — they click Yes — done forever.
///
/// No-op on non-Windows targets.
#[tauri::command]
async fn fix_firewall_windows() -> Result<(), String> {
    #[cfg(windows)]
    {
        // We chain the two New-NetFirewallRule calls + an
        // Invoke-Expression so a single UAC prompt covers both
        // rules. -ErrorAction SilentlyContinue swallows the
        // "rule already exists" case so the user can click
        // "Allow firewall" twice without seeing an error.
        let cmd = "Start-Process powershell -Verb RunAs -WindowStyle Hidden -ArgumentList '-NoProfile','-Command',\"New-NetFirewallRule -DisplayName 'Tether (TCP listener)' -Direction Inbound -Protocol TCP -LocalPort 31415 -Action Allow -Profile Any -ErrorAction SilentlyContinue; New-NetFirewallRule -DisplayName 'Tether (UDP discovery)' -Direction Inbound -Protocol UDP -LocalPort 31413 -Action Allow -Profile Any -ErrorAction SilentlyContinue\"";
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", cmd])
            .status()
            .map_err(|e| format!("failed to spawn elevated PowerShell: {e}"))?;
        if !status.success() {
            return Err(format!("elevated PowerShell exited {status}"));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err("firewall fix is Windows-only".into())
    }
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
            fix_firewall_windows,
        ])
        .run(tauri::generate_context!())
        .expect("Tether desktop failed to launch");
}

