import { lazy, Suspense, type ReactNode } from "react";
import { LOCAL_ID, type Host, type Tab } from "../types";
import { TabBar } from "./TabBar";

// Monaco는 번들이 크므로 에디터·Diff 탭을 처음 열 때 불러온다.
const EditorView = lazy(() => import("./EditorView"));
const DiffView = lazy(() => import("./DiffView"));

interface Props {
  /** 편집기 영역에 표시할 탭 (터미널 제외) */
  tabs: Tab[];
  activeTabId: string;
  hosts: Host[];
  onActivate: (id: string) => void;
  onClose: (id: string) => void;
  /** 탭 `id`를 탭 `targetId`가 있는 자리로 옮긴다 (드래그로 순서 변경). */
  onMove: (id: string, targetId: string) => void;
  /** 저장하지 않은 변경이 있는 탭 id */
  dirtyTabs: ReadonlySet<string>;
  onDirtyChange: (tabId: string, dirty: boolean) => void;
  /** 편집기 영역 아래에 붙는 터미널 패널 */
  children: ReactNode;
}

function Welcome({ hostCount }: { hostCount: number }) {
  return (
    <div className="welcome">
      <h1>⚒️ Anvil</h1>
      <p>SSH · SFTP · Git · 에디터를 하나로.</p>
      <ul>
        <li>
          맨 왼쪽 <b>+</b> 버튼으로 원격 호스트를 추가하세요. 💻는 이 컴퓨터입니다.
        </li>
        <li>
          호스트를 선택하면 사이드바의 <b>EXPLORER</b>에 파일 목록이 나타납니다. 파일을 누르면 여기에 편집기로
          열립니다.
        </li>
        <li>
          사이드바 <b>TERMINAL</b>의 <b>+</b>로 터미널을 열면 편집기 아래 패널에 나타납니다.
        </li>
        <li>등록된 호스트: {hostCount}개</li>
      </ul>
    </div>
  );
}

/** 메인 패널: 위는 편집기 영역(탭 + 에디터·Diff), 아래는 터미널 패널 */
export function MainPanel({
  tabs,
  activeTabId,
  hosts,
  onActivate,
  onClose,
  onMove,
  dirtyTabs,
  onDirtyChange,
  children,
}: Props) {
  const hasActive = tabs.some((t) => t.id === activeTabId);
  // 탭 순서를 바꿔도 에디터의 DOM은 옮겨지지 않도록 id 순으로 고정해 그린다.
  const panes = [...tabs].sort((a, b) => a.id.localeCompare(b.id));

  const content = (tab: Tab, active: boolean) => {
    if (tab.kind === "welcome") return <Welcome hostCount={hosts.length} />;
    // 로컬이면 host는 null. 원격인데 호스트가 없으면(삭제 직후 등) 아무것도 그리지 않는다.
    const host = hosts.find((h) => h.id === tab.hostId) ?? null;
    const known = tab.hostId === LOCAL_ID || host !== null;
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
      <div className="editor-area">
        <TabBar
          tabs={tabs}
          activeId={activeTabId}
          label="편집기 탭"
          onActivate={onActivate}
          onClose={onClose}
          onMove={onMove}
          dirtyTabs={dirtyTabs}
        />
        <div className="workspace">
          {/* 수정 중인 내용이 탭 전환에도 유지되도록, 모든 탭을 마운트해 두고 숨기기만 한다. */}
          {panes.map((t) => {
            const active = t.id === activeTabId;
            return (
              <div key={t.id} role="tabpanel" className="tab-pane" hidden={!active}>
                {content(t, active)}
              </div>
            );
          })}
          {!hasActive && <Welcome hostCount={hosts.length} />}
        </div>
      </div>
      {children}
    </main>
  );
}
