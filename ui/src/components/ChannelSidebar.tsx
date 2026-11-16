import { LOCAL_ID, type Host, type Tab, type TabKind } from "../types";

interface Props {
  /** null이면 로컬 머신 */
  host: Host | null;
  /** 이 호스트에 열려 있는 터미널 탭 */
  terminals: Tab[];
  activeTabId: string;
  onOpen: (hostId: string, kind: TabKind, title: string) => void;
  /** 호스트 id와 탭 제목에 쓸 이름 */
  onNewTerminal: (hostId: string, name: string) => void;
  onActivate: (tabId: string) => void;
  onRemove: (id: string) => void;
}

/** 선택한 호스트의 기능 채널 목록 (Discord의 채널 사이드바 위치) */
export function ChannelSidebar({
  host,
  terminals,
  activeTabId,
  onOpen,
  onNewTerminal,
  onActivate,
  onRemove,
}: Props) {
  const hostId = host?.id ?? LOCAL_ID;
  const prefix = host ? host.name : "로컬";

  const channel = (kind: TabKind, icon: string, label: string) => (
    <button className="channel" onClick={() => onOpen(hostId, kind, `${prefix} · ${label}`)}>
      <span className="channel-icon">{icon}</span>
      {label}
    </button>
  );

  return (
    <aside className="sidebar">
      <header className="sidebar-header">
        <div className="sidebar-title">{host ? host.name : "로컬 머신"}</div>
        {host && (
          <div className="sidebar-sub">
            {host.username}@{host.address}:{host.port}
          </div>
        )}
      </header>

      <div className="sidebar-body">
        <section>
          <h3 className="section-title">터미널</h3>
          {/* 열려 있는 세션은 채널처럼 나열하고, 누르면 그 탭으로 이동한다. */}
          {terminals.map((t) => (
            <button
              key={t.id}
              className={`channel ${t.id === activeTabId ? "active" : ""}`}
              onClick={() => onActivate(t.id)}
            >
              <span className="channel-icon">#</span>
              {t.title.slice(prefix.length + 3)}
            </button>
          ))}
          <button className="channel" onClick={() => onNewTerminal(hostId, prefix)}>
            <span className="channel-icon">+</span>새 터미널
          </button>
        </section>
        <section>
          <h3 className="section-title">파일</h3>
          {channel("files", "📁", host ? "SFTP 탐색기" : "파일 탐색기")}
        </section>
        <section>
          <h3 className="section-title">Git</h3>
          {channel("diff", "±", "변경사항")}
        </section>
      </div>

      {host && (
        <footer className="sidebar-footer">
          <button
            className="danger-link"
            onClick={() => {
              const open = terminals.length > 0 ? `\n열려 있는 터미널 ${terminals.length}개도 닫힙니다.` : "";
              if (confirm(`'${host.name}' 호스트를 삭제할까요?${open}`)) onRemove(host.id);
            }}
          >
            호스트 삭제
          </button>
        </footer>
      )}
    </aside>
  );
}
