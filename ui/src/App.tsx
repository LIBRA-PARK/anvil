import { useCallback, useEffect, useState } from "react";
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
      setHosts((prev) => prev.filter((h) => h.id !== id));
      setTabs((prev) => prev.filter((t) => t.hostId !== id));
      setSelectedId(LOCAL_ID);
    } catch (e) {
      setError(api.errorMessage(e));
    }
  };

  /** 같은 호스트·종류의 탭이 이미 있으면 그 탭으로 이동하고, 없으면 새로 연다. */
  const openTab = useCallback((hostId: string, kind: TabKind, title: string) => {
    const id = `${hostId}:${kind}`;
    setTabs((prev) => (prev.some((t) => t.id === id) ? prev : [...prev, { id, hostId, kind, title }]));
    setActiveTabId(id);
  }, []);

  const closeTab = (id: string) => {
    setTabs((prev) => {
      const next = prev.filter((t) => t.id !== id);
      if (id === activeTabId) setActiveTabId(next[next.length - 1]?.id ?? "");
      return next;
    });
  };

  return (
    <div className="app">
      <HostBar
        hosts={hosts}
        selectedId={selectedId}
        onSelect={setSelectedId}
        onAdd={() => setShowAdd(true)}
      />
      <ChannelSidebar host={selectedHost} onOpen={openTab} onRemove={handleRemove} />
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
