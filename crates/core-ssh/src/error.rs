/// SSH 계층 에러.
///
/// UI가 종류별로 다르게 반응해야 하므로(비밀번호 재입력, 호스트 키 확인 등)
/// 문자열 하나로 뭉치지 않고 variant로 구분한다.
#[derive(Debug, thiserror::Error)]
pub enum SshError {
    /// 네트워크 연결 실패 (DNS, 접속 거부, 시간 초과 등)
    #[error("서버에 연결할 수 없습니다: {0}")]
    Connect(String),

    /// 처음 보는 호스트 키. 사용자가 지문을 확인하고 신뢰해야 접속할 수 있다.
    #[error("알 수 없는 호스트 키입니다: {fingerprint}")]
    HostKeyUnknown {
        /// `"ssh-ed25519 SHA256:..."` 형식. 신뢰할 때 이 값을 그대로 돌려준다.
        fingerprint: String,
    },

    /// 저장된 것과 다른 호스트 키. 중간자 공격일 수 있으므로 접속하지 않는다.
    #[error(
        "호스트 키가 저장된 것과 다릅니다 ({fingerprint}). 중간자 공격일 수 있어 접속을 중단했습니다. \
         서버 키가 실제로 바뀐 것이라면 {path} 파일에서 해당 호스트 줄을 지운 뒤 다시 시도하세요."
    )]
    HostKeyMismatch { fingerprint: String, path: String },

    /// 비밀번호 인증인데 비밀번호가 주어지지 않음
    #[error("비밀번호가 필요합니다")]
    PasswordRequired,

    /// 개인키가 암호화되어 있는데 패스프레이즈가 없거나 틀림
    #[error("{0}")]
    PassphraseRequired(String),

    /// 서버가 인증을 거부함
    #[error("인증에 실패했습니다: {0}")]
    AuthFailed(String),

    /// 개인키 파일을 읽거나 해석할 수 없음
    #[error("개인키를 사용할 수 없습니다: {0}")]
    Key(String),

    /// ssh-agent에 연결할 수 없거나 쓸 수 있는 키가 없음
    #[error("ssh-agent를 사용할 수 없습니다: {0}")]
    Agent(String),

    /// 이미 닫힌 연결이나 셸에 요청함
    #[error("연결이 닫혔습니다")]
    Closed,

    /// 그 외 SSH 프로토콜 수준 오류
    #[error("SSH 오류: {0}")]
    Protocol(String),
}

impl From<russh::Error> for SshError {
    fn from(e: russh::Error) -> Self {
        match e {
            russh::Error::IO(io) => Self::Connect(io.to_string()),
            russh::Error::ConnectionTimeout
            | russh::Error::KeepaliveTimeout
            | russh::Error::InactivityTimeout => Self::Connect("응답 시간 초과".into()),
            russh::Error::Disconnect | russh::Error::SendError => Self::Closed,
            other => Self::Protocol(other.to_string()),
        }
    }
}
