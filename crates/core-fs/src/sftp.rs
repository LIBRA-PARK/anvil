//! SFTP 원격 파일 시스템.

use crate::{sort_entries, DirEntry, DirListing, EntryKind, FileSystem, FsError, Result};
use core_ssh::Connection;
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, StatusCode};
use std::sync::Arc;

/// 이미 인증된 SSH 연결 위에 연 SFTP 세션.
///
/// 세션이 살아 있는 동안 연결도 유지된다.
pub struct SftpFs {
    session: SftpSession,
    connection: Arc<Connection>,
}

impl SftpFs {
    /// 연결에 새 채널을 열어 SFTP 서브시스템을 시작한다.
    pub async fn open(connection: &Arc<Connection>) -> Result<Self> {
        let stream = connection.open_subsystem("sftp").await?;
        let connection = stream.connection();
        let session = SftpSession::new(stream)
            .await
            .map_err(|e| FsError::Io(format!("SFTP를 시작할 수 없습니다: {e}")))?;
        Ok(Self {
            session,
            connection,
        })
    }

    /// 아래 연결이 끊겼는지. 끊겼다면 이 세션은 버리고 다시 열어야 한다.
    pub fn is_closed(&self) -> bool {
        self.connection.is_closed()
    }

    fn map_err(&self, e: SftpError, path: &str) -> FsError {
        if self.is_closed() {
            return FsError::Disconnected;
        }
        match e {
            SftpError::Status(status) => match status.status_code {
                StatusCode::NoSuchFile => FsError::NotFound(path.into()),
                StatusCode::PermissionDenied => FsError::PermissionDenied(path.into()),
                _ => FsError::Io(format!("{path}: {}", status.error_message)),
            },
            SftpError::Timeout => FsError::Io(format!("{path}: 응답 시간 초과")),
            other => FsError::Io(format!("{path}: {other}")),
        }
    }
}

fn kind_of(attrs: &FileAttributes) -> EntryKind {
    if attrs.is_dir() {
        EntryKind::Dir
    } else if attrs.is_regular() {
        EntryKind::File
    } else {
        EntryKind::Other
    }
}

/// POSIX 경로의 상위 폴더. 루트면 `None`.
fn parent_of(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.rfind('/') {
        Some(0) => Some("/".into()),
        Some(index) => Some(trimmed[..index].into()),
        None => None,
    }
}

#[async_trait::async_trait]
impl FileSystem for SftpFs {
    async fn home(&self) -> Result<String> {
        // SFTP 세션의 시작 폴더가 로그인 사용자의 홈이다.
        self.session
            .canonicalize(".")
            .await
            .map_err(|e| self.map_err(e, "."))
    }

    async fn list_dir(&self, path: &str) -> Result<DirListing> {
        let dir = self
            .session
            .canonicalize(path)
            .await
            .map_err(|e| self.map_err(e, path))?;
        let read = match self.session.read_dir(dir.as_str()).await {
            Ok(read) => read,
            Err(e) => {
                // 서버는 파일을 폴더로 열려 해도 "No such file"로 답하므로 직접 구분해 준다.
                let is_file =
                    matches!(self.session.metadata(dir.as_str()).await, Ok(m) if !m.is_dir());
                return Err(if is_file {
                    FsError::NotADirectory(dir)
                } else {
                    self.map_err(e, &dir)
                });
            }
        };

        let mut entries = Vec::new();
        for item in read {
            let path = item.path();
            let own = item.metadata();
            let is_symlink = own.is_symlink();
            // 링크는 가리키는 대상의 정보로 보여 준다. 대상이 없으면 Other.
            let target = if is_symlink {
                self.session.metadata(path.as_str()).await.ok()
            } else {
                Some(own)
            };
            entries.push(DirEntry {
                name: item.file_name(),
                path,
                kind: target.as_ref().map(kind_of).unwrap_or(EntryKind::Other),
                is_symlink,
                size: target.as_ref().and_then(|m| m.size).unwrap_or(0),
                modified: target.and_then(|m| m.mtime).map(u64::from),
            });
        }
        sort_entries(&mut entries);

        Ok(DirListing {
            parent: parent_of(&dir),
            path: dir,
            entries,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::parent_of;

    #[test]
    fn 상위_폴더_계산() {
        assert_eq!(parent_of("/"), None);
        assert_eq!(parent_of("/home"), Some("/".into()));
        assert_eq!(parent_of("/home/user"), Some("/home".into()));
        assert_eq!(parent_of("/home/user/"), Some("/home".into()));
    }
}
