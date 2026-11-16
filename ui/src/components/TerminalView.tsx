import { useEffect, useRef, useState, type FormEvent } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import * as api from "../api";
import type { Host } from "../types";

interface Props {
  host: Host;
  /** 현재 보이는 탭인지 (숨겨진 동안에도 세션과 화면 내용은 유지된다) */
  active: boolean;
}

/** 한 번의 접속 시도에 쓰는 값. 새 객체로 바꾸면 다시 접속한다. */
interface Attempt {
  secret?: string;
  trustedHostKey?: string;
}

type Phase =
  | { kind: "connecting" }
  | { kind: "connected" }
  /** 비밀번호나 패스프레이즈를 입력받는 중 */
  | { kind: "secret"; label: string; error?: string }
  /** 처음 보는 호스트 키를 확인받는 중 */
  | { kind: "hostKey"; fingerprint: string }
  /** 셸이 끝났거나 접속에 실패함 */
  | { kind: "ended"; message: string; failed: boolean };

/** 접속 실패를 다음에 보여 줄 화면으로 바꾼다. */
function phaseForError(e: unknown, host: Host, attempt: Attempt): Phase {
  if (api.isAppError(e)) {
    switch (e.kind) {
      case "hostKeyUnknown":
        return { kind: "hostKey", fingerprint: e.message };
      case "passwordRequired":
        return { kind: "secret", label: "비밀번호" };
      case "passphraseRequired":
        // 이미 입력했는데 또 요구한다면 틀린 것이므로 이유를 보여 준다.
        return {
          kind: "secret",
          label: "개인키 패스프레이즈",
          error: attempt.secret === undefined ? undefined : e.message,
        };
      case "authFailed":
        if (host.auth.kind === "password") {
          return { kind: "secret", label: "비밀번호", error: e.message };
        }
    }
  }
  return { kind: "ended", message: api.errorMessage(e), failed: true };
}

/** SSH 터미널 탭 하나. xterm 인스턴스와 원격 셸 세션을 소유한다. */
export function TerminalView({ host, active }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const sessionRef = useRef<string | null>(null);
  const [attempt, setAttempt] = useState<Attempt>({});
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

  // 접속: attempt가 바뀔 때마다 새로 시도하고, 탭이 닫히면 세션도 닫는다.
  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    let cancelled = false;
    let sessionId: string | null = null;
    setPhase({ kind: "connecting" });

    api
      .openTerminal(
        { hostId: host.id, cols: term.cols, rows: term.rows, ...attempt },
        {
          onOutput: (data) => {
            if (!cancelled) term.write(data);
          },
          onExit: (code) => {
            if (cancelled) return;
            sessionRef.current = null;
            const message = code === null ? "연결이 종료되었습니다." : `셸이 종료되었습니다 (코드 ${code}).`;
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
        // 접속하는 동안 창 크기가 바뀌었을 수 있으므로 한 번 맞춰 준다.
        api.resizeTerminal(id, term.cols, term.rows).catch(() => {});
        setPhase((prev) => (prev.kind === "connecting" ? { kind: "connected" } : prev));
        term.focus();
      })
      .catch((e) => {
        if (!cancelled) setPhase(phaseForError(e, host, attempt));
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
  }, [attempt, host.id]);

  // 탭으로 돌아오면 바로 입력할 수 있게 한다.
  useEffect(() => {
    if (active && phase.kind === "connected") termRef.current?.focus();
  }, [active, phase.kind]);

  const retry = (next: Attempt) => {
    if (phase.kind === "ended") termRef.current?.write("\r\n");
    setAttempt(next);
  };
  const cancel = () => setPhase({ kind: "ended", message: "접속을 취소했습니다.", failed: false });

  return (
    <div className="terminal">
      <div className="terminal-screen" ref={containerRef} />
      {phase.kind === "connecting" && (
        <div className="terminal-overlay">
          <div className="terminal-card">
            {host.username}@{host.address}:{host.port}에 접속하는 중…
          </div>
        </div>
      )}
      {phase.kind === "secret" && (
        <SecretPrompt
          label={phase.label}
          error={phase.error}
          target={`${host.username}@${host.address}`}
          onSubmit={(secret) => retry({ ...attempt, secret })}
          onCancel={cancel}
        />
      )}
      {phase.kind === "hostKey" && (
        <div className="terminal-overlay">
          <div className="terminal-card">
            <h3>처음 접속하는 호스트입니다</h3>
            <p>
              <b>
                {host.address}:{host.port}
              </b>
              의 호스트 키를 아직 신뢰한 적이 없습니다. 아래 지문이 서버의 것과 같은지 확인하세요.
            </p>
            <code className="fingerprint">{phase.fingerprint}</code>
            <div className="modal-actions">
              <button className="btn-secondary" onClick={cancel}>
                취소
              </button>
              <button
                className="btn-primary"
                autoFocus
                onClick={() => retry({ ...attempt, trustedHostKey: phase.fingerprint })}
              >
                신뢰하고 접속
              </button>
            </div>
          </div>
        </div>
      )}
      {phase.kind === "ended" && (
        <div className="terminal-overlay bottom">
          <div className="terminal-card compact">
            <span className={phase.failed ? "form-error" : undefined}>{phase.message}</span>
            {/* 틀린 비밀 값을 다시 쓰지 않도록 처음부터 다시 시도한다. */}
            <button className="btn-primary" onClick={() => retry({})}>
              다시 연결
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

interface SecretPromptProps {
  label: string;
  error?: string;
  target: string;
  onSubmit: (secret: string) => void;
  onCancel: () => void;
}

/** 비밀번호 / 패스프레이즈 입력. 값은 접속 요청에만 쓰이고 저장되지 않는다. */
function SecretPrompt({ label, error, target, onSubmit, onCancel }: SecretPromptProps) {
  const [value, setValue] = useState("");
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (value) onSubmit(value);
  };
  return (
    <div className="terminal-overlay">
      <form className="terminal-card modal-like" onSubmit={submit}>
        <label>
          {target}의 {label}
          <input type="password" value={value} onChange={(e) => setValue(e.target.value)} autoFocus />
        </label>
        {error && <p className="form-error">{error}</p>}
        <div className="modal-actions">
          <button type="button" className="btn-secondary" onClick={onCancel}>
            취소
          </button>
          <button type="submit" className="btn-primary" disabled={!value}>
            접속
          </button>
        </div>
      </form>
    </div>
  );
}
