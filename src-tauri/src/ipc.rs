//! IPC로 큰 데이터를 보낼 때 쓰는 타입.

use tauri::ipc::{InvokeResponseBody, IpcResponse};

/// JSON으로 직렬화하지 않고 그대로 보내는 바이트열.
///
/// `Vec<u8>`을 그냥 보내면 JSON 숫자 배열이 되어 느리고 크다.
/// 이 타입은 raw 응답으로 전달되어 프론트엔드에서 `ArrayBuffer`로 받는다.
/// 커맨드 반환값과 `Channel`의 메시지 타입으로 쓸 수 있다.
pub struct RawBytes(pub Vec<u8>);

impl IpcResponse for RawBytes {
    fn body(self) -> tauri::Result<InvokeResponseBody> {
        Ok(InvokeResponseBody::Raw(self.0))
    }
}

impl specta::Type for RawBytes {
    fn definition(_: &mut specta::Types) -> specta::datatype::DataType {
        specta::datatype::DataType::Reference(specta_typescript::define("ArrayBuffer"))
    }
}
