//! 프론트엔드로 전달되는 공통 에러 타입.
//!
//! TS에서는 `{ kind: "notFound", message: "..." }` 형태로 받으므로
//! UI가 `kind`로 분기해 처리할 수 있다. (예: SSH 인증 실패 → 비밀번호 재입력)
//! 새 도메인 에러가 생기면 여기에 variant를 추가한다.

use core_config::ConfigError;
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
    /// 파일 등 입출력 실패
    #[error("{0}")]
    Io(String),
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

impl<T> From<std::sync::PoisonError<T>> for AppError {
    fn from(e: std::sync::PoisonError<T>) -> Self {
        Self::Internal(format!("상태 잠금 실패: {e}"))
    }
}

pub type AppResult<T> = Result<T, AppError>;
