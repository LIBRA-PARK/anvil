import { useState, type ReactNode } from "react";
import { LOCAL_ID, type DirEntry, type Host, type Tab } from "../types";
import { Explorer } from "./Explorer";
import { Resizer } from "./Resizer";

interface Props {
  /** 선택한 호스트. null이면 로컬 머신 */
  host: Host | null;
  hosts: Host[];
  /**
   * 한 번이라도 선택한 호스트의 id.
   * 호스트를 오가도 펼쳐 둔 폴더와 접속이 유지되도록 이들의 탐색기는 계속 마운트해 둔다.
   */
  visited: string[];
  /** 선택한 호스트에 열려 있는 터미널 */
  terminals: Tab[];
  /** 아래 패널에 지금 보이는 터미널 */
  activeTerminalId: string;
  width: number;
  onResize: (width: number) => void;
  onOpenFile: (hostId: string, entry: DirEntry) => void;
  /** `path`를 주면 그 폴더의 저장소를, 없으면 마지막에 보던 저장소를 연다. */
  onOpenDiff: (hostId: string, path?: string) => void;
  onNewTerminal: (hostId: string) => void;
  onActivateTerminal: (tabId: string) => void;
  onCloseTerminal: (tabId: string) => void;
  onRemove: (id: string) => void;
}

interface SectionProps {
  title: string;
  /** 남는 세로 공간을 모두 차지할지 (EXPLORER) */
  grow?: boolean;
  /** 제목 줄 오른쪽에 두는 버튼 */
  actions?: ReactNode;
  children: ReactNode;
}

/** 제목을 눌러 접고 펼 수 있는 사이드바 구역 */
function Section({ title, grow, actions, children }: SectionProps) {
  const [open, setOpen] = useState(true);
  return (
    <section className={`side-section ${grow && open ? "grow" : ""}`}>
      <div className="side-section-header">
        <button className="side-section-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
          <span className="file-chevron">{open ? "▼" : "▶"}</span>
          {title}
        </button>
        {actions}
      </div>
      {/* 접어도 안의 상태(펼친 폴더 등)가 유지되도록 숨기기만 한다. */}
      <div className="side-section-body" hidden={!open}>
        {children}
      </div>
    </section>
  );
}

/** 선택한 호스트의 사이드바: 파일 탐색기, 열린 터미널, Git (Discord의 채널 사이드바 위치) */
export function ChannelSidebar({
  host,
  hosts,
  visited,
  terminals,
  activeTerminalId,
  width,
  onResize,
  onOpenFile,
  onOpenDiff,
  onNewTerminal,
  onActivateTerminal,
  onCloseTerminal,
  onRemove,
}: Props) {
  const hostId = host?.id ?? LOCAL_ID;

  return (
    <aside className="sidebar" style={{ width }}>
      <header className="sidebar-header">
        <div className="sidebar-title">{host ? host.name : "로컬 머신"}</div>
        {host && (
          <div className="sidebar-sub">
            {host.username}@{host.address}:{host.port}
          </div>
        )}
      </header>

      <Section title="Explorer" grow>
        {visited.map((id) => {
          const visitedHost = hosts.find((h) => h.id === id) ?? null;
          // 삭제된 호스트는 그리지 않는다.
          if (id !== LOCAL_ID && !visitedHost) return null;
          return (
            <div key={id} className="explorer-slot" hidden={id !== hostId}>
              <Explorer
                host={visitedHost}
                onOpenFile={(entry) => onOpenFile(id, entry)}
                onOpenDiff={(path) => onOpenDiff(id, path)}
              />
            </div>
          );
        })}
      </Section>

      <Section
        title="Terminal"
        actions={
          <button className="side-section-action" title="새 터미널" onClick={() => onNewTerminal(hostId)}>
            ＋
          </button>
        }
      >
        <div className="side-list">
          {terminals.map((t) => (
            <div key={t.id} className={`side-item ${t.id === activeTerminalId ? "active" : ""}`}>
              <button className="side-item-main" onClick={() => onActivateTerminal(t.id)}>
                <span className="channel-icon">#</span>
                {/* 탭 제목은 "호스트 · 셸 n"이고, 여기서는 호스트가 이미 정해져 있으니 뒷부분만 보여 준다. */}
                {t.title.split(" · ").pop()}
              </button>
              <button
                className="side-item-close"
                title="터미널 닫기"
                aria-label={`${t.title} 닫기`}
                onClick={() => onCloseTerminal(t.id)}
              >
                ×
              </button>
            </div>
          ))}
          <button className="side-item-main" onClick={() => onNewTerminal(hostId)}>
            <span className="channel-icon">+</span>새 터미널
          </button>
        </div>
      </Section>

      <Section title="Git">
        <div className="side-list">
          <button className="side-item-main" onClick={() => onOpenDiff(hostId)}>
            <span className="channel-icon">±</span>
            변경사항
          </button>
        </div>
      </Section>

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

      <Resizer axis="x" value={width} min={180} max={640} label="사이드바 너비 조절" onChange={onResize} />
    </aside>
  );
}
