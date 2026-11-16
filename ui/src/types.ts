// Rust core-config 크레이트의 타입과 1:1로 맞춘다.
export type AuthMethod =
  | { kind: "password" }
  | { kind: "key"; path: string }
  | { kind: "agent" };

export interface Host {
  id: string;
  name: string;
  address: string;
  port: number;
  username: string;
  auth: AuthMethod;
}

export type NewHost = Omit<Host, "id">;

/** 로컬 머신을 나타내는 가상 호스트 id */
export const LOCAL_ID = "local";

export type TabKind = "welcome" | "terminal" | "files" | "editor" | "diff";

export interface Tab {
  id: string;
  hostId: string;
  kind: TabKind;
  title: string;
}
