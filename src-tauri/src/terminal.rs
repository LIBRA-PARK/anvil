//! 터미널 세션 커맨드. SSH 로직은 `core-ssh`에 있고 여기서는 IPC로 잇기만 한다.

use crate::error::{AppError, AppResult};
use crate::AppState;
use core_config::{AuthMethod, Host};
use core_ssh::{Auth, ConnectParams, HostKeyPolicy, Shell, ShellEvent, SshPool};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::{Channel, InvokeResponseBody, IpcResponse};
use tauri::State;

/// 열려 있는 터미널 세션과 SSH 연결 풀
pub struct Terminals {
    pool: SshPool,
    shells: Arc<Mutex<HashMap<String, Shell>>>,
    /// Anvil이 새로 신뢰한 호스트 키를 기록하는 파일
    known_hosts: PathBuf,
    /// 읽기만 하는 known_hosts (`~/.ssh/known_hosts`)
    extra_known_hosts: Vec<PathBuf>,
}

impl Terminals {
    pub fn new(known_hosts: PathBuf, extra_known_hosts: Vec<PathBuf>) -> Self {
        Self {
            pool: SshPool::new(),
            shells: Arc::default(),
            known_hosts,
            extra_known_hosts,
        }
    }

    /// 호스트가 삭제되면 연결 풀에서도 잊는다.
    pub fn forget_host(&self, host_id: &str) {
        self.pool.forget(host_id);
    }

    fn with_shell<T>(
        &self,
        session_id: &str,
        f: impl FnOnce(&Shell) -> core_ssh::Result<T>,
    ) -> AppResult<T> {
        let shells = self.shells.lock()?;
        let shell = shells
            .get(session_id)
            .ok_or_else(|| AppError::Connection("터미널 세션이 이미 종료되었습니다".into()))?;
        Ok(f(shell)?)
    }
}

/// 셸이 끝났을 때 프론트엔드로 보내는 알림
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExit {
    /// 원격 셸의 종료 코드. 연결이 끊겨서 끝났다면 `null`.
    pub code: Option<u32>,
}

/// 셸 출력 바이트.
///
/// JSON 숫자 배열로 직렬화되는 것을 피하려고 raw 응답으로 보낸다.
/// 프론트엔드에서는 `ArrayBuffer`로 받는다.
pub struct TerminalOutput(Vec<u8>);

impl IpcResponse for TerminalOutput {
    fn body(self) -> tauri::Result<InvokeResponseBody> {
        Ok(InvokeResponseBody::Raw(self.0))
    }
}

impl specta::Type for TerminalOutput {
    fn definition(_: &mut specta::Types) -> specta::datatype::DataType {
        specta::datatype::DataType::Reference(specta_typescript::define("ArrayBuffer"))
    }
}

fn connect_params(host: &Host, secret: Option<String>) -> ConnectParams {
    // 빈 문자열은 입력하지 않은 것으로 본다.
    let secret = secret.filter(|s| !s.is_empty());
    ConnectParams {
        address: host.address.clone(),
        port: host.port,
        username: host.username.clone(),
        auth: match &host.auth {
            AuthMethod::Password => Auth::Password(secret),
            AuthMethod::Key { path } => Auth::Key {
                path: path.clone(),
                passphrase: secret,
            },
            AuthMethod::Agent => Auth::Agent,
        },
    }
}

/// 호스트에 접속해(이미 연결되어 있으면 재사용) 새 셸을 연다. 세션 id를 돌려준다.
///
/// - `secret`: 인증 방식에 따라 비밀번호 또는 개인키 패스프레이즈. 저장하지 않는다.
/// - `trusted_host_key`: 사용자가 확인한 호스트 키 지문 (`hostKeyUnknown` 에러의 `message`).
/// - `on_output`: 셸 출력. JSON이 아니라 바이너리(`ArrayBuffer`)로 전달된다.
/// - `on_exit`: 셸이 끝나면 한 번 호출된다.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn terminal_open(
    state: State<'_, AppState>,
    host_id: String,
    cols: u16,
    rows: u16,
    secret: Option<String>,
    trusted_host_key: Option<String>,
    on_output: Channel<TerminalOutput>,
    on_exit: Channel<TerminalExit>,
) -> AppResult<String> {
    // 잠금을 await 너머로 들고 가지 않도록 필요한 값만 복사해 둔다.
    let host = state.hosts.lock()?.get(&host_id)?.clone();
    let terminals = &state.terminals;
    let policy = HostKeyPolicy {
        known_hosts: terminals.known_hosts.clone(),
        extra_known_hosts: terminals.extra_known_hosts.clone(),
        trusted_fingerprint: trusted_host_key,
    };

    let connection = terminals
        .pool
        .get_or_connect(&host_id, &connect_params(&host, secret), &policy)
        .await?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let exited = Arc::new(AtomicBool::new(false));
    let on_event = {
        let shells = Arc::clone(&terminals.shells);
        let session_id = session_id.clone();
        let exited = Arc::clone(&exited);
        move |event| match event {
            ShellEvent::Data(data) => {
                let _ = on_output.send(TerminalOutput(data));
            }
            ShellEvent::Exit { code } => {
                exited.store(true, Ordering::SeqCst);
                shells
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&session_id);
                let _ = on_exit.send(TerminalExit { code });
            }
        }
    };
    let shell = connection
        .open_shell(u32::from(cols), u32::from(rows), on_event)
        .await?;

    let mut shells = terminals.shells.lock()?;
    shells.insert(session_id.clone(), shell);
    // 등록하기도 전에 셸이 끝났다면 종료 콜백이 지울 기회를 놓쳤으므로 여기서 지운다.
    if exited.load(Ordering::SeqCst) {
        shells.remove(&session_id);
    }
    Ok(session_id)
}

/// 키 입력을 셸로 보낸다.
#[tauri::command]
#[specta::specta]
pub fn terminal_write(
    state: State<'_, AppState>,
    session_id: String,
    data: String,
) -> AppResult<()> {
    state
        .terminals
        .with_shell(&session_id, |shell| shell.write(data.into_bytes()))
}

/// 터미널 크기가 바뀌었음을 원격 PTY에 알린다.
#[tauri::command]
#[specta::specta]
pub fn terminal_resize(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> AppResult<()> {
    state.terminals.with_shell(&session_id, |shell| {
        shell.resize(u32::from(cols), u32::from(rows))
    })
}

/// 셸을 닫는다. 이미 끝난 세션이면 아무 일도 하지 않는다.
#[tauri::command]
#[specta::specta]
pub fn terminal_close(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    // 핸들을 버리면 core-ssh가 원격 채널을 닫는다.
    state.terminals.shells.lock()?.remove(&session_id);
    Ok(())
}
