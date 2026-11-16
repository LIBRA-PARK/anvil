import { useEffect, useRef, useState } from "react";
import * as api from "./api";
import { LOCAL_ID, type DirEntry, type Host, type NewHost, type Tab, type TabKind } from "./types";
import { HostBar } from "./components/HostBar";
import { ChannelSidebar } from "./components/ChannelSidebar";
import { MainPanel } from "./components/MainPanel";
import { AddHostModal } from "./components/AddHostModal";

const WELCOME_TAB: Tab = { id: "welcome", hostId: LOCAL_ID, kind: "welcome", title: "시작" };

export default function App() {
  const [hosts, setHosts] = useState<Host[]>([]);
  const [selectedId, setSelectedId] = useState<string>(LOCAL_ID);
  const [tabs, setTabs] = useState<Tab[]>([WELCOME_TAB]);
  const [activeTabId, setActiveTabId] = useState<string>(WELCOME_TAB.id);
  const [showAdd, setShowAdd] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .listHosts()
      .then(setHosts)
      .catch((e) => setError(api.errorMessage(e)));
  }, []);

  // 원격 파일 시스템(SFTP)을 쓰는 탭이 호스트에 하나도 남지 않으면 세션을 정리한다.
  const fsHosts = useRef(new Set<string>());
  useEffect(() => {
    const current = new Set(
      tabs.filter((t) => t.kind === "files" || t.kind === "editor" || t.kind === "diff").map((t) => t.hostId),
    );
    for (const hostId of fsHosts.current) {
      if (!current.has(hostId)) api.closeFiles(hostId).catch(() => {});
    }
    fsHosts.current = current;
  }, [tabs]);

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

  const handleAdd = async (input: NewHost) => {
    const host = await api.addHost(input);
    setHosts((prev) => [...prev, host]);
    setSelectedId(host.id);
    setShowAdd(false);
  };

  const handleRemove = async (id: string) => {
    const unsaved = tabs.filter((t) => t.hostId === id && dirtyTabs.has(t.id)).length;
    if (unsaved > 0 && !confirm(`저장하지 않은 파일 ${unsaved}개가 있습니다. 그래도 삭제할까요?`)) return;
    try {
      await api.removeHost(id);
      const remaining = tabs.filter((t) => t.hostId !== id);
      setHosts((prev) => prev.filter((h) => h.id !== id));
      setTabs(remaining);
      // 보고 있던 탭이 함께 닫혔다면 남은 탭으로 옮긴다.
      if (!remaining.some((t) => t.id === activeTabId)) {
        setActiveTabId(remaining[remaining.length - 1]?.id ?? "");
      }
      setSelectedId(LOCAL_ID);
    } catch (e) {
      setError(api.errorMessage(e));
    }
  };

  /** 같은 호스트·종류의 탭이 이미 있으면 그 탭으로 이동하고, 없으면 새로 연다. */
  const openTab = (hostId: string, kind: TabKind, title: string) => {
    const id = `${hostId}:${kind}`;
    if (!tabs.some((t) => t.id === id)) setTabs([...tabs, { id, hostId, kind, title }]);
    setActiveTabId(id);
  };

  /** 터미널은 한 호스트에 여러 세션을 열 수 있으므로 누를 때마다 새 탭을 만든다. */
  const terminalSeq = useRef(new Map<string, number>());
  const openTerminal = (hostId: string, name: string) => {
    const n = (terminalSeq.current.get(hostId) ?? 0) + 1;
    terminalSeq.current.set(hostId, n);
    const id = `${hostId}:terminal:${n}`;
    setTabs([...tabs, { id, hostId, kind: "terminal", title: `${name} · 셸 ${n}` }]);
    setActiveTabId(id);
  };

  /** 파일은 경로마다 탭 하나. 이미 열려 있으면 그 탭으로 이동한다. */
  const openFile = (hostId: string, entry: DirEntry) => {
    const id = `${hostId}:editor:${entry.path}`;
    if (!tabs.some((t) => t.id === id)) {
      setTabs([...tabs, { id, hostId, kind: "editor", title: entry.name, path: entry.path }]);
    }
    setActiveTabId(id);
  };

  /** Diff 탭은 호스트당 하나. 이미 열려 있으면 조사할 폴더만 바꾼다. */
  const openDiff = (hostId: string, path: string) => {
    const id = `${hostId}:diff`;
    const name = hosts.find((h) => h.id === hostId)?.name ?? "로컬";
    const tab: Tab = { id, hostId, kind: "diff", title: `${name} · 변경사항`, path };
    setTabs(tabs.some((t) => t.id === id) ? tabs.map((t) => (t.id === id ? tab : t)) : [...tabs, tab]);
    setActiveTabId(id);
  };

  /** 탭을 드래그해 다른 탭의 자리로 옮긴다. */
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
    const index = tabs.findIndex((t) => t.id === id);
    if (index < 0) return;
    if (dirtyTabs.has(id) && !confirm(`'${tabs[index].title}'의 변경 내용을 저장하지 않고 닫을까요?`)) return;
    const remaining = tabs.filter((t) => t.id !== id);
    setTabs(remaining);
    // 닫은 탭이 활성 탭이었다면 바로 옆 탭으로 옮긴다.
    if (id === activeTabId) setActiveTabId((remaining[index] ?? remaining[index - 1])?.id ?? "");
  };

  return (
    <div className="app">
      <HostBar
        hosts={hosts}
        selectedId={selectedId}
        onSelect={setSelectedId}
        onAdd={() => setShowAdd(true)}
      />
      <ChannelSidebar
        host={selectedHost}
        terminals={tabs.filter((t) => t.hostId === selectedId && t.kind === "terminal")}
        activeTabId={activeTabId}
        onOpen={openTab}
        onNewTerminal={openTerminal}
        onActivate={setActiveTabId}
        onRemove={handleRemove}
      />
      <MainPanel
        tabs={tabs}
        activeTabId={activeTabId}
        hosts={hosts}
        onActivate={setActiveTabId}
        onClose={closeTab}
        onMove={moveTab}
        onOpenFile={openFile}
        onOpenDiff={openDiff}
        dirtyTabs={dirtyTabs}
        onDirtyChange={handleDirtyChange}
      />
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
