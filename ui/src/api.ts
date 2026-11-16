import { Channel } from "@tauri-apps/api/core";
import { commands } from "./bindings";
import { LOCAL_ID, type AppError, type DirListing, type Host, type NewHost } from "./types";

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

// ── 터미널 ──

export interface TerminalOpenOptions {
  hostId: string;
  cols: number;
  rows: number;
  /** 비밀번호 또는 개인키 패스프레이즈 (필요할 때만) */
  secret?: string;
  /** 사용자가 확인한 호스트 키 지문 (`hostKeyUnknown` 에러의 message) */
  trustedHostKey?: string;
}

export interface TerminalHandlers {
  /** 셸 출력. xterm에 그대로 쓰면 된다. */
  onOutput: (data: Uint8Array) => void;
  /** 셸 종료. 연결이 끊겨서 끝났다면 code는 null. */
  onExit: (code: number | null) => void;
}

/** 브라우저 미리보기용: 입력을 그대로 되돌려 주는 가짜 셸 */
const mockTerminals = new Map<string, TerminalHandlers>();
const encoder = new TextEncoder();

/** 호스트에 접속해 셸을 열고 세션 id를 돌려준다. 실패하면 AppError를 던진다. */
export async function openTerminal(
  options: TerminalOpenOptions,
  handlers: TerminalHandlers,
): Promise<string> {
  if (!inTauri) {
    const id = crypto.randomUUID();
    mockTerminals.set(id, handlers);
    const notice = "브라우저 미리보기입니다. 실제 SSH 접속은 `npm run tauri dev`에서 동작합니다.\r\n$ ";
    setTimeout(() => handlers.onOutput(encoder.encode(notice)));
    return id;
  }
  const onOutput = new Channel<ArrayBuffer>();
  onOutput.onmessage = (buffer) => handlers.onOutput(new Uint8Array(buffer));
  const onExit = new Channel<{ code?: number | null }>();
  onExit.onmessage = ({ code }) => handlers.onExit(code ?? null);

  return commands.terminalOpen(
    options.hostId,
    options.cols,
    options.rows,
    options.secret ?? null,
    options.trustedHostKey ?? null,
    onOutput,
    onExit,
  );
}

export async function writeTerminal(sessionId: string, data: string): Promise<void> {
  if (!inTauri) {
    const echo = data.replace(/\r/g, "\r\n$ ").replace(/\x7f/g, "\b \b");
    mockTerminals.get(sessionId)?.onOutput(encoder.encode(echo));
    return;
  }
  await commands.terminalWrite(sessionId, data);
}

export async function resizeTerminal(sessionId: string, cols: number, rows: number): Promise<void> {
  if (!inTauri) return;
  await commands.terminalResize(sessionId, cols, rows);
}

export async function closeTerminal(sessionId: string): Promise<void> {
  if (!inTauri) {
    mockTerminals.delete(sessionId);
    return;
  }
  await commands.terminalClose(sessionId);
}

// ── 파일 ──

export interface ListDirOptions {
  /** `LOCAL_ID`면 이 머신, 아니면 해당 호스트를 SFTP로 읽는다. */
  hostId: string;
  /** 생략하면 홈 디렉터리 */
  path?: string;
  /** 원격 접속이 필요할 때만 (터미널과 같은 방식) */
  secret?: string;
  trustedHostKey?: string;
}

/** 브라우저 미리보기용 가짜 폴더 구조 */
function mockListing(path: string): DirListing {
  const depth = path.split("/").filter(Boolean).length;
  const entry = (name: string, kind: "dir" | "file", size = 0) => ({
    name,
    path: `${path === "/" ? "" : path}/${name}`,
    kind,
    isSymlink: false,
    size,
    modified: 1_700_000_000,
  });
  return {
    path,
    parent: path === "/" ? null : path.slice(0, path.lastIndexOf("/")) || "/",
    entries:
      depth > 3
        ? []
        : [
            entry("src", "dir"),
            entry(".config", "dir"),
            entry("README.md", "file", 2048),
            entry("main.rs", "file", 512),
          ],
  };
}

/** 폴더 내용을 읽는다. 실패하면 AppError를 던진다. */
export async function listDir(options: ListDirOptions): Promise<DirListing> {
  if (!inTauri) return mockListing(options.path ?? "/home/preview");
  return commands.fsList(
    options.hostId,
    options.path ?? null,
    options.secret ?? null,
    options.trustedHostKey ?? null,
  );
}

/** 파일 탐색기를 닫을 때 원격 SFTP 세션을 정리한다. */
export async function closeFiles(hostId: string): Promise<void> {
  if (!inTauri || hostId === LOCAL_ID) return;
  await commands.fsClose(hostId);
}
