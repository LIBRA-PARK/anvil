import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import * as api from "../api";
import { LOCAL_ID, type Host } from "../types";
import {
  ConnectPrompt,
  promptForError,
  rememberIfAsked,
  type Credentials,
  type Prompt,
} from "./ConnectPrompt";

interface Props {
  /** null이면 로컬 머신의 셸 */
  host: Host | null;
  /** 현재 보이는 터미널인지 (숨겨진 동안에도 세션과 화면 내용은 유지된다) */
  active: boolean;
  /** 값이 바뀌면 입력 포커스를 다시 가져온다. (이미 보이는 터미널을 목록에서 다시 눌렀을 때) */
  focusTick: number;
}

type Phase =
  | { kind: "connecting" }
  | { kind: "connected" }
  /** 비밀번호·패스프레이즈 입력이나 호스트 키 확인을 기다리는 중 */
  | { kind: "prompt"; prompt: Prompt }
  /** 셸이 끝났거나 접속에 실패함 */
  | { kind: "ended"; message: string; failed: boolean };

/** 터미널 하나. xterm 인스턴스와 셸 세션(로컬 또는 원격)을 소유한다. */
export function TerminalView({ host, active, focusTick }: Props) {
  const hostId = host?.id ?? LOCAL_ID;
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const sessionRef = useRef<string | null>(null);
  const [attempt, setAttempt] = useState<Credentials>({});
  const [phase, setPhase] = useState<Phase>({ kind: "connecting" });

  // xterm 인스턴스: 탭이 생길 때 만들고 닫힐 때 정리한다.
  useEffect(() => {
    const container = containerRef.current!;
    const term = new Terminal({
      cursorBlink: true,
      fontFamily: 'Menlo, Consolas, "DejaVu Sans Mono", "D2Coding", monospace',
      fontSize: 13,
      scrollback: 5000,
      theme: { background: "#1a1b1e", foreground: "#e3e5e8", cursor: "#e0763c" },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(container);
    termRef.current = term;

    const input = term.onData((data) => {
      if (sessionRef.current) api.writeTerminal(sessionRef.current, data).catch(() => {});
    });
    const resized = term.onResize(({ cols, rows }) => {
      if (sessionRef.current) api.resizeTerminal(sessionRef.current, cols, rows).catch(() => {});
    });
    // 창 크기가 바뀌거나 숨겨진 탭이 다시 보일 때 칸 수를 다시 맞춘다.
    // (숨겨진 동안에는 크기가 0이라 맞추면 안 된다)
    const observer = new ResizeObserver(() => {
      if (container.offsetWidth > 0 && container.offsetHeight > 0) fit.fit();
    });
    observer.observe(container);

    return () => {
      observer.disconnect();
      input.dispose();
      resized.dispose();
      term.dispose();
      termRef.current = null;
    };
  }, []);

  // 접속: attempt가 바뀔 때마다 새로 시도하고, 터미널이 닫히면 세션도 닫는다.
  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    let cancelled = false;
    let sessionId: string | null = null;
    setPhase({ kind: "connecting" });

    api
      .openTerminal(
        { hostId, cols: term.cols, rows: term.rows, ...attempt },
        {
          onOutput: (data) => {
            if (!cancelled) term.write(data);
          },
          onExit: (code) => {
            if (cancelled) return;
            sessionRef.current = null;
            const message = code === null ? "세션이 종료되었습니다." : `셸이 종료되었습니다 (코드 ${code}).`;
            setPhase({ kind: "ended", message, failed: false });
          },
        },
      )
      .then((id) => {
        // 기다리는 사이 탭이 닫혔다면 방금 열린 세션을 바로 닫는다.
        if (cancelled) {
          api.closeTerminal(id).catch(() => {});
          return;
        }
        sessionId = id;
        sessionRef.current = id;
        rememberIfAsked(hostId, attempt);
        // 접속하는 동안 창 크기가 바뀌었을 수 있으므로 한 번 맞춰 준다.
        api.resizeTerminal(id, term.cols, term.rows).catch(() => {});
        setPhase((prev) => (prev.kind === "connecting" ? { kind: "connected" } : prev));
        term.focus();
      })
      .catch((e) => {
        if (cancelled) return;
        const prompt = host ? promptForError(e, host, attempt) : null;
        setPhase(
          prompt ? { kind: "prompt", prompt } : { kind: "ended", message: api.errorMessage(e), failed: true },
        );
      });

    return () => {
      cancelled = true;
      if (sessionId) {
        api.closeTerminal(sessionId).catch(() => {});
        if (sessionRef.current === sessionId) sessionRef.current = null;
      }
    };
    // 호스트 정보는 탭이 살아 있는 동안 바뀌지 않는다(수정 기능 없음, 삭제 시 탭이 닫힘).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [attempt, hostId]);

  // 이 터미널이 선택되면 바로 입력할 수 있게 한다.
  useEffect(() => {
    if (active && phase.kind === "connected") termRef.current?.focus();
  }, [active, phase.kind, focusTick]);

  const retry = (next: Credentials) => {
    if (phase.kind === "ended") termRef.current?.write("\r\n");
    setAttempt(next);
  };
  const cancel = () => setPhase({ kind: "ended", message: "접속을 취소했습니다.", failed: false });

  return (
    <div className="terminal">
      <div className="terminal-screen" ref={containerRef} />
      {phase.kind === "connecting" && (
        <div className="overlay">
          <div className="overlay-card">
            {host ? `${host.username}@${host.address}:${host.port}에 접속하는 중…` : "셸을 시작하는 중…"}
          </div>
        </div>
      )}
      {phase.kind === "prompt" && host && (
        <ConnectPrompt
          host={host}
          prompt={phase.prompt}
          onSubmit={(credentials) => retry({ ...attempt, ...credentials })}
          onCancel={cancel}
        />
      )}
      {phase.kind === "ended" && (
        <div className="overlay bottom">
          <div className="overlay-card compact">
            <span className={phase.failed ? "form-error" : undefined}>{phase.message}</span>
            {/* 틀린 비밀 값을 다시 쓰지 않도록 처음부터 다시 시도한다. */}
            <button className="btn-primary" onClick={() => retry({})}>
              {host ? "다시 연결" : "다시 시작"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
