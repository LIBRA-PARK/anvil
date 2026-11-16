import { commands } from "./bindings";
import type { AppError, Host, NewHost } from "./types";

// 브라우저(vite dev)에서 단독으로 UI를 볼 때는 Tauri가 없으므로 메모리 목업을 쓴다.
const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
let mockHosts: Host[] = [];

/** Rust 커맨드가 던진 AppError인지 확인 */
export function isAppError(e: unknown): e is AppError {
  return typeof e === "object" && e !== null && "kind" in e && "message" in e;
}

/** 어떤 에러든 사용자에게 보여줄 문자열로 변환 */
export function errorMessage(e: unknown): string {
  if (isAppError(e)) return e.message;
  if (e instanceof Error) return e.message;
  return String(e);
}

export async function listHosts(): Promise<Host[]> {
  if (!inTauri) return [...mockHosts];
  return commands.listHosts();
}

export async function addHost(input: NewHost): Promise<Host> {
  if (!inTauri) {
    const host = { ...input, id: crypto.randomUUID() };
    mockHosts.push(host);
    return host;
  }
  return commands.addHost(input);
}

export async function removeHost(id: string): Promise<void> {
  if (!inTauri) {
    mockHosts = mockHosts.filter((h) => h.id !== id);
    return;
  }
  await commands.removeHost(id);
}
