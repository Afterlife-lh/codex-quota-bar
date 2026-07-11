import { useCallback, useEffect, useMemo, useState, type CSSProperties, type MouseEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Check, Clock3, Folder, LogOut, RefreshCw, Settings2, X } from "lucide-react";
import type { AppSettings, QuotaSnapshot, QuotaWindow } from "./types";
import { EMPTY_SNAPSHOT, normalizeSettings } from "./types";
import { findWindow, formatCountdown, quotaColor, statusText } from "./visual";

function useQuota() {
  const [snapshot, setSnapshot] = useState<QuotaSnapshot>(EMPTY_SNAPSHOT);
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    invoke<QuotaSnapshot>("get_status").then(setSnapshot).catch(() => undefined);
    const unlisten = listen<QuotaSnapshot>("quota-updated", (event) => setSnapshot(event.payload));
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => { window.clearInterval(timer); void unlisten.then((fn) => fn()); };
  }, []);
  return { snapshot, now };
}

function useSettings() {
  const [settings, setSettings] = useState<AppSettings>();
  useEffect(() => {
    invoke<Partial<AppSettings>>("get_settings").then((value) => setSettings(normalizeSettings(value))).catch(() => undefined);
    const unlisten = listen<Partial<AppSettings>>("settings-updated", (event) => setSettings(normalizeSettings(event.payload)));
    return () => { void unlisten.then((fn) => fn()); };
  }, []);
  return settings;
}

function useDarkTheme(followSystem = true) {
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  const [dark, setDark] = useState(query.matches);
  useEffect(() => {
    const listener = (event: MediaQueryListEvent) => setDark(event.matches);
    query.addEventListener("change", listener);
    return () => query.removeEventListener("change", listener);
  }, [query]);
  return followSystem ? dark : true;
}

function DualRing({ five, seven, size = 28, stroke = 2.3, dark, animated = true }: {
  five?: QuotaWindow; seven?: QuotaWindow; size?: number; stroke?: number; dark: boolean; animated?: boolean;
}) {
  const center = size / 2;
  const outerR = size / 2 - stroke;
  const innerR = outerR * 0.61;
  const circle = (radius: number, percent: number, color: string, key: string) => {
    const length = Math.PI * 2 * radius;
    return <>
      <circle key={`${key}-track`} cx={center} cy={center} r={radius} className="ring-track" strokeWidth={stroke} />
      <circle key={key} cx={center} cy={center} r={radius} fill="none" stroke={color} strokeWidth={stroke}
        strokeLinecap="round" strokeDasharray={length} strokeDashoffset={length * (1 - percent / 100)}
        className={animated ? "ring-value animated" : "ring-value"} />
    </>;
  };
  const neutral = dark ? "#8b929c" : "#737982";
  return <svg className="dual-ring" width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden="true">
    <g transform={`rotate(-90 ${center} ${center})`}>
      {circle(outerR, seven?.remainingPercent ?? 0, seven ? quotaColor(seven.remainingPercent, dark) : neutral, "outer")}
      {circle(innerR, five?.remainingPercent ?? 0, five ? quotaColor(five.remainingPercent, dark) : neutral, "inner")}
    </g>
  </svg>;
}

function QuotaRow({ window, label, now, dark, showCountdown = true }: { window?: QuotaWindow; label: string; now: number; dark: boolean; showCountdown?: boolean }) {
  const color = window ? quotaColor(window.remainingPercent, dark) : dark ? "#9aa0aa" : "#666b73";
  return <div className="quota-row">
    <span className="quota-main" style={{ color }}>{label}<strong>{window ? `${Math.round(window.remainingPercent)}%` : "--"}</strong></span>
    {showCountdown && <><span className="separator">·</span><span className="countdown">{formatCountdown(window?.resetAt, true, now)}</span></>}
  </div>;
}

function TaskbarView() {
  const { snapshot, now } = useQuota();
  const settings = useSettings();
  const dark = useDarkTheme(settings?.followSystemTheme ?? true);
  const five = findWindow(snapshot.windows, "five_hour");
  const seven = findWindow(snapshot.windows, "seven_day") ?? findWindow(snapshot.windows, "thirty_day");
  const openDetails = () => invoke("show_detail");
  const openMenu = (event: MouseEvent) => { event.preventDefault(); void invoke("show_menu"); };
  return <main className={`taskbar-widget ${snapshot.cached ? "is-stale" : ""} ${settings?.reverseLayout ? "is-reversed" : ""}`}
    style={{ "--font-scale": settings?.fontScale ?? 1 } as CSSProperties}
    onClick={openDetails} onContextMenu={openMenu}>
    <DualRing five={five} seven={seven} size={settings?.ringSize ?? 28} dark={dark} animated={settings?.animations ?? true} />
    <div className="quota-rows">
      {snapshot.windows.length ? <>
        <QuotaRow window={five} label="5h" now={now} dark={dark} showCountdown={settings?.showCountdown ?? true} />
        <QuotaRow window={seven} label={seven?.kind === "thirty_day" ? "30d" : "7d"} now={now} dark={dark} showCountdown={settings?.showCountdown ?? true} />
      </> : <div className="empty-state">{statusText(snapshot.credentialStatus)}</div>}
    </div>
    {snapshot.cached && <span className="stale-dot" title="显示的是上次成功数据" />}
  </main>;
}

function WindowHeader({ title }: { title: string }) {
  return <header className="window-header" data-tauri-drag-region>
    <div><span className="app-mark">CQ</span><strong>{title}</strong></div>
    <button className="icon-button" aria-label="关闭" onClick={() => invoke("hide_current_window")}><X size={16} /></button>
  </header>;
}

function QuotaCard({ item, dark, now }: { item?: QuotaWindow; dark: boolean; now: number }) {
  const color = item ? quotaColor(item.remainingPercent, dark) : "#8a9099";
  return <section className="quota-card">
    <div className="card-top"><span>{item?.label ?? "额度窗口"}</span><strong style={{ color }}>{item ? `${Math.round(item.remainingPercent)}%` : "--"}</strong></div>
    <div className="linear-track"><span style={{ width: `${item?.remainingPercent ?? 0}%`, background: color }} /></div>
    <div className="card-meta"><span>已用 {item ? `${Math.round(item.usedPercent)}%` : "--"}</span><span><Clock3 size={12} /> {formatCountdown(item?.resetAt, false, now)}</span></div>
    {item?.resetAt && <time>{new Date(item.resetAt).toLocaleString("zh-CN")} 重置</time>}
  </section>;
}

function SettingsPanel({ value, onSaved }: { value: AppSettings; onSaved: (next: AppSettings) => void }) {
  const [draft, setDraft] = useState(value);
  const [windowsGeneration, setWindowsGeneration] = useState<"windows10" | "windows11">("windows11");
  useEffect(() => setDraft(value), [value]);
  useEffect(() => { void invoke<"windows10" | "windows11">("get_windows_generation").then(setWindowsGeneration); }, []);
  const save = async () => {
    const saved = await invoke<AppSettings>("save_settings", { settings: draft });
    await invoke("set_autostart", { enabled: saved.autostart });
    onSaved(saved);
  };
  return <div className="settings-panel">
    <div className={`settings-preview ${draft.reverseLayout ? "is-reversed" : ""}`}>
      <DualRing size={Math.min(draft.ringSize, 34)} dark={true} animated={draft.animations}
        five={{ kind: "five_hour", label: "5 小时", usedPercent: 18, remainingPercent: 82 }}
        seven={{ kind: "seven_day", label: "7 天", usedPercent: 36, remainingPercent: 64 }} />
      <div><span style={{ color: quotaColor(82, true) }}>5h&nbsp; 82%</span><small>·&nbsp; 03:21</small><span style={{ color: quotaColor(64, true) }}>7d&nbsp; 64%</span><small>·&nbsp; 4d12h</small></div>
    </div>
    <label><span><Folder size={14} /> Codex 根目录</span><input value={draft.codexHome ?? ""} placeholder="默认：%USERPROFILE%\\.codex"
      onChange={(event) => setDraft({ ...draft, codexHome: event.target.value || undefined })} /></label>
    <div className="setting-section-title">尺寸与位置</div>
    <div className="setting-grid">
      <RangeSetting label="宽度" value={draft.displayWidth} min={140} max={420} unit="px" onChange={(displayWidth) => setDraft({ ...draft, displayWidth })} />
      <RangeSetting label="高度" value={draft.displayHeight} min={30} max={72} unit="px" onChange={(displayHeight) => setDraft({ ...draft, displayHeight })} />
      <RangeSetting label="水平偏移" value={draft.horizontalOffset} min={-240} max={240} unit="px" onChange={(horizontalOffset) => setDraft({ ...draft, horizontalOffset })} />
      <RangeSetting label="垂直偏移" value={draft.verticalOffset} min={-48} max={48} unit="px" onChange={(verticalOffset) => setDraft({ ...draft, verticalOffset })} />
    </div>
    <div className="setting-section-title">任务栏布局 · {windowsGeneration === "windows11" ? "Windows 11" : "Windows 10"}</div>
    {windowsGeneration === "windows11" ? <div className="setting-grid">
      <SelectSetting label="所在区域" value={draft.taskbarRegion} onChange={(taskbarRegion) => setDraft({ ...draft, taskbarRegion })} />
      <SelectSetting label="窗口对齐" value={draft.windowAlignment} onChange={(windowAlignment) => setDraft({ ...draft, windowAlignment })} />
    </div> : <div className="setting-hint">Windows 10 默认固定在托盘左侧。</div>}
    <div className="setting-section-title">外观</div>
    <div className="setting-grid">
      <RangeSetting label="字体缩放" value={draft.fontScale} min={0.75} max={1.6} step={0.05} unit="×" onChange={(fontScale) => setDraft({ ...draft, fontScale })} />
      <RangeSetting label="环形大小" value={draft.ringSize} min={18} max={42} unit="px" onChange={(ringSize) => setDraft({ ...draft, ringSize })} />
    </div>
    <label className="toggle"><span>显示重置倒计时</span><input type="checkbox" checked={draft.showCountdown} onChange={(e) => setDraft({ ...draft, showCountdown: e.target.checked })} /></label>
    <label className="toggle"><span>平滑动画</span><input type="checkbox" checked={draft.animations} onChange={(e) => setDraft({ ...draft, animations: e.target.checked })} /></label>
    <label className="toggle"><span>跟随系统主题</span><input type="checkbox" checked={draft.followSystemTheme} onChange={(e) => setDraft({ ...draft, followSystemTheme: e.target.checked })} /></label>
    <label className="toggle"><span>自动避让 Lyricify Lite</span><input type="checkbox" checked={draft.coordinateLyricify} onChange={(e) => setDraft({ ...draft, coordinateLyricify: e.target.checked })} /></label>
    <label className="toggle"><span>反转“环形－额度－倒计时”排列</span><input type="checkbox" checked={draft.reverseLayout} onChange={(e) => setDraft({ ...draft, reverseLayout: e.target.checked })} /></label>
    <label className="toggle"><span>随 Windows 登录启动</span><input type="checkbox" checked={draft.autostart} onChange={(e) => setDraft({ ...draft, autostart: e.target.checked })} /></label>
    <button className="primary-button" onClick={save}><Check size={15} />保存设置</button>
  </div>;
}

function SelectSetting({ label, value, onChange }: { label: string; value: "left" | "right"; onChange: (value: "left" | "right") => void }) {
  return <label className="select-setting"><span>{label}</span><select value={value} onChange={(event) => onChange(event.target.value as "left" | "right")}>
    <option value="left">左侧</option><option value="right">右侧</option>
  </select></label>;
}

function RangeSetting({ label, value, min, max, step = 1, unit, onChange }: { label: string; value: number; min: number; max: number; step?: number; unit: string; onChange: (value: number) => void }) {
  const safeValue = Number.isFinite(value) ? value : min;
  return <label className="range-setting"><span>{label}<output>{Number(safeValue.toFixed(2))}{unit}</output></span>
    <input type="range" min={min} max={max} step={step} value={safeValue} onChange={(event) => onChange(Number(event.target.value))} /></label>;
}

function DetailView() {
  const { snapshot, now } = useQuota();
  const dark = useDarkTheme();
  const [refreshing, setRefreshing] = useState(false);
  const five = findWindow(snapshot.windows, "five_hour");
  const seven = findWindow(snapshot.windows, "seven_day") ?? findWindow(snapshot.windows, "thirty_day");
  const refresh = async () => { setRefreshing(true); try { await invoke("refresh_now"); } finally { setRefreshing(false); } };
  return <main className="panel detail-panel">
    <WindowHeader title="Codex Quota Bar" />
      <section className="hero">
        <DualRing five={five} seven={seven} size={84} stroke={6} dark={dark} />
        <div><span>当前可用额度</span><strong>{snapshot.windows.length ? `${Math.round(Math.min(five?.remainingPercent ?? 100, seven?.remainingPercent ?? 100))}%` : statusText(snapshot.credentialStatus)}</strong>
          <small>{snapshot.cached ? "上次成功数据" : "每分钟自动更新"}</small></div>
      </section>
      {snapshot.error && <div className="error-banner">{snapshot.error}</div>}
      <QuotaCard item={five} dark={dark} now={now} />
      <QuotaCard item={seven} dark={dark} now={now} />
      <footer className="detail-actions">
        <span>{snapshot.queriedAt ? `更新于 ${new Date(snapshot.queriedAt).toLocaleTimeString("zh-CN")}` : "尚未成功查询"}</span>
        <div><button className="icon-button" title="刷新" onClick={refresh}><RefreshCw size={16} className={refreshing ? "spin" : ""} /></button>
          <button className="icon-button" title="设置" onClick={() => invoke("show_settings")}><Settings2 size={16} /></button></div>
      </footer>
  </main>;
}

function SettingsView() {
  const settings = useSettings();
  return <main className="panel settings-window">
    <WindowHeader title="个性化设置" />
    {settings ? <SettingsPanel value={settings} onSaved={() => undefined} /> : <div className="settings-loading">正在加载设置…</div>}
  </main>;
}

function MenuView() {
  const action = useCallback(async (command: string, args?: Record<string, unknown>) => {
    await invoke(command, args); if (command !== "quit_app") await invoke("hide_current_window");
  }, []);
  return <main className="panel context-menu">
    <button onClick={() => action("refresh_now")}><RefreshCw size={15} />立即刷新</button>
    <button onClick={() => action("show_detail")}><Settings2 size={15} />详情与设置</button>
    <div className="menu-separator" />
    <button className="danger" onClick={() => action("quit_app")}><LogOut size={15} />退出</button>
  </main>;
}

export function App({ windowLabel }: { windowLabel: string }) {
  return useMemo(() => {
    if (windowLabel === "taskbar") return <TaskbarView />;
    if (windowLabel === "menu") return <MenuView />;
    if (windowLabel === "settings") return <SettingsView />;
    return <DetailView />;
  }, [windowLabel]);
}
