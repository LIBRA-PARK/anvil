# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 프로젝트 개요

Anvil은 SSH 터미널 · SFTP 파일 탐색 · Git Diff · 코드 에디터를 하나로 묶은 **개인용 크로스플랫폼(Windows / macOS / Linux) 데스크톱 개발 도구**다. Rust + Tauri 2 백엔드, React 18 + TypeScript 프론트엔드(Vite).

UI는 Discord식 3단 레이아웃이다. 협업/채팅 기능은 없고 **레이아웃만** 차용한다.

- 왼쪽 아이콘 바(`HostBar`): Discord "서버" 자리에 SSH 호스트. 💻는 로컬 머신(`LOCAL_ID = "local"`).
- 채널 사이드바(`ChannelSidebar`): 선택한 호스트의 기능 채널(터미널 / 파일 / Git).
- 메인 패널(`MainPanel`): 탭으로 터미널·에디터·Diff 전환. 탭 id는 `${hostId}:${kind}`이며 같은 id면 새로 열지 않고 포커스한다. 터미널만 예외로 한 호스트에 여러 개 열 수 있어 `${hostId}:terminal:${번호}`를 쓴다.
- 탭은 모두 마운트해 두고 `hidden`으로 숨기기만 한다(터미널 세션과 화면 내용 유지). 탭을 닫으면 언마운트되면서 세션도 닫힌다.

## 로드맵 (현재 M2·M3 진행 중)

| 단계  | 내용                                                 | 계획된 구현                                                           |
| ----- | ---------------------------------------------------- | --------------------------------------------------------------------- |
| M1 ✅ | 뼈대, 레이아웃, 호스트 추가/삭제/저장                | `crates/core-config`                                                  |
| M2 🚧 | SSH 접속 + 다중 세션 터미널                          | `russh` + 프론트 `xterm.js`, `crates/core-ssh`                        |
| M3 🚧 | SFTP 파일 트리                                       | `russh-sftp`, `crates/core-fs`                                        |
| M4    | 코드 에디터 (원격 파일 열기 / 저장 시 SFTP 업로드)   | Monaco Editor                                                         |
| M5    | Git Diff (로컬 + 원격)                               | 로컬 `git2`, 원격은 SSH로 `git diff` 실행 후 파싱, 뷰어는 Monaco Diff |
| M6    | 릴리스 워크플로(설치 파일), 코드 서명, 자동 업데이트 |                                                                       |

확정된 설계 방향:

- `core-fs` / `core-git` 크레이트에서 **Local과 Remote를 같은 트레잇으로 추상화**해, 에디터·Diff UI가 대상이 로컬인지 원격인지 몰라도 되게 한다.
- 도메인 로직은 Tauri에 의존하지 않는 `crates/core-*`에 두고, `src-tauri`는 커맨드 연결만 담당한다.
- 비밀번호는 `hosts.json`에 저장하지 않는다. OS 키체인(`keyring` 크레이트)에 저장할 예정이다.
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

### SSH 터미널 (`crates/core-ssh` + `src-tauri/src/terminal.rs`)

- `SshPool`이 호스트 id별로 연결 하나를 공유한다(약한 참조). 탭을 여러 개 열어도 인증은 한 번이고, 셸이 모두 닫히면 연결도 끊긴다. M3의 SFTP도 이 연결에 채널을 추가하는 식으로 붙인다.
- 셸 출력은 `Channel<TerminalOutput>`(raw 바이트 → JS `ArrayBuffer`)으로 보낸다. JSON으로 보내지 않는다.
- 호스트 키: 새로 신뢰한 키는 앱 설정 폴더의 `known_hosts`에만 기록하고 `~/.ssh/known_hosts`는 읽기만 한다. 처음 보는 키는 `hostKeyUnknown` 에러(`message` = 지문)로 돌려보내고, UI가 사용자 확인 후 같은 지문을 `trustedHostKey`로 넣어 다시 호출한다. 키 불일치(`hostKeyMismatch`)는 우회 수단을 두지 않는다.
- 비밀번호·패스프레이즈는 `terminal_open`의 `secret` 인자로만 전달되고 어디에도 저장하지 않는다(키체인 저장은 아직 미구현). 필요하면 `passwordRequired` / `passphraseRequired` 에러로 UI에 입력을 요청한다.
- `core-ssh` 통합 테스트(`tests/shell.rs`)는 프로세스 안에 russh 서버를 띄워 돌리므로 외부 sshd가 필요 없다. `tests/fixtures/`의 키는 테스트 전용이다.
- russh는 Windows CI에서 NASM/CMake가 필요 없도록 `aws-lc-rs` 대신 `ring` 백엔드를 쓴다.
- M2 남은 일: 비밀번호 키체인 저장(`keyring`), 로컬 터미널(💻), keyboard-interactive 인증, 출력 폭주 시 묶어 보내기.

### 파일 탐색 (`crates/core-fs` + `src-tauri/src/fs.rs`)

- `FileSystem` 트레잇을 `LocalFs`(로컬 디스크)와 `SftpFs`(원격)가 구현한다. 커맨드 `fs_list`는 `host_id`가 `"local"`이면 로컬, 아니면 SFTP를 고르고 나머지 처리는 같다. M4의 파일 읽기/쓰기도 이 트레잇에 메서드를 추가한다.
- 경로는 문자열로 주고받고(로컬은 OS 형식, 원격은 POSIX), UI는 경로를 직접 조립하지 않는다. 백엔드가 `DirListing.parent`와 각 항목의 `path`를 내려 준다.
- SFTP 세션은 호스트당 하나를 `Files`에 캐시하고, 파일 탭을 닫으면(`fs_close`) 정리한다. SSH 연결은 터미널과 같은 `SshContext`(`src-tauri/src/ssh.rs`)의 풀을 공유한다.
- 원격 접속이 필요한 커맨드는 `secret` / `trusted_host_key`를 받고 터미널과 같은 에러 종류를 돌려준다. UI 쪽 입력 창은 `ConnectPrompt.tsx`(`promptForError` + `ConnectPrompt`)를 재사용한다.
- `u64` 필드는 specta가 BigInt로 내보내려다 실패하므로 `#[specta(type = specta_typescript::Number)]`를 붙인다.
- `SftpFs`는 자동 테스트가 없다(SSH+SFTP 서버를 프로세스 안에 띄우는 하네스가 아직 없음). 바꿀 때는 실제 sshd로 확인한다.
- M3 남은 일: 파일 조작(새 폴더·이름 변경·삭제), 업로드/다운로드, 숨김 파일 토글.

### 에러 처리

- 커맨드는 `AppResult<T>`(`src-tauri/src/error.rs`)를 반환한다. `AppError`는 `#[serde(tag = "kind", content = "message")]`라서 TS에서는 `{ kind: "notFound" | ..., message }`로 받는다.
- 새 도메인 에러(SSH 인증 실패, 호스트 키 불일치 등)는 `AppError`에 variant를 추가하고 크레이트 에러에서 `From`으로 변환한다. UI가 `kind`로 분기할 수 있게 하는 것이 목적이다.
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
