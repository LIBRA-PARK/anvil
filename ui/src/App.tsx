import { useEffect, useRef, useState } from "react";
import * as api from "./api";
import { LOCAL_ID, type Host, type NewHost, type Tab, type TabKind } from "./types";
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

  const selectedHost = hosts.find((h) => h.id === selectedId) ?? null;

  const handleAdd = async (input: NewHost) => {
    const host = await api.addHost(input);
    setHosts((prev) => [...prev, host]);
    setSelectedId(host.id);
    setShowAdd(false);
  };

  const handleRemove = async (id: string) => {
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
  const openTerminal = (host: Host) => {
    const n = (terminalSeq.current.get(host.id) ?? 0) + 1;
    terminalSeq.current.set(host.id, n);
    const id = `${host.id}:terminal:${n}`;
    setTabs([...tabs, { id, hostId: host.id, kind: "terminal", title: `${host.name} · 셸 ${n}` }]);
    setActiveTabId(id);
  };

  const closeTab = (id: string) => {
    const index = tabs.findIndex((t) => t.id === id);
    if (index < 0) return;
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
      />
      {showAdd && <AddHostModal onSubmit={handleAdd} onCancel={() => setShowAdd(false)} />}
      {error && (
        <div className="toast" role="alert" onClick={() => setError(null)}>
          {error}
        </div>
      )}
    </div>
  );
}
