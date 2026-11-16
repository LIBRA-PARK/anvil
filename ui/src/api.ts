import { invoke } from "@tauri-apps/api/core";
import type { Host, NewHost } from "./types";

// 브라우저(vite dev)에서 단독으로 UI를 볼 때는 Tauri가 없으므로 메모리 목업을 쓴다.
const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
let mockHosts: Host[] = [];

export async function listHosts(): Promise<Host[]> {
  if (!inTauri) return [...mockHosts];
  return invoke<Host[]>("list_hosts");
}

export async function addHost(input: NewHost): Promise<Host> {
  if (!inTauri) {
    const host = { ...input, id: crypto.randomUUID() };
    mockHosts.push(host);
    return host;
  }
  return invoke<Host>("add_host", { input });
}

export async function removeHost(id: string): Promise<void> {
  if (!inTauri) {
    mockHosts = mockHosts.filter((h) => h.id !== id);
    return;
  }
  return invoke("remove_host", { id });
}
