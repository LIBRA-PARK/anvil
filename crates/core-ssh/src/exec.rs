//! 원격 명령 실행 (PTY 없이, 출력을 모아서 돌려준다).

use crate::connection::Connection;
use crate::SshError;
use russh::ChannelMsg;

/// 원격 명령의 실행 결과
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// 종료 코드. 시그널로 죽었거나 연결이 끊겼다면 `None`.
    pub code: Option<u32>,
}

impl ExecOutput {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }

    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).trim().to_string()
    }
}

impl Connection {
    /// 명령을 서버의 로그인 셸로 실행하고 끝날 때까지 기다린다.
    ///
    /// `command`는 셸이 해석하므로, 경로 같은 인자는 반드시 [`shell_quote`]로 감싼다.
    pub async fn exec(&self, command: &str) -> Result<ExecOutput, SshError> {
        let mut channel = self.handle.channel_open_session().await?;
        channel.exec(true, command).await?;

        let mut output = ExecOutput {
            stdout: Vec::new(),
            stderr: Vec::new(),
            code: None,
        };
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) => output.stdout.extend_from_slice(&data),
                Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                    output.stderr.extend_from_slice(&data);
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => output.code = Some(exit_status),
                Some(ChannelMsg::Failure) => {
                    return Err(SshError::Protocol("서버가 명령 실행을 거부했습니다".into()));
                }
                Some(ChannelMsg::Close) | None => break,
                Some(_) => {}
            }
        }
        if output.code.is_none() && self.is_closed() {
            return Err(SshError::Closed);
        }
        Ok(output)
    }
}

/// 문자열을 POSIX 셸에서 인자 하나로 안전하게 쓸 수 있도록 작은따옴표로 감싼다.
pub fn shell_quote(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::shell_quote;

    #[test]
    fn 작은따옴표와_특수문자를_안전하게_감싼다() {
        assert_eq!(shell_quote("plain"), "'plain'");
        assert_eq!(shell_quote("a b; rm -rf $HOME"), "'a b; rm -rf $HOME'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote(""), "''");
    }
}
