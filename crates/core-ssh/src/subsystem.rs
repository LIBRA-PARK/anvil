//! SSH 서브시스템(SFTP 등)에 붙는 바이트 스트림.

use crate::connection::Connection;
use crate::SshError;
use russh::client::Msg;
use russh::ChannelStream;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// 서브시스템 채널의 양방향 스트림.
///
/// 스트림이 살아 있는 동안 연결도 유지되고, drop하면 채널이 닫힌다.
pub struct SubsystemStream {
    stream: ChannelStream<Msg>,
    connection: Arc<Connection>,
}

impl SubsystemStream {
    /// 이 스트림이 올라탄 연결이 끊겼는지
    pub fn is_closed(&self) -> bool {
        self.connection.is_closed()
    }

    /// 스트림을 넘겨준 뒤에도 연결 상태를 확인할 수 있도록 연결 참조를 돌려준다.
    pub fn connection(&self) -> Arc<Connection> {
        Arc::clone(&self.connection)
    }
}

impl Connection {
    /// 새 채널에서 서브시스템을 시작한다. (예: `"sftp"`)
    pub async fn open_subsystem(self: &Arc<Self>, name: &str) -> Result<SubsystemStream, SshError> {
        let channel = self.handle.channel_open_session().await?;
        channel.request_subsystem(true, name).await?;
        Ok(SubsystemStream {
            stream: channel.into_stream(),
            connection: Arc::clone(self),
        })
    }
}

impl AsyncRead for SubsystemStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for SubsystemStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
