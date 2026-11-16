//! 로컬 디스크.

use crate::{sort_entries, DirEntry, DirListing, EntryKind, FileSystem, FsError, Result};
use std::path::Path;
use std::time::UNIX_EPOCH;

#[derive(Debug, Default, Clone, Copy)]
pub struct LocalFs;

fn map_io(e: std::io::Error, path: &Path) -> FsError {
    let shown = path.display().to_string();
    match e.kind() {
        std::io::ErrorKind::NotFound => FsError::NotFound(shown),
        std::io::ErrorKind::PermissionDenied => FsError::PermissionDenied(shown),
        std::io::ErrorKind::NotADirectory => FsError::NotADirectory(shown),
        _ => FsError::Io(format!("{shown}: {e}")),
    }
}

fn read_dir(path: &str) -> Result<DirListing> {
    // `..` 등을 정리하되 심볼릭 링크는 풀지 않는다(사용자가 입력한 경로를 유지).
    // Windows에서 canonicalize가 붙이는 `\\?\` 접두사도 피할 수 있다.
    let dir = std::path::absolute(path).map_err(|e| map_io(e, Path::new(path)))?;

    let mut entries = Vec::new();
    for item in std::fs::read_dir(&dir).map_err(|e| map_io(e, &dir))? {
        let item = item.map_err(|e| map_io(e, &dir))?;
        let path = item.path();
        let is_symlink = item.file_type().map(|t| t.is_symlink()).unwrap_or(false);
        // 링크를 따라간 메타데이터. 대상이 없는 링크나 권한 문제는 Other로 보여 준다.
        let metadata = std::fs::metadata(&path).ok();
        let kind = match &metadata {
            Some(m) if m.is_dir() => EntryKind::Dir,
            Some(m) if m.is_file() => EntryKind::File,
            _ => EntryKind::Other,
        };
        entries.push(DirEntry {
            name: item.file_name().to_string_lossy().into_owned(),
            path: path.to_string_lossy().into_owned(),
            kind,
            is_symlink,
            size: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
            modified: metadata
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
        });
    }
    sort_entries(&mut entries);

    Ok(DirListing {
        parent: dir.parent().map(|p| p.to_string_lossy().into_owned()),
        path: dir.to_string_lossy().into_owned(),
        entries,
    })
}

#[async_trait::async_trait]
impl FileSystem for LocalFs {
    async fn home(&self) -> Result<String> {
        std::env::home_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .ok_or_else(|| FsError::Io("홈 디렉터리를 찾을 수 없습니다".into()))
    }

    async fn list_dir(&self, path: &str) -> Result<DirListing> {
        let path = path.to_string();
        // 큰 폴더나 느린 디스크가 비동기 런타임을 막지 않게 한다.
        tokio::task::spawn_blocking(move || read_dir(&path))
            .await
            .map_err(|e| FsError::Io(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("anvil-fs-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&dir).unwrap();
            // macOS의 /var → /private/var 같은 링크 때문에 비교가 어긋나지 않게 한다.
            Self(fs::canonicalize(dir).unwrap())
        }
        fn path(&self) -> &str {
            self.0.to_str().unwrap()
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }

    #[tokio::test]
    async fn 폴더_내용을_정렬해_돌려준다() {
        let dir = TempDir::new();
        fs::write(dir.0.join("b.txt"), "hello").unwrap();
        fs::write(dir.0.join("A.txt"), "").unwrap();
        fs::create_dir(dir.0.join("sub")).unwrap();

        let listing = LocalFs.list_dir(dir.path()).await.unwrap();
        assert_eq!(listing.path, dir.path());
        assert_eq!(
            listing.parent.as_deref(),
            dir.0.parent().and_then(|p| p.to_str())
        );

        let summary: Vec<_> = listing
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e.kind, e.size))
            .collect();
        assert_eq!(summary[0].0, "sub");
        assert_eq!(summary[0].1, EntryKind::Dir);
        assert_eq!(summary[1], ("A.txt", EntryKind::File, 0));
        assert_eq!(summary[2], ("b.txt", EntryKind::File, 5));
        assert!(listing.entries[2].modified.is_some());

        // 항목의 path를 그대로 넘겨 하위 폴더를 열 수 있어야 한다.
        let sub = LocalFs.list_dir(&listing.entries[0].path).await.unwrap();
        assert!(sub.entries.is_empty());
        assert_eq!(sub.parent.as_deref(), Some(dir.path()));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn 심볼릭_링크는_대상의_종류로_표시한다() {
        let dir = TempDir::new();
        fs::create_dir(dir.0.join("real")).unwrap();
        std::os::unix::fs::symlink(dir.0.join("real"), dir.0.join("link")).unwrap();
        std::os::unix::fs::symlink(dir.0.join("nowhere"), dir.0.join("broken")).unwrap();

        let listing = LocalFs.list_dir(dir.path()).await.unwrap();
        let find = |name: &str| listing.entries.iter().find(|e| e.name == name).unwrap();
        assert_eq!(
            (find("link").kind, find("link").is_symlink),
            (EntryKind::Dir, true)
        );
        assert_eq!(
            (find("broken").kind, find("broken").is_symlink),
            (EntryKind::Other, true)
        );
        assert!(!find("real").is_symlink);
    }

    #[tokio::test]
    async fn 없는_경로와_파일_경로는_구분되는_에러() {
        let dir = TempDir::new();
        fs::write(dir.0.join("file"), "").unwrap();

        let missing = dir.0.join("missing");
        assert!(matches!(
            LocalFs.list_dir(missing.to_str().unwrap()).await,
            Err(FsError::NotFound(_))
        ));
        let file = dir.0.join("file");
        assert!(matches!(
            LocalFs.list_dir(file.to_str().unwrap()).await,
            Err(FsError::NotADirectory(_))
        ));
    }

    #[tokio::test]
    async fn 홈_디렉터리를_읽을_수_있다() {
        let home = LocalFs.home().await.unwrap();
        LocalFs.list_dir(&home).await.unwrap();
    }
}
