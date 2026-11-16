import { useState, type FormEvent } from "react";
import * as api from "../api";
import type { Host } from "../types";

/** 접속 과정에서 사용자가 입력·확인한 값. 요청에만 쓰이고 저장되지 않는다. */
export interface Credentials {
  /** 비밀번호 또는 개인키 패스프레이즈 */
  secret?: string;
  /** 사용자가 확인한 호스트 키 지문 */
  trustedHostKey?: string;
}

/** 접속을 이어 가려면 사용자에게 받아야 하는 것 */
export type Prompt =
  { kind: "secret"; label: string; error?: string } | { kind: "hostKey"; fingerprint: string };

/**
 * 접속 실패가 사용자 입력으로 해결될 수 있는 종류라면 띄울 창을 돌려준다.
 * 그렇지 않으면 null (그냥 에러로 보여 주면 된다).
 */
export function promptForError(e: unknown, host: Host, tried: Credentials): Prompt | null {
  if (!api.isAppError(e)) return null;
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
        error: tried.secret === undefined ? undefined : e.message,
      };
    case "authFailed":
      return host.auth.kind === "password" ? { kind: "secret", label: "비밀번호", error: e.message } : null;
    default:
      return null;
  }
}

interface Props {
  host: Host;
  prompt: Prompt;
  /** 입력받은 값만 담아 호출한다. 기존 값과 합치는 것은 호출한 쪽 몫이다. */
  onSubmit: (credentials: Credentials) => void;
  onCancel: () => void;
}

/** 비밀번호 입력 / 호스트 키 확인 창. 부모 영역을 덮는 오버레이로 그려진다. */
export function ConnectPrompt({ host, prompt, onSubmit, onCancel }: Props) {
  if (prompt.kind === "secret") {
    return (
      <SecretPrompt
        label={prompt.label}
        error={prompt.error}
        target={`${host.username}@${host.address}`}
        onSubmit={(secret) => onSubmit({ secret })}
        onCancel={onCancel}
      />
    );
  }
  return (
    <div className="overlay">
      <div className="overlay-card">
        <h3>처음 접속하는 호스트입니다</h3>
        <p>
          <b>
            {host.address}:{host.port}
          </b>
          의 호스트 키를 아직 신뢰한 적이 없습니다. 아래 지문이 서버의 것과 같은지 확인하세요.
        </p>
        <code className="fingerprint">{prompt.fingerprint}</code>
        <div className="modal-actions">
          <button className="btn-secondary" onClick={onCancel}>
            취소
          </button>
          <button
            className="btn-primary"
            autoFocus
            onClick={() => onSubmit({ trustedHostKey: prompt.fingerprint })}
          >
            신뢰하고 접속
          </button>
        </div>
      </div>
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

function SecretPrompt({ label, error, target, onSubmit, onCancel }: SecretPromptProps) {
  const [value, setValue] = useState("");
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (value) onSubmit(value);
  };
  return (
    <div className="overlay">
      <form className="overlay-card" onSubmit={submit}>
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
