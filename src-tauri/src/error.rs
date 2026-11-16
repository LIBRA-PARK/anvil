//! 프론트엔드로 전달되는 공통 에러 타입.
//!
//! TS에서는 `{ kind: "notFound", message: "..." }` 형태로 받으므로
//! UI가 `kind`로 분기해 처리할 수 있다. (예: SSH 인증 실패 → 비밀번호 재입력)
//! 새 도메인 에러가 생기면 여기에 variant를 추가한다.

use core_config::ConfigError;
use core_fs::FsError;
use core_ssh::SshError;
use serde::Serialize;

#[derive(Debug, thiserror::Error, Serialize, specta::Type)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum AppError {
    /// 요청한 대상이 없음
    #[error("{0}")]
    NotFound(String),
    /// 사용자 입력이 잘못됨
    #[error("{0}")]
    InvalidInput(String),
    /// 파일이나 폴더에 접근할 권한이 없음
    #[error("{0}")]
    PermissionDenied(String),
    /// 파일 등 입출력 실패
    #[error("{0}")]
    Io(String),
    /// 서버에 연결할 수 없거나 연결이 끊김
    #[error("{0}")]
    Connection(String),
    /// 처음 보는 호스트 키. `message`는 사용자에게 확인받을 지문이며,
    /// 신뢰한다면 이 값을 그대로 `terminal_open`의 `trusted_host_key`로 돌려보낸다.
    #[error("{0}")]
    HostKeyUnknown(String),
    /// 저장된 것과 다른 호스트 키 (접속 중단)
    #[error("{0}")]
    HostKeyMismatch(String),
    /// 비밀번호를 입력받아 다시 시도해야 함
    #[error("{0}")]
    PasswordRequired(String),
    /// 개인키 패스프레이즈를 입력받아 다시 시도해야 함
    #[error("{0}")]
    PassphraseRequired(String),
    /// 서버가 인증을 거부함
    #[error("{0}")]
    AuthFailed(String),
    /// 그 외 내부 오류 (버그에 가까움)
    #[error("{0}")]
    Internal(String),
}

impl From<ConfigError> for AppError {
    fn from(e: ConfigError) -> Self {
        let message = e.to_string();
        match e {
            ConfigError::NotFound(_) => Self::NotFound(message),
            ConfigError::Invalid(_) => Self::InvalidInput(message),
            ConfigError::Io(_) => Self::Io(message),
            ConfigError::Json(_) => Self::Internal(message),
        }
    }
}

impl From<FsError> for AppError {
    fn from(e: FsError) -> Self {
        let message = e.to_string();
        match e {
            FsError::NotFound(_) => Self::NotFound(message),
            FsError::PermissionDenied(_) => Self::PermissionDenied(message),
            FsError::NotADirectory(_) => Self::InvalidInput(message),
            FsError::Disconnected => Self::Connection(message),
            // 접속 단계의 에러는 종류를 그대로 살려 UI가 비밀번호 입력 등으로 이어 갈 수 있게 한다.
            FsError::Ssh(ssh) => ssh.into(),
            FsError::Io(_) => Self::Io(message),
        }
    }
}

impl From<SshError> for AppError {
    fn from(e: SshError) -> Self {
        let message = e.to_string();
        match e {
            SshError::Connect(_) | SshError::Closed => Self::Connection(message),
            // UI가 지문만 따로 쓸 수 있도록 설명 문구 없이 지문 자체를 담는다.
            SshError::HostKeyUnknown { fingerprint } => Self::HostKeyUnknown(fingerprint),
            SshError::HostKeyMismatch { .. } => Self::HostKeyMismatch(message),
            SshError::PasswordRequired => Self::PasswordRequired(message),
            SshError::PassphraseRequired(_) => Self::PassphraseRequired(message),
            SshError::AuthFailed(_) | SshError::Agent(_) => Self::AuthFailed(message),
            SshError::Key(_) => Self::InvalidInput(message),
            SshError::Protocol(_) => Self::Internal(message),
        }
    }
}

impl<T> From<std::sync::PoisonError<T>> for AppError {
    fn from(e: std::sync::PoisonError<T>) -> Self {
        Self::Internal(format!("상태 잠금 실패: {e}"))
    }
}

pub type AppResult<T> = Result<T, AppError>;
