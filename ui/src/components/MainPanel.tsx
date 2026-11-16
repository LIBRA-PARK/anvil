import { lazy, Suspense, useRef, useState, type PointerEvent } from "react";
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
  /** 탭 `id`를 탭 `targetId`가 있는 자리로 옮긴다 (드래그로 순서 변경). */
  onMove: (id: string, targetId: string) => void;
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
  onMove,
  onOpenFile,
  onOpenDiff,
  dirtyTabs,
  onDirtyChange,
}: Props) {
  const hasActive = tabs.some((t) => t.id === activeTabId);
  const panes = [...tabs].sort((a, b) => a.id.localeCompare(b.id));

  // ── 탭 드래그로 순서 바꾸기 ──
  // HTML5 드래그 앤 드롭은 Tauri의 파일 드롭 처리와 충돌(특히 Windows)하므로 포인터 이벤트로 직접 구현한다.
  const tabBarRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ id: string; startX: number; moved: boolean } | null>(null);
  /** 드래그를 끝낸 직후의 click은 탭 선택으로 처리하지 않는다. */
  const suppressClick = useRef(false);
  const [draggingId, setDraggingId] = useState<string | null>(null);

  const startDrag = (e: PointerEvent<HTMLDivElement>, id: string) => {
    // 왼쪽 버튼만, 닫기 버튼 위에서는 시작하지 않는다.
    if (e.button !== 0 || (e.target as HTMLElement).closest(".tab-close")) return;
    drag.current = { id, startX: e.clientX, moved: false };
    // 포인터가 탭 밖으로 나가도 계속 따라오게 한다.
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const continueDrag = (e: PointerEvent<HTMLDivElement>) => {
    const current = drag.current;
    if (!current) return;
    if (!current.moved) {
      // 살짝 흔들린 클릭을 드래그로 오해하지 않도록 조금 움직인 뒤에 시작한다.
      if (Math.abs(e.clientX - current.startX) < 5) return;
      current.moved = true;
      setDraggingId(current.id);
      onActivate(current.id);
    }
    const order = tabs.map((t) => t.id);
    const from = order.indexOf(current.id);
    const elements = tabBarRef.current?.querySelectorAll<HTMLElement>("[data-tab-id]") ?? [];
    for (const element of elements) {
      const targetId = element.dataset.tabId!;
      if (targetId === current.id) continue;
      const rect = element.getBoundingClientRect();
      if (e.clientX < rect.left || e.clientX > rect.right) continue;
      // 상대 탭의 가운데를 넘어섰을 때만 자리를 바꾼다. (폭이 다른 탭끼리 왔다 갔다 하는 것을 막는다)
      const middle = (rect.left + rect.right) / 2;
      const to = order.indexOf(targetId);
      if ((from < to && e.clientX > middle) || (from > to && e.clientX < middle)) {
        onMove(current.id, targetId);
      }
      break;
    }
  };

  const endDrag = () => {
    if (drag.current?.moved) suppressClick.current = true;
    drag.current = null;
    setDraggingId(null);
  };

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
      <div className="tab-bar" role="tablist" ref={tabBarRef}>
        {tabs.map((t) => (
          <div
            key={t.id}
            role="tab"
            aria-selected={t.id === activeTabId}
            data-tab-id={t.id}
            className={`tab ${t.id === activeTabId ? "active" : ""} ${t.id === draggingId ? "dragging" : ""}`}
            onPointerDown={(e) => startDrag(e, t.id)}
            onPointerMove={continueDrag}
            onPointerUp={endDrag}
            onPointerCancel={endDrag}
            onClick={() => {
              if (suppressClick.current) {
                suppressClick.current = false;
                return;
              }
              onActivate(t.id);
            }}
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
        {/* 탭 순서를 바꿔도 화면(터미널·에디터)의 DOM은 옮겨지지 않도록 id 순으로 고정해 그린다. */}
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
    </main>
  );
}
