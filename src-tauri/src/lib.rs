mod error;
mod terminal;

use core_config::{Host, HostStore, NewHost};
use error::AppResult;
use std::sync::Mutex;
use tauri::{Manager, State};
use tauri_specta::{collect_commands, Builder};
use terminal::Terminals;

pub use error::AppError;

/// 생성되는 TS 바인딩 파일 경로
const BINDINGS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../ui/src/bindings.ts");

/// 앱 전역 상태
struct AppState {
    hosts: Mutex<HostStore>,
    terminals: Terminals,
}

#[tauri::command]
#[specta::specta]
fn list_hosts(state: State<'_, AppState>) -> AppResult<Vec<Host>> {
    Ok(state.hosts.lock()?.list().to_vec())
}

#[tauri::command]
#[specta::specta]
fn add_host(state: State<'_, AppState>, input: NewHost) -> AppResult<Host> {
    Ok(state.hosts.lock()?.add(input)?)
}

#[tauri::command]
#[specta::specta]
fn remove_host(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.hosts.lock()?.remove(&id)?;
    state.terminals.forget_host(&id);
    Ok(())
}

/// 커맨드 등록과 TS 바인딩 생성을 한곳에서 관리한다.
/// 새 커맨드는 반드시 여기에 추가해야 프론트엔드에서 보인다.
fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            list_hosts,
            add_host,
            remove_host,
            terminal::terminal_open,
            terminal::terminal_write,
            terminal::terminal_resize,
            terminal::terminal_close,
        ])
        .typ::<AppError>()
}

/// `ui/src/bindings.ts`를 생성한다. (`tests/bindings.rs`와 개발 실행 시 호출)
#[doc(hidden)]
pub fn export_bindings() {
    specta_builder()
        .export(
            specta_typescript::Typescript::default()
                .header("// 자동 생성 파일입니다. 직접 수정하지 마세요.\n"),
            BINDINGS_PATH,
        )
        .expect("TS 바인딩 생성 실패");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 개발 실행 시마다 바인딩을 최신으로 갱신
    #[cfg(debug_assertions)]
    export_bindings();

    let builder = specta_builder();

    tauri::Builder::default()
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            // OS별 설정 경로 (예: macOS ~/Library/Application Support/dev.anvil.app)
            let config_dir = app.path().app_config_dir()?;
            let store = HostStore::load(config_dir.join("hosts.json"))?;

            // 새로 신뢰한 호스트 키는 앱 설정 폴더에만 기록하고,
            // 사용자의 ~/.ssh/known_hosts는 읽기만 한다.
            let user_known_hosts = app
                .path()
                .home_dir()
                .map(|home| home.join(".ssh").join("known_hosts"))
                .ok();
            let terminals = Terminals::new(
                config_dir.join("known_hosts"),
                user_known_hosts.into_iter().collect(),
            );

            app.manage(AppState {
                hosts: Mutex::new(store),
                terminals,
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Anvil 실행 중 오류 발생");
}
