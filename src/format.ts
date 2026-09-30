export function formatSize(bytes: number): string {
  if (!bytes) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 || i === 0 ? v.toFixed(0) : v.toFixed(v >= 10 ? 1 : 2)} ${units[i]}`;
}

export function formatDate(secs: number | null | undefined): string {
  if (!secs) return "—";
  return new Date(secs * 1000).toLocaleDateString("vi-VN", { day: "2-digit", month: "2-digit", year: "numeric" });
}

export function formatDateTime(secs: number | null | undefined): string {
  if (!secs) return "—";
  return new Date(secs * 1000).toLocaleString("vi-VN", {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function formatNumber(n: number): string {
  return n.toLocaleString("en-US");
}

export function errorText(err: unknown): string {
  return typeof err === "string" ? err : err instanceof Error ? err.message : JSON.stringify(err);
}

/** Chạy trên macOS? (một số chữ trong giao diện khác Windows) */
export const IS_MAC = typeof navigator !== "undefined" && /Mac/i.test(navigator.userAgent);
/** Trình quản lý file của hệ điều hành */
export const FILE_MANAGER = IS_MAC ? "Finder" : "Explorer";
/** Ví dụ thư mục tài nguyên */
export const EXAMPLE_ROOT = IS_MAC ? "/Volumes/Data/Resources" : "D:\Resources";
