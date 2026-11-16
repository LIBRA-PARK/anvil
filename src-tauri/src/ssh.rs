//! 터미널·파일·Git이 함께 쓰는 SSH 접속 창구.

use crate::error::{AppError, AppResult};
use crate::AppState;
use core_config::{AuthMethod, Host};
use core_secrets::SecretStore;
use core_ssh::{Auth, ConnectParams, Connection, HostKeyPolicy, SshError, SshPool};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

/// OS 보안 저장소에서 이 앱의 항목을 구분하는 이름 (앱 식별자와 같다)
const KEYCHAIN_SERVICE: &str = "dev.anvil.app";

/// 사용자가 접속 과정에서 입력·확인한 값
#[derive(Default)]
pub struct Credentials {
    /// 인증 방식에 따라 비밀번호 또는 개인키 패스프레이즈
    pub secret: Option<String>,
    /// 사용자가 확인한 호스트 키 지문 (`hostKeyUnknown` 에러의 `message`)
    pub trusted_host_key: Option<String>,
}

pub struct SshContext {
    pool: SshPool,
    /// Anvil이 새로 신뢰한 호스트 키를 기록하는 파일
    known_hosts: PathBuf,
    /// 읽기만 하는 known_hosts (`~/.ssh/known_hosts`)
    extra_known_hosts: Vec<PathBuf>,
    /// 사용자가 저장하기로 한 비밀번호·패스프레이즈 (키: 호스트 id)
    secrets: SecretStore,
}

/// 보안 저장소 호출은 OS 응답을 기다리며 멈출 수 있어 별도 스레드에서 실행한다.
async fn keychain<T: Send + 'static>(
    f: impl FnOnce() -> core_secrets::Result<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .map_err(|e| AppError::Io(e.to_string()))
}

impl SshContext {
    pub fn new(known_hosts: PathBuf, extra_known_hosts: Vec<PathBuf>) -> Self {
        Self {
            pool: SshPool::new(),
            known_hosts,
            extra_known_hosts,
            secrets: SecretStore::new(KEYCHAIN_SERVICE),
        }
    }

    async fn attempt(
        &self,
        host: &Host,
        secret: Option<String>,
        trusted_host_key: Option<String>,
    ) -> Result<Arc<Connection>, SshError> {
        let params = ConnectParams {
            address: host.address.clone(),
            port: host.port,
            username: host.username.clone(),
            auth: match &host.auth {
                AuthMethod::Password => Auth::Password(secret),
                AuthMethod::Key { path } => Auth::Key {
                    path: path.clone(),
                    passphrase: secret,
                },
                AuthMethod::Agent => Auth::Agent,
            },
        };
        let policy = HostKeyPolicy {
            known_hosts: self.known_hosts.clone(),
            extra_known_hosts: self.extra_known_hosts.clone(),
            trusted_fingerprint: trusted_host_key,
        };
        self.pool.get_or_connect(&host.id, &params, &policy).await
    }

    /// 호스트에 접속한다. 이미 살아 있는 연결이 있으면 재사용한다.
    ///
    /// 비밀 값이 필요한데 주어지지 않았다면 보안 저장소에 저장해 둔 값을 먼저 써 본다.
    /// 저장된 값이 더 이상 맞지 않으면 지우고, 사용자에게 다시 묻도록 원래 에러를 돌려준다.
    pub async fn connect(
        &self,
        host: &Host,
        credentials: Credentials,
    ) -> AppResult<Arc<Connection>> {
        // 빈 문자열은 입력하지 않은 것으로 본다.
        let supplied = credentials.secret.filter(|s| !s.is_empty());
        let trusted = credentials.trusted_host_key;

        let needs_secret = match self.attempt(host, supplied.clone(), trusted.clone()).await {
            Err(e @ (SshError::PasswordRequired | SshError::PassphraseRequired(_)))
                if supplied.is_none() =>
            {
                e
            }
            other => return Ok(other?),
        };

        // 저장소를 쓸 수 없는 환경이면 저장된 값이 없는 것으로 보고 사용자에게 묻는다.
        let (store, key) = (self.secrets.clone(), host.id.clone());
        let Some(stored) = keychain(move || store.get(&key)).await.ok().flatten() else {
            return Err(needs_secret.into());
        };
        match self.attempt(host, Some(stored), trusted).await {
            Err(SshError::AuthFailed(_) | SshError::PassphraseRequired(_)) => {
                self.forget_secret(&host.id).await.ok();
                Err(needs_secret.into())
            }
            other => Ok(other?),
        }
    }

    pub async fn save_secret(&self, host_id: &str, secret: String) -> AppResult<()> {
        let (store, key) = (self.secrets.clone(), host_id.to_string());
        keychain(move || store.set(&key, &secret)).await
    }

    pub async fn forget_secret(&self, host_id: &str) -> AppResult<()> {
        let (store, key) = (self.secrets.clone(), host_id.to_string());
        keychain(move || store.delete(&key)).await
    }

    /// 호스트가 삭제되면 연결 풀에서도 잊는다.
    pub fn forget_host(&self, host_id: &str) {
        self.pool.forget(host_id);
    }
}

/// 방금 접속에 성공한 비밀번호(또는 개인키 패스프레이즈)를 OS 보안 저장소에 저장한다.
///
/// 사용자가 입력 창에서 "저장"을 선택했을 때만 호출한다. 이후에는 묻지 않고 이 값으로 접속한다.
/// 저장소를 쓸 수 없는 환경이면 `io` 에러를 돌려준다.
#[tauri::command]
#[specta::specta]
pub async fn ssh_save_secret(
    state: State<'_, AppState>,
    host_id: String,
    secret: String,
) -> AppResult<()> {
    // 없는 호스트의 값을 남기지 않도록 먼저 확인한다.
    state.hosts.lock()?.get(&host_id)?;
    state.ssh.save_secret(&host_id, secret).await
}
