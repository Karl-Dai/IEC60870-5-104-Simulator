<div align="center">

# ⚡ IEC 60870-5-104 Simulator

**A cross-platform IEC 60870-5-104 protocol simulator — Slave _and_ Master, in one desktop toolkit.**

[![Release](https://img.shields.io/github/v/release/Karl-Dai/IEC60870-5-104-Simulator?label=release&color=2ea043)](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases)
[![Downloads](https://img.shields.io/github/downloads/Karl-Dai/IEC60870-5-104-Simulator/total?color=1f6feb)](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases)
[![Stars](https://img.shields.io/github/stars/Karl-Dai/IEC60870-5-104-Simulator?color=e3b341)](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/stargazers)
[![License: MIT](https://img.shields.io/badge/License-MIT-lightgrey.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows%20·%20macOS%20·%20Linux-informational)]()

Built with **Rust** · **Tauri 2** · **Vue 3**

**English** · [中文](README_CN.md)

![Master multi-CA tree and new-connection dialog](docs/screenshots/master-multi-ca-newconn.png)

</div>

---

## Why this project

Testing an IEC 104 integration usually means borrowing a real RTU or a master station. This project puts **both ends on your desktop**:

- 🛰️ **Slave & Master in one repo** — simulate a substation device, or drive one, with no external hardware.
- 🔌 **Full protocol coverage** — 8 monitored data types, every control command, GI / Counter / Clock-Sync, over **TCP or mutual TLS**.
- 🌐 **Multi-CA on a single link** — one TCP connection talks to many Common Addresses at once, each kept separate.
- 🖥️ **Native desktop app** — small Rust + Tauri binaries for Windows, macOS and Linux, with in-app auto-update.
- 🌏 **Bilingual UI** — full English / 简体中文, switchable at runtime.

## Table of Contents

- [Screenshots](#screenshots)
- [Features](#features)
- [Download](#download)
- [Build from Source](#build-from-source)
- [Quick Start](#quick-start)
- [Protocol Support](#protocol-support)
- [Architecture](#architecture)
- [Contributing](#contributing)
- [Changelog](#changelog)
- [macOS First Launch](#macos-first-launch)
- [License](#license)

## Screenshots

**Master · multi-CA on one TCP link**

One IEC 104 master connection can talk to several stations (Common Addresses) at once. Configure the CA list as `1, 2, 3` in the **New Connection** dialog and the connection tree expands to **Connection → CA badge → category**, with per-CA point counts — so two stations sharing the same IOA never collide on screen.

![Master multi-CA tree and new-connection dialog](docs/screenshots/master-multi-ca-newconn.png)

**Master · communication log with TLS handshake & per-CA GI**

The bottom log panel shows every TLS handshake step, U/I/S frame, COT decode, and the raw hex bytes side-by-side. Here the master sends **GI CA=1** and **GI CA=2** in sequence and receives the spontaneous response stream from each station.

![Master communication log with TLS and multi-CA GI](docs/screenshots/master-multi-ca-comm-log.png)

## Features

### 🛰️ Slave — `IEC104Slave`

- **IEC 104 server** with TCP and TLS support
- **8 data types** — Single Point, Double Point, Step Position, Bitstring, Normalized, Scaled, Short Float, Integrated Totals
- **Data point management** — add single or batch points with IOA range and ASDU type selection
- **Random mutation** — simulate value changes at a configurable interval
- **Spontaneous transmission** (COT=3) — automatically pushes changed values to connected masters
- **Cyclic transmission** — periodic data sending at a configurable interval
- **General Interrogation** (GI) and **Counter Interrogation** responses
- **Control command handling** — Single, Double, Step and Setpoint commands
- **Communication log** with hex frame display and CSV export
- Server auto-starts on creation

### 📡 Master — `IEC104Master`

- **IEC 104 client** with TCP and TLS support
- **Multi-CA per connection** — drive 1..N Common Addresses over a single TCP link. Auto-GI / Clock-Sync / Counter-Read fan out to every CA; data is stored per-CA so colliding IOAs from different stations stay separate
- **Three-level connection tree** for multi-CA setups (Connection → CA badge → category) with independent per-CA counts; single-CA connections keep the classic flat tree
- **Real-time data display** with incremental polling and virtual scrolling
- **Category tree** with live point counts (SP, DP, ST, BO, ME_NA, ME_NB, ME_NC, IT)
- **Custom Control dialog** — pick a CA from the connection's configured list, type any IOA + value; stays open after a successful send for fast iteration and remembers your last CA / IOA / type / value via localStorage
- **Control commands** — Direct Execute and Select-before-Operate (SbO); a right-click on any point routes to its actual source CA in multi-CA setups
- **Value panel** showing selected point details
- **General Interrogation**, **Counter Read** and **Clock Sync** commands
- **Communication log** with TLS handshake events, U/I/S frame decode, COT names, raw hex bytes and CSV export
- **In-app auto-update** from GitHub Releases (ed25519-signed bundles, 6 h check throttle, "later" snoozes 24 h)

## Download

Pre-built installers for every platform are on the **[Releases page](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases)**.

| Platform | Installer |
|----------|-----------|
| Windows  | `.msi` / `.exe` (NSIS) |
| macOS    | `.dmg` (Apple Silicon & Intel) |
| Linux    | `.AppImage` / `.deb` |

Both apps **auto-update** from GitHub Releases since v1.0.9. macOS users need [one extra step on first launch](#macos-first-launch).

### China mirror

Users in mainland China may have unstable access to GitHub Releases. Recommended mirror for direct installer downloads:

- <https://ghfast.top/https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/latest>

Starting from the version that includes this change, the in-app updater automatically falls back through multiple proxies — no manual action needed. However, **the very first upgrade from an older version** uses the endpoint compiled into the old binary (github.com only); if the in-app update check fails, please download and install the new version once via the mirror above, after which the updater will route through proxies automatically.

## Build from Source

### Prerequisites

- [Rust](https://rustup.rs/) 1.77+
- [Node.js](https://nodejs.org/) 18+
- [Tauri CLI](https://tauri.app/) — `cargo install tauri-cli`

### Steps

```bash
# install frontend dependencies
cd frontend && npm install
cd ../master-frontend && npm install

# run the Slave
cd crates/iec104sim-app && cargo tauri dev

# run the Master
cd crates/iec104master-app && cargo tauri dev
```

## Quick Start

A full round-trip in four steps — drive the simulated Slave from the Master, no hardware required. (Screenshots show the Chinese UI; flip to English any time with the **中 / EN** toggle.)

### 1 · Slave — create a server with data points

Open **IEC104Slave** and click **新建服务器 (New Server)**: it binds `0.0.0.0:2404` and auto-starts. Add a station, then batch-add points spanning all 8 monitored types — single/double point, step position, bitstring, normalized, scaled, short-float and integrated totals. Each point carries an IOA, a value and quality flags.

![Slave with a running server and data points](docs/screenshots/tut-1-slave.png)

### 2 · Master — create a connection

Open **IEC104Master** and click **新建连接 (New Connection)**. The defaults already target the local Slave: address `127.0.0.1`, port `2404`, Common Address `1`. To reach several stations over one link, list the CAs comma-separated (`1, 2, 3`); tick **启用 TLS (Enable TLS)** for a secure link. Click **创建 (Create)**, then **连接 (Connect)**.

![New Connection dialog](docs/screenshots/tut-2-master-newconn.png)

### 3 · Master — General Interrogation fills the table

Press **总召唤 (General Interrogation)**. The Slave answers with every point; the connection tree shows per-category counts and the table fills with the received IOAs, values and quality.

![Master data table after General Interrogation](docs/screenshots/tut-3-master-data.png)

### 4 · Watch the wire — and live updates

Expand **通信日志 (Communication Log)** at the bottom: every U/I/S frame is decoded — frame type, Cause of Transmission, a readable detail and the raw hex side by side. Back on the Slave, click **随机变化 (Random Mutation)** — changed values are pushed spontaneously (COT=3) and surface in the Master's table and log in real time.

![Communication log with decoded frames and raw hex](docs/screenshots/tut-4-master-log.png)

## Protocol Support

| Feature | Supported Types |
|---------|-----------------|
| Monitor (Slave→Master) | M_SP_NA/TB, M_DP_NA/TB, M_ST_NA/TB, M_BO_NA/TB, M_ME_NA/TD, M_ME_NB/TE, M_ME_NC/TF, M_IT_NA/TB |
| Control (Master→Slave) | C_SC_NA, C_DC_NA, C_RC_NA, C_SE_NA/NB/NC |
| System | C_IC_NA (GI), C_CI_NA (Counter), C_CS_NA (Clock Sync) |
| COT | Spontaneous(3), Activation(6), ActivationCon(7), ActivationTerm(10), Interrogated(20), CounterInterrogated(37) |
| Transport | TCP, TLS (mutual TLS supported) |

## Architecture

```
IEC104Sim/
├── crates/
│   ├── iec104sim-core/     # Core IEC 104 protocol library
│   ├── iec104sim-app/      # Slave Tauri application
│   └── iec104master-app/   # Master Tauri application
├── frontend/               # Slave Vue 3 frontend
├── master-frontend/        # Master Vue 3 frontend
└── shared-frontend/        # Shared Vue components, i18n, styles
```

| Layer | Stack |
|-------|-------|
| Backend | Rust, Tokio (async runtime), native-tls |
| Frontend | Vue 3, TypeScript, Vite |
| Desktop | Tauri 2 |

## Contributing

Issues and pull requests are welcome. For a code change, please make sure `cargo test --workspace` and the frontend `npm test` suites pass before opening a PR.

## Changelog

See [CHANGELOG.md](CHANGELOG.md) or the [Releases page](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases).

Starting from v1.0.9, both apps check GitHub Releases on startup and prompt to install new versions. Users on v1.0.8 or earlier need to upgrade manually once.

## macOS First Launch

The bundles are **not Apple-notarized** (no paid Developer Program). On first launch macOS shows *"IEC104Slave / IEC104Master cannot be opened — Apple could not verify…"* with only *Done* and *Move to Trash* buttons. This is the standard macOS 15 (Sequoia) block for ad-hoc-signed apps — the app is **not damaged**.

<details>
<summary><b>How to allow it (pick one)</b></summary>

**1. GUI path**

- Double-click the `.app`, see the block dialog, click *Done*.
- Open *System Settings → Privacy & Security*, scroll to the bottom.
- You'll see *"IEC104Slave was blocked…"* — click *Open Anyway* and enter your password.
- The next dialog has an *Open* button; click it. Subsequent launches go straight through.

**2. One-line Terminal**

```bash
xattr -dr com.apple.quarantine "/Applications/IEC104Slave.app"
xattr -dr com.apple.quarantine "/Applications/IEC104Master.app"
```

Strips the quarantine flag so macOS stops blocking.

If you instead see *"is damaged, can't be opened"*, that's a v1.1.1-or-earlier build with no signature at all — upgrade to v1.1.2+ (the in-app updater will push it) or run the `xattr` command above.

</details>

## License

[MIT](LICENSE)
