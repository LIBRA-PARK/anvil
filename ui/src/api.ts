import { Channel } from "@tauri-apps/api/core";
import { commands } from "./bindings";
import type { AppError, DirListing, FileDiff, GitStatus, Host, NewHost } from "./types";

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

/** 방금 접속에 성공한 비밀번호·패스프레이즈를 OS 보안 저장소(키체인 등)에 저장한다. */
export async function saveSecret(hostId: string, secret: string): Promise<void> {
  if (!inTauri) return;
  await commands.sshSaveSecret(hostId, secret);
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

export interface FileOptions {
  hostId: string;
  path: string;
  secret?: string;
  trustedHostKey?: string;
}

/** 브라우저 미리보기에서 저장한 내용 */
const mockFiles = new Map<string, string>();

/** 텍스트 파일을 읽는다. UTF-8 텍스트가 아니면 에러를 던진다. */
export async function readFile(options: FileOptions): Promise<string> {
  if (!inTauri) {
    const key = `${options.hostId}:${options.path}`;
    return mockFiles.get(key) ?? `// ${options.path}\n// 브라우저 미리보기용 내용입니다.\n`;
  }
  const buffer = await commands.fsRead(
    options.hostId,
    options.path,
    options.secret ?? null,
    options.trustedHostKey ?? null,
  );
  const bytes = new Uint8Array(buffer);
  // NUL 바이트가 있거나 UTF-8로 해석되지 않으면 바이너리로 본다.
  // 잘못 해석한 채로 저장하면 파일이 망가지므로 아예 열지 않는다.
  if (bytes.subarray(0, 8192).includes(0)) throw new Error("바이너리 파일은 열 수 없습니다.");
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new Error("UTF-8 텍스트 파일이 아니어서 열 수 없습니다.");
  }
}

/** 파일 내용을 통째로 저장한다. 원격이면 SFTP로 업로드된다. */
export async function writeFile(options: FileOptions & { content: string }): Promise<void> {
  if (!inTauri) {
    mockFiles.set(`${options.hostId}:${options.path}`, options.content);
    return;
  }
  await commands.fsWrite(
    options.hostId,
    options.path,
    options.content,
    options.secret ?? null,
    options.trustedHostKey ?? null,
  );
}

/** `dir` 안에 새 폴더를 만들고 그 경로를 돌려준다. */
export async function createDir(hostId: string, dir: string, name: string): Promise<string> {
  if (!inTauri) return `${dir}/${name}`;
  return commands.fsCreateDir(hostId, dir, name);
}

/** 같은 폴더 안에서 이름을 바꾸고 새 경로를 돌려준다. */
export async function renamePath(hostId: string, path: string, newName: string): Promise<string> {
  if (!inTauri) return path.slice(0, path.lastIndexOf("/") + 1) + newName;
  return commands.fsRename(hostId, path, newName);
}

/** 파일이나 폴더(내용 포함)를 지운다. 되돌릴 수 없다. */
export async function removePath(hostId: string, path: string): Promise<void> {
  if (!inTauri) return;
  await commands.fsRemove(hostId, path);
}

// ── Git ──

/** `path`가 속한 저장소의 변경 파일 목록 (마지막 커밋 ↔ 작업 폴더) */
export async function gitStatus(options: FileOptions): Promise<GitStatus> {
  if (!inTauri) {
    return {
      root: options.path,
      branch: "main",
      changes: [
        { path: "README.md", oldPath: null, kind: "modified" },
        { path: "src/new.rs", oldPath: null, kind: "untracked" },
      ],
    };
  }
  return commands.gitStatus(
    options.hostId,
    options.path,
    options.secret ?? null,
    options.trustedHostKey ?? null,
  );
}

export interface GitFileDiffOptions {
  hostId: string;
  /** `gitStatus`가 돌려준 저장소 루트 */
  root: string;
  /** 저장소 루트 기준 경로 */
  path: string;
  /** 이름이 바뀐 파일의 원래 경로 */
  oldPath?: string;
  secret?: string;
  trustedHostKey?: string;
}

/** 변경된 파일 하나의 전후 내용 */
export async function gitFileDiff(options: GitFileDiffOptions): Promise<FileDiff> {
  if (!inTauri) {
    const modified = `# ${options.path}\n\n브라우저 미리보기용 내용입니다.\n새로 추가된 줄\n`;
    const original = options.path.endsWith(".md") ? `# ${options.path}\n\n예전 내용입니다.\n` : null;
    return { original, modified, binary: false };
  }
  return commands.gitFileDiff(
    options.hostId,
    options.root,
    options.path,
    options.oldPath ?? null,
    options.secret ?? null,
    options.trustedHostKey ?? null,
  );
}

// ── 자동 업데이트 ──

export interface AvailableUpdate {
  version: string;
  /** 릴리스 노트 (없을 수 있다) */
  notes: string | null;
  /** 내려받아 설치한 뒤 앱을 다시 시작한다. */
  install: () => Promise<void>;
}

/**
 * 새 버전이 있는지 확인한다. 없으면 null.
 * 개발 실행이나 브라우저 미리보기에서는 확인하지 않는다.
 */
export async function checkForUpdate(): Promise<AvailableUpdate | null> {
  if (!inTauri || import.meta.env.DEV) return null;
  // 시작 속도에 영향을 주지 않도록 필요할 때만 플러그인을 불러온다.
  const { check } = await import("@tauri-apps/plugin-updater");
  const update = await check();
  if (!update) return null;
  return {
    version: update.version,
    notes: update.body ?? null,
    install: async () => {
      await update.downloadAndInstall();
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    },
  };
}
