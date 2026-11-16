// Rust에서 오는 타입(Host, AuthMethod 등)은 bindings.ts에서 자동 생성된다.
// 이 파일에는 UI 전용 타입만 둔다.
export type {
  AppError,
  AuthMethod,
  DirEntry_Serialize as DirEntry,
  DirListing_Serialize as DirListing,
  FileDiff_Serialize as FileDiff,
  GitChange_Serialize as GitChange,
  GitStatus_Serialize as GitStatus,
  Host,
  NewHost,
} from "./bindings";

/** 로컬 머신을 나타내는 가상 호스트 id */
export const LOCAL_ID = "local";

export type TabKind = "welcome" | "terminal" | "files" | "editor" | "diff";

export interface Tab {
  /**
   * 기본은 `${hostId}:${kind}` (호스트당 하나, 다시 열면 기존 탭으로 이동).
   * 터미널은 한 호스트에 여러 개 열 수 있어 `${hostId}:terminal:${번호}`를,
   * 에디터는 파일마다 하나씩이라 `${hostId}:editor:${경로}`를 쓴다.
   */
  id: string;
  hostId: string;
  kind: TabKind;
  title: string;
  /** 에디터 탭이 연 파일의 전체 경로, 또는 Diff 탭이 조사할 폴더 */
  path?: string;
}
