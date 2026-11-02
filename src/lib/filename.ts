const pad = (n: number) => String(n).padStart(2, "0");

/** "Prole 2026-11-02 at 22.24.08.png" — macOS's screenshot naming, in local time. */
export function screenshotFileName(d: Date): string {
  const date = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  const time = `${pad(d.getHours())}.${pad(d.getMinutes())}.${pad(d.getSeconds())}`;
  return `Prole ${date} at ${time}.png`;
}

/** Last component of a POSIX path: the file or folder name. */
export function baseName(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  return trimmed.slice(trimmed.lastIndexOf("/") + 1) || path;
}

/** Name of the folder a file sits in ("Desktop" for ~/Desktop/a.png). */
export function folderName(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  const parent = trimmed.slice(0, trimmed.lastIndexOf("/"));
  return parent ? baseName(parent) : "/";
}
