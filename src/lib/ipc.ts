import { invoke } from "@tauri-apps/api/core";

/** Text of anything thrown — invoke() rejects with the command's error string. */
export function errorText(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

/** invoke() that always rejects with an Error. */
export async function call<T = void>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new Error(errorText(e));
  }
}

/** Keep a record of an error the page is already showing inline. */
export function logError(context: string, e: unknown) {
  void invoke("log_error", { message: `${context}: ${errorText(e)}` }).catch(() => {});
}

/** Uncaught errors in this page go to the log and a toast instead of vanishing. */
export function reportUncaught(page: string) {
  const send = (detail: string) =>
    void invoke("report_error", { title: `Something went wrong in ${page}`, detail }).catch(() => {});
  window.addEventListener("error", (e) => send(e.message || errorText(e.error)));
  window.addEventListener("unhandledrejection", (e) => send(errorText(e.reason)));
}
