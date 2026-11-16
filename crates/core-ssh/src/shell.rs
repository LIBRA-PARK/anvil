//! PTY가 붙은 원격 셸.

use crate::connection::Connection;
use crate::SshError;
use russh::ChannelMsg;
use std::sync::Arc;
use tokio::sync::mpsc;

/// 셸에서 나오는 이벤트
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellEvent {
    /// 터미널에 그대로 써야 하는 출력 바이트 (stdout과 stderr가 섞여 있다)
    Data(Vec<u8>),
    /// 셸이 끝났다. 정확히 한 번, 마지막에 전달된다.
    /// 연결이 끊겨서 끝난 경우에는 종료 코드가 없다.
    Exit { code: Option<u32> },
}

enum Input {
    Data(Vec<u8>),
    Resize { cols: u32, rows: u32 },
}

/// 열려 있는 셸의 조작 핸들. drop하면 원격 셸도 닫힌다.
pub struct Shell {
    tx: mpsc::UnboundedSender<Input>,
}

impl Shell {
    /// 키 입력 등을 셸로 보낸다.
    pub fn write(&self, data: Vec<u8>) -> Result<(), SshError> {
        self.tx
            .send(Input::Data(data))
            .map_err(|_| SshError::Closed)
    }

    /// 터미널 크기가 바뀌었음을 알린다.
    pub fn resize(&self, cols: u32, rows: u32) -> Result<(), SshError> {
        self.tx
            .send(Input::Resize { cols, rows })
            .map_err(|_| SshError::Closed)
    }
}

impl Connection {
    /// 새 채널에 PTY를 요청하고 로그인 셸을 띄운다.
    ///
    /// 출력과 종료는 `on_event`로 전달된다. 셸이 살아 있는 동안 연결도 유지된다.
    pub async fn open_shell(
        self: &Arc<Self>,
        cols: u32,
        rows: u32,
        mut on_event: impl FnMut(ShellEvent) + Send + 'static,
    ) -> Result<Shell, SshError> {
        let mut channel = self.handle.channel_open_session().await?;
        channel
            .request_pty(false, "xterm-256color", cols, rows, 0, 0, &[])
            .await?;
        channel.request_shell(true).await?;

        // 셸 요청에 대한 응답을 기다린다. (그 전에 온 출력은 버리지 않고 전달)
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Success) => break,
                Some(ChannelMsg::Failure) => {
                    return Err(SshError::Protocol("서버가 셸 요청을 거부했습니다".into()));
                }
                Some(ChannelMsg::Data { data }) => on_event(ShellEvent::Data(data.to_vec())),
                Some(ChannelMsg::Close) | None => return Err(SshError::Closed),
                Some(_) => {}
            }
        }

        let (tx, mut rx) = mpsc::unbounded_channel();
        let connection = Arc::clone(self);
        tokio::spawn(async move {
            // 셸이 끝날 때까지 연결을 붙잡아 둔다.
            let _connection = connection;
            let mut code = None;
            loop {
                tokio::select! {
                    msg = channel.wait() => match msg {
                        Some(ChannelMsg::Data { data })
                        | Some(ChannelMsg::ExtendedData { data, .. }) => {
                            on_event(ShellEvent::Data(data.to_vec()));
                        }
                        Some(ChannelMsg::ExitStatus { exit_status }) => code = Some(exit_status),
                        Some(ChannelMsg::Close) | None => break,
                        Some(_) => {}
                    },
                    input = rx.recv() => match input {
                        Some(Input::Data(data)) => {
                            if channel.data_bytes(data).await.is_err() {
                                break;
                            }
                        }
                        Some(Input::Resize { cols, rows }) => {
                            let _ = channel.window_change(cols, rows, 0, 0).await;
                        }
                        // Shell 핸들이 drop됨 → 사용자가 탭을 닫은 것
                        None => {
                            let _ = channel.eof().await;
                            let _ = channel.close().await;
                            break;
                        }
                    },
                }
            }
            on_event(ShellEvent::Exit { code });
        });

        Ok(Shell { tx })
    }
}
