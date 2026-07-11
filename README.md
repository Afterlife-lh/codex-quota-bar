# Codex Quota Bar

[![Windows](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-1674d1)](https://github.com/provance-bot/codex-quota-bar/releases)
[![GitHub release](https://img.shields.io/github/v/release/provance-bot/codex-quota-bar)](https://github.com/provance-bot/codex-quota-bar/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

Codex Quota Bar is a lightweight Windows utility that reads your existing
Codex ChatGPT login and displays the remaining 5-hour and 7-day quota directly
beside the notification area.

## Features

- Transparent two-line taskbar widget with nested 5h/7d quota rings.
- Remaining percentage, reset countdown, continuous quota colors, and stale-data status.
- Native tray icon with refresh, details, personalization, autostart, and exit actions.
- Adjustable width, height, offsets, font scale, ring size, theme, and animations.
- Windows 10 tray-left placement and Windows 11 left/right region and alignment controls.
- Reversible ring/quota/countdown layout.
- Smooth repositioning when task or notification icons change.
- Optional Lyricify Lite collision avoidance (`lyrics → quota → tray`).
- Read-only Codex authentication and no independent OAuth flow.

## Install

Download the current x64 MSI from [GitHub Releases](https://github.com/provance-bot/codex-quota-bar/releases/latest).
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
