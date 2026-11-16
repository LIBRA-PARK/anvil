//! 파일 커맨드. 로컬과 원격(SFTP)을 `core-fs`의 같은 트레잇으로 다룬다.

use crate::error::AppResult;
use crate::ipc::RawBytes;
use crate::ssh::Credentials;
use crate::AppState;
use core_fs::{DirListing, FileSystem, FsError, LocalFs, SftpFs};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tauri::State;

/// 로컬 머신을 가리키는 가상 호스트 id (프론트엔드의 `LOCAL_ID`와 같다)
pub const LOCAL_ID: &str = "local";

/// 에디터로 열 수 있는 최대 파일 크기
const MAX_EDIT_BYTES: u64 = 10 * 1024 * 1024;

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
    ///
    /// 호스트를 삭제할 때만 부른다. 그 밖에는 사이드바의 탐색기가 선택했던 호스트의
    /// 세션을 계속 쓰므로, 앱이 떠 있는 동안 세션을 유지한다.
    pub async fn close(&self, host_id: &str) {
        self.sessions.lock().await.remove(host_id);
    }
}

/// 호스트에 맞는 파일 시스템을 골라 `op`를 실행한다.
///
/// `host_id`가 [`LOCAL_ID`]면 이 머신의 디스크, 아니면 해당 호스트의 SFTP다.
/// 원격 세션이 그사이 끊겼다면 한 번만 다시 접속해 본다.
/// (비밀번호가 필요한 호스트라면 그때 `passwordRequired`가 UI로 전달된다)
pub async fn with_fs<T, Fut>(
    state: &AppState,
    host_id: &str,
    credentials: Credentials,
    op: impl Fn(Arc<dyn FileSystem>) -> Fut,
) -> AppResult<T>
where
    Fut: Future<Output = core_fs::Result<T>>,
{
    if host_id == LOCAL_ID {
        return Ok(op(Arc::new(LocalFs)).await?);
    }
    let fs = state.files.remote(state, host_id, credentials).await?;
    match op(fs).await {
        Err(FsError::Disconnected) => {
            let fs = state
                .files
                .remote(state, host_id, Credentials::default())
                .await?;
            Ok(op(fs).await?)
        }
        other => Ok(other?),
    }
}

/// 폴더 내용을 읽는다. `path`가 없으면 홈 디렉터리를 읽는다.
///
/// 원격은 접속이 필요할 수 있어 `terminal_open`과 같은 방식으로
/// `secret` / `trusted_host_key`를 받고 같은 에러(`passwordRequired` 등)를 돌려준다.
/// 아래 파일 커맨드들도 모두 같다.
#[tauri::command]
#[specta::specta]
pub async fn fs_list(
    state: State<'_, AppState>,
    host_id: String,
    path: Option<String>,
    secret: Option<String>,
    trusted_host_key: Option<String>,
) -> AppResult<DirListing> {
    let credentials = Credentials {
        secret,
        trusted_host_key,
    };
    with_fs(&state, &host_id, credentials, |fs| {
        let path = path.clone();
        async move {
            match path {
                Some(path) => fs.list_dir(&path).await,
                None => fs.list_dir(&fs.home().await?).await,
            }
        }
    })
    .await
}

/// 파일 내용을 읽는다. JSON이 아니라 바이너리(`ArrayBuffer`)로 전달된다.
///
/// 에디터용이므로 10MB를 넘는 파일은 `invalidInput` 에러로 거부한다.
#[tauri::command]
#[specta::specta]
pub async fn fs_read(
    state: State<'_, AppState>,
    host_id: String,
    path: String,
    secret: Option<String>,
    trusted_host_key: Option<String>,
) -> AppResult<RawBytes> {
    let credentials = Credentials {
        secret,
        trusted_host_key,
    };
    let bytes = with_fs(&state, &host_id, credentials, |fs| {
        let path = path.clone();
        async move { fs.read_file(&path, MAX_EDIT_BYTES).await }
    })
    .await?;
    Ok(RawBytes(bytes))
}

/// 파일 내용을 통째로 바꿔 쓴다(UTF-8). 원격이면 SFTP로 업로드한다.
#[tauri::command]
#[specta::specta]
pub async fn fs_write(
    state: State<'_, AppState>,
    host_id: String,
    path: String,
    content: String,
    secret: Option<String>,
    trusted_host_key: Option<String>,
) -> AppResult<()> {
    let credentials = Credentials {
        secret,
        trusted_host_key,
    };
    let content = Arc::new(content);
    with_fs(&state, &host_id, credentials, |fs| {
        let path = path.clone();
        let content = Arc::clone(&content);
        async move { fs.write_file(&path, content.as_bytes()).await }
    })
    .await
}

/// `dir` 안에 새 폴더를 만들고 그 경로를 돌려준다.
#[tauri::command]
#[specta::specta]
pub async fn fs_create_dir(
    state: State<'_, AppState>,
    host_id: String,
    dir: String,
    name: String,
) -> AppResult<String> {
    with_fs(&state, &host_id, Credentials::default(), |fs| {
        let (dir, name) = (dir.clone(), name.clone());
        async move { fs.create_dir(&dir, &name).await }
    })
    .await
}

/// 같은 폴더 안에서 이름을 바꾸고 새 경로를 돌려준다.
#[tauri::command]
#[specta::specta]
pub async fn fs_rename(
    state: State<'_, AppState>,
    host_id: String,
    path: String,
    new_name: String,
) -> AppResult<String> {
    with_fs(&state, &host_id, Credentials::default(), |fs| {
        let (path, new_name) = (path.clone(), new_name.clone());
        async move { fs.rename(&path, &new_name).await }
    })
    .await
}

/// 파일이나 폴더(내용 포함)를 지운다. 되돌릴 수 없으므로 UI에서 먼저 확인받는다.
#[tauri::command]
#[specta::specta]
pub async fn fs_remove(state: State<'_, AppState>, host_id: String, path: String) -> AppResult<()> {
    with_fs(&state, &host_id, Credentials::default(), |fs| {
        let path = path.clone();
        async move { fs.remove(&path).await }
    })
    .await
}
