//! 터미널과 파일 탐색기가 함께 쓰는 SSH 접속 창구.

use crate::error::AppResult;
use core_config::{AuthMethod, Host};
use core_ssh::{Auth, ConnectParams, Connection, HostKeyPolicy, SshPool};
use std::path::PathBuf;
use std::sync::Arc;

/// 사용자가 접속 과정에서 입력·확인한 값. 어디에도 저장하지 않는다.
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
}

impl SshContext {
    pub fn new(known_hosts: PathBuf, extra_known_hosts: Vec<PathBuf>) -> Self {
        Self {
            pool: SshPool::new(),
            known_hosts,
            extra_known_hosts,
        }
    }

    /// 호스트에 접속한다. 이미 살아 있는 연결이 있으면 재사용한다.
    pub async fn connect(
        &self,
        host: &Host,
        credentials: Credentials,
    ) -> AppResult<Arc<Connection>> {
        // 빈 문자열은 입력하지 않은 것으로 본다.
        let secret = credentials.secret.filter(|s| !s.is_empty());
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
            trusted_fingerprint: credentials.trusted_host_key,
        };
        Ok(self.pool.get_or_connect(&host.id, &params, &policy).await?)
    }

    /// 호스트가 삭제되면 연결 풀에서도 잊는다.
    pub fn forget_host(&self, host_id: &str) {
        self.pool.forget(host_id);
    }
}
