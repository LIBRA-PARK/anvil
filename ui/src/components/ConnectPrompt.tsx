import { useState, type FormEvent } from "react";
import * as api from "../api";
import type { Host } from "../types";

/** 접속 과정에서 사용자가 입력·확인한 값 */
export interface Credentials {
  /** 비밀번호 또는 개인키 패스프레이즈 */
  secret?: string;
  /** 접속에 성공하면 `secret`을 OS 보안 저장소에 저장할지 */
  remember?: boolean;
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

/**
 * 접속에 성공한 뒤 호출한다. 사용자가 저장을 선택했다면 비밀 값을 OS 보안 저장소에 넣는다.
 * 저장에 실패해도 접속 자체는 이미 된 것이므로 에러를 던지지 않는다.
 */
export function rememberIfAsked(hostId: string, used: Credentials): void {
  if (used.remember && used.secret) {
    api.saveSecret(hostId, used.secret).catch((e) => console.warn("비밀 값 저장 실패:", e));
  }
}

interface Props {
  host: Host;
  prompt: Prompt;
  /** 입력받은 값만 담아 호출한다. 기존 값과 합치는 것은 호출한 쪽 몫이다. */
  onSubmit: (credentials: Credentials) => void;
  onCancel: () => void;
}

/**
 * 비밀번호 입력 / 호스트 키 확인 창.
 *
 * 어디서 띄우든 앱 전체를 덮는 창으로 그려진다. 사이드바나 아래 터미널 패널처럼
 * 좁은 영역에서 접속이 시작돼도 내용이 잘리지 않게 하기 위해서다.
 */
export function ConnectPrompt({ host, prompt, onSubmit, onCancel }: Props) {
  if (prompt.kind === "secret") {
    return (
      <SecretPrompt
        label={prompt.label}
        error={prompt.error}
        target={`${host.username}@${host.address}`}
        onSubmit={(secret, remember) => onSubmit({ secret, remember })}
        onCancel={onCancel}
      />
    );
  }
  return (
    <div className="overlay modal">
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
  onSubmit: (secret: string, remember: boolean) => void;
  onCancel: () => void;
}

function SecretPrompt({ label, error, target, onSubmit, onCancel }: SecretPromptProps) {
  const [value, setValue] = useState("");
  const [remember, setRemember] = useState(false);
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (value) onSubmit(value, remember);
  };
  return (
    <div className="overlay modal">
      <form className="overlay-card" onSubmit={submit}>
        <label>
          {target}의 {label}
          <input type="password" value={value} onChange={(e) => setValue(e.target.value)} autoFocus />
        </label>
        <label className="checkbox">
          <input type="checkbox" checked={remember} onChange={(e) => setRemember(e.target.checked)} />이
          컴퓨터의 보안 저장소(키체인)에 저장
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
