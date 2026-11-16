import { lazy, Suspense } from "react";
import { LOCAL_ID, type DirEntry, type Host, type Tab } from "../types";
import { FilesView } from "./FilesView";
import { TerminalView } from "./TerminalView";

// Monaco는 번들이 크므로 에디터·Diff 탭을 처음 열 때 불러온다.
const EditorView = lazy(() => import("./EditorView"));
const DiffView = lazy(() => import("./DiffView"));

interface Props {
  tabs: Tab[];
  activeTabId: string;
  hosts: Host[];
  onActivate: (id: string) => void;
  onClose: (id: string) => void;
  onOpenFile: (hostId: string, entry: DirEntry) => void;
  /** 파일 탐색기에서 보고 있는 폴더의 Git 변경사항 열기 */
  onOpenDiff: (hostId: string, path: string) => void;
  /** 저장하지 않은 변경이 있는 탭 id */
  dirtyTabs: ReadonlySet<string>;
  onDirtyChange: (tabId: string, dirty: boolean) => void;
}

function Welcome({ hostCount }: { hostCount: number }) {
  return (
    <div className="welcome">
      <h1>⚒️ Anvil</h1>
      <p>SSH · SFTP · Git · 에디터를 하나로.</p>
      <ul>
        <li>
          왼쪽 <b>+</b> 버튼으로 원격 호스트를 추가하세요.
        </li>
        <li>호스트를 선택하고 채널을 눌러 터미널, 파일, Git을 여세요.</li>
        <li>등록된 호스트: {hostCount}개</li>
      </ul>
    </div>
  );
}

/** 탭 바 + 작업 영역 */
export function MainPanel({
  tabs,
  activeTabId,
  hosts,
  onActivate,
  onClose,
  onOpenFile,
  onOpenDiff,
  dirtyTabs,
  onDirtyChange,
}: Props) {
  const hasActive = tabs.some((t) => t.id === activeTabId);

  const content = (tab: Tab, active: boolean) => {
    if (tab.kind === "welcome") return <Welcome hostCount={hosts.length} />;
    // 로컬이면 host는 null. 원격인데 호스트가 없으면(삭제 직후 등) 아무것도 그리지 않는다.
    const host = hosts.find((h) => h.id === tab.hostId) ?? null;
    const known = tab.hostId === LOCAL_ID || host !== null;
    if (tab.kind === "terminal" && known) return <TerminalView host={host} active={active} />;
    if (tab.kind === "files" && known) {
      return (
        <FilesView
          host={host}
          onOpenFile={(entry) => onOpenFile(tab.hostId, entry)}
          onOpenDiff={(path) => onOpenDiff(tab.hostId, path)}
        />
      );
    }
    if (tab.kind === "diff" && known) {
      return (
        <Suspense fallback={<div className="placeholder">Diff 뷰어를 불러오는 중…</div>}>
          <DiffView host={host} initialPath={tab.path} />
        </Suspense>
      );
    }
    if (tab.kind === "editor" && known && tab.path !== undefined) {
      return (
        <Suspense fallback={<div className="placeholder">에디터를 불러오는 중…</div>}>
          <EditorView
            host={host}
            path={tab.path}
            active={active}
            onDirtyChange={(dirty) => onDirtyChange(tab.id, dirty)}
          />
        </Suspense>
      );
    }
    return <div className="placeholder" />;
  };

  return (
    <main className="main">
      <div className="tab-bar" role="tablist">
        {tabs.map((t) => (
          <div
            key={t.id}
            role="tab"
            aria-selected={t.id === activeTabId}
            className={`tab ${t.id === activeTabId ? "active" : ""}`}
            onClick={() => onActivate(t.id)}
          >
            <span className="tab-title" title={t.path}>
              {dirtyTabs.has(t.id) && <span className="tab-dirty">● </span>}
              {t.title}
            </span>
            <button
              className="tab-close"
              aria-label={`${t.title} 닫기`}
              onClick={(e) => {
                e.stopPropagation();
                onClose(t.id);
              }}
            >
              ×
            </button>
          </div>
        ))}
      </div>

      <div className="workspace">
        {/* 터미널 세션과 화면 내용이 탭 전환에도 유지되도록, 모든 탭을 마운트해 두고 숨기기만 한다. */}
        {tabs.map((t) => {
          const active = t.id === activeTabId;
          return (
            <div key={t.id} role="tabpanel" className="tab-pane" hidden={!active}>
              {content(t, active)}
            </div>
          );
        })}
        {!hasActive && <Welcome hostCount={hosts.length} />}
      </div>
    </main>
  );
}
