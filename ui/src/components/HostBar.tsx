import { LOCAL_ID, type Host } from "../types";

interface Props {
  hosts: Host[];
  selectedId: string;
  onSelect: (id: string) => void;
  onAdd: () => void;
}

/** 호스트 이름에서 아이콘용 이니셜 2글자를 뽑는다. */
function initials(name: string): string {
  const words = name.trim().split(/[\s\-_.]+/).filter(Boolean);
  const raw = words.length > 1 ? words[0][0] + words[1][0] : name.slice(0, 2);
  return raw.toUpperCase();
}

/** 왼쪽 세로 아이콘 바 (Discord의 서버 목록 위치) */
export function HostBar({ hosts, selectedId, onSelect, onAdd }: Props) {
  const item = (id: string, label: string, title: string, extra = "") => (
    <button
      key={id}
      className={`host-icon ${selectedId === id ? "active" : ""} ${extra}`}
      title={title}
      onClick={() => onSelect(id)}
    >
      {label}
    </button>
  );

  return (
    <nav className="host-bar" aria-label="호스트 목록">
      {item(LOCAL_ID, "💻", "로컬 머신", "local")}
      <div className="host-divider" />
      {hosts.map((h) => item(h.id, initials(h.name), `${h.name} (${h.username}@${h.address})`))}
      <button className="host-icon add" title="호스트 추가" onClick={onAdd}>
        +
      </button>
    </nav>
  );
}
