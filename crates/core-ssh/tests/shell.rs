//! 프로세스 안에 띄운 russh 서버를 상대로 접속·인증·호스트 키 검증·셸을 검증한다.
//! 실제 sshd 없이 모든 OS의 CI에서 돌아간다.

use core_ssh::{connect, Auth, ConnectParams, HostKeyPolicy, ShellEvent, SshError, SshPool};
use russh::keys::{decode_secret_key, PublicKey};
use russh::server::{self, Auth as ServerAuth, Msg, Server as _, Session, VerifiedPublicKey};
use russh::{Channel, ChannelId, Pty};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const USER: &str = "tester";
const PASSWORD: &str = "correct-horse";
const SERVER_KEY: &str = include_str!("fixtures/server_key");
const OTHER_SERVER_KEY: &str = include_str!("fixtures/client_key");
const CLIENT_PUBLIC_KEY: &str = include_str!("fixtures/client_key.pub");

fn fixture(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

// ── 테스트용 서버: 입력을 그대로 되돌려 주는 가짜 셸 ──

#[derive(Clone)]
struct TestServer {
    client_key: PublicKey,
}

impl server::Server for TestServer {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        self.clone()
    }
}

impl server::Handler for TestServer {
    type Error = russh::Error;

    async fn auth_password(
        &mut self,
        user: &str,
        password: &str,
    ) -> Result<ServerAuth, Self::Error> {
        Ok(if user == USER && password == PASSWORD {
            ServerAuth::Accept
        } else {
            ServerAuth::reject()
        })
    }

    async fn auth_publickey(
        &mut self,
        user: &str,
        key: VerifiedPublicKey<'_>,
    ) -> Result<ServerAuth, Self::Error> {
        Ok(
            if user == USER && key.public_key.key_data() == self.client_key.key_data() {
                ServerAuth::Accept
            } else {
                ServerAuth::reject()
            },
        )
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        term: &str,
        cols: u32,
        rows: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.data(
            channel,
            format!("pty {term} {cols}x{rows}\r\n").into_bytes(),
        )?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        session.data(channel, b"ready\r\n".to_vec())?;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        command: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        // 명령을 그대로 stdout으로, 고정 문구를 stderr로 돌려주고 종료 코드 3으로 끝낸다.
        session.data(channel, [b"ran:", command].concat())?;
        session.extended_data(channel, 1, b"warn".to_vec())?;
        session.exit_status_request(channel, 3)?;
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        cols: u32,
        rows: u32,
        _pix_width: u32,
        _pix_height: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.data(channel, format!("resized {cols}x{rows}\r\n").into_bytes())?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if data == b"exit\n" {
            session.exit_status_request(channel, 7)?;
            session.close(channel)?;
        } else {
            session.data(channel, [b"echo:", data].concat())?;
        }
        Ok(())
    }
}

/// 서버를 임의 포트에 띄우고 포트 번호를 돌려준다.
async fn start_server(host_key: &str) -> u16 {
    let config = Arc::new(server::Config {
        keys: vec![decode_secret_key(host_key, None).unwrap()],
        auth_rejection_time: Duration::from_millis(10),
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut server = TestServer {
        client_key: PublicKey::from_openssh(CLIENT_PUBLIC_KEY).unwrap(),
    };
    tokio::spawn(async move {
        let _ = server.run_on_socket(config, &listener).await;
    });
    port
}

// ── 클라이언트 쪽 도우미 ──

struct Client {
    port: u16,
    dir: PathBuf,
}

impl Client {
    fn new(port: u16) -> Self {
        let dir =
            std::env::temp_dir().join(format!("anvil-ssh-test-{}-{port}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self { port, dir }
    }

    fn params(&self, auth: Auth) -> ConnectParams {
        ConnectParams {
            address: "127.0.0.1".into(),
            port: self.port,
            username: USER.into(),
            auth,
        }
    }

    fn policy(&self, trusted: Option<String>) -> HostKeyPolicy {
        HostKeyPolicy {
            known_hosts: self.dir.join("known_hosts"),
            extra_known_hosts: vec![],
            trusted_fingerprint: trusted,
        }
    }

    /// 처음 접속해 지문을 받아 온다. (사용자가 확인 창에서 보게 되는 값)
    async fn fingerprint(&self) -> String {
        match connect(&self.params(password()), &self.policy(None)).await {
            Err(SshError::HostKeyUnknown { fingerprint }) => fingerprint,
            other => panic!("HostKeyUnknown을 기대했지만: {:?}", other.err()),
        }
    }

    /// 호스트 키를 신뢰한 상태로 만들어 둔다.
    async fn trust(&self) {
        let fingerprint = self.fingerprint().await;
        connect(&self.params(password()), &self.policy(Some(fingerprint)))
            .await
            .unwrap();
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

fn password() -> Auth {
    Auth::Password(Some(PASSWORD.into()))
}

fn key(name: &str, passphrase: Option<&str>) -> Auth {
    Auth::Key {
        path: fixture(name),
        passphrase: passphrase.map(Into::into),
    }
}

/// 출력을 모으다가 `needle`이 나타나면 그때까지의 내용을 돌려준다.
async fn read_until(rx: &mut mpsc::UnboundedReceiver<ShellEvent>, needle: &str) -> String {
    let mut seen = String::new();
    let wait = async {
        while let Some(event) = rx.recv().await {
            match event {
                ShellEvent::Data(data) => seen.push_str(&String::from_utf8_lossy(&data)),
                ShellEvent::Exit { .. } => break,
            }
            if seen.contains(needle) {
                return;
            }
        }
        panic!("'{needle}'이 나오기 전에 셸이 끝남");
    };
    if tokio::time::timeout(Duration::from_secs(5), wait)
        .await
        .is_err()
    {
        panic!("'{needle}'을 기다리다 시간 초과. 지금까지: {seen:?}");
    }
    seen
}

async fn wait_exit(rx: &mut mpsc::UnboundedReceiver<ShellEvent>) -> Option<u32> {
    let wait = async {
        loop {
            match rx.recv().await {
                Some(ShellEvent::Exit { code }) => return code,
                Some(ShellEvent::Data(_)) => {}
                None => panic!("Exit 이벤트 없이 끝남"),
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(5), wait)
        .await
        .expect("Exit 이벤트를 기다리다 시간 초과")
}

// ── 테스트 ──

#[tokio::test]
async fn 처음_보는_호스트는_지문을_돌려주고_신뢰하면_저장한다() {
    let client = Client::new(start_server(SERVER_KEY).await);

    let fingerprint = client.fingerprint().await;
    assert!(
        fingerprint.starts_with("ssh-ed25519 SHA256:"),
        "{fingerprint}"
    );

    // 다른 지문을 신뢰한다고 해서는 통과하지 못한다.
    let wrong = client.policy(Some("ssh-ed25519 SHA256:wrong".into()));
    assert!(matches!(
        connect(&client.params(password()), &wrong).await,
        Err(SshError::HostKeyUnknown { .. })
    ));

    // 올바른 지문을 신뢰하면 접속되고, 이후에는 묻지 않는다.
    let trusted = client.policy(Some(fingerprint));
    connect(&client.params(password()), &trusted).await.unwrap();
    connect(&client.params(password()), &client.policy(None))
        .await
        .unwrap();
}

#[tokio::test]
async fn 호스트_키가_바뀌면_신뢰_여부와_상관없이_거부한다() {
    let client = Client::new(start_server(SERVER_KEY).await);
    client.trust().await;

    // 같은 주소·포트로 기록된 키와 다른 키를 내놓는 서버를 흉내 낸다.
    let known_hosts = client.dir.join("known_hosts");
    let recorded = std::fs::read_to_string(&known_hosts).unwrap();
    let impostor = Client::new(start_server(OTHER_SERVER_KEY).await);
    let rewritten = recorded.replace(&client.port.to_string(), &impostor.port.to_string());
    std::fs::create_dir_all(&impostor.dir).unwrap();
    std::fs::write(impostor.dir.join("known_hosts"), rewritten).unwrap();

    let fingerprint = match connect(&impostor.params(password()), &impostor.policy(None)).await {
        Err(SshError::HostKeyMismatch { fingerprint, .. }) => fingerprint,
        other => panic!("HostKeyMismatch를 기대했지만: {:?}", other.err()),
    };
    // 사용자가 그 지문을 "신뢰"한다고 해도 불일치는 넘어가지 않는다.
    assert!(matches!(
        connect(
            &impostor.params(password()),
            &impostor.policy(Some(fingerprint))
        )
        .await,
        Err(SshError::HostKeyMismatch { .. })
    ));
}

#[tokio::test]
async fn 읽기_전용_known_hosts에_있는_호스트는_묻지_않는다() {
    let client = Client::new(start_server(SERVER_KEY).await);
    client.trust().await;

    // 방금 기록된 파일을 ~/.ssh/known_hosts 역할로 쓰고, 앱 자체 파일은 비워 둔다.
    let policy = HostKeyPolicy {
        known_hosts: client.dir.join("empty"),
        extra_known_hosts: vec![client.dir.join("known_hosts")],
        trusted_fingerprint: None,
    };
    connect(&client.params(password()), &policy).await.unwrap();
    assert!(
        !client.dir.join("empty").exists(),
        "읽기 전용 경로만으로 통과했으면 기록하지 않는다"
    );
}

#[tokio::test]
async fn 비밀번호_인증() {
    let client = Client::new(start_server(SERVER_KEY).await);
    client.trust().await;
    let policy = client.policy(None);

    assert!(matches!(
        connect(&client.params(Auth::Password(None)), &policy).await,
        Err(SshError::PasswordRequired)
    ));
    assert!(matches!(
        connect(
            &client.params(Auth::Password(Some("wrong".into()))),
            &policy
        )
        .await,
        Err(SshError::AuthFailed(_))
    ));
    connect(&client.params(password()), &policy).await.unwrap();
}

#[tokio::test]
async fn 개인키_인증() {
    let client = Client::new(start_server(SERVER_KEY).await);
    client.trust().await;
    let policy = client.policy(None);

    connect(&client.params(key("client_key", None)), &policy)
        .await
        .unwrap();

    // 서버가 모르는 키
    assert!(matches!(
        connect(&client.params(key("server_key", None)), &policy).await,
        Err(SshError::AuthFailed(_))
    ));
    // 없는 파일
    assert!(matches!(
        connect(&client.params(key("no_such_key", None)), &policy).await,
        Err(SshError::Key(_))
    ));
}

#[tokio::test]
async fn 암호화된_개인키는_패스프레이즈를_요구한다() {
    let client = Client::new(start_server(SERVER_KEY).await);
    client.trust().await;
    let policy = client.policy(None);
    let attempt =
        |passphrase| connect_err(&client, key("client_key_encrypted", passphrase), &policy);

    assert!(matches!(
        attempt(None).await,
        SshError::PassphraseRequired(_)
    ));
    assert!(matches!(
        attempt(Some("wrong")).await,
        SshError::PassphraseRequired(_)
    ));
    // 패스프레이즈가 맞으면 키는 풀리지만, 서버에 등록된 키가 아니므로 인증에서 거부된다.
    assert!(matches!(
        attempt(Some("secret-pass")).await,
        SshError::AuthFailed(_)
    ));
}

async fn connect_err(client: &Client, auth: Auth, policy: &HostKeyPolicy) -> SshError {
    match connect(&client.params(auth), policy).await {
        Err(e) => e,
        Ok(_) => panic!("실패를 기대했지만 접속됨"),
    }
}

#[tokio::test]
async fn 접속할_수_없는_포트는_연결_에러() {
    // 포트를 잡았다가 바로 놓아서 아무도 듣지 않는 포트를 얻는다.
    let port = {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.local_addr().unwrap().port()
    };
    let client = Client::new(port);
    assert!(matches!(
        connect(&client.params(password()), &client.policy(None)).await,
        Err(SshError::Connect(_))
    ));
}

#[tokio::test]
async fn 셸_입출력과_크기_변경_종료_코드() {
    let client = Client::new(start_server(SERVER_KEY).await);
    client.trust().await;
    let connection = Arc::new(
        connect(&client.params(password()), &client.policy(None))
            .await
            .unwrap(),
    );

    let (tx, mut rx) = mpsc::unbounded_channel();
    let shell = connection
        .open_shell(120, 40, move |event| {
            let _ = tx.send(event);
        })
        .await
        .unwrap();

    let greeting = read_until(&mut rx, "ready").await;
    assert!(
        greeting.contains("pty xterm-256color 120x40"),
        "{greeting:?}"
    );

    shell.write("한글 ✓".as_bytes().to_vec()).unwrap();
    read_until(&mut rx, "echo:한글 ✓").await;

    shell.resize(80, 24).unwrap();
    read_until(&mut rx, "resized 80x24").await;

    shell.write(b"exit\n".to_vec()).unwrap();
    assert_eq!(wait_exit(&mut rx).await, Some(7));
    assert!(
        rx.recv().await.is_none(),
        "Exit 뒤에는 이벤트가 없어야 한다"
    );
}

#[tokio::test]
async fn 셸_핸들을_버리면_종료_이벤트가_온다() {
    let client = Client::new(start_server(SERVER_KEY).await);
    client.trust().await;
    let connection = Arc::new(
        connect(&client.params(password()), &client.policy(None))
            .await
            .unwrap(),
    );

    let (tx, mut rx) = mpsc::unbounded_channel();
    let shell = connection
        .open_shell(80, 24, move |event| {
            let _ = tx.send(event);
        })
        .await
        .unwrap();
    read_until(&mut rx, "ready").await;

    drop(shell);
    assert_eq!(wait_exit(&mut rx).await, None);
}

#[tokio::test]
async fn 풀은_연결을_공유하고_셸이_모두_닫히면_다시_접속한다() {
    let client = Client::new(start_server(SERVER_KEY).await);
    client.trust().await;
    let pool = SshPool::new();
    let params = client.params(password());
    let policy = client.policy(None);

    let first = pool.get_or_connect("host", &params, &policy).await.unwrap();
    // 연결이 살아 있으면 비밀번호 없이도 재사용된다.
    let no_password = client.params(Auth::Password(None));
    let second = pool
        .get_or_connect("host", &no_password, &policy)
        .await
        .unwrap();
    assert!(Arc::ptr_eq(&first, &second));

    // 한 연결에서 셸 두 개가 독립적으로 동작한다.
    let (tx_a, mut rx_a) = mpsc::unbounded_channel();
    let (tx_b, mut rx_b) = mpsc::unbounded_channel();
    let shell_a = first
        .open_shell(80, 24, move |e| drop(tx_a.send(e)))
        .await
        .unwrap();
    let shell_b = second
        .open_shell(80, 24, move |e| drop(tx_b.send(e)))
        .await
        .unwrap();
    shell_a.write(b"a".to_vec()).unwrap();
    shell_b.write(b"b".to_vec()).unwrap();
    let a = read_until(&mut rx_a, "echo:a").await;
    let b = read_until(&mut rx_b, "echo:b").await;
    assert!(!a.contains("echo:b") && !b.contains("echo:a"));

    // 연결을 쓰는 곳이 모두 사라지면 풀도 놓아주고, 다음에는 새로 접속해야 한다.
    drop((shell_a, shell_b, first, second));
    wait_exit(&mut rx_a).await;
    wait_exit(&mut rx_b).await;
    // 셸 태스크가 연결 참조를 내려놓을 시간을 준다.
    let mut result = pool.get_or_connect("host", &no_password, &policy).await;
    for _ in 0..50 {
        if result.is_err() {
            break;
        }
        drop(result);
        tokio::time::sleep(Duration::from_millis(20)).await;
        result = pool.get_or_connect("host", &no_password, &policy).await;
    }
    assert!(matches!(result, Err(SshError::PasswordRequired)));
}

#[tokio::test]
async fn 명령_실행은_출력과_종료_코드를_모아_돌려준다() {
    let client = Client::new(start_server(SERVER_KEY).await);
    client.trust().await;
    let connection = connect(&client.params(password()), &client.policy(None))
        .await
        .unwrap();

    let output = connection.exec("git status").await.unwrap();
    assert_eq!(output.stdout, b"ran:git status");
    assert_eq!(output.stderr_text(), "warn");
    assert_eq!(output.code, Some(3));
    assert!(!output.success());
}
