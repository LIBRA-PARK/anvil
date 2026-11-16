use core_config::{Host, HostStore, NewHost};
use std::sync::Mutex;
use tauri::{Manager, State};

/// 앱 전역 상태
struct AppState {
    hosts: Mutex<HostStore>,
}

/// 프론트엔드로 넘길 때 에러는 문자열로 변환한다.
type CmdResult<T> = Result<T, String>;

#[tauri::command]
fn list_hosts(state: State<'_, AppState>) -> CmdResult<Vec<Host>> {
    let store = state.hosts.lock().map_err(|e| e.to_string())?;
    Ok(store.list().to_vec())
}

#[tauri::command]
fn add_host(state: State<'_, AppState>, input: NewHost) -> CmdResult<Host> {
    let mut store = state.hosts.lock().map_err(|e| e.to_string())?;
    store.add(input).map_err(|e| e.to_string())
}

#[tauri::command]
fn remove_host(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let mut store = state.hosts.lock().map_err(|e| e.to_string())?;
    store.remove(&id).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // OS별 설정 경로 (예: macOS ~/Library/Application Support/dev.anvil.app)
            let path = app.path().app_config_dir()?.join("hosts.json");
            let store = HostStore::load(path)?;
            app.manage(AppState { hosts: Mutex::new(store) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_hosts, add_host, remove_host])
        .run(tauri::generate_context!())
        .expect("Anvil 실행 중 오류 발생");
}
