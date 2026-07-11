export type CredentialStatus = "valid" | "not_found" | "expired" | "parse_error";
export type WindowKind = "five_hour" | "seven_day" | "thirty_day" | "unknown";

export interface QuotaWindow {
  kind: WindowKind;
  label: string;
  usedPercent: number;
  remainingPercent: number;
  resetAt?: number;
}

export interface QuotaSnapshot {
  windows: QuotaWindow[];
  queriedAt?: number;
  cached: boolean;
  credentialStatus: CredentialStatus;
  error?: string;
}

export interface AppSettings {
  codexHome?: string;
  displayWidth: number;
  displayHeight: number;
  horizontalOffset: number;
  verticalOffset: number;
  fontScale: number;
  ringSize: number;
  showCountdown: boolean;
  animations: boolean;
  followSystemTheme: boolean;
  autostart: boolean;
  coordinateLyricify: boolean;
  taskbarRegion: "left" | "right";
  windowAlignment: "left" | "right";
  reverseLayout: boolean;
}

export const DEFAULT_SETTINGS: AppSettings = {
  displayWidth: 218,
  displayHeight: 42,
  horizontalOffset: 0,
  verticalOffset: 0,
  fontScale: 1,
  ringSize: 28,
  showCountdown: true,
  animations: true,
  followSystemTheme: true,
  autostart: true,
  coordinateLyricify: true,
  taskbarRegion: "right",
  windowAlignment: "right",
  reverseLayout: false,
};

export function normalizeSettings(value?: Partial<AppSettings> | null): AppSettings {
  return { ...DEFAULT_SETTINGS, ...(value ?? {}) };
}

export const EMPTY_SNAPSHOT: QuotaSnapshot = {
  windows: [],
  cached: false,
  credentialStatus: "not_found",
};
