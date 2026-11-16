import { LOCAL_ID, type Host, type TabKind } from "../types";

interface Props {
  /** null이면 로컬 머신 */
  host: Host | null;
  onOpen: (hostId: string, kind: TabKind, title: string) => void;
  onRemove: (id: string) => void;
}

/** 선택한 호스트의 기능 채널 목록 (Discord의 채널 사이드바 위치) */
export function ChannelSidebar({ host, onOpen, onRemove }: Props) {
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
          {channel("terminal", "#", "셸")}
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
              if (confirm(`'${host.name}' 호스트를 삭제할까요?`)) onRemove(host.id);
            }}
          >
            호스트 삭제
          </button>
        </footer>
      )}
    </aside>
  );
}
