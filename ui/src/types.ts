// Rust에서 오는 타입(Host, AuthMethod 등)은 bindings.ts에서 자동 생성된다.
// 이 파일에는 UI 전용 타입만 둔다.
export type { AppError, AuthMethod, Host, NewHost } from "./bindings";

/** 로컬 머신을 나타내는 가상 호스트 id */
export const LOCAL_ID = "local";

export type TabKind = "welcome" | "terminal" | "files" | "editor" | "diff";

export interface Tab {
  /**
   * 기본은 `${hostId}:${kind}` (호스트당 하나, 다시 열면 기존 탭으로 이동).
   * 터미널은 한 호스트에 여러 개 열 수 있어 `${hostId}:terminal:${번호}`를 쓴다.
   */
  id: string;
  hostId: string;
  kind: TabKind;
  title: string;
}
