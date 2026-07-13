import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type MouseEvent, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { flushSync } from "react-dom";
import {
  Activity, AlertCircle, Check, CheckCircle2, Clock3, Download, Folder,
  Gauge, LogOut, Moon, Palette, RadioTower, RefreshCw, Settings2, ShieldCheck, Sparkles, Sun, X,
} from "lucide-react";
import type { AppSettings, QuotaSnapshot, QuotaWindow } from "./types";
import { EMPTY_SNAPSHOT, normalizeSettings } from "./types";
import { findWindow, formatCountdown, quotaColor, statusText } from "./visual";

interface UpdateStatus {
  state: "idle" | "checking" | "up_to_date" | "available" | "downloading" | "installing" | "error";
  currentVersion: string;
  availableVersion?: string;
  message?: string;
}

interface RadarSnapshot {
  models: Array<{ id: string; label: string; score?: number; status?: string; passed?: number; validTasks?: number; invalidTasks?: number; wallTime?: string; costUsd?: number; communityScore?: number; communityVotes?: number }>;
  quotaRows: Array<{ tier: string; fiveHour?: number; sevenDay?: number; basis?: string }>;
  status?: string; signal?: string; resetSignals?: Array<{ label: string; headline: string }>; batch?: string; quotaBatch?: string; updatedAt?: string;
  fetchedAt?: number; source: string; attribution: string; siteUrl: string; cached: boolean; error?: string;
}

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

function useUpdateStatus() {
  const [status, setStatus] = useState<UpdateStatus>({ state: "idle", currentVersion: "" });
  useEffect(() => {
    invoke<UpdateStatus>("get_update_status").then(setStatus).catch(() => undefined);
    const unlisten = listen<UpdateStatus>("update-status", (event) => setStatus(event.payload));
    return () => { void unlisten.then((fn) => fn()); };
  }, []);
  return status;
}

function useRadar() {
  const [radar, setRadar] = useState<RadarSnapshot>({ models: [], quotaRows: [], source: "public_summary", attribution: "数据来自 Codex 雷达 codexradar.com", siteUrl: "https://codexradar.com", cached: false });
  useEffect(() => {
    invoke<RadarSnapshot>("get_radar_status").then(setRadar).catch(() => undefined);
    const unlisten = listen<RadarSnapshot>("radar-updated", (event) => setRadar(event.payload));
    return () => { void unlisten.then((fn) => fn()); };
  }, []);
  return radar;
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

function trackPointer(event: MouseEvent<HTMLElement>) {
  const element = event.currentTarget;
  const rect = element.getBoundingClientRect();
  const x = (event.clientX - rect.left) / rect.width;
  const y = (event.clientY - rect.top) / rect.height;
  element.style.setProperty("--pointer-x", `${x * 100}%`);
  element.style.setProperty("--pointer-y", `${y * 100}%`);
  element.style.setProperty("--tilt-x", `${(0.5 - y) * 3.2}deg`);
  element.style.setProperty("--tilt-y", `${(x - 0.5) * 3.2}deg`);
}

function resetPointer(event: MouseEvent<HTMLElement>) {
  const element = event.currentTarget;
  element.style.setProperty("--tilt-x", "0deg");
  element.style.setProperty("--tilt-y", "0deg");
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
      {seven && circle(outerR, seven.remainingPercent, quotaColor(seven.remainingPercent, dark), "outer")}
      {five && circle(seven ? innerR : outerR, five.remainingPercent, quotaColor(five.remainingPercent, dark), "inner")}
      {!five && !seven && circle(outerR, 0, neutral, "outer")}
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
  const update = useUpdateStatus();
  const settings = useSettings();
  const dark = useDarkTheme(settings?.followSystemTheme ?? true);
  const five = findWindow(snapshot.windows, "five_hour");
  const seven = findWindow(snapshot.windows, "seven_day") ?? findWindow(snapshot.windows, "thirty_day");
  const toggleDetails = () => invoke("toggle_detail");
  const openMenu = (event: MouseEvent) => { event.preventDefault(); void invoke("show_menu"); };
  return <main className={`taskbar-widget ${snapshot.cached ? "is-stale" : ""} ${settings?.reverseLayout ? "is-reversed" : ""}`}
    style={{
      "--font-scale": settings?.fontScale ?? 1,
      "--taskbar-foreground": dark ? "#F2F5FC" : "#202631",
    } as CSSProperties}
    onClick={toggleDetails} onContextMenu={openMenu}>
    {(five || seven) && <DualRing five={five} seven={seven} size={settings?.ringSize ?? 28} dark={dark} animated={settings?.animations ?? true} />}
    <div className={`quota-rows ${[five, seven].filter(Boolean).length === 1 ? "is-single" : ""}`}>
      {five || seven ? <>
        {five && <QuotaRow window={five} label="5h" now={now} dark={dark} showCountdown={settings?.showCountdown ?? true} />}
        {seven && <QuotaRow window={seven} label={seven.kind === "thirty_day" ? "30d" : "7d"} now={now} dark={dark} showCountdown={settings?.showCountdown ?? true} />}
      </> : <div className="empty-state">{statusText(snapshot.credentialStatus)}</div>}
    </div>
    {snapshot.cached && <span className="stale-dot" title="显示的是上次成功数据" />}
    {update.state === "available" && <span className="update-dot" title={`发现新版本 ${update.availableVersion ?? ""}`} />}
  </main>;
}

function WindowHeader({ title, subtitle, icon, actions, onClose }: { title: string; subtitle: string; icon: ReactNode; actions?: ReactNode; onClose?: () => void }) {
  return <header className="window-header" data-tauri-drag-region>
    <div className="brand-lockup"><span className="soft-app-mark">{icon}</span><span><strong>{title}</strong><small>{subtitle}</small></span></div>
    <div className="window-actions">{actions}<button className="soft-icon-button" aria-label="关闭" onClick={onClose ?? (() => { void invoke("hide_current_window"); })}><X size={16} /></button></div>
  </header>;
}

function QuotaCard({ item, dark, now }: { item: QuotaWindow; dark: boolean; now: number }) {
  const color = item ? quotaColor(item.remainingPercent, dark) : "#8a9099";
  return <section className="soft-card quota-card interactive-surface" data-reveal onMouseMove={trackPointer} onMouseLeave={resetPointer}>
    <div className="card-top"><span><Gauge size={15} />{item?.label ?? "额度窗口"}</span><strong style={{ color }}>{item ? `${Math.round(item.remainingPercent)}%` : "--"}</strong></div>
    <div className="soft-progress"><span style={{ width: `${item?.remainingPercent ?? 0}%`, background: color }} /></div>
    <div className="card-meta"><span>已使用 {item ? `${Math.round(item.usedPercent)}%` : "--"}</span><span><Clock3 size={12} /> {formatCountdown(item?.resetAt, false, now)}</span></div>
    {item?.resetAt && <time>{new Date(item.resetAt).toLocaleString("zh-CN")} 重置</time>}
  </section>;
}

function UpdateBadge({ status }: { status: UpdateStatus }) {
  const busy = ["checking", "downloading", "installing"].includes(status.state);
  const error = status.state === "error";
  return <span className={`update-badge ${busy ? "is-busy" : ""} ${error ? "is-error" : ""}`}>
    {error ? <AlertCircle size={12} /> : busy ? <RefreshCw size={12} className="spin" /> : <ShieldCheck size={12} />}
    {status.state === "available" ? `发现 ${status.availableVersion}` : status.message ?? `v${status.currentVersion}`}
  </span>;
}

const CURRENT_CHANGELOG = "采用 View Transitions API 重做主题圆形扩散动画；更新区域常驻显示当前版本信息；Radar 自动同步发重置卡与硬重置研判。";
const CURRENT_RELEASE_DATE = "2026-07-13";

function UpdatePanel({ status, onRefresh, refreshing }: { status: UpdateStatus; onRefresh: () => void; refreshing: boolean }) {
  const available = status.state === "available";
  return <section className={`soft-card update-panel ${available ? "has-update" : ""}`} data-reveal>
    <div><span className="eyebrow"><Download size={13} /> {available ? "可用更新" : "当前版本"}</span><strong>Codex Quota Bar {available ? status.availableVersion : `v${status.currentVersion}`}</strong></div>
    <p>{available ? (status.message ?? "新版本已经可以安装。") : CURRENT_CHANGELOG}</p>
    {!available && <time>更新时间：{CURRENT_RELEASE_DATE}</time>}
    <div className="update-panel-actions">
      <button className="soft-button" onClick={onRefresh} disabled={refreshing}><RefreshCw size={14} className={refreshing ? "spin" : ""} />重新检查</button>
      {available && <button className="soft-button accent install-update-button" onClick={() => invoke("install_update")}><Download size={14} />安装更新</button>}
    </div>
  </section>;
}

function DetailView() {
  const { snapshot, now } = useQuota();
  const systemDark = useDarkTheme();
  const [themeOverride, setThemeOverride] = useState<"light" | "dark" | null>(() => {
    const saved = window.localStorage.getItem("detail-theme");
    return saved === "light" || saved === "dark" ? saved : null;
  });
  const dark = themeOverride ? themeOverride === "dark" : systemDark;
  useEffect(() => { document.documentElement.dataset.detailTheme = dark ? "dark" : "light"; }, [dark]);
  const update = useUpdateStatus();
  const radar = useRadar();
  const settings = useSettings();
  const [detailTab, setDetailTab] = useState<"quota" | "radar">("quota");
  const [refreshing, setRefreshing] = useState(false);
  const [windowCycle, setWindowCycle] = useState(0);
  const [windowPhase, setWindowPhase] = useState<"entering" | "visible" | "leaving">("entering");
  const closeTimer = useRef<number>();
  const closing = useRef(false);
  const hasFocused = useRef(false);
  const five = findWindow(snapshot.windows, "five_hour");
  const seven = findWindow(snapshot.windows, "seven_day") ?? findWindow(snapshot.windows, "thirty_day");
  const visibleWindows = [five, seven].filter((item): item is QuotaWindow => Boolean(item));
  const remaining = visibleWindows.length ? Math.round(Math.min(...visibleWindows.map((item) => item.remainingPercent))) : undefined;
  const refresh = async () => { setRefreshing(true); try { await Promise.all([invoke("refresh_now"), invoke("check_for_updates")]); } finally { setRefreshing(false); } };
  const closeAnimated = useCallback(() => {
    if (closing.current) return;
    closing.current = true;
    setWindowPhase("leaving");
    window.clearTimeout(closeTimer.current);
    closeTimer.current = window.setTimeout(async () => {
      await invoke("hide_current_window");
      closing.current = false;
      setWindowPhase("entering");
    }, 210);
  }, []);
  useEffect(() => {
    const current = getCurrentWindow();
    const beginEntry = () => {
      hasFocused.current = true;
      closing.current = false;
      window.clearTimeout(closeTimer.current);
      setWindowCycle((value) => value + 1);
      setWindowPhase("entering");
      window.setTimeout(() => setWindowPhase("visible"), 360);
    };
    const focusListener = current.onFocusChanged(({ payload }) => {
      if (payload) {
        if (!hasFocused.current) beginEntry();
      } else if (hasFocused.current) {
        hasFocused.current = false;
        closeAnimated();
      }
    });
    const prepareListener = listen("prepare-detail-show", beginEntry);
    const closeListener = listen("request-detail-close", closeAnimated);
    return () => {
      window.clearTimeout(closeTimer.current);
      void focusListener.then((unlisten) => unlisten());
      void prepareListener.then((unlisten) => unlisten());
      void closeListener.then((unlisten) => unlisten());
    };
  }, [closeAnimated]);
  useEffect(() => {
    const root = document.querySelector(".detail-content");
    const elements = root?.querySelectorAll<HTMLElement>("[data-reveal]");
    if (!elements?.length) return;
    if (!("IntersectionObserver" in window)) {
      elements.forEach((element) => element.classList.add("is-revealed"));
      return;
    }
    const observer = new IntersectionObserver((entries) => entries.forEach((entry) => {
      if (entry.isIntersecting) {
        entry.target.classList.add("is-revealed");
        observer.unobserve(entry.target);
      }
    }), { root, threshold: 0.14 });
    elements.forEach((element) => observer.observe(element));
    return () => observer.disconnect();
  }, [detailTab, radar.models.length, snapshot.windows.length, update.state, windowCycle]);
  const toggleTheme = (event: MouseEvent<HTMLButtonElement>) => {
    const next = dark ? "light" : "dark";
    const x = event.clientX;
    const y = event.clientY;
    const radius = Math.hypot(Math.max(x, window.innerWidth - x), Math.max(y, window.innerHeight - y));
    const applyTheme = () => flushSync(() => {
      window.localStorage.setItem("detail-theme", next);
      document.documentElement.dataset.detailTheme = next;
      setThemeOverride(next);
    });
    const startViewTransition = (document as Document & { startViewTransition?: (callback: () => void) => { ready: Promise<void> } }).startViewTransition;
    if (!startViewTransition) { applyTheme(); return; }
    const transition = startViewTransition.call(document, applyTheme);
    void transition.ready.then(() => {
      const path = [`circle(0px at ${x}px ${y}px)`, `circle(${radius}px at ${x}px ${y}px)`];
      document.documentElement.animate(
        { clipPath: next === "dark" ? [...path].reverse() : path },
        { duration: 420, easing: "ease-in", fill: "forwards", pseudoElement: next === "dark" ? "::view-transition-old(root)" : "::view-transition-new(root)" },
      );
    });
  };
  return <main key={windowCycle} className={`soft-shell detail-panel window-${windowPhase}`} data-theme={dark ? "dark" : "light"}>
    <span className="ambient-orb orb-one" /><span className="ambient-orb orb-two" />
    <WindowHeader title="Codex Quota Bar" subtitle="额度中心" icon={<Sparkles size={15} />} onClose={closeAnimated}
      actions={<button className="soft-icon-button theme-button" aria-label={dark ? "切换为亮色主题" : "切换为暗色主题"} title={dark ? "切换为亮色主题" : "切换为暗色主题"} onClick={toggleTheme}>{dark ? <Moon size={16} /> : <Sun size={16} />}</button>} />
    <nav className="detail-tabs"><button className={detailTab === "quota" ? "is-active" : ""} onClick={() => setDetailTab("quota")}><Gauge size={13} />额度</button>
      {settings?.radarEnabled !== false && <button className={detailTab === "radar" ? "is-active" : ""} onClick={() => setDetailTab("radar")}><RadioTower size={13} />Codex Radar</button>}</nav>
    <div className="soft-scroll detail-content">
      {detailTab === "radar" && settings?.radarEnabled !== false ? <RadarView radar={radar} /> : <>
      <section className="soft-card hero-card interactive-surface" data-reveal onMouseMove={trackPointer} onMouseLeave={resetPointer}>
        <div className="hero-ring"><DualRing five={five} seven={seven} size={92} stroke={6.5} dark={dark} /></div>
        <div className="hero-copy"><span className="eyebrow"><Activity size={12} /> 当前可用额度</span>
          <strong>{remaining !== undefined ? `${remaining}%` : statusText(snapshot.credentialStatus)}</strong>
          <small><span className="live-dot" />{snapshot.cached ? "显示上次成功数据" : "每分钟自动同步"}</small></div>
      </section>
      {snapshot.error && <div className="soft-alert"><AlertCircle size={14} />{snapshot.error}</div>}
      {five ? <QuotaCard item={five} dark={dark} now={now} /> : <div className="quota-unavailable" data-reveal>当前账户没有 5 小时限额</div>}
      {seven ? <QuotaCard item={seven} dark={dark} now={now} /> : <div className="quota-unavailable" data-reveal>当前账户没有 7 天限额</div>}
      <footer className="detail-actions" data-reveal>
        <div><UpdateBadge status={update} /><small>{snapshot.queriedAt ? `额度更新于 ${new Date(snapshot.queriedAt).toLocaleTimeString("zh-CN")}` : "尚未成功查询"}</small></div>
        <div><button className="soft-icon-button" title="刷新" onClick={refresh}><RefreshCw size={16} className={refreshing ? "spin" : ""} /></button>
          <button className="soft-icon-button accent" title="设置" onClick={() => invoke("show_settings")}><Settings2 size={16} /></button></div>
      </footer>
      </>}
      <UpdatePanel status={update} onRefresh={refresh} refreshing={refreshing} />
    </div>
  </main>;
}

function radarColor(score?: number) {
  if (score === undefined) return "#8c96a8";
  if (score >= 110) return "#43c98a";
  if (score >= 90) return "#7289ff";
  if (score >= 60) return "#e5ae36";
  return "#ef6670";
}

function formatRadarCost(value?: number) {
  if (value === undefined) return "--";
  return `$${value >= 100 ? value.toFixed(0) : value.toFixed(1)}`;
}

function RadarView({ radar }: { radar: RadarSnapshot }) {
  const [refreshing, setRefreshing] = useState(false);
  const refresh = async () => { setRefreshing(true); try { await invoke("refresh_radar"); } finally { setRefreshing(false); } };
  return <div className="radar-panel">
    <section className="soft-card radar-heading interactive-surface" data-reveal onMouseMove={trackPointer} onMouseLeave={resetPointer}><div><span className={`radar-live ${radar.status ?? ""}`} /><div><strong>模型能力雷达</strong><small>{radar.batch ?? "等待数据"} · Public</small></div></div>
      <button className="soft-icon-button" title="刷新 Radar" onClick={refresh}><RefreshCw size={15} className={refreshing ? "spin" : ""} /></button></section>
    {radar.error && <div className="soft-alert"><AlertCircle size={14} />{radar.error}{radar.cached ? "（显示缓存）" : ""}</div>}
    {radar.resetSignals?.length ? <div className="radar-reset-signals" data-reveal>{radar.resetSignals.map((item) => <div key={item.label}><span>{item.label}</span><strong>{item.headline}</strong></div>)}</div>
      : radar.signal && <div className="radar-signal" data-reveal><RadioTower size={14} /><span>{radar.signal}</span></div>}
    <div className="radar-model-grid">{radar.models.length ? radar.models.map((model) => <article className="soft-card radar-model interactive-surface" data-reveal key={model.id} style={{ "--radar-color": radarColor(model.score) } as CSSProperties} onMouseMove={trackPointer} onMouseLeave={resetPointer}>
      <strong className="radar-model-name">{model.label}</strong>
      <div className="radar-iq"><small>IQ 分数</small><strong>{model.score?.toFixed(1) ?? "--"}</strong></div>
      <div className="radar-community"><span>社区体感</span><strong>{model.communityScore?.toFixed(1) ?? "--"}</strong></div>
      <div className="radar-reference"><span><small>参考价格</small><strong>{formatRadarCost(model.costUsd)}</strong></span><span><small>参考时间</small><strong>{model.wallTime ?? "--"}</strong></span></div>
    </article>) : <div className="radar-empty">暂无模型评分，点击刷新重试</div>}</div>
    <footer className="radar-footer" data-reveal><span>{radar.attribution}</span><span>{radar.updatedAt ? new Date(radar.updatedAt).toLocaleString("zh-CN") : "尚未更新"}</span></footer>
  </div>;
}

function Section({ icon, title, children }: { icon: ReactNode; title: string; children: ReactNode }) {
  return <section className="soft-card settings-section interactive-surface" onMouseMove={trackPointer} onMouseLeave={resetPointer}><h3><span>{icon}</span>{title}</h3>{children}</section>;
}

function SettingsPanel({ value, onSaved }: { value: AppSettings; onSaved: (next: AppSettings) => void }) {
  const [draft, setDraft] = useState(value);
  const [savedFlash, setSavedFlash] = useState(false);
  const [windowsGeneration, setWindowsGeneration] = useState<"windows10" | "windows11">("windows11");
  const update = useUpdateStatus();
  useEffect(() => setDraft(value), [value]);
  useEffect(() => { void invoke<"windows10" | "windows11">("get_windows_generation").then(setWindowsGeneration); }, []);
  const save = async () => {
    const saved = await invoke<AppSettings>("save_settings", { settings: draft });
    await invoke("set_autostart", { enabled: saved.autostart });
    onSaved(saved); setSavedFlash(true); window.setTimeout(() => setSavedFlash(false), 1400);
  };
  const checkUpdate = () => invoke("check_for_updates");
  const installUpdate = () => invoke("install_update");
  return <div className="settings-panel">
    <div className={`settings-preview ${draft.reverseLayout ? "is-reversed" : ""}`}>
      <DualRing size={Math.min(draft.ringSize, 34)} dark={true} animated={draft.animations}
        five={{ kind: "five_hour", label: "5 小时", usedPercent: 18, remainingPercent: 82 }}
        seven={{ kind: "seven_day", label: "7 天", usedPercent: 36, remainingPercent: 64 }} />
      <div><span style={{ color: quotaColor(82, true) }}>5h&nbsp; 82%</span><small>·&nbsp; 03:21</small><span style={{ color: quotaColor(64, true) }}>7d&nbsp; 64%</span><small>·&nbsp; 4d12h</small></div>
    </div>
    <Section icon={<Folder size={15} />} title="Codex 账户">
      <label className="soft-field"><span>Codex 根目录</span><input value={draft.codexHome ?? ""} placeholder="默认：%USERPROFILE%\.codex"
        onChange={(event) => setDraft({ ...draft, codexHome: event.target.value || undefined })} /></label>
    </Section>
    <Section icon={<Gauge size={15} />} title="尺寸与位置">
      <div className="setting-grid">
        <RangeSetting label="宽度" value={draft.displayWidth} min={140} max={420} unit="px" onChange={(displayWidth) => setDraft({ ...draft, displayWidth })} />
        <RangeSetting label="高度" value={draft.displayHeight} min={30} max={72} unit="px" onChange={(displayHeight) => setDraft({ ...draft, displayHeight })} />
        <RangeSetting label="水平偏移" value={draft.horizontalOffset} min={-240} max={240} unit="px" onChange={(horizontalOffset) => setDraft({ ...draft, horizontalOffset })} />
        <RangeSetting label="垂直偏移" value={draft.verticalOffset} min={-48} max={48} unit="px" onChange={(verticalOffset) => setDraft({ ...draft, verticalOffset })} />
      </div>
      <div className="section-divider" /><div className="subheading">任务栏布局 · {windowsGeneration === "windows11" ? "Windows 11" : "Windows 10"}</div>
      {windowsGeneration === "windows11" ? <div className="setting-grid">
        <SelectSetting label="所在区域" value={draft.taskbarRegion} onChange={(taskbarRegion) => setDraft({ ...draft, taskbarRegion })} />
        <SelectSetting label="窗口对齐" value={draft.windowAlignment} onChange={(windowAlignment) => setDraft({ ...draft, windowAlignment })} />
      </div> : <div className="setting-hint">Windows 10 默认固定在托盘左侧。</div>}
    </Section>
    <Section icon={<Palette size={15} />} title="外观与行为">
      <div className="setting-grid">
        <RangeSetting label="字体缩放" value={draft.fontScale} min={0.75} max={1.6} step={0.05} unit="×" onChange={(fontScale) => setDraft({ ...draft, fontScale })} />
        <RangeSetting label="环形大小" value={draft.ringSize} min={18} max={42} unit="px" onChange={(ringSize) => setDraft({ ...draft, ringSize })} />
      </div>
      <ToggleSetting label="显示重置倒计时" checked={draft.showCountdown} onChange={(showCountdown) => setDraft({ ...draft, showCountdown })} />
      <ToggleSetting label="平滑动画" checked={draft.animations} onChange={(animations) => setDraft({ ...draft, animations })} />
      <ToggleSetting label="跟随系统主题" checked={draft.followSystemTheme} onChange={(followSystemTheme) => setDraft({ ...draft, followSystemTheme })} />
      <ToggleSetting label="反转环形－额度－倒计时" checked={draft.reverseLayout} onChange={(reverseLayout) => setDraft({ ...draft, reverseLayout })} />
      <ToggleSetting label="随 Windows 登录启动" checked={draft.autostart} onChange={(autostart) => setDraft({ ...draft, autostart })} />
      <ToggleSetting label="启用 Codex Radar" checked={draft.radarEnabled} onChange={(radarEnabled) => setDraft({ ...draft, radarEnabled })} />
    </Section>
    <Section icon={<Download size={15} />} title="软件更新">
      <div className="update-row"><div><strong>自动安装更新</strong><small>关闭时仍检查版本，但只在手动确认后安装</small></div><Toggle checked={draft.autoUpdate} onChange={(autoUpdate) => setDraft({ ...draft, autoUpdate })} /></div>
      <div className="update-actions"><UpdateBadge status={update} />
        {update.state === "available" ? <button className="soft-button accent" onClick={installUpdate}><Download size={14} />安装并重启</button>
          : <button className="soft-button" disabled={["checking", "downloading", "installing"].includes(update.state)} onClick={checkUpdate}><RefreshCw size={14} />检查更新</button>}</div>
    </Section>
    <button className={`primary-button ${savedFlash ? "is-saved" : ""}`} onClick={save}>{savedFlash ? <CheckCircle2 size={16} /> : <Check size={16} />}{savedFlash ? "已保存" : "保存设置"}</button>
  </div>;
}

function Toggle({ checked, onChange }: { checked: boolean; onChange: (checked: boolean) => void }) {
  return <button type="button" className={`soft-toggle ${checked ? "is-on" : ""}`} role="switch" aria-checked={checked} onClick={() => onChange(!checked)}><span /></button>;
}

function ToggleSetting({ label, checked, onChange }: { label: string; checked: boolean; onChange: (checked: boolean) => void }) {
  return <div className="toggle-setting"><span>{label}</span><Toggle checked={checked} onChange={onChange} /></div>;
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

function SettingsView() {
  const settings = useSettings();
  const dark = useDarkTheme(settings?.followSystemTheme ?? true);
  return <main className="soft-shell settings-window" data-theme={dark ? "dark" : "light"}>
    <span className="ambient-orb orb-one" /><span className="ambient-orb orb-two" />
    <WindowHeader title="个性化设置" subtitle="Soft UI 控制中心" icon={<Settings2 size={15} />} />
    <div className="soft-scroll settings-content">{settings ? <SettingsPanel value={settings} onSaved={() => undefined} /> : <div className="settings-loading"><RefreshCw className="spin" />正在加载设置…</div>}</div>
  </main>;
}

function MenuView() {
  const action = useCallback(async (command: string, args?: Record<string, unknown>) => {
    await invoke(command, args); if (command !== "quit_app") await invoke("hide_current_window");
  }, []);
  return <main className="soft-shell context-menu">
    <button onClick={() => action("refresh_now")}><RefreshCw size={15} />立即刷新</button>
    <button onClick={() => action("show_detail")}><Activity size={15} />额度详情</button>
    <button onClick={() => action("show_settings")}><Settings2 size={15} />个性化设置</button>
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
