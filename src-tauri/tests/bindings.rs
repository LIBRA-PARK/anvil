//! `cargo test`로 TS 바인딩을 생성한다.
//! CI는 이후 `git diff`로 커밋된 bindings.ts가 최신인지 검사한다.
//!
//! Tauri에 의존하는 테스트는 lib 단위 테스트가 아니라 여기(통합 테스트)에 둔다.
//! Windows에서 테스트 바이너리에 매니페스트를 넣는 방법은 build.rs 참고.

#[test]
fn export_bindings() {
    anvil_lib::export_bindings();
}
