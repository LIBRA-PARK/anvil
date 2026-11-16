//! 로컬 디스크.

use crate::{
    sort_entries, too_large, validate_name, DirEntry, DirListing, EntryKind, FileSystem, FsError,
    Result,
};
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
        std::io::ErrorKind::AlreadyExists => FsError::AlreadyExists(shown),
        std::io::ErrorKind::IsADirectory => FsError::Io(format!("폴더입니다: {shown}")),
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
        blocking(move || read_dir(&path)).await
    }

    async fn read_file(&self, path: &str, max_bytes: u64) -> Result<Vec<u8>> {
        let path = path.to_string();
        blocking(move || {
            let file = Path::new(&path);
            let size = std::fs::metadata(file).map_err(|e| map_io(e, file))?.len();
            if size > max_bytes {
                return Err(too_large(&path, size, max_bytes));
            }
            std::fs::read(file).map_err(|e| map_io(e, file))
        })
        .await
    }

    async fn write_file(&self, path: &str, data: &[u8]) -> Result<()> {
        let path = path.to_string();
        let data = data.to_vec();
        blocking(move || std::fs::write(&path, data).map_err(|e| map_io(e, Path::new(&path)))).await
    }

    async fn create_dir(&self, dir: &str, name: &str) -> Result<String> {
        let (dir, name) = (dir.to_string(), name.to_string());
        blocking(move || create_dir(&dir, &name)).await
    }

    async fn rename(&self, path: &str, new_name: &str) -> Result<String> {
        let (path, new_name) = (path.to_string(), new_name.to_string());
        blocking(move || rename(&path, &new_name)).await
    }

    async fn remove(&self, path: &str) -> Result<()> {
        let path = path.to_string();
        blocking(move || remove(&path)).await
    }
}

fn create_dir(dir: &str, name: &str) -> Result<String> {
    let path = Path::new(dir).join(validate_name(name)?);
    std::fs::create_dir(&path).map_err(|e| map_io(e, &path))?;
    Ok(path.to_string_lossy().into_owned())
}

fn rename(path: &str, new_name: &str) -> Result<String> {
    let from = Path::new(path);
    let to = from.with_file_name(validate_name(new_name)?);
    // std::fs::rename은 대상을 조용히 덮어쓰므로 먼저 확인한다.
    // (대소문자만 바꾸는 경우는 같은 파일이라 허용한다)
    if to.symlink_metadata().is_ok() && !path.eq_ignore_ascii_case(&to.to_string_lossy()) {
        return Err(FsError::AlreadyExists(to.display().to_string()));
    }
    std::fs::rename(from, &to).map_err(|e| map_io(e, from))?;
    Ok(to.to_string_lossy().into_owned())
}

fn remove(path: &str) -> Result<()> {
    let target = Path::new(path);
    // 링크를 따라가지 않는 메타데이터: 폴더를 가리키는 링크는 링크 파일로 취급한다.
    let metadata = target.symlink_metadata().map_err(|e| map_io(e, target))?;
    if metadata.is_dir() {
        std::fs::remove_dir_all(target).map_err(|e| map_io(e, target))
    } else {
        std::fs::remove_file(target).map_err(|e| map_io(e, target))
    }
}

/// 디스크 작업이 비동기 런타임을 막지 않게 별도 스레드에서 실행한다.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| FsError::Io(e.to_string()))?
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
    async fn 파일을_쓰고_다시_읽는다() {
        let dir = TempDir::new();
        let file = dir.0.join("note.txt");
        let path = file.to_str().unwrap();

        LocalFs
            .write_file(path, "첫 내용, 조금 더 긴".as_bytes())
            .await
            .unwrap();
        // 더 짧은 내용으로 덮어써도 예전 내용이 남지 않아야 한다.
        LocalFs.write_file(path, "짧게".as_bytes()).await.unwrap();
        assert_eq!(
            LocalFs.read_file(path, 1024).await.unwrap(),
            "짧게".as_bytes()
        );

        assert!(matches!(
            LocalFs.read_file(path, 3).await,
            Err(FsError::TooLarge { .. })
        ));
        let missing = dir.0.join("missing.txt");
        assert!(matches!(
            LocalFs.read_file(missing.to_str().unwrap(), 1024).await,
            Err(FsError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn 폴더를_만들고_이름을_바꾸고_지운다() {
        let dir = TempDir::new();
        let made = LocalFs.create_dir(dir.path(), "새 폴더").await.unwrap();
        assert!(Path::new(&made).is_dir());
        assert!(matches!(
            LocalFs.create_dir(dir.path(), "새 폴더").await,
            Err(FsError::AlreadyExists(_))
        ));
        assert!(matches!(
            LocalFs.create_dir(dir.path(), "../밖").await,
            Err(FsError::InvalidName(_))
        ));

        fs::write(Path::new(&made).join("안쪽.txt"), "x").unwrap();
        let renamed = LocalFs.rename(&made, "바뀐 이름").await.unwrap();
        assert!(!Path::new(&made).exists());
        assert!(Path::new(&renamed).join("안쪽.txt").is_file());

        // 이미 있는 이름으로는 바꾸지 않는다(덮어쓰기 방지).
        fs::write(dir.0.join("a.txt"), "a").unwrap();
        fs::write(dir.0.join("b.txt"), "b").unwrap();
        let a = dir.0.join("a.txt");
        assert!(matches!(
            LocalFs.rename(a.to_str().unwrap(), "b.txt").await,
            Err(FsError::AlreadyExists(_))
        ));
        assert_eq!(fs::read_to_string(dir.0.join("b.txt")).unwrap(), "b");

        // 내용이 있는 폴더도 통째로 지운다.
        LocalFs.remove(&renamed).await.unwrap();
        assert!(!Path::new(&renamed).exists());
        LocalFs.remove(a.to_str().unwrap()).await.unwrap();
        assert!(!a.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn 폴더_링크를_지워도_대상은_남는다() {
        let dir = TempDir::new();
        fs::create_dir(dir.0.join("real")).unwrap();
        fs::write(dir.0.join("real/keep.txt"), "지워지면 안 됨").unwrap();
        std::os::unix::fs::symlink(dir.0.join("real"), dir.0.join("link")).unwrap();

        LocalFs
            .remove(dir.0.join("link").to_str().unwrap())
            .await
            .unwrap();
        assert!(!dir.0.join("link").exists());
        assert!(dir.0.join("real/keep.txt").is_file());
    }

    #[tokio::test]
    async fn 홈_디렉터리를_읽을_수_있다() {
        let home = LocalFs.home().await.unwrap();
        LocalFs.list_dir(&home).await.unwrap();
    }
}
