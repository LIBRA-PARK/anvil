import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import * as api from "../api";
import { LOCAL_ID, type DirEntry, type DirListing, type Host } from "../types";
import {
  ConnectPrompt,
  promptForError,
  rememberIfAsked,
  type Credentials,
  type Prompt,
} from "./ConnectPrompt";

interface Props {
  /** null이면 로컬 머신 */
  host: Host | null;
  /** 파일을 더블클릭했을 때 */
  onOpenFile: (entry: DirEntry) => void;
  /** 지금 보고 있는 폴더의 Git 변경사항 보기 */
  onOpenDiff: (path: string) => void;
}

/** 펼친 폴더 하나의 상태 */
type Children =
  { status: "loading" } | { status: "error"; message: string } | { status: "ready"; entries: DirEntry[] };

/** 무엇을 읽으려 했는지. 접속 확인 창을 거친 뒤 같은 요청을 다시 보내는 데 쓴다. */
type Request =
  /** `keep`이면 펼쳐 둔 하위 폴더와 선택을 유지한 채 목록만 새로 읽는다. */
  { kind: "root"; path?: string; keep?: boolean } | { kind: "expand"; path: string };

/** 선택한 항목과 그 항목이 들어 있는 폴더 */
interface Selection {
  entry: DirEntry;
  dir: string;
}

/** 이름을 입력받는 창 (새 폴더, 이름 변경) */
interface NameRequest {
  title: string;
  initial: string;
  action: string;
  submit: (name: string) => Promise<void>;
}

function without<T>(record: Record<string, T>, key: string): Record<string, T> {
  const copy = { ...record };
  delete copy[key];
  return copy;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

/** 파일 트리. 로컬 디스크와 SFTP 원격을 같은 화면으로 보여 준다. */
export function FilesView({ host, onOpenFile, onOpenDiff }: Props) {
  const hostId = host?.id ?? LOCAL_ID;
  const [root, setRoot] = useState<DirListing | null>(null);
  /** 루트를 읽지 못했을 때의 메시지 */
  const [rootError, setRootError] = useState<string | null>(null);
  const [loadingRoot, setLoadingRoot] = useState(true);
  /** 펼친 하위 폴더들 (경로 → 내용). 키가 있으면 펼쳐진 것이다. */
  const [children, setChildren] = useState<Record<string, Children>>({});
  const [selected, setSelected] = useState<Selection | null>(null);
  const [nameRequest, setNameRequest] = useState<NameRequest | null>(null);
  /** 삭제 등 파일 조작이 실패했을 때의 메시지 */
  const [opError, setOpError] = useState<string | null>(null);
  const [pathInput, setPathInput] = useState("");
  const [pending, setPending] = useState<{ request: Request; prompt: Prompt; tried: Credentials } | null>(
    null,
  );
  // 늦게 도착한 이전 루트 요청의 응답이 화면을 덮어쓰지 않게 한다.
  const rootSeq = useRef(0);

  const run = useCallback(
    async (request: Request, credentials: Credentials = {}) => {
      const isRoot = request.kind === "root";
      const seq = isRoot ? ++rootSeq.current : rootSeq.current;
      if (isRoot) {
        setLoadingRoot(true);
        setRootError(null);
      } else {
        setChildren((prev) => ({ ...prev, [request.path]: { status: "loading" } }));
      }

      try {
        const listing = await api.listDir({ hostId, path: request.path, ...credentials });
        rememberIfAsked(hostId, credentials);
        if (seq !== rootSeq.current) return;
        if (isRoot) {
          setRoot(listing);
          setPathInput(listing.path);
          if (!request.keep) {
            setChildren({});
            setSelected(null);
          }
        } else {
          setChildren((prev) =>
            request.path in prev
              ? { ...prev, [request.path]: { status: "ready", entries: listing.entries } }
              : prev,
          );
        }
      } catch (e) {
        if (seq !== rootSeq.current) return;
        const prompt = host ? promptForError(e, host, credentials) : null;
        if (prompt) {
          // 창이 떠 있는 동안에는 "불러오는 중" 대신 접힌 상태로 둔다.
          if (!isRoot) setChildren((prev) => without(prev, request.path));
          setPending({ request, prompt, tried: credentials });
        } else if (isRoot) {
          setRootError(api.errorMessage(e));
        } else {
          setChildren((prev) => ({
            ...prev,
            [request.path]: { status: "error", message: api.errorMessage(e) },
          }));
        }
      } finally {
        if (isRoot && seq === rootSeq.current) setLoadingRoot(false);
      }
    },
    [host, hostId],
  );

  // 탭이 열리면 홈 디렉터리부터 읽는다. (원격 세션 정리는 App이 탭 목록을 보고 한다)
  useEffect(() => {
    run({ kind: "root" });
    return () => {
      rootSeq.current += 1;
    };
  }, [run]);

  const toggle = (entry: DirEntry, dir: string) => {
    setSelected({ entry, dir });
    if (entry.kind !== "dir") return;
    if (entry.path in children) {
      setChildren((prev) => without(prev, entry.path));
    } else {
      run({ kind: "expand", path: entry.path });
    }
  };

  /** 폴더 하나의 목록만 다시 읽는다. (파일 조작 뒤 화면 갱신용) */
  const refreshDir = (dir: string) => {
    if (dir === root?.path) run({ kind: "root", path: dir, keep: true });
    else run({ kind: "expand", path: dir });
  };

  const newFolder = () => {
    if (!root) return;
    // 폴더를 골랐으면 그 안에, 파일을 골랐으면 같은 폴더에, 아무것도 안 골랐으면 지금 보는 폴더에 만든다.
    const dir = selected ? (selected.entry.kind === "dir" ? selected.entry.path : selected.dir) : root.path;
    setNameRequest({
      title: `새 폴더 (${dir})`,
      initial: "",
      action: "만들기",
      submit: async (name) => {
        await api.createDir(hostId, dir, name);
        refreshDir(dir);
      },
    });
  };

  const renameSelected = () => {
    if (!selected) return;
    const { entry, dir } = selected;
    setNameRequest({
      title: `이름 변경 (${entry.name})`,
      initial: entry.name,
      action: "변경",
      submit: async (name) => {
        if (name === entry.name) return;
        await api.renamePath(hostId, entry.path, name);
        setSelected(null);
        refreshDir(dir);
      },
    });
  };

  const removeSelected = async () => {
    if (!selected) return;
    const { entry, dir } = selected;
    const what = entry.kind === "dir" && !entry.isSymlink ? "폴더와 그 안의 모든 내용을" : "파일을";
    if (!confirm(`'${entry.name}' ${what} 삭제할까요?\n되돌릴 수 없습니다.\n\n${entry.path}`)) return;
    setOpError(null);
    try {
      await api.removePath(hostId, entry.path);
      setSelected(null);
      refreshDir(dir);
    } catch (e) {
      setOpError(api.errorMessage(e));
    }
  };

  const submitPath = (e: FormEvent) => {
    e.preventDefault();
    const path = pathInput.trim();
    if (path) run({ kind: "root", path });
  };

  const rows = (entries: DirEntry[], depth: number, dir: string): JSX.Element[] =>
    entries.flatMap((entry) => {
      const state = children[entry.path];
      const indent = { paddingLeft: 8 + depth * 16 };
      const isDir = entry.kind === "dir";
      const row = (
        <button
          key={entry.path}
          className={`file-row ${entry.path === selected?.entry.path ? "selected" : ""} ${entry.name.startsWith(".") ? "dim" : ""}`}
          style={indent}
          title={entry.path}
          onClick={() => toggle(entry, dir)}
          onDoubleClick={() => entry.kind === "file" && onOpenFile(entry)}
        >
          <span className="file-chevron">{isDir ? (state ? "▼" : "▶") : ""}</span>
          <span>{isDir ? "📁" : entry.kind === "file" ? "📄" : "❔"}</span>
          <span className="file-name">
            {entry.name}
            {entry.isSymlink && " ↗"}
          </span>
          {entry.kind === "file" && <span className="file-meta">{formatSize(entry.size)}</span>}
        </button>
      );
      if (!state) return [row];

      const noteIndent = { paddingLeft: 8 + (depth + 1) * 16 + 20 };
      const note = (text: string, error = false) => (
        <div key={`${entry.path}\0note`} className={`file-note ${error ? "error" : ""}`} style={noteIndent}>
          {text}
        </div>
      );
      if (state.status === "loading") return [row, note("불러오는 중…")];
      if (state.status === "error") return [row, note(state.message, true)];
      if (state.entries.length === 0) return [row, note("(비어 있음)")];
      return [row, ...rows(state.entries, depth + 1, entry.path)];
    });

  return (
    <div className="files">
      <form className="files-toolbar" onSubmit={submitPath}>
        <button
          type="button"
          className="icon-button"
          title="상위 폴더"
          disabled={!root?.parent}
          onClick={() => root?.parent && run({ kind: "root", path: root.parent })}
        >
          ↑
        </button>
        <button
          type="button"
          className="icon-button"
          title="새로 고침"
          disabled={loadingRoot}
          onClick={() => run({ kind: "root", path: root?.path })}
        >
          ⟳
        </button>
        <button
          type="button"
          className="icon-button"
          title="이 폴더의 Git 변경사항"
          disabled={!root}
          onClick={() => root && onOpenDiff(root.path)}
        >
          ±
        </button>
        <button type="button" className="icon-button" title="새 폴더" disabled={!root} onClick={newFolder}>
          ＋
        </button>
        <button
          type="button"
          className="icon-button"
          title="이름 변경"
          disabled={!selected}
          onClick={renameSelected}
        >
          ✎
        </button>
        <button
          type="button"
          className="icon-button"
          title="삭제"
          disabled={!selected}
          onClick={() => void removeSelected()}
        >
          🗑
        </button>
        <input
          value={pathInput}
          onChange={(e) => setPathInput(e.target.value)}
          placeholder="경로를 입력하고 Enter"
          spellCheck={false}
          aria-label="경로"
        />
      </form>

      {opError && (
        <div className="file-note error" role="alert" onClick={() => setOpError(null)}>
          {opError}
        </div>
      )}

      {rootError ? (
        <div className="files-status">
          <span className="form-error">{rootError}</span>
          <button className="btn-primary" onClick={() => run({ kind: "root", path: root?.path })}>
            다시 시도
          </button>
        </div>
      ) : !root ? (
        <div className="files-status">{loadingRoot ? "불러오는 중…" : ""}</div>
      ) : (
        <div className="files-tree" role="tree">
          {root.entries.length === 0 ? (
            <div className="file-note">(비어 있음)</div>
          ) : (
            rows(root.entries, 0, root.path)
          )}
        </div>
      )}

      {nameRequest && <NameDialog request={nameRequest} onClose={() => setNameRequest(null)} />}

      {pending && host && (
        <ConnectPrompt
          host={host}
          prompt={pending.prompt}
          onSubmit={(credentials) => {
            setPending(null);
            run(pending.request, { ...pending.tried, ...credentials });
          }}
          onCancel={() => {
            setPending(null);
            if (pending.request.kind === "root" && !root) setRootError("접속을 취소했습니다.");
          }}
        />
      )}
    </div>
  );
}

/** 이름 하나를 입력받아 작업을 실행한다. 실패하면 창을 닫지 않고 이유를 보여 준다. */
function NameDialog({ request, onClose }: { request: NameRequest; onClose: () => void }) {
  const [name, setName] = useState(request.initial);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) return;
    setBusy(true);
    setError(null);
    try {
      await request.submit(trimmed);
      onClose();
    } catch (err) {
      setError(api.errorMessage(err));
      setBusy(false);
    }
  };

  return (
    <div className="overlay">
      <form className="overlay-card" onSubmit={submit}>
        <label>
          {request.title}
          <input value={name} onChange={(e) => setName(e.target.value)} autoFocus spellCheck={false} />
        </label>
        {error && <p className="form-error">{error}</p>}
        <div className="modal-actions">
          <button type="button" className="btn-secondary" onClick={onClose}>
            취소
          </button>
          <button type="submit" className="btn-primary" disabled={busy || !name.trim()}>
            {request.action}
          </button>
        </div>
      </form>
    </div>
  );
}
