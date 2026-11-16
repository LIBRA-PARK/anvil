//! 호스트 목록 등 앱 설정을 관리하는 크레이트.
//! Tauri에 의존하지 않도록 저장 경로는 외부에서 주입받는다.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// SSH 인증 방식
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AuthMethod {
    /// 비밀번호 (비밀번호 자체는 OS 키체인에 저장 예정)
    Password,
    /// 개인키 파일
    Key { path: String },
    /// ssh-agent 사용
    Agent,
}

/// 저장된 원격 호스트
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    pub id: String,
    pub name: String,
    pub address: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
}

/// 새 호스트 생성 요청 (id는 서버에서 발급)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewHost {
    pub name: String,
    pub address: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("설정 파일 입출력 실패: {0}")]
    Io(#[from] std::io::Error),
    #[error("설정 파일 형식 오류: {0}")]
    Json(#[from] serde_json::Error),
    #[error("호스트를 찾을 수 없음: {0}")]
    NotFound(String),
    #[error("잘못된 입력: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, ConfigError>;

/// JSON 파일 기반 호스트 저장소
#[derive(Debug)]
pub struct HostStore {
    path: PathBuf,
    hosts: Vec<Host>,
}

impl HostStore {
    /// 파일에서 불러오기. 파일이 없으면 빈 저장소로 시작한다.
    pub fn load(path: PathBuf) -> Result<Self> {
        let hosts = if path.exists() {
            serde_json::from_str(&fs::read_to_string(&path)?)?
        } else {
            Vec::new()
        };
        Ok(Self { path, hosts })
    }

    pub fn list(&self) -> &[Host] {
        &self.hosts
    }

    pub fn add(&mut self, input: NewHost) -> Result<Host> {
        if input.name.trim().is_empty() || input.address.trim().is_empty() {
            return Err(ConfigError::Invalid("이름과 주소는 필수입니다".into()));
        }
        if input.username.trim().is_empty() {
            return Err(ConfigError::Invalid("사용자 이름은 필수입니다".into()));
        }
        let host = Host {
            id: uuid::Uuid::new_v4().to_string(),
            name: input.name.trim().to_string(),
            address: input.address.trim().to_string(),
            port: input.port,
            username: input.username.trim().to_string(),
            auth: input.auth,
        };
        self.hosts.push(host.clone());
        self.save()?;
        Ok(host)
    }

    pub fn remove(&mut self, id: &str) -> Result<()> {
        let before = self.hosts.len();
        self.hosts.retain(|h| h.id != id);
        if self.hosts.len() == before {
            return Err(ConfigError::NotFound(id.to_string()));
        }
        self.save()
    }

    /// 임시 파일에 쓴 뒤 교체하여 중간에 실패해도 기존 파일이 깨지지 않게 한다.
    fn save(&self) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(&self.hosts)?)?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> NewHost {
        NewHost {
            name: "prod".into(),
            address: "10.0.0.1".into(),
            port: 22,
            username: "ubuntu".into(),
            auth: AuthMethod::Key { path: "~/.ssh/id_ed25519".into() },
        }
    }

    #[test]
    fn 추가_삭제_후_다시_불러오기() {
        let dir = std::env::temp_dir().join(format!("anvil-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("hosts.json");

        let mut store = HostStore::load(path.clone()).unwrap();
        let host = store.add(sample()).unwrap();
        assert_eq!(HostStore::load(path.clone()).unwrap().list(), &[host.clone()]);

        store.remove(&host.id).unwrap();
        assert!(HostStore::load(path).unwrap().list().is_empty());

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn 빈_이름은_거부() {
        let path = std::env::temp_dir().join(format!("anvil-{}.json", uuid::Uuid::new_v4()));
        let mut store = HostStore::load(path).unwrap();
        let mut input = sample();
        input.name = "  ".into();
        assert!(matches!(store.add(input), Err(ConfigError::Invalid(_))));
    }
}
