//! SFTP 원격 파일 시스템.

use crate::{
    sort_entries, too_large, validate_name, DirEntry, DirListing, EntryKind, FileSystem, FsError,
    Result,
};
use core_ssh::Connection;
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, OpenFlags, StatusCode};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

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

/// POSIX 경로에 이름 하나를 잇는다.
fn join(dir: &str, name: &str) -> String {
    format!("{}/{name}", dir.trim_end_matches('/'))
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

    async fn read_file(&self, path: &str, max_bytes: u64) -> Result<Vec<u8>> {
        let metadata = self
            .session
            .metadata(path)
            .await
            .map_err(|e| self.map_err(e, path))?;
        if metadata.is_dir() {
            return Err(FsError::Io(format!("폴더입니다: {path}")));
        }
        let size = metadata.size.unwrap_or(0);
        if size > max_bytes {
            return Err(too_large(path, size, max_bytes));
        }
        self.session
            .read(path)
            .await
            .map_err(|e| self.map_err(e, path))
    }

    async fn write_file(&self, path: &str, data: &[u8]) -> Result<()> {
        let flags = OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE;
        let mut file = self
            .session
            .open_with_flags(path, flags)
            .await
            .map_err(|e| self.map_err(e, path))?;
        let io = |e: std::io::Error| {
            if self.is_closed() {
                FsError::Disconnected
            } else {
                FsError::Io(format!("{path}: {e}"))
            }
        };
        file.write_all(data).await.map_err(io)?;
        // 닫기까지 성공해야 서버에 모두 기록된 것이다.
        file.shutdown().await.map_err(io)?;
        Ok(())
    }

    async fn create_dir(&self, dir: &str, name: &str) -> Result<String> {
        let path = join(dir, validate_name(name)?);
        if self.session.symlink_metadata(path.as_str()).await.is_ok() {
            return Err(FsError::AlreadyExists(path));
        }
        self.session
            .create_dir(path.as_str())
            .await
            .map_err(|e| self.map_err(e, &path))?;
        Ok(path)
    }

    async fn rename(&self, path: &str, new_name: &str) -> Result<String> {
        let name = validate_name(new_name)?;
        let to = match parent_of(path) {
            Some(parent) => join(&parent, name),
            None => return Err(FsError::InvalidName(new_name.into())),
        };
        // 서버에 따라 대상을 덮어쓸 수 있으므로 먼저 확인한다.
        if to != path && self.session.symlink_metadata(to.as_str()).await.is_ok() {
            return Err(FsError::AlreadyExists(to));
        }
        self.session
            .rename(path, to.as_str())
            .await
            .map_err(|e| self.map_err(e, path))?;
        Ok(to)
    }

    async fn remove(&self, path: &str) -> Result<()> {
        // 링크를 따라가지 않는 정보: 폴더를 가리키는 링크는 링크만 지운다.
        let metadata = self
            .session
            .symlink_metadata(path)
            .await
            .map_err(|e| self.map_err(e, path))?;
        if !metadata.is_dir() {
            return self
                .session
                .remove_file(path)
                .await
                .map_err(|e| self.map_err(e, path));
        }

        // SFTP에는 재귀 삭제가 없어서 안쪽부터 하나씩 지운다. (재귀 호출 대신 스택 사용)
        // `pending`에는 아직 내용을 읽지 않은 폴더, `emptied`에는 내용을 다 지운 뒤 지울 폴더가 쌓인다.
        let mut pending = vec![path.to_string()];
        let mut emptied = Vec::new();
        while let Some(dir) = pending.pop() {
            let entries = self
                .session
                .read_dir(dir.as_str())
                .await
                .map_err(|e| self.map_err(e, &dir))?;
            for entry in entries {
                let child = entry.path();
                // read_dir이 주는 속성은 링크 자체의 것이므로 링크는 파일로 지워진다.
                if entry.metadata().is_dir() {
                    pending.push(child);
                } else {
                    self.session
                        .remove_file(child.as_str())
                        .await
                        .map_err(|e| self.map_err(e, &child))?;
                }
            }
            emptied.push(dir);
        }
        // 나중에 발견한(더 깊은) 폴더부터 지운다.
        for dir in emptied.into_iter().rev() {
            self.session
                .remove_dir(dir.as_str())
                .await
                .map_err(|e| self.map_err(e, &dir))?;
        }
        Ok(())
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
