//! 비밀번호·패스프레이즈를 OS 보안 저장소에 보관한다.
//!
//! - macOS: 키체인
//! - Windows: 자격 증명 관리자
//! - Linux: Secret Service (GNOME Keyring, KWallet 등)
//!
//! `hosts.json` 같은 평문 파일에는 비밀 값을 절대 쓰지 않는다.
//! 모든 호출은 OS에 요청을 보내느라 잠깐 멈출 수 있으므로, 비동기 코드에서는 별도 스레드에서 부른다.

use keyring::Entry;

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// 보안 저장소가 없거나 잠겨 있음 (예: Secret Service가 없는 Linux 서버)
    #[error("OS 보안 저장소를 사용할 수 없습니다: {0}")]
    Unavailable(String),
}

pub type Result<T> = std::result::Result<T, SecretError>;

/// 한 앱(서비스 이름)에 속한 비밀 값들의 저장소
#[derive(Debug, Clone)]
pub struct SecretStore {
    service: String,
}

impl SecretStore {
    /// `service`는 보안 저장소에서 이 앱의 항목을 구분하는 이름이다.
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, key: &str) -> Result<Entry> {
        Entry::new(&self.service, key).map_err(unavailable)
    }

    /// 저장된 값을 읽는다. 없으면 `None`.
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        match self.entry(key)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(unavailable(e)),
        }
    }

    /// 값을 저장한다. 이미 있으면 바꾼다.
    pub fn set(&self, key: &str, secret: &str) -> Result<()> {
        self.entry(key)?.set_password(secret).map_err(unavailable)
    }

    /// 값을 지운다. 없어도 성공으로 본다.
    pub fn delete(&self, key: &str) -> Result<()> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(unavailable(e)),
        }
    }
}

fn unavailable(e: keyring::Error) -> SecretError {
    SecretError::Unavailable(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 실제 OS 보안 저장소를 건드리므로 평소에는 돌리지 않는다.
    /// (CI의 Linux에는 Secret Service가 없고, macOS는 권한 창이 뜰 수 있다)
    /// `cargo test -p core-secrets -- --ignored`로 직접 실행한다.
    #[test]
    #[ignore = "실제 OS 보안 저장소가 필요함"]
    fn 저장하고_읽고_지운다() {
        let store = SecretStore::new("dev.anvil.app.test");
        let key = "core-secrets-test";

        store.delete(key).unwrap();
        assert_eq!(store.get(key).unwrap(), None);

        store.set(key, "첫 값").unwrap();
        store.set(key, "바꾼 값 🔑").unwrap();
        assert_eq!(store.get(key).unwrap().as_deref(), Some("바꾼 값 🔑"));

        store.delete(key).unwrap();
        store.delete(key).unwrap();
        assert_eq!(store.get(key).unwrap(), None);
    }
}
