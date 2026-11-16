//! 파일 탐색 커맨드. 로컬과 원격(SFTP)을 `core-fs`의 같은 트레잇으로 다룬다.

use crate::error::AppResult;
use crate::ssh::Credentials;
use crate::AppState;
use core_fs::{DirListing, FileSystem, FsError, LocalFs, SftpFs};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;

/// 로컬 머신을 가리키는 가상 호스트 id (프론트엔드의 `LOCAL_ID`와 같다)
const LOCAL_ID: &str = "local";

/// 호스트별로 열어 둔 SFTP 세션
#[derive(Default)]
pub struct Files {
    sessions: tokio::sync::Mutex<HashMap<String, Arc<SftpFs>>>,
}

impl Files {
    /// 호스트의 SFTP 세션을 돌려준다. 없거나 끊겼으면 (필요하면 접속부터) 새로 연다.
    async fn remote(
        &self,
        state: &AppState,
        host_id: &str,
        credentials: Credentials,
    ) -> AppResult<Arc<SftpFs>> {
        let mut sessions = self.sessions.lock().await;
        if let Some(fs) = sessions.get(host_id) {
            if !fs.is_closed() {
                return Ok(Arc::clone(fs));
            }
        }
        let host = state.hosts.lock()?.get(host_id)?.clone();
        let connection = state.ssh.connect(&host, credentials).await?;
        let fs = Arc::new(SftpFs::open(&connection).await?);
        sessions.insert(host_id.to_string(), Arc::clone(&fs));
        Ok(fs)
    }

    /// 세션을 닫는다. 이 세션만 쓰던 연결이라면 연결도 함께 끊긴다.
    pub async fn close(&self, host_id: &str) {
        self.sessions.lock().await.remove(host_id);
    }
}

async fn list(fs: &dyn FileSystem, path: Option<&str>) -> core_fs::Result<DirListing> {
    match path {
        Some(path) => fs.list_dir(path).await,
        None => fs.list_dir(&fs.home().await?).await,
    }
}

/// 폴더 내용을 읽는다. `path`가 없으면 홈 디렉터리를 읽는다.
///
/// `host_id`가 `"local"`이면 이 머신의 디스크를, 아니면 해당 호스트를 SFTP로 읽는다.
/// 원격은 접속이 필요할 수 있어 `terminal_open`과 같은 방식으로
/// `secret` / `trusted_host_key`를 받고 같은 에러(`passwordRequired` 등)를 돌려준다.
#[tauri::command]
#[specta::specta]
pub async fn fs_list(
    state: State<'_, AppState>,
    host_id: String,
    path: Option<String>,
    secret: Option<String>,
    trusted_host_key: Option<String>,
) -> AppResult<DirListing> {
    let path = path.as_deref();
    if host_id == LOCAL_ID {
        return Ok(list(&LocalFs, path).await?);
    }

    let credentials = Credentials {
        secret,
        trusted_host_key,
    };
    let fs = state.files.remote(&state, &host_id, credentials).await?;
    match list(fs.as_ref(), path).await {
        // 열어 둔 사이 연결이 끊겼다면 한 번만 다시 접속해 본다.
        // (비밀번호가 필요한 호스트라면 여기서 passwordRequired가 UI로 전달된다)
        Err(FsError::Disconnected) => {
            let fs = state
                .files
                .remote(&state, &host_id, Credentials::default())
                .await?;
            Ok(list(fs.as_ref(), path).await?)
        }
        other => Ok(other?),
    }
}

/// 파일 탐색기 탭을 닫을 때 호출한다. 호스트의 SFTP 세션을 정리한다.
#[tauri::command]
#[specta::specta]
pub async fn fs_close(state: State<'_, AppState>, host_id: String) -> AppResult<()> {
    state.files.close(&host_id).await;
    Ok(())
}
