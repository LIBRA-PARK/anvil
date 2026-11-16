//! 파일 시스템 추상화.
//!
//! 로컬 디스크([`LocalFs`])와 SFTP 원격([`SftpFs`])을 같은 [`FileSystem`] 트레잇으로 다뤄,
//! 파일 트리·에디터 UI가 대상이 로컬인지 원격인지 몰라도 되게 한다.
//!
//! 경로는 모두 문자열로 주고받는다. 로컬은 OS 고유 형식, 원격은 POSIX 형식이며,
//! UI가 경로를 직접 조립하지 않도록 상위 폴더와 각 항목의 전체 경로를 함께 돌려준다.

mod local;
mod sftp;

pub use local::LocalFs;
pub use sftp::SftpFs;

use serde::Serialize;
use specta_typescript::Number;

#[derive(Debug, thiserror::Error)]
pub enum FsError {
    #[error("경로를 찾을 수 없습니다: {0}")]
    NotFound(String),
    #[error("접근 권한이 없습니다: {0}")]
    PermissionDenied(String),
    #[error("폴더가 아닙니다: {0}")]
    NotADirectory(String),
    /// 원격 연결이 끊김. 다시 접속하면 해결될 수 있다.
    #[error("원격 연결이 끊겼습니다")]
    Disconnected,
    #[error("SFTP를 시작할 수 없습니다: {0}")]
    Ssh(#[from] core_ssh::SshError),
    #[error("{0}")]
    Io(String),
}

pub type Result<T> = std::result::Result<T, FsError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    File,
    Dir,
    /// 소켓, 장치 파일, 대상이 없는 심볼릭 링크 등
    Other,
}

/// 폴더 안의 항목 하나
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    /// 이 항목의 전체 경로. 하위 폴더를 열 때 그대로 `list_dir`에 넘긴다.
    pub path: String,
    /// 심볼릭 링크라면 링크가 가리키는 대상의 종류
    pub kind: EntryKind,
    pub is_symlink: bool,
    /// 바이트 단위 크기 (폴더는 의미 없음)
    #[specta(type = Number)]
    pub size: u64,
    /// 마지막 수정 시각 (유닉스 초)
    #[specta(type = Option<Number>)]
    pub modified: Option<u64>,
}

/// 폴더 하나를 읽은 결과
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    /// 정규화된 절대 경로
    pub path: String,
    /// 상위 폴더. 최상위(루트)면 `None`.
    pub parent: Option<String>,
    /// 폴더 먼저, 그다음 이름순(대소문자 무시)으로 정렬되어 있다.
    pub entries: Vec<DirEntry>,
}

#[async_trait::async_trait]
pub trait FileSystem: Send + Sync {
    /// 처음 보여 줄 폴더 (홈 디렉터리)
    async fn home(&self) -> Result<String>;

    /// 폴더 내용을 읽는다.
    async fn list_dir(&self, path: &str) -> Result<DirListing>;
}

/// 폴더 먼저, 그다음 이름순으로 정렬한다.
pub(crate) fn sort_entries(entries: &mut [DirEntry]) {
    entries.sort_by_cached_key(|e| {
        (
            e.kind != EntryKind::Dir,
            e.name.to_lowercase(),
            e.name.clone(),
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, kind: EntryKind) -> DirEntry {
        DirEntry {
            name: name.into(),
            path: name.into(),
            kind,
            is_symlink: false,
            size: 0,
            modified: None,
        }
    }

    #[test]
    fn 폴더가_먼저_오고_대소문자를_무시해_정렬한다() {
        let mut entries = vec![
            entry("b.txt", EntryKind::File),
            entry("Zeta", EntryKind::Dir),
            entry("A.txt", EntryKind::File),
            entry("alpha", EntryKind::Dir),
            entry(".hidden", EntryKind::File),
        ];
        sort_entries(&mut entries);
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["alpha", "Zeta", ".hidden", "A.txt", "b.txt"]);
    }
}
