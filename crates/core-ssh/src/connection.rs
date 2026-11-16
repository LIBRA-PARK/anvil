//! 접속과 인증.

use crate::host_key::HostKeyPolicy;
use crate::SshError;
use russh::client::{self, AuthResult};
use russh::keys::agent::client::{AgentClient, AgentStream};
use russh::keys::agent::AgentIdentity;
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// TCP 연결부터 키 교환까지 기다리는 최대 시간
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// 이 시간 동안 서버에서 아무것도 오지 않으면 keepalive를 보낸다.
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);

/// 인증 수단. 비밀 값은 호출하는 쪽이 그때그때 넣어 준다(이 크레이트는 저장하지 않는다).
#[derive(Clone)]
pub enum Auth {
    /// `None`이면 [`SshError::PasswordRequired`]를 돌려준다.
    Password(Option<String>),
    /// 개인키 파일. `~/`로 시작하면 홈 디렉터리로 풀어 준다.
    Key {
        path: String,
        passphrase: Option<String>,
    },
    /// 실행 중인 ssh-agent
    Agent,
}

#[derive(Clone)]
pub struct ConnectParams {
    pub address: String,
    pub port: u16,
    pub username: String,
    pub auth: Auth,
}

pub(crate) struct ClientHandler {
    host: String,
    port: u16,
    policy: HostKeyPolicy,
}

impl client::Handler for ClientHandler {
    type Error = SshError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        match server_public_key {
            PublicKeyOrCertificate::PublicKey { key, .. } => {
                self.policy.verify(&self.host, self.port, key)?;
                Ok(true)
            }
            PublicKeyOrCertificate::Certificate(_) => Err(SshError::Protocol(
                "호스트 인증서는 아직 지원하지 않습니다".into(),
            )),
        }
    }
}

/// 인증까지 끝난 SSH 연결.
pub struct Connection {
    pub(crate) handle: client::Handle<ClientHandler>,
}

impl Connection {
    pub fn is_closed(&self) -> bool {
        self.handle.is_closed()
    }
}

/// 서버에 접속하고 호스트 키 검증과 사용자 인증을 마친다.
pub async fn connect(
    params: &ConnectParams,
    policy: &HostKeyPolicy,
) -> Result<Connection, SshError> {
    // 접속을 시도하기 전에 알 수 있는 문제는 먼저 걸러 낸다.
    if matches!(params.auth, Auth::Password(None)) {
        return Err(SshError::PasswordRequired);
    }

    let config = Arc::new(client::Config {
        keepalive_interval: Some(KEEPALIVE_INTERVAL),
        // 키 입력이 Nagle 알고리즘 때문에 지연되지 않게 한다.
        nodelay: true,
        ..Default::default()
    });
    let handler = ClientHandler {
        host: params.address.clone(),
        port: params.port,
        policy: policy.clone(),
    };

    let connecting = client::connect(config, (params.address.as_str(), params.port), handler);
    let mut handle = tokio::time::timeout(CONNECT_TIMEOUT, connecting)
        .await
        .map_err(|_| SshError::Connect("연결 시간 초과".into()))??;

    authenticate(&mut handle, params).await?;
    Ok(Connection { handle })
}

async fn authenticate(
    handle: &mut client::Handle<ClientHandler>,
    params: &ConnectParams,
) -> Result<(), SshError> {
    let user = params.username.as_str();
    match &params.auth {
        Auth::Password(None) => Err(SshError::PasswordRequired),
        Auth::Password(Some(password)) => {
            let result = handle.authenticate_password(user, password).await?;
            check(result, "비밀번호가 올바르지 않습니다")
        }
        Auth::Key { path, passphrase } => {
            let key = load_key(path, passphrase.as_deref())?;
            // RSA 키는 서버가 지원하는 해시(rsa-sha2-512 등)를 골라야 한다.
            let hash = if key.algorithm().is_rsa() {
                handle.best_supported_rsa_hash().await?.flatten()
            } else {
                None
            };
            let key = PrivateKeyWithHashAlg::new(Arc::new(key), hash);
            let result = handle.authenticate_publickey(user, key).await?;
            check(result, "서버가 이 키를 받아들이지 않았습니다")
        }
        Auth::Agent => authenticate_with_agent(handle, user).await,
    }
}

fn check(result: AuthResult, reason: &str) -> Result<(), SshError> {
    if result.success() {
        Ok(())
    } else {
        Err(SshError::AuthFailed(reason.into()))
    }
}

fn load_key(path: &str, passphrase: Option<&str>) -> Result<russh::keys::PrivateKey, SshError> {
    let path = expand_tilde(path);
    load_secret_key(&path, passphrase).map_err(|e| match e {
        russh::keys::Error::KeyIsEncrypted => {
            SshError::PassphraseRequired("개인키 패스프레이즈가 필요합니다".into())
        }
        russh::keys::Error::IO(io) => SshError::Key(format!("{}: {io}", path.display())),
        // 패스프레이즈를 넣었는데 복호화에 실패했다면 틀렸을 가능성이 가장 높다.
        _ if passphrase.is_some() => {
            SshError::PassphraseRequired("패스프레이즈가 올바르지 않습니다".into())
        }
        other => SshError::Key(format!("{}: {other}", path.display())),
    })
}

/// `~` 또는 `~/...`를 홈 디렉터리로 풀어 준다. (셸이 아니므로 직접 처리해야 한다)
fn expand_tilde(path: &str) -> PathBuf {
    let rest = match path {
        "~" => Some(""),
        _ => path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")),
    };
    match (rest, std::env::home_dir()) {
        (Some(rest), Some(home)) => home.join(Path::new(rest)),
        _ => PathBuf::from(path),
    }
}

async fn authenticate_with_agent(
    handle: &mut client::Handle<ClientHandler>,
    user: &str,
) -> Result<(), SshError> {
    #[cfg(unix)]
    let agent = AgentClient::connect_env().await;
    // Windows 기본 OpenSSH 에이전트의 named pipe
    #[cfg(windows)]
    let agent = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await;

    let agent = agent.map_err(|e| SshError::Agent(e.to_string()))?;
    try_agent_identities(handle, user, agent).await
}

/// 에이전트가 가진 키를 하나씩 시도한다.
async fn try_agent_identities<S>(
    handle: &mut client::Handle<ClientHandler>,
    user: &str,
    mut agent: AgentClient<S>,
) -> Result<(), SshError>
where
    S: AgentStream + Send + Unpin,
{
    let identities = agent
        .request_identities()
        .await
        .map_err(|e| SshError::Agent(e.to_string()))?;
    if identities.is_empty() {
        return Err(SshError::Agent(
            "에이전트에 등록된 키가 없습니다 (ssh-add로 추가하세요)".into(),
        ));
    }

    for identity in identities {
        // 인증서 신원은 아직 지원하지 않는다.
        let AgentIdentity::PublicKey { key, .. } = identity else {
            continue;
        };
        let hash = if key.algorithm().is_rsa() {
            handle.best_supported_rsa_hash().await?.flatten()
        } else {
            None
        };
        // 한 키가 실패해도(서명 거부 등) 다음 키를 계속 시도한다.
        if let Ok(result) = handle
            .authenticate_publickey_with(user, key, hash, &mut agent)
            .await
        {
            if result.success() {
                return Ok(());
            }
        }
        if handle.is_closed() {
            return Err(SshError::Closed);
        }
    }
    Err(SshError::AuthFailed(
        "에이전트의 어떤 키도 서버가 받아들이지 않았습니다".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 물결표는_홈_디렉터리로_풀린다() {
        let home = std::env::home_dir().expect("홈 디렉터리");
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(
            expand_tilde("~/.ssh/id_ed25519"),
            home.join(".ssh/id_ed25519")
        );
    }

    #[test]
    fn 물결표가_없으면_그대로_둔다() {
        assert_eq!(expand_tilde("/etc/key"), PathBuf::from("/etc/key"));
        // `~user` 형식은 지원하지 않으므로 건드리지 않는다.
        assert_eq!(expand_tilde("~other/key"), PathBuf::from("~other/key"));
    }
}
