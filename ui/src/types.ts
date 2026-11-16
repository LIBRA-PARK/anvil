// Rust에서 오는 타입(Host, AuthMethod 등)은 bindings.ts에서 자동 생성된다.
// 이 파일에는 UI 전용 타입만 둔다.
export type { AppError, AuthMethod, Host, NewHost } from "./bindings";

/** 로컬 머신을 나타내는 가상 호스트 id */
export const LOCAL_ID = "local";

export type TabKind = "welcome" | "terminal" | "files" | "editor" | "diff";

export interface Tab {
  id: string;
  hostId: string;
  kind: TabKind;
  title: string;
}
