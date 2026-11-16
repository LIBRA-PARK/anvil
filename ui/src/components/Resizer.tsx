import { useRef, type PointerEvent } from "react";

interface Props {
  /** "x"는 좌우로 끌어 너비를, "y"는 위아래로 끌어 높이를 바꾼다. */
  axis: "x" | "y";
  /** 현재 크기(px) */
  value: number;
  min: number;
  max: number;
  /** 끄는 방향과 크기 변화가 반대일 때 (예: 위로 끌수록 커지는 아래쪽 패널) */
  invert?: boolean;
  /** 화면 읽기 프로그램용 이름 */
  label: string;
  onChange: (next: number) => void;
}

/** 영역 경계에 두는 크기 조절 손잡이 */
export function Resizer({ axis, value, min, max, invert, label, onChange }: Props) {
  const start = useRef<{ position: number; value: number } | null>(null);
  const position = (e: PointerEvent) => (axis === "x" ? e.clientX : e.clientY);

  return (
    <div
      role="separator"
      aria-orientation={axis === "x" ? "vertical" : "horizontal"}
      aria-label={label}
      className={`resizer ${axis}`}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        start.current = { position: position(e), value };
        // 포인터가 에디터나 터미널 위로 지나가도 계속 따라오게 한다.
        e.currentTarget.setPointerCapture(e.pointerId);
        e.preventDefault();
      }}
      onPointerMove={(e) => {
        const from = start.current;
        if (!from) return;
        const delta = (position(e) - from.position) * (invert ? -1 : 1);
        onChange(Math.min(max, Math.max(min, from.value + delta)));
      }}
      onPointerUp={() => (start.current = null)}
      onPointerCancel={() => (start.current = null)}
    />
  );
}
