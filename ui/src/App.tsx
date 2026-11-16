import { useEffect, useRef, useState } from "react";
import * as api from "./api";
import { LOCAL_ID, type DirEntry, type Host, type NewHost, type Tab } from "./types";
import { HostBar } from "./components/HostBar";
import { ChannelSidebar } from "./components/ChannelSidebar";
import { MainPanel } from "./components/MainPanel";
import { TerminalPanel } from "./components/TerminalPanel";
import { AddHostModal } from "./components/AddHostModal";
import { usePersistentNumber } from "./components/usePersistentNumber";

const WELCOME_TAB: Tab = { id: "welcome", hostId: LOCAL_ID, kind: "welcome", title: "시작" };

/** 터미널 탭은 아래 패널에, 나머지 탭은 위 편집기 영역에 표시된다. */
const isTerminal = (tab: Tab) => tab.kind === "terminal";

export default function App() {
  const [hosts, setHosts] = useState<Host[]>([]);
  const [selectedId, setSelectedId] = useState<string>(LOCAL_ID);
  /** 한 번이라도 선택한 호스트. 이들의 탐색기(와 접속)는 호스트를 오가도 유지한다. */
  const [visited, setVisited] = useState<string[]>([LOCAL_ID]);
  /** 모든 탭. 편집기 영역 탭과 터미널이 한 배열에 있고, 종류로 표시 위치가 갈린다. */
  const [tabs, setTabs] = useState<Tab[]>([WELCOME_TAB]);
  /** 편집기 영역에서 보이는 탭 */
  const [activeTabId, setActiveTabId] = useState<string>(WELCOME_TAB.id);
  /** 터미널 패널에서 보이는 터미널 */
  const [activeTerminalId, setActiveTerminalId] = useState("");
  const [panelOpen, setPanelOpen] = useState(true);
  /** 터미널을 고를 때마다 올려서, 이미 보이는 터미널도 입력 포커스를 다시 가져오게 한다. */
  const [terminalFocus, setTerminalFocus] = useState(0);
  const [sidebarWidth, setSidebarWidth] = usePersistentNumber("anvil.layout.sidebarWidth", 260);
  const [panelHeight, setPanelHeight] = usePersistentNumber("anvil.layout.panelHeight", 280);
  const [showAdd, setShowAdd] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .listHosts()
      .then(setHosts)
      .catch((e) => setError(api.errorMessage(e)));
  }, []);

  /** 저장하지 않은 변경이 있는 에디터 탭 */
  const [dirtyTabs, setDirtyTabs] = useState<ReadonlySet<string>>(new Set());
  const handleDirtyChange = (tabId: string, dirty: boolean) => {
    setDirtyTabs((prev) => {
      if (prev.has(tabId) === dirty) return prev;
      const next = new Set(prev);
      if (dirty) next.add(tabId);
      else next.delete(tabId);
      return next;
    });
  };

  // 시작할 때 새 버전을 확인한다. 실패(오프라인 등)는 조용히 넘어간다.
  const [update, setUpdate] = useState<api.AvailableUpdate | null>(null);
  const [updating, setUpdating] = useState(false);
  useEffect(() => {
    api
      .checkForUpdate()
      .then(setUpdate)
      .catch((e) => console.warn("업데이트 확인 실패:", e));
  }, []);
  const installUpdate = async () => {
    if (!update) return;
    const unsaved = dirtyTabs.size;
    if (
      unsaved > 0 &&
      !confirm(`저장하지 않은 파일 ${unsaved}개가 있습니다. 업데이트하면 앱이 다시 시작됩니다. 계속할까요?`)
    ) {
      return;
    }
    setUpdating(true);
    try {
      await update.install();
    } catch (e) {
      setError(`업데이트 실패: ${api.errorMessage(e)}`);
      setUpdating(false);
    }
  };

  const selectedHost = hosts.find((h) => h.id === selectedId) ?? null;
  const hostName = (hostId: string) => hosts.find((h) => h.id === hostId)?.name ?? "로컬";
  const editorTabs = tabs.filter((t) => !isTerminal(t));
  const terminalTabs = tabs.filter(isTerminal);

  /** 호스트를 선택한다. 처음 선택하는 호스트는 탐색기가 마운트되면서 바로 접속을 시작한다. */
  const selectHost = (id: string) => {
    setSelectedId(id);
    setVisited((prev) => (prev.includes(id) ? prev : [...prev, id]));
  };

  const handleAdd = async (input: NewHost) => {
    const host = await api.addHost(input);
    setHosts((prev) => [...prev, host]);
    selectHost(host.id);
    setShowAdd(false);
  };

  const handleRemove = async (id: string) => {
    const unsaved = tabs.filter((t) => t.hostId === id && dirtyTabs.has(t.id)).length;
    if (unsaved > 0 && !confirm(`저장하지 않은 파일 ${unsaved}개가 있습니다. 그래도 삭제할까요?`)) return;
    try {
      await api.removeHost(id);
      const remaining = tabs.filter((t) => t.hostId !== id);
      const last = (group: Tab[]) => group[group.length - 1]?.id ?? "";
      setHosts((prev) => prev.filter((h) => h.id !== id));
      setVisited((prev) => prev.filter((v) => v !== id));
      setTabs(remaining);
      // 보고 있던 탭이나 터미널이 함께 닫혔다면 남은 것으로 옮긴다.
      if (!remaining.some((t) => t.id === activeTabId)) {
        setActiveTabId(last(remaining.filter((t) => !isTerminal(t))));
      }
      if (!remaining.some((t) => t.id === activeTerminalId)) {
        setActiveTerminalId(last(remaining.filter(isTerminal)));
      }
      setSelectedId(LOCAL_ID);
    } catch (e) {
      setError(api.errorMessage(e));
    }
  };

  /** 터미널을 아래 패널에 보이게 하고 입력 포커스를 준다. (패널이 접혀 있으면 펼친다) */
  const activateTerminal = (id: string) => {
    setActiveTerminalId(id);
    setPanelOpen(true);
    setTerminalFocus((n) => n + 1);
  };

  /** 터미널은 한 호스트에 여러 세션을 열 수 있으므로 누를 때마다 새로 만든다. */
  const terminalSeq = useRef(new Map<string, number>());
  const openTerminal = (hostId: string) => {
    const n = (terminalSeq.current.get(hostId) ?? 0) + 1;
    terminalSeq.current.set(hostId, n);
    const id = `${hostId}:terminal:${n}`;
    setTabs([...tabs, { id, hostId, kind: "terminal", title: `${hostName(hostId)} · 셸 ${n}` }]);
    activateTerminal(id);
  };

  /** 파일은 경로마다 탭 하나. 이미 열려 있으면 그 탭으로 이동한다. */
  const openFile = (hostId: string, entry: DirEntry) => {
    const id = `${hostId}:editor:${entry.path}`;
    if (!tabs.some((t) => t.id === id)) {
      setTabs([...tabs, { id, hostId, kind: "editor", title: entry.name, path: entry.path }]);
    }
    setActiveTabId(id);
  };

  /**
   * Diff 탭은 호스트당 하나.
   * `path`를 주면 그 폴더의 저장소를 조사하고, 없으면 마지막에 보던 저장소를 그대로 보여 준다.
   */
  const openDiff = (hostId: string, path?: string) => {
    const id = `${hostId}:diff`;
    const existing = tabs.find((t) => t.id === id);
    if (!existing) {
      setTabs([...tabs, { id, hostId, kind: "diff", title: `${hostName(hostId)} · 변경사항`, path }]);
    } else if (path !== undefined && path !== existing.path) {
      setTabs(tabs.map((t) => (t.id === id ? { ...t, path } : t)));
    }
    setActiveTabId(id);
  };

  /** 탭을 드래그해 다른 탭의 자리로 옮긴다. (편집기 탭끼리, 터미널끼리) */
  const moveTab = (id: string, targetId: string) => {
    const from = tabs.findIndex((t) => t.id === id);
    const to = tabs.findIndex((t) => t.id === targetId);
    if (from < 0 || to < 0 || from === to) return;
    const next = [...tabs];
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    setTabs(next);
  };

  const closeTab = (id: string) => {
    const tab = tabs.find((t) => t.id === id);
    if (!tab) return;
    if (dirtyTabs.has(id) && !confirm(`'${tab.title}'의 변경 내용을 저장하지 않고 닫을까요?`)) return;
    // 닫은 것이 보이던 탭이었다면 같은 영역(편집기 또는 터미널)의 바로 옆 탭으로 옮긴다.
    const group = tabs.filter((t) => isTerminal(t) === isTerminal(tab));
    const index = group.findIndex((t) => t.id === id);
    const neighbor = (group[index + 1] ?? group[index - 1])?.id ?? "";
    setTabs(tabs.filter((t) => t.id !== id));
    if (isTerminal(tab)) {
      if (id === activeTerminalId) setActiveTerminalId(neighbor);
    } else if (id === activeTabId) {
      setActiveTabId(neighbor);
    }
  };

  return (
    <div className="app">
      <HostBar hosts={hosts} selectedId={selectedId} onSelect={selectHost} onAdd={() => setShowAdd(true)} />
      <ChannelSidebar
        host={selectedHost}
        hosts={hosts}
        visited={visited}
        terminals={terminalTabs.filter((t) => t.hostId === selectedId)}
        activeTerminalId={activeTerminalId}
        width={sidebarWidth}
        onResize={setSidebarWidth}
        onOpenFile={openFile}
        onOpenDiff={openDiff}
        onNewTerminal={openTerminal}
        onActivateTerminal={activateTerminal}
        onCloseTerminal={closeTab}
        onRemove={handleRemove}
      />
      <MainPanel
        tabs={editorTabs}
        activeTabId={activeTabId}
        hosts={hosts}
        onActivate={setActiveTabId}
        onClose={closeTab}
        onMove={moveTab}
        dirtyTabs={dirtyTabs}
        onDirtyChange={handleDirtyChange}
      >
        <TerminalPanel
          terminals={terminalTabs}
          activeId={activeTerminalId}
          hosts={hosts}
          open={panelOpen}
          height={panelHeight}
          focusTick={terminalFocus}
          onActivate={activateTerminal}
          onClose={closeTab}
          onMove={moveTab}
          onNew={() => openTerminal(selectedId)}
          onToggle={() => setPanelOpen(!panelOpen)}
          onResize={setPanelHeight}
        />
      </MainPanel>
      {showAdd && <AddHostModal onSubmit={handleAdd} onCancel={() => setShowAdd(false)} />}
      {update && (
        <div className="update-banner" role="status">
          <span title={update.notes ?? undefined}>새 버전 {update.version}을 설치할 수 있습니다.</span>
          <button className="btn-primary small" disabled={updating} onClick={installUpdate}>
            {updating ? "설치 중…" : "업데이트 후 다시 시작"}
          </button>
          <button className="btn-secondary small" disabled={updating} onClick={() => setUpdate(null)}>
            나중에
          </button>
        </div>
      )}
      {error && (
        <div className="toast" role="alert" onClick={() => setError(null)}>
          {error}
        </div>
      )}
    </div>
  );
}
