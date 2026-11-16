import { useState, type FormEvent } from "react";
import { errorMessage } from "../api";
import type { AuthMethod, NewHost } from "../types";

interface Props {
  onSubmit: (input: NewHost) => Promise<void>;
  onCancel: () => void;
}

/** 원격 호스트 추가 폼 */
export function AddHostModal({ onSubmit, onCancel }: Props) {
  const [name, setName] = useState("");
  const [address, setAddress] = useState("");
  const [port, setPort] = useState("22");
  const [username, setUsername] = useState("");
  const [authKind, setAuthKind] = useState<AuthMethod["kind"]>("key");
  const [keyPath, setKeyPath] = useState("~/.ssh/id_ed25519");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const portNum = Number(port);
    if (!Number.isInteger(portNum) || portNum < 1 || portNum > 65535) {
      setError("포트는 1~65535 사이의 숫자여야 합니다.");
      return;
    }
    const auth: AuthMethod = authKind === "key" ? { kind: "key", path: keyPath } : { kind: authKind };

    setBusy(true);
    setError(null);
    try {
      await onSubmit({ name, address, port: portNum, username, auth });
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={onCancel}>
      <form className="modal" onClick={(e) => e.stopPropagation()} onSubmit={submit}>
        <h2>호스트 추가</h2>

        <label>
          이름
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="prod-server"
            required
            autoFocus
          />
        </label>
        <div className="row">
          <label className="grow">
            주소
            <input
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              placeholder="192.168.0.10"
              required
            />
          </label>
          <label className="port">
            포트
            <input value={port} onChange={(e) => setPort(e.target.value)} inputMode="numeric" />
          </label>
        </div>
        <label>
          사용자
          <input
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            placeholder="ubuntu"
            required
          />
        </label>
        <label>
          인증 방식
          <select value={authKind} onChange={(e) => setAuthKind(e.target.value as AuthMethod["kind"])}>
            <option value="key">개인키 파일</option>
            <option value="agent">ssh-agent</option>
            <option value="password">비밀번호 (접속 시 입력)</option>
          </select>
        </label>
        {authKind === "key" && (
          <label>
            개인키 경로
            <input value={keyPath} onChange={(e) => setKeyPath(e.target.value)} />
          </label>
        )}

        {error && <p className="form-error">{error}</p>}

        <div className="modal-actions">
          <button type="button" className="btn-secondary" onClick={onCancel}>
            취소
          </button>
          <button type="submit" className="btn-primary" disabled={busy}>
            {busy ? "저장 중…" : "추가"}
          </button>
        </div>
      </form>
    </div>
  );
}
