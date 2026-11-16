//! 로컬 저장소 (libgit2).

use crate::{sort_changes, ChangeKind, GitChange, GitError, GitRepo, GitStatus, Result};
use git2::{ErrorCode, Repository, Status, StatusOptions};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, Copy)]
pub struct LocalGit;

fn other(e: git2::Error) -> GitError {
    GitError::Other(e.message().to_string())
}

/// Git 경로는 바이트열이지만 UI로 보내려면 문자열이어야 한다. 구분자는 `/`로 통일한다.
fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn classify(status: Status) -> Option<ChangeKind> {
    if status.is_ignored() {
        return None;
    }
    Some(if status.is_conflicted() {
        ChangeKind::Conflicted
    } else if status.is_index_deleted() || status.is_wt_deleted() {
        ChangeKind::Deleted
    } else if status.is_index_renamed() || status.is_wt_renamed() {
        ChangeKind::Renamed
    } else if status.is_index_new() {
        ChangeKind::Added
    } else if status.is_wt_new() {
        ChangeKind::Untracked
    } else {
        ChangeKind::Modified
    })
}

fn current_branch(repo: &Repository) -> Option<String> {
    match repo.head() {
        Ok(head) if head.is_branch() => head.shorthand().ok().map(str::to_string),
        Ok(_) => None,
        // 아직 커밋이 없는 저장소: HEAD가 가리키는 브랜치 이름만 읽는다.
        Err(e) if e.code() == ErrorCode::UnbornBranch => repo
            .find_reference("HEAD")
            .ok()?
            .symbolic_target()
            .ok()
            .flatten()?
            .strip_prefix("refs/heads/")
            .map(str::to_string),
        Err(_) => None,
    }
}

fn status(dir: &str) -> Result<GitStatus> {
    let repo = Repository::discover(dir).map_err(|e| match e.code() {
        ErrorCode::NotFound => GitError::NotARepository(dir.into()),
        _ => other(e),
    })?;
    let root = repo
        .workdir()
        .ok_or_else(|| GitError::Other("작업 폴더가 없는(bare) 저장소입니다".into()))?;

    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true);

    let mut changes = Vec::new();
    for entry in repo.statuses(Some(&mut options)).map_err(other)?.iter() {
        let Some(kind) = classify(entry.status()) else {
            continue;
        };
        // 이름이 바뀐 항목은 HEAD→인덱스 비교 결과에 옛 경로와 새 경로가 들어 있다.
        let renamed = entry
            .head_to_index()
            .filter(|_| kind == ChangeKind::Renamed);
        let path = match renamed.as_ref().and_then(|d| d.new_file().path()) {
            Some(path) => path_string(path),
            None => String::from_utf8_lossy(entry.path_bytes()).into_owned(),
        };
        let old_path = renamed
            .and_then(|d| d.old_file().path().map(path_string))
            .filter(|old| *old != path);
        changes.push(GitChange {
            path,
            old_path,
            kind,
        });
    }
    sort_changes(&mut changes);

    // libgit2는 Windows에서도 `/`로 구분하고 끝에 `/`를 붙여 돌려준다.
    // 구성 요소로 풀었다 다시 모아 OS 형식(`C:\\Users\\...`)으로 맞추고 끝의 구분자를 없앤다.
    let root: PathBuf = root.components().collect();
    let root = root.to_string_lossy().into_owned();
    Ok(GitStatus {
        root,
        branch: current_branch(&repo),
        changes,
    })
}

fn head_file(root: &str, path: &str, max_bytes: u64) -> Result<Option<Vec<u8>>> {
    let repo = Repository::open(root).map_err(other)?;
    let tree = match repo.head().and_then(|head| head.peel_to_tree()) {
        Ok(tree) => tree,
        // 아직 커밋이 없으면 HEAD에는 아무 파일도 없다.
        Err(e) if matches!(e.code(), ErrorCode::UnbornBranch | ErrorCode::NotFound) => {
            return Ok(None)
        }
        Err(e) => return Err(other(e)),
    };
    let entry = match tree.get_path(Path::new(path)) {
        Ok(entry) => entry,
        Err(e) if e.code() == ErrorCode::NotFound => return Ok(None),
        Err(e) => return Err(other(e)),
    };
    let Ok(blob) = repo.find_blob(entry.id()) else {
        // 서브모듈이나 폴더는 비교할 내용이 없다.
        return Ok(None);
    };
    if blob.size() as u64 > max_bytes {
        return Err(GitError::TooLarge {
            path: path.into(),
            limit_mb: max_bytes / (1024 * 1024),
        });
    }
    Ok(Some(blob.content().to_vec()))
}

/// libgit2 호출은 디스크를 읽으므로 비동기 런타임을 막지 않게 별도 스레드에서 실행한다.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| GitError::Other(e.to_string()))?
}

#[async_trait::async_trait]
impl GitRepo for LocalGit {
    async fn status(&self, dir: &str) -> Result<GitStatus> {
        let dir = dir.to_string();
        blocking(move || status(&dir)).await
    }

    async fn head_file(&self, root: &str, path: &str, max_bytes: u64) -> Result<Option<Vec<u8>>> {
        let (root, path) = (root.to_string(), path.to_string());
        blocking(move || head_file(&root, &path, max_bytes)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// 임시 저장소. 커밋은 libgit2로 직접 만들어 git 실행 파일 없이도 테스트가 돈다.
    struct TempRepo {
        dir: PathBuf,
        repo: Repository,
    }

    impl TempRepo {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("anvil-git-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&dir).unwrap();
            let dir = fs::canonicalize(dir).unwrap();
            let repo = Repository::init(&dir).unwrap();
            Self { dir, repo }
        }

        fn write(&self, path: &str, content: &str) {
            let file = self.dir.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, content).unwrap();
        }

        /// 작업 폴더 전체를 스테이징한다.
        fn stage_all(&self) {
            let mut index = self.repo.index().unwrap();
            index
                .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
                .unwrap();
            index.update_all(["*"], None).unwrap();
            index.write().unwrap();
        }

        fn commit_all(&self) {
            self.stage_all();
            let mut index = self.repo.index().unwrap();
            let tree = self.repo.find_tree(index.write_tree().unwrap()).unwrap();
            let signature = git2::Signature::now("테스트", "test@example.com").unwrap();
            let parent = self.repo.head().ok().and_then(|h| h.peel_to_commit().ok());
            let parents: Vec<&git2::Commit> = parent.iter().collect();
            self.repo
                .commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    "커밋",
                    &tree,
                    &parents,
                )
                .unwrap();
        }

        fn path(&self) -> &str {
            self.dir.to_str().unwrap()
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.dir).ok();
        }
    }

    fn kinds(status: &GitStatus) -> Vec<(&str, ChangeKind)> {
        status
            .changes
            .iter()
            .map(|c| (c.path.as_str(), c.kind))
            .collect()
    }

    #[tokio::test]
    async fn 변경_종류를_구분한다() {
        let repo = TempRepo::new();
        repo.write("keep.txt", "그대로\n");
        repo.write("edit.txt", "수정 전\n");
        repo.write("gone.txt", "삭제될 파일\n");
        repo.write(
            "src/old_name.rs",
            "fn main() { println!(\"이름이 바뀔 파일\"); }\n",
        );
        repo.commit_all();

        repo.write("edit.txt", "수정 후\n");
        fs::remove_file(repo.dir.join("gone.txt")).unwrap();
        fs::rename(
            repo.dir.join("src/old_name.rs"),
            repo.dir.join("src/new_name.rs"),
        )
        .unwrap();
        repo.write("staged.txt", "스테이징된 새 파일\n");
        repo.stage_all();
        repo.write("sub/untracked.txt", "추적하지 않는 파일\n");

        // 하위 폴더를 줘도 저장소 루트를 찾아야 한다.
        let sub = repo.dir.join("src");
        let status = LocalGit.status(sub.to_str().unwrap()).await.unwrap();
        // Windows에서는 canonicalize가 `\\\\?\\` 접두사를 붙이므로 양쪽을 같은 방식으로 정규화해 비교한다.
        assert_eq!(fs::canonicalize(&status.root).unwrap(), repo.dir);
        assert!(!status.root.ends_with(['/', '\\']));
        if cfg!(windows) {
            assert!(
                !status.root.contains('/'),
                "OS 형식의 경로여야 한다: {}",
                status.root
            );
        }
        assert!(status.branch.is_some());

        use ChangeKind::*;
        assert_eq!(
            kinds(&status),
            vec![
                ("edit.txt", Modified),
                ("gone.txt", Deleted),
                ("src/new_name.rs", Renamed),
                ("staged.txt", Added),
                ("sub/untracked.txt", Untracked),
            ]
        );
        let renamed = status.changes.iter().find(|c| c.kind == Renamed).unwrap();
        assert_eq!(renamed.old_path.as_deref(), Some("src/old_name.rs"));
    }

    #[tokio::test]
    async fn head의_파일_내용을_읽는다() {
        let repo = TempRepo::new();
        repo.write("dir/file.txt", "커밋된 내용\n");
        repo.commit_all();
        repo.write("dir/file.txt", "작업 중인 내용\n");
        repo.write("new.txt", "새 파일\n");

        let read = |path: &'static str, max| LocalGit.head_file(repo.path(), path, max);
        assert_eq!(
            read("dir/file.txt", 1024).await.unwrap().as_deref(),
            Some("커밋된 내용\n".as_bytes())
        );
        assert_eq!(read("new.txt", 1024).await.unwrap(), None);
        assert!(matches!(
            read("dir/file.txt", 3).await,
            Err(GitError::TooLarge { .. })
        ));
    }

    #[tokio::test]
    async fn 커밋이_없는_저장소도_다룬다() {
        let repo = TempRepo::new();
        repo.write("first.txt", "첫 파일\n");

        let status = LocalGit.status(repo.path()).await.unwrap();
        assert_eq!(kinds(&status), vec![("first.txt", ChangeKind::Untracked)]);
        assert!(
            status.branch.is_some(),
            "아직 커밋이 없어도 브랜치 이름은 알 수 있다"
        );
        assert_eq!(
            LocalGit
                .head_file(repo.path(), "first.txt", 1024)
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn 저장소가_아니면_구분되는_에러() {
        let dir = std::env::temp_dir().join(format!("anvil-not-a-repo-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let result = LocalGit.status(dir.to_str().unwrap()).await;
        fs::remove_dir_all(&dir).ok();
        // 임시 폴더의 상위가 우연히 저장소일 가능성은 없다고 본다.
        assert!(matches!(result, Err(GitError::NotARepository(_))));
    }
}
