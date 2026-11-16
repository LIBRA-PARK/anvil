//! 서버 호스트 키 검증 (known_hosts + 최초 접속 시 사용자 확인).

use crate::SshError;
use russh::keys::known_hosts::{check_known_hosts_path, learn_known_hosts_path};
use russh::keys::{HashAlg, PublicKey};
use std::path::PathBuf;

/// 호스트 키를 어떻게 검증할지 정한다.
#[derive(Debug, Clone, Default)]
pub struct HostKeyPolicy {
    /// Anvil이 관리하는 known_hosts. 새로 신뢰한 키는 여기에만 기록한다.
    pub known_hosts: PathBuf,
    /// 읽기만 하는 추가 known_hosts (보통 `~/.ssh/known_hosts`).
    /// OpenSSH로 이미 접속해 본 서버는 다시 묻지 않기 위한 것이다.
    pub extra_known_hosts: Vec<PathBuf>,
    /// 사용자가 방금 확인하고 신뢰한 지문.
    /// 서버가 내놓은 키의 지문과 정확히 같을 때만 받아들이고 `known_hosts`에 기록한다.
    pub trusted_fingerprint: Option<String>,
}

/// 사용자에게 보여 주고, 신뢰 여부를 되돌려 받을 때 쓰는 지문 문자열.
pub(crate) fn fingerprint(key: &PublicKey) -> String {
    format!("{} {}", key.algorithm(), key.fingerprint(HashAlg::Sha256))
}

impl HostKeyPolicy {
    pub(crate) fn verify(&self, host: &str, port: u16, key: &PublicKey) -> Result<(), SshError> {
        let fingerprint = fingerprint(key);

        let own = std::iter::once((&self.known_hosts, true));
        let extra = self.extra_known_hosts.iter().map(|p| (p, false));
        for (path, is_own) in own.chain(extra) {
            match check_known_hosts_path(host, port, key, path) {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(russh::keys::Error::KeyChanged { .. }) => {
                    return Err(SshError::HostKeyMismatch {
                        fingerprint,
                        path: path.display().to_string(),
                    });
                }
                // 우리 파일이 깨졌다면 알려야 하지만, 남의 파일(~/.ssh/known_hosts)에
                // 해석 못 하는 줄이 있다고 접속을 막을 이유는 없다.
                Err(e) if is_own => {
                    return Err(SshError::Protocol(format!(
                        "known_hosts를 읽을 수 없습니다 ({}): {e}",
                        path.display()
                    )));
                }
                Err(_) => {}
            }
        }

        if self.trusted_fingerprint.as_deref() == Some(fingerprint.as_str()) {
            learn_known_hosts_path(host, port, key, &self.known_hosts)
                .map_err(|e| SshError::Protocol(format!("호스트 키를 저장할 수 없습니다: {e}")))?;
            return Ok(());
        }
        Err(SshError::HostKeyUnknown { fingerprint })
    }
}
