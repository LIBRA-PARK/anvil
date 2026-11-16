# ⚒️ Anvil

SSH · SFTP · Git · 코드 에디터를 하나로 묶은 크로스플랫폼(Windows / macOS / Linux) 개발 도구입니다.
Rust + [Tauri 2](https://v2.tauri.app) + React로 만듭니다.

## 기능 로드맵

| 단계 | 내용 | 상태 |
|---|---|---|
| M1 | Tauri 뼈대, 3단 레이아웃, 호스트 추가/삭제/저장 | ✅ |
| M2 | SSH 접속 + xterm.js 터미널 (다중 세션) | ⏳ |
| M3 | SFTP 파일 트리 | ⏳ |
| M4 | Monaco 코드 에디터 (원격 파일 열기/저장) | ⏳ |
| M5 | Git Diff (로컬 · 원격) | ⏳ |
| M6 | GitHub Actions 3개 OS 크로스 빌드 | ⏳ |

## 구조

```
anvil/
├── Cargo.toml          # Cargo 워크스페이스
├── crates/
│   └── core-config/    # 호스트 목록 저장 (Tauri 비의존)
├── src-tauri/          # Tauri 앱 (커맨드 연결)
└── ui/                 # React + TypeScript 프론트엔드
```

이후 `core-ssh`, `core-fs`, `core-git` 크레이트가 추가됩니다.

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
