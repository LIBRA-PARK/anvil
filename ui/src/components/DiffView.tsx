import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import * as api from "../api";
import { BASE_OPTIONS, createModel, fileUri, monaco } from "../monaco";
import { LOCAL_ID, type GitChange, type GitStatus, type Host } from "../types";
import { useConnectRetry } from "./useConnectRetry";

interface Props {
  /** null이면 로컬 머신 */
  host: Host | null;
  /** 파일 탐색기에서 넘어온 폴더. 바뀌면 그 폴더의 저장소를 다시 조사한다. */
  initialPath?: string;
}

const BADGE: Record<GitChange["kind"], { label: string; title: string }> = {
  modified: { label: "M", title: "수정됨" },
  added: { label: "A", title: "추가됨" },
  deleted: { label: "D", title: "삭제됨" },
  renamed: { label: "R", title: "이름 변경" },
  untracked: { label: "U", title: "추적하지 않는 새 파일" },
  conflicted: { label: "!", title: "충돌" },
};

/** 한쪽 화면을 차지하는 안내 문구 (파일을 고르기 전, 바이너리 등) */
type Notice = { text: string; error?: boolean } | null;

const storageKey = (hostId: string) => `anvil.git.path.${hostId}`;

function recallPath(hostId: string): string {
  try {
    return localStorage.getItem(storageKey(hostId)) ?? "";
  } catch {
    return "";
  }
}

/** Git 변경사항 탭: 변경 파일 목록 + 마지막 커밋과 작업 폴더의 비교 */
export default function DiffView({ host, initialPath }: Props) {
  const hostId = host?.id ?? LOCAL_ID;
  const [pathInput, setPathInput] = useState(() => initialPath ?? recallPath(hostId));
  const [status, setStatus] = useState<GitStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<GitChange | null>(null);
  const [notice, setNotice] = useState<Notice>({ text: "" });
  const { call, prompt } = useConnectRetry(host);

  const containerRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<monaco.editor.IStandaloneDiffEditor | null>(null);
  // 늦게 도착한 이전 요청의 응답이 화면을 덮어쓰지 않게 한다.
  const statusSeq = useRef(0);
  const diffSeq = useRef(0);
  /** 새로 고친 뒤에도 보던 파일을 유지하려고 마지막 선택을 기억한다. */
  const selectedRef = useRef<GitChange | null>(null);

  // Diff 에디터는 탭이 열릴 때 한 번 만들고, 닫힐 때 모델과 함께 정리한다.
  useEffect(() => {
    const editor = monaco.editor.createDiffEditor(containerRef.current!, {
      ...BASE_OPTIONS,
      readOnly: true,
      originalEditable: false,
      renderSideBySide: true,
    });
    editorRef.current = editor;
    return () => {
      const model = editor.getModel();
      editor.dispose();
      model?.original.dispose();
      model?.modified.dispose();
      editorRef.current = null;
    };
  }, []);

  const showDiff = useCallback(
    async (root: string, change: GitChange) => {
      const seq = ++diffSeq.current;
      selectedRef.current = change;
      setSelected(change);
      setNotice({ text: "불러오는 중…" });
      try {
        const diff = await call((credentials) =>
          api.gitFileDiff({
            hostId,
            root,
            path: change.path,
            oldPath: change.oldPath ?? undefined,
            ...credentials,
          }),
        );
        const editor = editorRef.current;
        if (seq !== diffSeq.current || !editor) return;
        if (diff.binary) {
          setNotice({ text: "바이너리 파일이라 내용을 비교할 수 없습니다." });
          return;
        }
        const previous = editor.getModel();
        editor.setModel({
          // 한쪽이 없으면(새 파일·삭제된 파일) 빈 내용과 비교한다.
          original: createModel(diff.original ?? "", fileUri(hostId, change.path, "head")),
          modified: createModel(diff.modified ?? "", fileUri(hostId, change.path, "worktree")),
        });
        previous?.original.dispose();
        previous?.modified.dispose();
        setNotice(null);
      } catch (e) {
        if (seq === diffSeq.current) setNotice({ text: api.errorMessage(e), error: true });
      }
    },
    [call, hostId],
  );

  const load = useCallback(
    async (path: string) => {
      const seq = ++statusSeq.current;
      diffSeq.current += 1;
      setLoading(true);
      setError(null);
      try {
        const result = await call((credentials) => api.gitStatus({ hostId, path, ...credentials }));
        if (seq !== statusSeq.current) return;
        setStatus(result);
        setPathInput(result.root);
        try {
          localStorage.setItem(storageKey(hostId), result.root);
        } catch {
          // 저장소 접근이 막혀 있어도 동작에는 지장이 없다.
        }
        // 보던 파일이 여전히 바뀐 상태면 그대로 다시 보여 주고, 아니면 선택을 푼다.
        const previous = selectedRef.current;
        const still = previous && result.changes.find((c) => c.path === previous.path);
        if (still) {
          void showDiff(result.root, still);
        } else {
          selectedRef.current = null;
          setSelected(null);
          setNotice({ text: result.changes.length === 0 ? "" : "왼쪽에서 파일을 선택하세요." });
        }
      } catch (e) {
        if (seq !== statusSeq.current) return;
        setStatus(null);
        selectedRef.current = null;
        setSelected(null);
        setNotice({ text: "" });
        setError(api.errorMessage(e));
      } finally {
        if (seq === statusSeq.current) setLoading(false);
      }
    },
    [call, hostId, showDiff],
  );

  // 처음 열릴 때와 파일 탐색기에서 다른 폴더가 넘어올 때 조사한다.
  useEffect(() => {
    const path = initialPath ?? recallPath(hostId);
    if (!path) return;
    setPathInput(path);
    void load(path);
    return () => {
      statusSeq.current += 1;
      diffSeq.current += 1;
    };
  }, [initialPath, hostId, load]);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const path = pathInput.trim();
    if (path) void load(path);
  };

  return (
    <div className="diff">
      <form className="files-toolbar" onSubmit={submit}>
        <button type="submit" className="icon-button" title="새로 고침" disabled={loading}>
          ⟳
        </button>
        <input
          value={pathInput}
          onChange={(e) => setPathInput(e.target.value)}
          placeholder="Git 저장소(또는 그 안의 폴더) 경로를 입력하고 Enter"
          spellCheck={false}
          aria-label="저장소 경로"
        />
        {status && <span className="diff-branch">⎇ {status.branch ?? "(detached HEAD)"}</span>}
      </form>

      <div className="diff-body">
        <div className="diff-list">
          {error ? (
            <div className="file-note error">{error}</div>
          ) : loading && !status ? (
            <div className="file-note">불러오는 중…</div>
          ) : !status ? (
            <div className="file-note">
              저장소 경로를 입력하세요. 파일 탐색기의 ± 버튼으로도 열 수 있습니다.
            </div>
          ) : status.changes.length === 0 ? (
            <div className="file-note">변경사항이 없습니다.</div>
          ) : (
            status.changes.map((change) => (
              <button
                key={change.path}
                className={`file-row ${selected?.path === change.path ? "selected" : ""}`}
                title={change.oldPath ? `${change.oldPath} → ${change.path}` : change.path}
                onClick={() => void showDiff(status.root, change)}
              >
                <span className={`diff-badge ${change.kind}`} title={BADGE[change.kind].title}>
                  {BADGE[change.kind].label}
                </span>
                <span className="file-name">{change.path}</span>
              </button>
            ))
          )}
        </div>
        <div className="diff-editor">
          <div className="editor-surface" ref={containerRef} />
          {notice && (
            <div className="editor-status diff-notice">
              <span className={notice.error ? "form-error" : undefined}>{notice.text}</span>
            </div>
          )}
        </div>
      </div>
      {prompt}
    </div>
  );
}
