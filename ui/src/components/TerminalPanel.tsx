import { LOCAL_ID, type Host, type Tab } from "../types";
import { Resizer } from "./Resizer";
import { TabBar } from "./TabBar";
import { TerminalView } from "./TerminalView";

interface Props {
  /** 열려 있는 모든 터미널 (모든 호스트) */
  terminals: Tab[];
  activeId: string;
  hosts: Host[];
  /** 펼쳐져 있는지. 접으면 탭 줄만 남고 셸은 계속 돈다. */
  open: boolean;
  height: number;
  /** 값이 바뀌면 활성 터미널이 입력 포커스를 다시 가져온다. */
  focusTick: number;
  onActivate: (id: string) => void;
  onClose: (id: string) => void;
  onMove: (id: string, targetId: string) => void;
  /** 지금 선택된 호스트에 새 터미널 열기 */
  onNew: () => void;
  onToggle: () => void;
  onResize: (height: number) => void;
}

/** 편집기 아래의 터미널 패널. 터미널이 하나도 없으면 아무것도 그리지 않는다. */
export function TerminalPanel({
  terminals,
  activeId,
  hosts,
  open,
  height,
  focusTick,
  onActivate,
  onClose,
  onMove,
  onNew,
  onToggle,
  onResize,
}: Props) {
  if (terminals.length === 0) return null;
  // 탭 순서를 바꿔도 터미널 화면의 DOM은 옮겨지지 않도록 id 순으로 고정해 그린다.
  const panes = [...terminals].sort((a, b) => a.id.localeCompare(b.id));

  return (
    <>
      {open && (
        <Resizer
          axis="y"
          invert
          value={height}
          min={100}
          // 편집기 영역이 완전히 가려지지 않을 만큼만 키울 수 있다.
          max={Math.max(100, window.innerHeight - 160)}
          label="터미널 패널 높이 조절"
          onChange={onResize}
        />
      )}
      <section className="terminal-panel" style={open ? { height } : undefined} aria-label="터미널">
        <div className="panel-header">
          <span className="panel-title">TERMINAL</span>
          <TabBar
            tabs={terminals}
            activeId={activeId}
            label="터미널 탭"
            onActivate={onActivate}
            onClose={onClose}
            onMove={onMove}
          />
          <button className="panel-action" title="새 터미널" onClick={onNew}>
            ＋
          </button>
          <button className="panel-action" title={open ? "패널 접기" : "패널 펼치기"} onClick={onToggle}>
            {open ? "▾" : "▴"}
          </button>
        </div>
        {/* 접어도 세션과 화면 내용이 유지되도록 언마운트하지 않고 숨기기만 한다. */}
        <div className="panel-body" hidden={!open}>
          {panes.map((t) => {
            const active = t.id === activeId;
            const host = hosts.find((h) => h.id === t.hostId) ?? null;
            // 원격인데 호스트가 없으면(삭제 직후 등) 아무것도 그리지 않는다.
            if (t.hostId !== LOCAL_ID && !host) return null;
            return (
              <div key={t.id} role="tabpanel" className="tab-pane" hidden={!active}>
                <TerminalView host={host} active={open && active} focusTick={focusTick} />
              </div>
            );
          })}
        </div>
      </section>
    </>
  );
}
