import { useCallback, useState, type ReactNode } from "react";
import type { Host } from "../types";
import {
  ConnectPrompt,
  promptForError,
  rememberIfAsked,
  type Credentials,
  type Prompt,
} from "./ConnectPrompt";

interface Pending {
  prompt: Prompt;
  answer: (credentials: Credentials | null) => void;
}

/**
 * 원격 접속이 필요할 수 있는 요청을 감싼다.
 *
 * 요청이 비밀번호 입력이나 호스트 키 확인을 요구하며 실패하면 창을 띄우고,
 * 사용자가 답하면 그 값으로 같은 요청을 다시 보낸다. 취소하면 에러를 던진다.
 * 돌려받은 `prompt`는 컴포넌트 안(position: relative인 영역)에 그대로 렌더링한다.
 */
export function useConnectRetry(host: Host | null) {
  const [pending, setPending] = useState<Pending | null>(null);

  const call = useCallback(
    async <T,>(request: (credentials: Credentials) => Promise<T>): Promise<T> => {
      let credentials: Credentials = {};
      for (;;) {
        try {
          const result = await request(credentials);
          if (host) rememberIfAsked(host.id, credentials);
          return result;
        } catch (e) {
          const prompt = host ? promptForError(e, host, credentials) : null;
          if (!prompt) throw e;
          const answer = await new Promise<Credentials | null>((resolve) =>
            setPending({ prompt, answer: resolve }),
          );
          setPending(null);
          if (!answer) throw new Error("접속을 취소했습니다.");
          credentials = { ...credentials, ...answer };
        }
      }
    },
    [host],
  );

  const prompt: ReactNode =
    pending && host ? (
      <ConnectPrompt
        host={host}
        prompt={pending.prompt}
        onSubmit={pending.answer}
        onCancel={() => pending.answer(null)}
      />
    ) : null;

  return { call, prompt };
}
