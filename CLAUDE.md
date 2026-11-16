# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 프로젝트 개요

Anvil은 SSH 터미널 · SFTP 파일 탐색 · Git Diff · 코드 에디터를 하나로 묶은 **개인용 크로스플랫폼(Windows / macOS / Linux) 데스크톱 개발 도구**다. Rust + Tauri 2 백엔드, React 18 + TypeScript 프론트엔드(Vite).

UI는 Discord식 3단 레이아웃이다. 협업/채팅 기능은 없고 **레이아웃만** 차용한다.

- 왼쪽 아이콘 바(`HostBar`): Discord "서버" 자리에 SSH 호스트. 💻는 로컬 머신(`LOCAL_ID = "local"`).
- 채널 사이드바(`ChannelSidebar`): 선택한 호스트의 기능 채널(터미널 / 파일 / Git).
- 메인 패널(`MainPanel`): 탭으로 터미널·에디터·Diff 전환. 탭 id는 `${hostId}:${kind}`이며 같은 id면 새로 열지 않고 포커스한다. 예외는 둘이다. 터미널은 한 호스트에 여러 개 열 수 있어 `${hostId}:terminal:${번호}`, 에디터는 파일마다 하나라 `${hostId}:editor:${경로}`를 쓴다.
- 탭은 모두 마운트해 두고 `hidden`으로 숨기기만 한다(터미널 세션, 에디터의 수정 내용 유지). 탭을 닫으면 언마운트되면서 세션·에디터가 정리된다.
- 로컬 머신(💻)과 원격 호스트는 터미널·파일·에디터·Diff 모두 같은 컴포넌트를 쓴다. 컴포넌트는 `host: Host | null`(null이면 로컬)을 받는다.

## 로드맵 (M1~M6 구현됨)

| 단계 | 내용                                                 | 구현                                                                         |
| ---- | ---------------------------------------------------- | ---------------------------------------------------------------------------- |
| M1   | 뼈대, 레이아웃, 호스트 추가/삭제/저장                | `crates/core-config`                                                         |
| M2   | SSH 접속 + 다중 세션 터미널 (로컬 셸 포함)           | `crates/core-ssh`(russh), `crates/core-pty`(portable-pty), 프론트 `xterm.js` |
| M3   | 파일 트리 (로컬 + SFTP), 새 폴더·이름 변경·삭제      | `crates/core-fs`(russh-sftp)                                                 |
| M4   | 코드 에디터 (원격 파일 열기 / 저장 시 SFTP 업로드)   | Monaco Editor                                                                |
| M5   | Git Diff (로컬 + 원격)                               | `crates/core-git`(로컬 git2, 원격은 SSH로 `git` 실행), 뷰어는 Monaco Diff    |
| M6   | 릴리스 워크플로(설치 파일), 코드 서명, 자동 업데이트 | `.github/workflows/release.yml`, `tauri-plugin-updater`                      |

아직 없는 것: keyboard-interactive 인증(OTP 등), 터미널 출력 폭주 시 묶어 보내기, 파일 업로드/다운로드, 숨김 파일 토글, Windows 로컬 드라이브 목록, 에디터의 외부 변경 감지(저장 시 덮어씀), 창을 닫을 때 저장하지 않은 파일 경고.

확정된 설계 방향:

- `core-fs` / `core-git` 크레이트에서 **Local과 Remote를 같은 트레잇으로 추상화**해, 에디터·Diff UI가 대상이 로컬인지 원격인지 몰라도 되게 한다.
- 도메인 로직은 Tauri에 의존하지 않는 `crates/core-*`에 두고, `src-tauri`는 커맨드 연결만 담당한다.
- 비밀번호는 `hosts.json`에 저장하지 않는다. 사용자가 원할 때만 OS 보안 저장소(`crates/core-secrets`, `keyring` 크레이트)에 저장한다.
- 터미널 출력이나 큰 파일처럼 대량 데이터는 JSON 이벤트가 아니라 `tauri::ipc::Channel`이나 바이너리 응답으로 전달한다(IPC 직렬화 비용).
- Monaco/xterm 인스턴스가 메모리 비용의 대부분이므로 탭은 지연 생성하고 닫을 때 정리한다.

## 명령어

```bash
npm install                 # 최초 1회
npm run tauri dev           # 데스크톱 앱 실행 (Vite dev 서버 1420 포트 자동 기동)
npm run dev                 # 브라우저에서 UI만 미리보기 (Tauri 없이 api.ts 목업으로 동작)
npm run build               # tsc + vite build → dist/
npm run check               # ESLint + Prettier 검사 + tsc (CI와 동일)
npm run format              # Prettier 자동 수정
cargo fmt --all
cargo clippy --workspace --all-targets --locked -- -D warnings   # CI와 동일, 경고도 실패
cargo test --workspace --locked
cargo test -p core-config 추가_삭제   # 단일 테스트 (이름 일부로 필터)
npm run tauri build         # 배포용 번들
```

Tauri 크레이트를 컴파일하기 전에 `dist/`가 있어야 한다. 새로 clone했다면 `npm run build`를 먼저 실행한다(CI도 이 순서로 실행한다).

## 아키텍처와 규칙

### Rust ↔ TS 타입 자동 생성 (tauri-specta)

- `ui/src/bindings.ts`는 **자동 생성 파일이다. 직접 수정하지 않는다.** `npm run tauri dev`(디버그 빌드)와 `cargo test`(`src-tauri/tests/bindings.rs`)가 다시 생성한다.
- CI는 Linux에서 테스트를 돌린 뒤 `git diff --exit-code ui/src/bindings.ts`를 실행한다. Rust 커맨드나 타입을 바꾸면 **재생성된 bindings.ts를 같이 커밋**해야 한다.
- 새 커맨드 추가 순서:
  1. `#[tauri::command]`와 `#[specta::specta]`를 붙인다.
  2. `src-tauri/src/lib.rs`의 `specta_builder()`에 있는 `collect_commands![]`에 등록한다.
  3. 커맨드에 쓰는 타입에 `specta::Type`을 derive한다.
- specta 계열(`specta`, `specta-typescript`, `tauri-specta`)은 RC 버전이라 루트 `Cargo.toml`의 `[workspace.dependencies]`에 `=` 정확 버전으로 고정되어 있다. 버전을 올릴 때는 세 개를 함께 맞춘다.
- 프론트엔드는 `commands.*`를 직접 부르지 않고 `ui/src/api.ts` 래퍼를 거친다. 브라우저 미리보기용 목업 분기(`__TAURI_INTERNALS__` 존재 여부 확인)가 여기에 있다.
- `ui/src/types.ts`는 bindings의 타입을 다시 export하고 UI 전용 타입(`Tab`, `TabKind`, `LOCAL_ID`)만 정의한다.

### SSH 접속 (`crates/core-ssh` + `src-tauri/src/ssh.rs`)

- `SshPool`이 호스트 id별로 연결 하나를 공유한다(약한 참조). 터미널·SFTP·Git이 같은 연결에 채널만 추가하므로 인증은 한 번이고, 쓰는 곳이 모두 사라지면 연결도 끊긴다.
- 원격 접속이 필요한 커맨드는 모두 `secret` / `trusted_host_key`를 받고 `SshContext::connect`를 거친다. 필요한 입력이 없으면 `passwordRequired` / `passphraseRequired` / `hostKeyUnknown` 에러를 돌려주고, UI가 입력을 받아 같은 커맨드를 다시 호출한다.
- UI 쪽 입력 창은 `ConnectPrompt.tsx`에 있다. 새 화면은 `useConnectRetry` 훅(`call(fn)` + `prompt`)을 쓰면 된다. 터미널과 파일 트리는 자체 상태 머신에서 `promptForError`를 직접 쓴다.
- 호스트 키: 새로 신뢰한 키는 앱 설정 폴더의 `known_hosts`에만 기록하고 `~/.ssh/known_hosts`는 읽기만 한다. 처음 보는 키는 `hostKeyUnknown` 에러(`message` = 지문)로 돌려보내고, UI가 사용자 확인 후 같은 지문을 `trustedHostKey`로 넣어 다시 호출한다. 키 불일치(`hostKeyMismatch`)는 우회 수단을 두지 않는다.
- 비밀번호·패스프레이즈: 사용자가 입력 창에서 "저장"을 고르면 접속 성공 후 `ssh_save_secret`으로 OS 보안 저장소에 넣는다(키: 호스트 id, 서비스: `dev.anvil.app`). `connect`는 비밀 값이 필요한데 없을 때만 저장소를 조회하고, 저장된 값이 틀리면 지운 뒤 사용자에게 다시 묻는다. 호스트를 삭제하면 저장된 값도 지운다.
- `Connection::exec`은 명령을 서버의 셸로 실행한다. 경로 같은 인자는 반드시 `shell_quote`로 감싼다.
- `core-ssh` 통합 테스트(`tests/shell.rs`)는 프로세스 안에 russh 서버를 띄워 돌리므로 외부 sshd가 필요 없다. `tests/fixtures/`의 키는 테스트 전용이다.
- russh는 Windows CI에서 NASM/CMake가 필요 없도록 `aws-lc-rs` 대신 `ring` 백엔드를 쓴다.

### 터미널 (`src-tauri/src/terminal.rs`)

- 세션은 `Session::Remote(core_ssh::Shell)` 또는 `Session::Local(core_pty::LocalShell)`이다. `terminal_open`의 `host_id`가 `"local"`이면 로컬 셸을 띄운다. 쓰기·크기 변경·종료는 둘이 같은 경로를 탄다.
- 셸 출력은 `Channel<RawBytes>`(raw 바이트 → JS `ArrayBuffer`)으로 보낸다. JSON으로 보내지 않는다. `RawBytes`(`src-tauri/src/ipc.rs`)는 커맨드 반환값으로도 쓴다(`fs_read`).
- `core-pty` 테스트는 Unix에서만 돈다(ConPTY는 CI 환경에 따라 동작이 달라 제외).

### 파일 (`crates/core-fs` + `src-tauri/src/fs.rs`)

- `FileSystem` 트레잇을 `LocalFs`(로컬 디스크)와 `SftpFs`(원격)가 구현한다. 커맨드는 `with_fs`로 `host_id`에 맞는 구현을 골라 실행한다(`"local"`이면 로컬). 새 파일 기능은 트레잇에 메서드를 추가하고 두 구현을 모두 채운다.
- 경로는 문자열로 주고받고(로컬은 OS 형식, 원격은 POSIX), UI는 경로를 직접 조립하지 않는다. 백엔드가 `DirListing.parent`와 각 항목의 `path`를 내려 주고, 새 폴더·이름 변경은 `(폴더, 이름)`을 받아 백엔드가 잇는다. 이름에 경로 구분자가 있으면 `validate_name`이 거부한다.
- 삭제는 폴더 내용까지 지운다. 심볼릭 링크는 링크만 지우고 대상은 건드리지 않는다(로컬·SFTP 모두). SFTP에는 재귀 삭제가 없어 `SftpFs::remove`가 직접 순회한다.
- 이름 변경은 대상이 이미 있으면 덮어쓰지 않고 `AlreadyExists`로 실패한다.
- `fs_read`는 바이너리로 내려 주고 프론트(`api.readFile`)가 UTF-8로 해석한다. NUL이 있거나 UTF-8이 아니면 열지 않는다(잘못 해석한 채 저장하면 파일이 망가지므로). 에디터용 최대 크기는 10MB.
- 저장(`write_file`)은 임시 파일 교체가 아니라 제자리 쓰기다. 권한·소유자·심볼릭 링크를 유지하기 위해서다.
- SFTP 세션은 호스트당 하나를 `Files`에 캐시한다. 그 호스트의 파일·에디터·Diff 탭이 모두 닫히면 `App.tsx`가 `fs_close`를 호출해 정리한다.
- `u64` 필드는 specta가 BigInt로 내보내려다 실패하므로 `#[specta(type = specta_typescript::Number)]`를 붙인다.
- `SftpFs`와 `RemoteGit`은 자동 테스트가 없다(SSH+SFTP 서버를 프로세스 안에 띄우는 하네스가 아직 없음). 바꿀 때는 실제 sshd로 확인한다.

### 에디터와 Diff (Monaco)

- Monaco는 수 MB라서 `EditorView` / `DiffView`를 `React.lazy`로 불러온다. Monaco를 import하는 코드는 이 두 컴포넌트와 `ui/src/monaco.ts`에만 둔다(다른 곳에서 import하면 시작 번들에 들어간다).
- 워커는 CDN이 아니라 번들에 포함한다(`monaco.ts`의 `?worker` import). 패키지의 exports 패턴을 Vite가 워커 경로에서 해석하지 못해 `node_modules` 경로를 직접 가리킨다.
- 모델 URI는 `fileUri(hostId, path, tag)`로 만든다. 확장자로 언어가 정해지고, 호스트·용도가 다르면 다른 모델이 된다. 모델은 탭이 닫힐 때 직접 dispose해야 한다.
- 에디터의 수정 여부는 Monaco의 `getAlternativeVersionId()`를 저장 시점 값과 비교해 판단한다(되돌리기로 원래 내용이 되면 수정 표시가 사라진다).

### Git (`crates/core-git` + `src-tauri/src/git.rs`)

- `GitRepo` 트레잇을 `LocalGit`(libgit2)과 `RemoteGit`(SSH로 `git` 실행 후 porcelain 출력 해석)이 구현한다.
- 비교 기준은 항상 마지막 커밋(HEAD) ↔ 작업 폴더다. 스테이징 여부는 구분하지 않는다. `core-git`은 HEAD 쪽 내용만 주고, 작업 폴더 쪽은 `core-fs`로 읽는다.
- `git2`는 네트워크 기능 없이(`default-features = false`) 소스를 함께 빌드한다(`vendored-libgit2`). 시스템 libgit2나 OpenSSL에 의존하지 않는다.
- `LocalGit` 테스트는 libgit2로 커밋까지 만들어 git 실행 파일 없이 돈다. 두 구현이 같은 결과를 내야 하므로, 한쪽의 분류 규칙(`porcelain::classify` / `local::classify`)을 바꾸면 다른 쪽도 맞춘다.

### 릴리스와 자동 업데이트

- `v*` 태그를 푸시하면 `.github/workflows/release.yml`이 macOS(Apple Silicon·Intel)·Linux·Windows 설치 파일을 빌드해 **릴리스 초안**에 올린다. 초안을 공개해야 자동 업데이트 대상이 된다.
- 버전은 세 곳(`Cargo.toml`의 `[workspace.package]`, `package.json`, `src-tauri/tauri.conf.json`)에 있다. 태그와 하나라도 다르면 워크플로가 실패한다.
- 서명용 시크릿은 모두 선택 사항이고, 등록된 것만 쓴다.
  - `TAURI_SIGNING_PRIVATE_KEY`(+`_PASSWORD`): 자동 업데이트 서명. 없으면 업데이트용 파일(`latest.json`)을 만들지 않는다. 공개키는 `tauri.conf.json`의 `plugins.updater.pubkey`에 있고, 개인키를 잃어버리면 기존 설치본은 업데이트를 받을 수 없다.
  - `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`: macOS 서명. `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`까지 있으면 공증도 한다. 없으면 임시(ad-hoc) 서명만 한다.
  - `WINDOWS_CERTIFICATE`(base64 .pfx), `WINDOWS_CERTIFICATE_PASSWORD`: Windows 서명. 없으면 서명하지 않는다.
- 업데이트용 파일 생성(`createUpdaterArtifacts`)은 `src-tauri/tauri.release.conf.json`에만 켜 두었다. 개인키가 없는 로컬 `npm run tauri build`가 실패하지 않게 하기 위해서다.
- 앱은 시작할 때 `api.checkForUpdate()`로 새 버전을 확인한다(개발 실행에서는 건너뜀). 플러그인 권한은 `src-tauri/capabilities/default.json`에 있다.

### 에러 처리

- 커맨드는 `AppResult<T>`(`src-tauri/src/error.rs`)를 반환한다. `AppError`는 `#[serde(tag = "kind", content = "message")]`라서 TS에서는 `{ kind: "notFound" | ..., message }`로 받는다.
- 새 도메인 에러는 `AppError`에 variant를 추가하고 크레이트 에러(`SshError`, `FsError`, `GitError`, `PtyError`)에서 `From`으로 변환한다. UI가 `kind`로 분기할 수 있게 하는 것이 목적이다.
- 프론트에서 에러를 화면에 띄울 때는 `api.errorMessage(e)`를 쓴다. `String(e)`는 `[object Object]`가 된다.

### Windows 테스트 제약 (중요)

- Tauri는 Windows Common Controls v6가 필요하다. `tauri-build`는 이를 선언한 매니페스트를 **bin 타깃에만** 넣기 때문에, 매니페스트가 없는 테스트 바이너리는 `STATUS_ENTRYPOINT_NOT_FOUND`로 실행 즉시 종료된다.
- 대응: `src-tauri/build.rs`가 `rustc-link-arg-tests`로 통합 테스트에 `windows-test-manifest.xml`을 넣는다. 그리고 `src-tauri`의 lib / bin 단위 테스트는 `test = false`로 꺼 두었다.
- 따라서 **Tauri를 링크하는 테스트는 `src-tauri/tests/`에만 작성한다.** 순수 로직 테스트는 `crates/core-*`의 단위 테스트로 작성한다(Tauri에 의존하지 않으므로 제약 없음).

### 크로스플랫폼

- `.gitattributes`가 모든 텍스트 파일을 LF로 강제한다(Windows CRLF 변환 방지).
- 경로는 문자열로 이어 붙이지 않고 `Path::join`을 쓴다. 앱 설정 경로는 `app.path().app_config_dir()`이다(`hosts.json`은 `HostStore::save`에서 임시 파일 → rename으로 원자적으로 저장).
- Rust 버전은 `rust-toolchain.toml`(1.100.0)로 고정되어 있다.
- 워크스페이스 린트로 `unsafe_code = "forbid"`가 걸려 있다.

### CI (`.github/workflows/ci.yml`)

- `lint` job(Ubuntu): `npm run check`, `cargo fmt --check`, `clippy -D warnings`.
- `test` job: Ubuntu / macOS / Windows matrix에서 `npm run build`와 `cargo test`를 실행하고, Linux에서만 바인딩 최신 여부를 확인한다.
- Linux 빌드에는 `libwebkit2gtk-4.1-dev` 등 시스템 패키지가 필요하다(워크플로에 설치 단계가 있음).

## 언어 규칙

- 응답, 코드 주석, 커밋 메시지, 문서는 **한국어**로 작성한다. 변수명과 함수명은 영어로 짓는다.
- Rust 테스트 함수 이름은 한국어로 지어도 된다(기존 테스트가 그렇게 되어 있음).
