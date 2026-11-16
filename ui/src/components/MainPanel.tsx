import { LOCAL_ID, type Host, type Tab } from "../types";
import { FilesView } from "./FilesView";
import { TerminalView } from "./TerminalView";

interface Props {
  tabs: Tab[];
  activeTabId: string;
  hosts: Host[];
  onActivate: (id: string) => void;
  onClose: (id: string) => void;
}

// 각 기능은 이후 마일스톤에서 실제 컴포넌트로 교체된다.
const PLACEHOLDER: Record<Tab["kind"], string> = {
  welcome: "",
  terminal: "로컬 터미널은 아직 지원하지 않습니다. 원격 호스트를 추가하면 SSH 터미널을 열 수 있습니다.",
  files: "",
  editor: "M4: Monaco 코드 에디터가 들어갈 자리입니다.",
  diff: "M5: Monaco Diff 뷰어가 들어갈 자리입니다.",
};

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
export function MainPanel({ tabs, activeTabId, hosts, onActivate, onClose }: Props) {
  const hasActive = tabs.some((t) => t.id === activeTabId);

  const content = (tab: Tab, active: boolean) => {
    if (tab.kind === "welcome") return <Welcome hostCount={hosts.length} />;
    if (tab.kind === "terminal" && tab.hostId !== LOCAL_ID) {
      const host = hosts.find((h) => h.id === tab.hostId);
      if (host) return <TerminalView host={host} active={active} />;
    }
    if (tab.kind === "files") {
      if (tab.hostId === LOCAL_ID) return <FilesView host={null} />;
      const host = hosts.find((h) => h.id === tab.hostId);
      if (host) return <FilesView host={host} />;
    }
    return <div className="placeholder">{PLACEHOLDER[tab.kind]}</div>;
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
            <span className="tab-title">{t.title}</span>
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
