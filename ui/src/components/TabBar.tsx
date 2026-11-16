import { useRef, useState, type PointerEvent } from "react";
import type { Tab } from "../types";

interface Props {
  tabs: Tab[];
  activeId: string;
  /** 화면 읽기 프로그램용 이름 (예: "편집기 탭") */
  label: string;
  onActivate: (id: string) => void;
  onClose: (id: string) => void;
  /** 탭 `id`를 탭 `targetId`가 있는 자리로 옮긴다 (드래그로 순서 변경). */
  onMove: (id: string, targetId: string) => void;
  /** 저장하지 않은 변경이 있는 탭 id */
  dirtyTabs?: ReadonlySet<string>;
}

/** 탭 한 줄. 편집기 영역과 터미널 패널이 함께 쓴다. 드래그로 순서를 바꿀 수 있다. */
export function TabBar({ tabs, activeId, label, onActivate, onClose, onMove, dirtyTabs }: Props) {
  // HTML5 드래그 앤 드롭은 Tauri의 파일 드롭 처리와 충돌(특히 Windows)하므로 포인터 이벤트로 직접 구현한다.
  const barRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ id: string; startX: number; moved: boolean } | null>(null);
  /** 드래그를 끝낸 직후의 click은 탭 선택으로 처리하지 않는다. */
  const suppressClick = useRef(false);
  const [draggingId, setDraggingId] = useState<string | null>(null);

  const startDrag = (e: PointerEvent<HTMLDivElement>, id: string) => {
    // 왼쪽 버튼만, 닫기 버튼 위에서는 시작하지 않는다.
    if (e.button !== 0 || (e.target as HTMLElement).closest(".tab-close")) return;
    drag.current = { id, startX: e.clientX, moved: false };
    // 포인터가 탭 밖으로 나가도 계속 따라오게 한다.
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const continueDrag = (e: PointerEvent<HTMLDivElement>) => {
    const current = drag.current;
    if (!current) return;
    if (!current.moved) {
      // 살짝 흔들린 클릭을 드래그로 오해하지 않도록 조금 움직인 뒤에 시작한다.
      if (Math.abs(e.clientX - current.startX) < 5) return;
      current.moved = true;
      setDraggingId(current.id);
      onActivate(current.id);
    }
    const order = tabs.map((t) => t.id);
    const from = order.indexOf(current.id);
    const elements = barRef.current?.querySelectorAll<HTMLElement>("[data-tab-id]") ?? [];
    for (const element of elements) {
      const targetId = element.dataset.tabId!;
      if (targetId === current.id) continue;
      const rect = element.getBoundingClientRect();
      if (e.clientX < rect.left || e.clientX > rect.right) continue;
      // 상대 탭의 가운데를 넘어섰을 때만 자리를 바꾼다. (폭이 다른 탭끼리 왔다 갔다 하는 것을 막는다)
      const middle = (rect.left + rect.right) / 2;
      const to = order.indexOf(targetId);
      if ((from < to && e.clientX > middle) || (from > to && e.clientX < middle)) {
        onMove(current.id, targetId);
      }
      break;
    }
  };

  const endDrag = () => {
    if (drag.current?.moved) suppressClick.current = true;
    drag.current = null;
    setDraggingId(null);
  };

  return (
    <div className="tab-bar" role="tablist" aria-label={label} ref={barRef}>
      {tabs.map((t) => (
        <div
          key={t.id}
          role="tab"
          aria-selected={t.id === activeId}
          data-tab-id={t.id}
          className={`tab ${t.id === activeId ? "active" : ""} ${t.id === draggingId ? "dragging" : ""}`}
          onPointerDown={(e) => startDrag(e, t.id)}
          onPointerMove={continueDrag}
          onPointerUp={endDrag}
          onPointerCancel={endDrag}
          onClick={() => {
            if (suppressClick.current) {
              suppressClick.current = false;
              return;
            }
            onActivate(t.id);
          }}
        >
          <span className="tab-title" title={t.path}>
            {dirtyTabs?.has(t.id) && <span className="tab-dirty">● </span>}
            {t.title}
          </span>
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
  );
}
