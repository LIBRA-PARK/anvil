//! Git 변경사항 커맨드. 로컬(libgit2)과 원격(SSH로 git 실행)을 `core-git`의 같은 트레잇으로 다룬다.

use crate::error::AppResult;
use crate::fs::{with_fs, LOCAL_ID};
use crate::ssh::Credentials;
use crate::AppState;
use core_fs::FsError;
use core_git::{GitRepo, GitStatus, LocalGit, RemoteGit};
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;
use tauri::State;

/// Diff로 보여 줄 수 있는 최대 파일 크기 (한쪽 기준)
const MAX_DIFF_BYTES: u64 = 5 * 1024 * 1024;

async fn repo(
    state: &AppState,
    host_id: &str,
    credentials: Credentials,
) -> AppResult<Arc<dyn GitRepo>> {
    if host_id == LOCAL_ID {
        return Ok(Arc::new(LocalGit));
    }
    let host = state.hosts.lock()?.get(host_id)?.clone();
    let connection = state.ssh.connect(&host, credentials).await?;
    Ok(Arc::new(RemoteGit::new(connection)))
}

/// `path`가 속한 저장소의 변경 파일 목록을 조사한다 (마지막 커밋 ↔ 작업 폴더).
///
/// 원격은 접속이 필요할 수 있어 다른 커맨드처럼 `secret` / `trusted_host_key`를 받는다.
#[tauri::command]
#[specta::specta]
pub async fn git_status(
    state: State<'_, AppState>,
    host_id: String,
    path: String,
    secret: Option<String>,
    trusted_host_key: Option<String>,
) -> AppResult<GitStatus> {
    let credentials = Credentials {
        secret,
        trusted_host_key,
    };
    let repo = repo(&state, &host_id, credentials).await?;
    Ok(repo.status(&path).await?)
}

/// 파일 하나의 변경 전후 내용
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    /// 마지막 커밋의 내용. 새 파일이면 `null`.
    pub original: Option<String>,
    /// 작업 폴더의 현재 내용. 삭제된 파일이면 `null`.
    pub modified: Option<String>,
    /// 텍스트가 아니어서 내용을 보여 줄 수 없음 (이때 두 내용은 모두 `null`)
    pub binary: bool,
}

/// 변경된 파일 하나의 전후 내용을 읽는다.
///
/// `root`와 `path`는 `git_status`가 돌려준 값을 그대로 넘긴다.
/// 이름이 바뀐 파일은 `old_path`의 내용이 변경 전 내용이 된다.
#[tauri::command]
#[specta::specta]
pub async fn git_file_diff(
    state: State<'_, AppState>,
    host_id: String,
    root: String,
    path: String,
    old_path: Option<String>,
    secret: Option<String>,
    trusted_host_key: Option<String>,
) -> AppResult<FileDiff> {
    let credentials = Credentials {
        secret,
        trusted_host_key,
    };
    let repo = repo(&state, &host_id, credentials).await?;
    let head_path = old_path.as_deref().unwrap_or(&path);
    let original = repo.head_file(&root, head_path, MAX_DIFF_BYTES).await?;

    // 작업 폴더 쪽은 파일 시스템에서 읽는다. 로컬은 OS 경로 규칙으로, 원격은 POSIX로 잇는다.
    let full_path = if host_id == LOCAL_ID {
        Path::new(&root).join(&path).to_string_lossy().into_owned()
    } else {
        format!("{}/{path}", root.trim_end_matches('/'))
    };
    // 위에서 접속이 끝났으므로 여기서는 인증 정보가 다시 필요하지 않다.
    let modified = with_fs(&state, &host_id, Credentials::default(), |fs| {
        let full_path = full_path.clone();
        async move {
            match fs.read_file(&full_path, MAX_DIFF_BYTES).await {
                Ok(bytes) => Ok(Some(bytes)),
                // 삭제된 파일
                Err(FsError::NotFound(_)) => Ok(None),
                Err(e) => Err(e),
            }
        }
    })
    .await?;

    let text = |bytes: Option<Vec<u8>>| match bytes {
        None => Ok(None),
        Some(bytes) if bytes.contains(&0) => Err(()),
        Some(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| ()),
    };
    Ok(match (text(original), text(modified)) {
        (Ok(original), Ok(modified)) => FileDiff {
            original,
            modified,
            binary: false,
        },
        _ => FileDiff {
            original: None,
            modified: None,
            binary: true,
        },
    })
}
