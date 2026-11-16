# ⚒️ Anvil

SSH · SFTP · Git · 코드 에디터를 하나로 묶은 크로스플랫폼(Windows / macOS / Linux) 개발 도구입니다.
Rust + [Tauri 2](https://v2.tauri.app) + React로 만듭니다.

## 기능 로드맵

| 단계 | 내용                                            | 상태 |
| ---- | ----------------------------------------------- | ---- |
| M1   | Tauri 뼈대, 3단 레이아웃, 호스트 추가/삭제/저장 | ✅   |
| M2   | SSH 접속 + xterm.js 터미널 (다중 세션)          | 🚧   |
| M3   | SFTP 파일 트리                                  | 🚧   |
| M4   | Monaco 코드 에디터 (원격 파일 열기/저장)        | ⏳   |
| M5   | Git Diff (로컬 · 원격)                          | ⏳   |
| M6   | GitHub Actions 3개 OS 크로스 빌드               | ⏳   |

## 구조

```
anvil/
├── Cargo.toml          # Cargo 워크스페이스
├── crates/
│   ├── core-config/    # 호스트 목록 저장 (Tauri 비의존)
│   ├── core-ssh/       # SSH 접속 · 인증 · 원격 셸 (russh, Tauri 비의존)
│   └── core-fs/        # 로컬 · SFTP 파일 시스템 공통 트레잇
├── src-tauri/          # Tauri 앱 (커맨드 연결)
└── ui/                 # React + TypeScript 프론트엔드
```

이후 `core-git` 크레이트가 추가됩니다.

## 개발 환경 준비

1. Rust 설치: <https://rustup.rs>
2. OS별 사전 요구사항: <https://v2.tauri.app/start/prerequisites/>
3. 의존성 설치

```bash
npm install
```

## 실행

```bash
npm run tauri dev      # 데스크톱 앱 실행
npm run dev            # 브라우저에서 UI만 미리보기 (목업 데이터)
cargo test --workspace # Rust 테스트
npm run tauri build    # 배포용 빌드
```

## 개발 규칙

| 명령                                                    | 설명                          |
| ------------------------------------------------------- | ----------------------------- |
| `npm run check`                                         | ESLint + Prettier + 타입 검사 |
| `npm run format`                                        | 프론트엔드 코드 자동 포맷     |
| `cargo fmt --all`                                       | Rust 코드 자동 포맷           |
| `cargo clippy --workspace --all-targets -- -D warnings` | Rust 린트 (CI와 동일)         |

- **Rust ↔ TS 타입**: `ui/src/bindings.ts`는 [tauri-specta](https://github.com/specta-rs/tauri-specta)가 자동 생성합니다. 커맨드나 타입을 바꾼 뒤 `cargo test` 또는 `npm run tauri dev`를 실행하고, 갱신된 파일을 함께 커밋하세요. 최신이 아니면 CI가 실패합니다.
- **새 커맨드**: `#[tauri::command]`와 `#[specta::specta]`를 붙이고 `src-tauri/src/lib.rs`의 `specta_builder()`에 등록합니다.
- **에러**: 커맨드는 `AppResult<T>`를 반환합니다. 프론트엔드에서는 `{ kind, message }` 형태로 받습니다.
- **Rust 버전**: `rust-toolchain.toml`에 고정되어 있어 rustup이 자동으로 맞춥니다.
- **줄바꿈**: `.gitattributes`로 LF를 강제합니다. Windows에서도 그대로 작업하면 됩니다.

## 라이선스

[MIT](LICENSE)
