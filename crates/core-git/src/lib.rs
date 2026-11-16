//! Git 저장소의 변경사항 조회.
//!
//! 로컬([`LocalGit`], libgit2)과 원격([`RemoteGit`], SSH로 `git` 실행)을
//! 같은 [`GitRepo`] 트레잇으로 다뤄 Diff UI가 대상을 구분하지 않아도 되게 한다.
//!
//! 비교 기준은 항상 **마지막 커밋(HEAD) ↔ 작업 폴더**다. 스테이징 여부는 구분하지 않는다.
//! 작업 폴더 쪽 파일 내용은 `core-fs`로 읽으므로 여기서는 HEAD 쪽 내용만 제공한다.

mod local;
mod porcelain;
mod remote;

pub use local::LocalGit;
pub use remote::RemoteGit;

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("Git 저장소가 아닙니다: {0}")]
    NotARepository(String),
    #[error("서버에 git이 설치되어 있지 않습니다")]
    GitNotInstalled,
    #[error("파일이 너무 큽니다 (최대 {limit_mb} MB): {path}")]
    TooLarge { path: String, limit_mb: u64 },
    #[error("원격 명령을 실행할 수 없습니다: {0}")]
    Ssh(#[from] core_ssh::SshError),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, GitError>;

/// HEAD와 비교했을 때 파일이 어떻게 달라졌는지
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Modified,
    Added,
    Deleted,
    Renamed,
    /// Git이 아직 추적하지 않는 새 파일
    Untracked,
    /// 병합 충돌
    Conflicted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GitChange {
    /// 저장소 루트 기준 경로 (구분자는 항상 `/`)
    pub path: String,
    /// 이름이 바뀐 경우 원래 경로
    pub old_path: Option<String>,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    /// 저장소 루트(작업 폴더)의 절대 경로
    pub root: String,
    /// 현재 브랜치. detached HEAD면 `None`.
    pub branch: Option<String>,
    /// 경로순으로 정렬된 변경 파일 목록
    pub changes: Vec<GitChange>,
}

#[async_trait::async_trait]
pub trait GitRepo: Send + Sync {
    /// `dir`이 속한 저장소를 찾아(상위 폴더로 올라가며) 변경사항을 조사한다.
    async fn status(&self, dir: &str) -> Result<GitStatus>;

    /// HEAD 커밋에 들어 있는 파일 내용. HEAD에 없는 파일(새 파일 등)이면 `None`.
    ///
    /// `root`는 [`GitStatus::root`], `path`는 [`GitChange::path`] 형식이다.
    async fn head_file(&self, root: &str, path: &str, max_bytes: u64) -> Result<Option<Vec<u8>>>;
}

pub(crate) fn sort_changes(changes: &mut [GitChange]) {
    changes.sort_by(|a, b| a.path.cmp(&b.path));
}
