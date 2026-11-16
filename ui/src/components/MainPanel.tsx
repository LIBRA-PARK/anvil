import type { Host, Tab } from "../types";

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
  terminal: "M2: xterm.js + russh 기반 SSH 터미널이 들어갈 자리입니다.",
  files: "M3: SFTP 파일 트리가 들어갈 자리입니다.",
  editor: "M4: Monaco 코드 에디터가 들어갈 자리입니다.",
  diff: "M5: Monaco Diff 뷰어가 들어갈 자리입니다.",
};

/** 탭 바 + 작업 영역 */
export function MainPanel({ tabs, activeTabId, hosts, onActivate, onClose }: Props) {
  const active = tabs.find((t) => t.id === activeTabId);

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
        {!active || active.kind === "welcome" ? (
          <div className="welcome">
            <h1>⚒️ Anvil</h1>
            <p>SSH · SFTP · Git · 에디터를 하나로.</p>
            <ul>
              <li>
                왼쪽 <b>+</b> 버튼으로 원격 호스트를 추가하세요.
              </li>
              <li>호스트를 선택하고 채널을 눌러 터미널, 파일, Git을 여세요.</li>
              <li>등록된 호스트: {hosts.length}개</li>
            </ul>
          </div>
        ) : (
          <div className="placeholder">{PLACEHOLDER[active.kind]}</div>
        )}
      </div>
    </main>
  );
}
