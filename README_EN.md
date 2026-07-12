# Codex Quota Bar

[简体中文](README.md) | English

[![Windows](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-1674d1)](https://github.com/Afterlife-lh/codex-quota-bar/releases)
[![GitHub release](https://img.shields.io/github/v/release/Afterlife-lh/codex-quota-bar)](https://github.com/Afterlife-lh/codex-quota-bar/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

Codex Quota Bar is a lightweight Windows utility that reads your existing
Codex ChatGPT login and displays the remaining 5-hour and 7-day quota directly
beside the notification area.

![Codex Quota Bar taskbar preview](QC_35dc4aadd8c7f218.png)

## Interface Preview

| Codex Radar | Quota details |
| --- | --- |
| ![Codex Radar model intelligence view](QC_700b3d87b3644f8d.png) | ![Codex quota details view](QC_91f67c0ea8e32e77.png) |

> The interface continues to evolve; current releases may refine the layout and displayed data.

## Features

- Transparent two-line taskbar widget with nested 5h/7d quota rings.
- Remaining percentage, reset countdown, continuous quota colors, and stale-data status.
- Native tray icon with refresh, details, personalization, autostart, and exit actions.
- Built-in Codex Radar view for public model scores, quota radar, and current signals.
- Adjustable width, height, offsets, font scale, ring size, theme, and animations.
- Windows 10 tray-left placement and Windows 11 left/right region and alignment controls.
- Reversible ring/quota/countdown layout.
- Smooth repositioning when task or notification icons change.
- Optional Lyricify Lite collision avoidance (`lyrics → quota → tray`).
- Read-only Codex authentication and no independent OAuth flow.

## Install

Download the current x64 MSI from
[GitHub Releases](https://github.com/Afterlife-lh/codex-quota-bar/releases/latest).
Installing a newer MSI upgrades an existing installation.

## Privacy and compatibility

- Credentials are read only from a configured Codex home, `CODEX_HOME`, or
  `%USERPROFILE%\.codex`, in that order.
- Access tokens never leave the Rust backend and are never written to logs.
- Never copy your personal `auth.json` into this repository or an issue report.
- The ChatGPT quota endpoint used by Codex is an undocumented compatibility
  layer and may change without notice.
- The app targets the unmodified primary Windows taskbar. Tools such as
  ExplorerPatcher and StartAllBack are not supported.
- Lyricify Lite coordination is enabled by default and can be disabled in the
  appearance settings if another taskbar customization tool controls layout.

## Changelog

### 0.9.0

- Fixed the fully rendered detail page flashing before its entrance animation begins.
- Made model-card pointer tracking substantially more responsive.
- The theme button now shows the sun or moon for the active theme and switches through a button-originated ripple animation.
- The updater now waits for MSI completion and automatically relaunches after confirming the new executable exists.

### 0.8.0

- Added scale, motion, opacity, and blur entrance animation every time the detail window opens, plus consistent animated exits.
- Clicking outside, losing focus, or clicking the taskbar widget again now closes details after the exit animation.
- Added pointer-tracked card glow, subtle 3D tilt, hover depth, and scroll-driven reveal animations.
- Added a remembered sun/moon theme toggle to the detail window.
- Added reference price and time to Radar cards, removed vote counts, and enlarged model names.

### 0.7.0

- Reworked Radar card hierarchy to emphasize model name, IQ, community perception, then rating count.
- Removed task pass counts, the community `/10` suffix, and quota-radar table to give the eight model cards more space.
- Added explicit Windows current-user system proxy discovery for both single-address and `http=…;https=…` formats.
- Quota, Radar, and updater connections now re-read proxy settings without requiring an app restart.

### 0.6.0

- Replaced the fixed Radar model cap with a dynamic model list, currently showing all eight IQ models from the official site.
- Matched the official order: Sol max→low, Terra xhigh→medium, then Luna medium; future models are appended automatically.
- Added rolling 24-hour community perception averages and vote counts to each IQ card.
- Changed Radar background refresh to five minutes to match the public rating cache.
- Restored GitHub Actions MSI builds to validate the remote auto-update path.

### 0.5.0

- Added a Codex Radar detail view backed by the codexradar.com public summary, including dynamic model scores, task results, quota tiers, and radar signals.
- Added a Codex Radar setting, manual refresh, and 30-minute background refresh.
- Fixed post-update restart failures caused by Windows extended `\\?\` paths being parsed as a missing `\\` file.
- Replaced the `cmd start` updater helper with a hidden PowerShell MSI installer and safe process restart.

### 0.4.1

- Fixed manual and scheduled refreshes alternating between quota snapshots returned by inconsistent cache nodes.
- Extended premature quota-increase confirmation to every window before its current reset time.
- Fixed low-contrast countdown and separator text on dark taskbars.

### 0.4.0

- Redesigned the quota details and personalization windows with light and dark Soft UI themes.
- Increased detail and settings text sizes for better readability on high-resolution displays.
- Added entrance, card, progress, button-feedback, and ambient animations.
- Added automatic GitHub Release updates with installation and app restart.
- Hide the taskbar widget and auxiliary windows while a fullscreen app is active or the taskbar is hidden.
- Clicking the taskbar widget again now closes the open details window.
- Added a GitHub Actions workflow for Windows MSI releases.

### 0.3.1

- Fixed countdown clipping and overlap in narrow reversed layouts.

### 0.3.0

- Added Windows 10/11 placement detection, region/alignment controls, layout reversal, and movement animation.
- Added confirmation for suspicious quota jumps.

## Development

Requirements: Node.js 20+, pnpm, Rust 1.77.2+, WebView2, and the Windows
tooling required by Tauri 2.

```powershell
corepack enable
pnpm install
pnpm dev
```

Checks and release build:

```powershell
pnpm typecheck
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml
pnpm build
```

The MSI is emitted under `src-tauri/target/release/bundle/msi/`.

## Attribution

The credential and quota request implementation was adapted from the
MIT-licensed [CC Switch](https://github.com/farion1231/cc-switch) project.
See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

The Codex Radar integration design was adapted from the MIT-licensed
[codex-monitor-macos](https://github.com/jackiemingnew/codex-monitor-macos),
with public data attributed to [codexradar.com](https://codexradar.com).
