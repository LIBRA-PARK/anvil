//! 호스트별 연결 공유.

use crate::connection::{connect, ConnectParams, Connection};
use crate::host_key::HostKeyPolicy;
use crate::SshError;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

type Slot = Arc<tokio::sync::Mutex<Weak<Connection>>>;

/// 키(보통 호스트 id)별로 연결을 하나만 유지한다.
///
/// 풀은 연결을 약한 참조로만 들고 있다. 그 연결을 쓰는 셸이 모두 닫히면
/// 연결도 함께 끊기고, 다음 요청 때 새로 접속한다.
#[derive(Default)]
pub struct SshPool {
    slots: Mutex<HashMap<String, Slot>>,
}

impl SshPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// 살아 있는 연결이 있으면 재사용하고, 없으면 새로 접속한다.
    ///
    /// 같은 키에 대한 동시 요청은 한 번만 접속하도록 줄을 세운다.
    /// (다른 키끼리는 서로 기다리지 않는다)
    pub async fn get_or_connect(
        &self,
        key: &str,
        params: &ConnectParams,
        policy: &HostKeyPolicy,
    ) -> Result<Arc<Connection>, SshError> {
        let slot = {
            let mut slots = self.slots.lock().unwrap_or_else(|e| e.into_inner());
            Arc::clone(slots.entry(key.to_string()).or_default())
        };

        let mut current = slot.lock().await;
        if let Some(connection) = current.upgrade() {
            if !connection.is_closed() {
                return Ok(connection);
            }
        }
        let connection = Arc::new(connect(params, policy).await?);
        *current = Arc::downgrade(&connection);
        Ok(connection)
    }

    /// 키에 대한 기록을 지운다. (호스트 삭제 시)
    /// 이미 열려 있는 셸은 각자 닫힐 때까지 유지된다.
    pub fn forget(&self, key: &str) {
        self.slots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(key);
    }
}
