import type { QuotaWindow, WindowKind } from "./types";

const DARK = ["#FF5A5F", "#F6C344", "#43D17A"] as const;
const LIGHT = ["#C62828", "#8A6500", "#147A3F"] as const;

const hexToRgb = (hex: string) => [1, 3, 5].map((i) => Number.parseInt(hex.slice(i, i + 2), 16));
const toHex = (value: number) => Math.round(value).toString(16).padStart(2, "0");

export function quotaColor(percent: number, dark: boolean): string {
  const value = Math.max(0, Math.min(100, percent));
  const palette = dark ? DARK : LIGHT;
  const [from, to, t] = value <= 50
    ? [palette[0], palette[1], value / 50]
    : [palette[1], palette[2], (value - 50) / 50];
  const a = hexToRgb(from);
  const b = hexToRgb(to);
  return `#${a.map((channel, i) => toHex(channel + (b[i] - channel) * t)).join("")}`;
}

export function findWindow(windows: QuotaWindow[], kind: WindowKind): QuotaWindow | undefined {
  return windows.find((window) => window.kind === kind);
}

export function formatCountdown(resetAt?: number, compact = false, now = Date.now()): string {
  if (!resetAt) return "--:--";
  const total = Math.max(0, Math.floor((resetAt - now) / 1000));
  const days = Math.floor(total / 86400);
  const hours = Math.floor((total % 86400) / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  if (days > 0) return compact ? `${days}d${hours}h` : `${days}d ${hours}h`;
  if (hours > 0) return compact ? `${hours}h${minutes}m` : `${hours}h ${minutes}m`;
  const seconds = total % 60;
  return `${minutes.toString().padStart(2, "0")}:${seconds.toString().padStart(2, "0")}`;
}

export function statusText(status: string): string {
  if (status === "expired") return "登录失效";
  if (status === "parse_error") return "凭据异常";
  if (status === "not_found") return "未登录";
  return "暂不可用";
}
