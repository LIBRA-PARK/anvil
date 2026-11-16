//! SSH 접속과 원격 셸을 다루는 크레이트 (russh 기반).
//!
//! Tauri에 의존하지 않는다. 출력은 콜백으로 전달하므로
//! 호출하는 쪽(`src-tauri`)이 IPC 채널 등 원하는 방식으로 내보내면 된다.
//!
//! - [`SshPool`]: 호스트별 연결을 공유한다. 터미널 탭을 여러 개 열어도 인증은 한 번만 한다.
//! - [`Connection`]: 인증까지 끝난 SSH 연결. 채널(셸)을 여러 개 열 수 있다.
//! - [`Shell`]: PTY가 붙은 원격 셸 하나. drop하면 닫힌다.
//! - [`SubsystemStream`]: SFTP 같은 서브시스템에 붙는 바이트 스트림.

mod connection;
mod error;
mod exec;
mod host_key;
mod pool;
mod shell;
mod subsystem;

pub use connection::{connect, Auth, ConnectParams, Connection};
pub use error::SshError;
pub use exec::{shell_quote, ExecOutput};
pub use host_key::HostKeyPolicy;
pub use pool::SshPool;
pub use shell::{Shell, ShellEvent};
pub use subsystem::SubsystemStream;

pub type Result<T> = std::result::Result<T, SshError>;
