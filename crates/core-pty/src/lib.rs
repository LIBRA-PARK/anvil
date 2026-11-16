//! 이 머신에서 도는 로컬 셸 (의사 터미널).
//!
//! `core-ssh`의 원격 셸과 같은 모양(쓰기 / 크기 변경 / 이벤트 콜백)이라
//! 터미널 UI가 로컬인지 원격인지 구분하지 않아도 된다.
//! Unix는 PTY, Windows는 ConPTY를 쓴다(`portable-pty`).

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum PtyError {
    #[error("로컬 셸을 시작할 수 없습니다: {0}")]
    Spawn(String),
    #[error("셸이 이미 종료되었습니다")]
    Closed,
}

/// 셸에서 나오는 이벤트
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PtyEvent {
    /// 터미널에 그대로 써야 하는 출력 바이트
    Data(Vec<u8>),
    /// 셸이 끝났다. 정확히 한 번, 마지막에 전달된다.
    Exit { code: Option<u32> },
}

/// 셸이 끝나면 PTY를 닫아야 하므로 여러 스레드가 함께 들고 있는다.
type SharedMaster = Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>;

/// 실행 중인 로컬 셸의 조작 핸들. drop하면 셸을 종료시킨다.
pub struct LocalShell {
    writer: Mutex<Box<dyn Write + Send>>,
    master: SharedMaster,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

impl LocalShell {
    /// 사용자의 기본 셸을 홈 디렉터리에서 띄운다.
    ///
    /// 출력과 종료는 `on_event`로 전달된다(별도 스레드에서 호출된다).
    pub fn spawn(
        cols: u16,
        rows: u16,
        mut on_event: impl FnMut(PtyEvent) + Send + 'static,
    ) -> Result<Self, PtyError> {
        let spawn_error = |e: &dyn std::fmt::Display| PtyError::Spawn(e.to_string());

        let pair = native_pty_system()
            .openpty(size(cols, rows))
            .map_err(|e| spawn_error(&e))?;

        // Unix: 로그인 셸($SHELL), Windows: %ComSpec%
        let mut command = CommandBuilder::new_default_prog();
        command.env("TERM", "xterm-256color");
        if let Some(home) = std::env::home_dir() {
            command.cwd(home);
        }
        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| spawn_error(&e))?;
        // 자식이 물려받은 쪽은 닫아야 셸이 끝났을 때 읽기가 EOF로 끝난다.
        drop(pair.slave);

        let killer = child.clone_killer();
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| spawn_error(&e))?;
        let writer = pair.master.take_writer().map_err(|e| spawn_error(&e))?;
        let master: SharedMaster = Arc::new(Mutex::new(Some(pair.master)));

        // 종료를 기다리는 스레드: 종료 코드를 읽기 스레드에 넘기고 PTY를 닫는다.
        let (exit_tx, exit_rx) = mpsc::channel();
        let master_for_wait = Arc::clone(&master);
        std::thread::spawn(move || {
            let code = child.wait().ok().map(|status| status.exit_code());
            let _ = exit_tx.send(code);
            // Windows(ConPTY)는 PTY를 닫기 전에는 읽기가 끝나지 않는다.
            // 마지막 출력이 흘러나올 시간을 잠깐 준 뒤 닫는다.
            std::thread::sleep(Duration::from_millis(100));
            lock(&master_for_wait).take();
        });

        // 읽기 스레드: 출력을 전달하다가 EOF가 오면 종료 이벤트로 마무리한다.
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => on_event(PtyEvent::Data(buffer[..n].to_vec())),
                }
            }
            let code = exit_rx.recv().ok().flatten();
            on_event(PtyEvent::Exit { code });
        });

        Ok(Self {
            writer: Mutex::new(writer),
            master,
            killer: Mutex::new(killer),
        })
    }

    /// 키 입력 등을 셸로 보낸다.
    pub fn write(&self, data: &[u8]) -> Result<(), PtyError> {
        let mut writer = lock(&self.writer);
        writer
            .write_all(data)
            .and_then(|()| writer.flush())
            .map_err(|_| PtyError::Closed)
    }

    /// 터미널 크기가 바뀌었음을 알린다.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), PtyError> {
        match lock(&self.master).as_ref() {
            Some(master) => master
                .resize(size(cols, rows))
                .map_err(|_| PtyError::Closed),
            None => Err(PtyError::Closed),
        }
    }
}

impl Drop for LocalShell {
    fn drop(&mut self) {
        // 탭을 닫으면 셸과 그 안에서 돌던 프로그램도 함께 끝낸다.
        let _ = lock(&self.killer).kill();
    }
}

// ConPTY는 CI 환경에 따라 동작이 달라 Unix에서만 검증한다.
#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn spawn() -> (LocalShell, mpsc::Receiver<PtyEvent>) {
        let (tx, rx) = mpsc::channel();
        let shell = LocalShell::spawn(80, 24, move |event| {
            let _ = tx.send(event);
        })
        .unwrap();
        (shell, rx)
    }

    /// `needle`이 나올 때까지 출력을 모은다. 셸이 먼저 끝나면 그때의 종료 코드를 돌려준다.
    fn read_until(rx: &mpsc::Receiver<PtyEvent>, needle: &str) -> Result<String, Option<u32>> {
        let mut seen = String::new();
        loop {
            match rx.recv_timeout(Duration::from_secs(20)) {
                Ok(PtyEvent::Data(data)) => {
                    seen.push_str(&String::from_utf8_lossy(&data));
                    if seen.contains(needle) {
                        return Ok(seen);
                    }
                }
                Ok(PtyEvent::Exit { code }) => return Err(code),
                Err(_) => panic!("'{needle}'을 기다리다 시간 초과. 지금까지: {seen:?}"),
            }
        }
    }

    fn wait_exit(rx: &mpsc::Receiver<PtyEvent>) -> Option<u32> {
        loop {
            match rx.recv_timeout(Duration::from_secs(20)) {
                Ok(PtyEvent::Exit { code }) => return code,
                Ok(PtyEvent::Data(_)) => {}
                Err(_) => panic!("종료 이벤트를 기다리다 시간 초과"),
            }
        }
    }

    #[test]
    fn 명령을_실행하고_종료_코드를_돌려준다() {
        let (shell, rx) = spawn();
        // 따옴표로 쪼개 두면 입력이 그대로 되비친 것과 실제 실행 결과를 구분할 수 있다.
        shell.write(b"echo anv''il-ok; stty size\n").unwrap();
        let output = read_until(&rx, "anvil-ok").unwrap();
        assert!(!output.is_empty());
        read_until(&rx, "24 80").unwrap();

        shell.resize(100, 30).unwrap();
        shell.write(b"stty size\n").unwrap();
        read_until(&rx, "30 100").unwrap();

        shell.write(b"exit 7\n").unwrap();
        assert_eq!(wait_exit(&rx), Some(7));
        assert!(
            rx.recv_timeout(Duration::from_secs(5)).is_err(),
            "Exit 뒤에는 이벤트가 없어야 한다"
        );
    }

    #[test]
    fn 핸들을_버리면_셸이_종료된다() {
        let (shell, rx) = spawn();
        shell.write(b"echo re''ady\n").unwrap();
        read_until(&rx, "ready").unwrap();
        drop(shell);
        wait_exit(&rx);
    }
}
