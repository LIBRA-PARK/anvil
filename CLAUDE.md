# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 프로젝트 개요

Anvil은 SSH 터미널 · SFTP 파일 탐색 · Git Diff · 코드 에디터를 하나로 묶은 **개인용 크로스플랫폼(Windows / macOS / Linux) 데스크톱 개발 도구**다. Rust + Tauri 2 백엔드, React 18 + TypeScript 프론트엔드(Vite).

UI는 Discord식 3단 레이아웃이다. 협업/채팅 기능은 없고 **레이아웃만** 차용한다.

- 왼쪽 아이콘 바(`HostBar`): Discord "서버" 자리에 SSH 호스트. 💻는 로컬 머신(`LOCAL_ID = "local"`).
- 채널 사이드바(`ChannelSidebar`): 선택한 호스트의 기능 채널(터미널 / 파일 / Git).
- 메인 패널(`MainPanel`): 탭으로 터미널·에디터·Diff 전환. 탭 id는 `${hostId}:${kind}`이며 같은 id면 새로 열지 않고 포커스한다.

## 로드맵 (현재 M1 완료, 다음은 M2)

| 단계  | 내용                                                 | 계획된 구현                                                           |
| ----- | ---------------------------------------------------- | --------------------------------------------------------------------- |
| M1 ✅ | 뼈대, 레이아웃, 호스트 추가/삭제/저장                | `crates/core-config`                                                  |
| M2    | SSH 접속 + 다중 세션 터미널                          | `russh` + 프론트 `xterm.js`, 새 크레이트 `core-ssh`                   |
| M3    | SFTP 파일 트리                                       | `russh-sftp`                                                          |
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
