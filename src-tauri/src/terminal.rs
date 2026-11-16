//! 터미널 세션 커맨드. 원격 셸은 `core-ssh`, 로컬 셸은 `core-pty`가 맡고 여기서는 IPC로 잇기만 한다.

use crate::error::{AppError, AppResult};
use crate::fs::LOCAL_ID;
use crate::ipc::RawBytes;
use crate::ssh::Credentials;
use crate::AppState;
use core_pty::{LocalShell, PtyEvent};
use core_ssh::{Shell, ShellEvent};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::State;

/// 터미널 세션 하나. 로컬과 원격을 같은 방식으로 다룬다.
enum Session {
    Remote(Shell),
    Local(LocalShell),
}

impl Session {
    fn write(&self, data: Vec<u8>) -> AppResult<()> {
        match self {
            Self::Remote(shell) => Ok(shell.write(data)?),
            Self::Local(shell) => Ok(shell.write(&data)?),
        }
    }

    fn resize(&self, cols: u16, rows: u16) -> AppResult<()> {
        match self {
            Self::Remote(shell) => Ok(shell.resize(u32::from(cols), u32::from(rows))?),
            Self::Local(shell) => Ok(shell.resize(cols, rows)?),
        }
    }
}

/// 열려 있는 터미널 세션
#[derive(Default)]
pub struct Terminals {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
}

impl Terminals {
    fn with_session<T>(
        &self,
        session_id: &str,
        f: impl FnOnce(&Session) -> AppResult<T>,
    ) -> AppResult<T> {
        let sessions = self.sessions.lock()?;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| AppError::Connection("터미널 세션이 이미 종료되었습니다".into()))?;
        f(session)
    }
}

/// 셸이 끝났을 때 프론트엔드로 보내는 알림
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExit {
    /// 셸의 종료 코드. 연결이 끊기거나 강제로 종료되어 알 수 없으면 `null`.
    pub code: Option<u32>,
}

/// 새 셸을 열고 세션 id를 돌려준다.
///
/// `host_id`가 `"local"`이면 이 머신의 기본 셸을, 아니면 해당 호스트에 접속해
/// (이미 연결되어 있으면 재사용) 원격 셸을 연다.
///
/// - `secret`: 인증 방식에 따라 비밀번호 또는 개인키 패스프레이즈.
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
    on_output: Channel<RawBytes>,
    on_exit: Channel<TerminalExit>,
) -> AppResult<String> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let exited = Arc::new(AtomicBool::new(false));
    // 로컬·원격 셸의 이벤트를 같은 방식으로 프론트엔드에 전달한다.
    let forward = {
        let sessions = Arc::clone(&state.terminals.sessions);
        let session_id = session_id.clone();
        let exited = Arc::clone(&exited);
        move |data: Option<Vec<u8>>, code: Option<u32>| match data {
            Some(data) => {
                let _ = on_output.send(RawBytes(data));
            }
            None => {
                exited.store(true, Ordering::SeqCst);
                sessions
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&session_id);
                let _ = on_exit.send(TerminalExit { code });
            }
        }
    };

    let session = if host_id == LOCAL_ID {
        Session::Local(LocalShell::spawn(cols, rows, move |event| match event {
            PtyEvent::Data(data) => forward(Some(data), None),
            PtyEvent::Exit { code } => forward(None, code),
        })?)
    } else {
        // 잠금을 await 너머로 들고 가지 않도록 필요한 값만 복사해 둔다.
        let host = state.hosts.lock()?.get(&host_id)?.clone();
        let credentials = Credentials {
            secret,
            trusted_host_key,
        };
        let connection = state.ssh.connect(&host, credentials).await?;
        let on_event = move |event| match event {
            ShellEvent::Data(data) => forward(Some(data), None),
            ShellEvent::Exit { code } => forward(None, code),
        };
        Session::Remote(
            connection
                .open_shell(u32::from(cols), u32::from(rows), on_event)
                .await?,
        )
    };

    let mut sessions = state.terminals.sessions.lock()?;
    sessions.insert(session_id.clone(), session);
    // 등록하기도 전에 셸이 끝났다면 종료 콜백이 지울 기회를 놓쳤으므로 여기서 지운다.
    if exited.load(Ordering::SeqCst) {
        sessions.remove(&session_id);
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
        .with_session(&session_id, |session| session.write(data.into_bytes()))
}

/// 터미널 크기가 바뀌었음을 셸 쪽 PTY에 알린다.
#[tauri::command]
#[specta::specta]
pub fn terminal_resize(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> AppResult<()> {
    state
        .terminals
        .with_session(&session_id, |session| session.resize(cols, rows))
}

/// 셸을 닫는다. 이미 끝난 세션이면 아무 일도 하지 않는다.
#[tauri::command]
#[specta::specta]
pub fn terminal_close(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    // 핸들을 버리면 원격은 채널이 닫히고, 로컬은 셸 프로세스가 종료된다.
    state.terminals.sessions.lock()?.remove(&session_id);
    Ok(())
}
