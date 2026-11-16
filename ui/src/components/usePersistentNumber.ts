import { useCallback, useState } from "react";

/**
 * 숫자 하나를 localStorage에 기억하는 상태. 사이드바 너비처럼 다음 실행에도 유지할 값에 쓴다.
 * 저장소를 쓸 수 없는 환경에서는 그냥 메모리 상태로 동작한다.
 */
export function usePersistentNumber(key: string, initial: number): [number, (next: number) => void] {
  const [value, setValue] = useState(() => {
    try {
      const raw = localStorage.getItem(key);
      const stored = raw === null ? NaN : Number(raw);
      return Number.isFinite(stored) ? stored : initial;
    } catch {
      return initial;
    }
  });

  const update = useCallback(
    (next: number) => {
      setValue(next);
      try {
        localStorage.setItem(key, String(Math.round(next)));
      } catch {
        // 저장하지 못해도 이번 실행 동안은 정상 동작한다.
      }
    },
    [key],
  );

  return [value, update];
}
