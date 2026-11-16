import { useEffect, useRef, useState } from "react";
import * as api from "../api";
import { BASE_OPTIONS, createModel, fileUri, monaco } from "../monaco";
import { LOCAL_ID, type Host } from "../types";
import { useConnectRetry } from "./useConnectRetry";

interface Props {
  /** null이면 로컬 머신 */
  host: Host | null;
  path: string;
  active: boolean;
  /** 저장하지 않은 변경이 생기거나 없어질 때 호출된다. */
  onDirtyChange: (dirty: boolean) => void;
}

type Status = { kind: "loading" } | { kind: "ready" } | { kind: "error"; message: string };

/** 코드 에디터 탭. 로컬 파일과 원격(SFTP) 파일을 같은 방식으로 열고 저장한다. */
export default function EditorView({ host, path, active, onDirtyChange }: Props) {
  const hostId = host?.id ?? LOCAL_ID;
  const containerRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  /** 마지막으로 저장된(또는 불러온) 시점의 버전. 현재 버전과 다르면 수정된 것이다. */
  const savedVersion = useRef(0);
  const [status, setStatus] = useState<Status>({ kind: "loading" });
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [language, setLanguage] = useState("");
  const [reload, setReload] = useState(0);
  const { call, prompt } = useConnectRetry(host);

  // 부모가 매 렌더마다 새 콜백을 넘겨도 에디터를 다시 만들지 않도록 ref로 들고 있는다.
  const notifyDirty = useRef(onDirtyChange);
  notifyDirty.current = onDirtyChange;
  const markDirty = (value: boolean) => {
    setDirty(value);
    notifyDirty.current(value);
  };

  const save = async () => {
    const model = editorRef.current?.getModel();
    if (!model || saving) return;
    const version = model.getAlternativeVersionId();
    setSaving(true);
    setSaveError(null);
    try {
      await call((credentials) => api.writeFile({ hostId, path, content: model.getValue(), ...credentials }));
      savedVersion.current = version;
      // 저장하는 동안 더 입력했다면 여전히 수정된 상태다.
      markDirty(model.getAlternativeVersionId() !== version);
    } catch (e) {
      setSaveError(api.errorMessage(e));
    } finally {
      setSaving(false);
    }
  };
  // Monaco 단축키 콜백이 항상 최신 save를 부르게 한다.
  const saveRef = useRef(save);
  saveRef.current = save;

  // 파일을 읽어 에디터를 만든다. 탭이 닫히면 에디터와 모델을 정리한다.
  useEffect(() => {
    let cancelled = false;
    let editor: monaco.editor.IStandaloneCodeEditor | null = null;
    let model: monaco.editor.ITextModel | null = null;
    setStatus({ kind: "loading" });

    call((credentials) => api.readFile({ hostId, path, ...credentials }))
      .then((text) => {
        if (cancelled) return;
        model = createModel(text, fileUri(hostId, path));
        editor = monaco.editor.create(containerRef.current!, { ...BASE_OPTIONS, model });
        editorRef.current = editor;
        savedVersion.current = model.getAlternativeVersionId();
        setLanguage(model.getLanguageId());

        const current = model;
        model.onDidChangeContent(() => {
          const changed = current.getAlternativeVersionId() !== savedVersion.current;
          setDirty(changed);
          notifyDirty.current(changed);
        });
        editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => void saveRef.current());
        setStatus({ kind: "ready" });
        editor.focus();
      })
      .catch((e) => {
        if (!cancelled) setStatus({ kind: "error", message: api.errorMessage(e) });
      });

    return () => {
      cancelled = true;
      editor?.dispose();
      model?.dispose();
      editorRef.current = null;
      notifyDirty.current(false);
    };
  }, [call, hostId, path, reload]);

  // 탭으로 돌아오면 바로 입력할 수 있게 한다.
  useEffect(() => {
    if (active && status.kind === "ready") editorRef.current?.focus();
  }, [active, status.kind]);

  return (
    <div className="editor">
      <div className="editor-surface" ref={containerRef} />
      {status.kind !== "ready" && (
        <div className="editor-status">
          {status.kind === "loading" ? (
            "불러오는 중…"
          ) : (
            <>
              <span className="form-error">{status.message}</span>
              <button className="btn-primary" onClick={() => setReload((n) => n + 1)}>
                다시 시도
              </button>
            </>
          )}
        </div>
      )}
      <footer className="editor-footer">
        <span className="editor-path" title={path}>
          {path}
        </span>
        {saveError && <span className="form-error">저장 실패: {saveError}</span>}
        <span>{language}</span>
        <span>{saving ? "저장 중…" : dirty ? "수정됨" : "저장됨"}</span>
        <button
          className="btn-primary small"
          disabled={status.kind !== "ready" || !dirty || saving}
          onClick={() => void save()}
          title="저장 (Ctrl/⌘+S)"
        >
          저장
        </button>
      </footer>
      {prompt}
    </div>
  );
}
