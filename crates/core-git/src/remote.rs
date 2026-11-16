//! 원격 저장소: SSH로 서버의 `git`을 실행해 결과를 해석한다.

use crate::{porcelain, GitError, GitRepo, GitStatus, Result};
use core_ssh::{shell_quote, Connection, ExecOutput};
use std::sync::Arc;

pub struct RemoteGit {
    connection: Arc<Connection>,
}

impl RemoteGit {
    pub fn new(connection: Arc<Connection>) -> Self {
        Self { connection }
    }

    /// `git -C <dir> <args>`를 실행한다. `args`의 각 항목은 셸 인용 처리된다.
    async fn git(&self, dir: &str, args: &[&str]) -> Result<ExecOutput> {
        let mut command = format!("git -C {}", shell_quote(dir));
        for arg in args {
            command.push(' ');
            command.push_str(&shell_quote(arg));
        }
        let output = self.connection.exec(&command).await?;
        // 셸이 명령을 찾지 못하면 127로 끝난다.
        if output.code == Some(127) {
            return Err(GitError::GitNotInstalled);
        }
        Ok(output)
    }
}

fn failed(output: &ExecOutput) -> GitError {
    GitError::Other(match output.stderr_text() {
        text if text.is_empty() => format!("git이 실패했습니다 (종료 코드 {:?})", output.code),
        text => text,
    })
}

#[async_trait::async_trait]
impl GitRepo for RemoteGit {
    async fn status(&self, dir: &str) -> Result<GitStatus> {
        let toplevel = self.git(dir, &["rev-parse", "--show-toplevel"]).await?;
        if !toplevel.success() {
            // 저장소가 아니거나 폴더가 없으면 128로 끝난다.
            return Err(if toplevel.code == Some(128) {
                GitError::NotARepository(dir.into())
            } else {
                failed(&toplevel)
            });
        }
        let root = String::from_utf8_lossy(&toplevel.stdout)
            .trim_end()
            .to_string();

        let status = self
            .git(
                &root,
                &[
                    "status",
                    "--porcelain=v1",
                    "-z",
                    "--branch",
                    "--untracked-files=all",
                ],
            )
            .await?;
        if !status.success() {
            return Err(failed(&status));
        }
        let (branch, changes) = porcelain::parse(&status.stdout);
        Ok(GitStatus {
            root,
            branch,
            changes,
        })
    }

    async fn head_file(&self, root: &str, path: &str, max_bytes: u64) -> Result<Option<Vec<u8>>> {
        let object = format!("HEAD:{path}");
        // 내용을 받기 전에 크기부터 확인한다. HEAD에 없는 파일이면 여기서 실패한다.
        let size = self.git(root, &["cat-file", "-s", &object]).await?;
        if !size.success() {
            return Ok(None);
        }
        let bytes: u64 = String::from_utf8_lossy(&size.stdout)
            .trim()
            .parse()
            .unwrap_or(0);
        if bytes > max_bytes {
            return Err(GitError::TooLarge {
                path: path.into(),
                limit_mb: max_bytes / (1024 * 1024),
            });
        }
        let content = self.git(root, &["cat-file", "blob", &object]).await?;
        if !content.success() {
            return Err(failed(&content));
        }
        Ok(Some(content.stdout))
    }
}
