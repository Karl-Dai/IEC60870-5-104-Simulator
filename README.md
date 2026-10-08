<div align="center">

# ⚡ IEC 60870-5-104 Simulator

**A cross-platform IEC 60870-5-104 protocol simulator — Slave _and_ Master, in one desktop toolkit.**

### 🌐 [Try SimLab Online — simlab.carldai.cloud](https://simlab.carldai.cloud)

[![Release](https://img.shields.io/github/v/release/Karl-Dai/IEC60870-5-104-Simulator?label=release&color=2ea043)](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases)
[![Downloads](https://img.shields.io/github/downloads/Karl-Dai/IEC60870-5-104-Simulator/total?color=1f6feb)](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases)
[![Stars](https://img.shields.io/github/stars/Karl-Dai/IEC60870-5-104-Simulator?color=e3b341)](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/stargazers)
[![License: MIT](https://img.shields.io/badge/License-MIT-lightgrey.svg)](LICENSE)
![Platform](https://img.shields.io/badge/Platform-Windows%20·%20macOS%20·%20Linux-informational)

Built with **Rust** · **Tauri 2** · **Vue 3**

**English** · [中文](README_CN.md)

<picture>
  <source srcset="docs/screenshots/iec104-simulator-demo.webp" type="image/webp">
  <img src="docs/screenshots/tut-1-slave-current-main.png" alt="Animated IEC104 Slave and Master workflow" width="100%">
</picture>
<br>
<sub>Slave point model → traffic analysis → random simulation → Master multi-CA</sub>

</div>

---

## Why this project

Testing an IEC 104 integration usually means borrowing a real RTU or a master station. This project puts **both ends on your desktop**:

- 🛰️ **Slave & Master in one repo** — simulate a substation device, or drive one, with no external hardware.
- 🔌 **Common protocol operations** — 8 monitored data categories (with the implemented CP24/CP56 variants), control / setpoint / bitstring commands, GI / Counter / Clock-Sync, over **TCP or mutual TLS**.
- 🎛️ **Control points as first-class objects** — declare command / setpoint points (Type 45–51 / 58–64), map each to a monitor point across CA/IOA, with per-point qualifier and Select-Before-Operate.
- 🌐 **Multi-CA on a single link** — one TCP connection talks to many Common Addresses at once, each kept separate.
- 🖥️ **Native desktop app** — small Rust + Tauri binaries for Windows, macOS and Linux, with in-app auto-update.
- 🌏 **Bilingual UI** — full English / 简体中文, switchable at runtime.

## Table of Contents

- [Screenshots](#screenshots)
- [Features](#features)
- [Workspace save and restore](#workspace-save-and-restore)
- [TLS and certificates](#tls-and-certificates)
- [Download](#download)
- [Build from Source](#build-from-source)
- [Quick Start (Tutorial)](#quick-start-tutorial)
- [Protocol Support](#protocol-support)
- [Architecture](#architecture)
- [Contributing](#contributing)
- [Changelog](#changelog)
- [macOS First Launch](#macos-first-launch)
- [License](#license)

## Screenshots

These are illustrative captures from earlier versions; menu placement and version labels may differ. Follow the current menu paths in the tutorial below.

**Slave · point CSV and searchable traffic logs**

Use **Points → Import CSV / Export CSV / Download Template** for point-table files. Its expanded communication log can filter by direction and frame kind or Type ID, search decoded details and raw bytes, report visible/total counts, and export the current filtered view.

![Slave point CSV actions and searchable traffic logs](docs/screenshots/slave-point-csv-log-analysis.png)

**Slave · live Master connection viewer**

Every Slave server node shows the number of connected Masters in real time. Click the badge — or use **View Master Connections** in the server context menu — to inspect each peer's IP/port and whether IEC 104 data transfer is active after STARTDT. The count and detail list refresh automatically as clients connect, activate data transfer, or disconnect.

![Slave live Master connection viewer](docs/screenshots/slave-master-connections.png)

**Slave · bounded random simulation**

Select any numeric point and open **Simulation Settings** to run independent periodic values in **Random** mode. The drawer shows the selected IOA and Type ID, period, Min / Max bounds, current value, and every active simulation; the point table keeps a live per-row mode indicator.

**Master · multi-CA on one TCP link**

One IEC 104 master connection can talk to several stations (Common Addresses) at once. Configure the CA list as `1, 2, 3` in the **New Connection** dialog and the connection tree expands to **Connection → CA badge → category**, with per-CA point counts — so two stations sharing the same IOA never collide on screen.

![Master multi-CA tree and new-connection dialog](docs/screenshots/master-multi-ca-newconn.png)

**Master · SOCKS5 proxy with optional authentication and remote DNS**

Route an IEC 104 connection through a SOCKS5 proxy when the target is not directly reachable. The **New Connection** dialog accepts the proxy host and port, optional username/password authentication, and remote target resolution to avoid a local DNS lookup.

![Master SOCKS5 proxy connection settings](docs/screenshots/master-socks5-connection.png)

**Master · communication log with TLS handshake & per-CA GI**

The bottom log panel shows every TLS handshake step, U/I/S frame, COT decode, and the raw hex bytes side-by-side. Here the master sends **GI CA=1** and **GI CA=2** in sequence and receives the GI response data (COT=20) from each station.

![Master communication log with TLS and multi-CA GI](docs/screenshots/master-multi-ca-comm-log.png)

## Features

Both apps provide light/dark themes (initially following the OS, with a saved manual choice), runtime Chinese/English switching, local workspace restoration and signed in-app updates.

### 🛰️ Slave — `IEC104Slave`

- **IEC 104 server** with TCP and TLS support
- **8 data types** — Single Point, Double Point, Step Position, Bitstring, Normalized, Scaled, Short Float, Integrated Totals; monitor direction includes CP24 (short) and CP56 (full) time-tagged variants
- **Data point management** — add points singly or in batch, with IOA ranges and non-contiguous expressions (e.g. `6001, 6003, 6010-6050`); the edit dialog can move a point to a new IOA, and multi-select batch-sets control QU/QL and S/E
- **Point-table CSV round-trip** — download a schema template, export a station's complete point configuration, then import it in Merge or Replace mode while the server is stopped; validation reports the exact row and field, and periodic-mutation settings round-trip with the points
- **Point-event JSON playback** — set point values at relative millisecond offsets and send COT=3; same-time events of one type use SQ=0 packing, including repeated IOAs with different values in one ASDU
- **Batch value write by IOA expression** — type a mix of single IOAs and ranges (e.g. `100, 1000-2000, 5000`), pick a type, and write one value to every matching point — with a live matched/ignored preview, no Ctrl-clicking across thousands of rows
- **Per-point periodic mutation** — right-click any point(s) to start/stop a periodic change with an in-row pulse indicator; analog points and counters ramp as a triangle wave (increment/decrement with step and bounds), discrete points flip; points mutate concurrently and independently
- **Random mutation** and **cyclic transmission** — simulate value changes / periodic sending at a configurable interval
- **Spontaneous transmission** (COT=3) — automatically pushes changed values to connected masters
- **Live Master connection viewer** — each Slave server shows its current Master count; click the badge or use the server context menu to inspect peer IP/port and whether IEC 104 data transfer is active (STARTDT), refreshed automatically
- **General Interrogation** (GI) and **Counter Interrogation** responses
- **Control command handling** — Single, Double, Step, Setpoint and Bitstring commands (Type 45–51 and timestamped 58–64), with protocol-correct negative confirmations (unknown IOA/type, qualifier mismatch, SBO violations)
- **Control points as data points** — declare control-direction points, edit them in place, map each to a monitor point across CA/IOA, and set a per-point QOC/QL qualifier and S/E execution mode (direct / Select-Before-Operate); the legacy same-CA+IOA auto write-back stays available as a compatibility switch (on by default)
- **Editable listen address/port** — change a stopped server's bind address/port in place, no delete-and-recreate
- **Communication log analysis** — filter RX/TX and I/S/U frames or a specific Type ID, search decoded detail and raw bytes, resize columns and panel height, auto-follow live traffic, and export either all logs or the current filtered view to CSV
- **Bulk operations** — start/stop all servers, batch-delete servers/stations/points, and batch-edit point types and control options
- **Simulation upload throttle** — configure a point-update batch size and delay per server; each Master connection is paced independently while protocol replies continue
- Server auto-starts on creation

### Point-event JSON format

Choose **Import Event JSON** from the **Points** menu. The server must be running and at least one Master must have completed STARTDT. Each `time_ms` is measured from the confirmed playback start.

```json
[
  { "time_ms": 1000, "type": "M_SP_TB_1", "ioa": 1001, "value": false },
  { "time_ms": 1000, "type": "M_SP_TB_1", "ioa": 1001, "value": true }
]
```

Use booleans or numbers for simple values. Step position uses `{"value":-1,"transient":false}`; integrated totals use `{"value":123,"carry":false,"sequence":0}`. Normalized measurements use the raw wire NVA integer range `-32768..32767`. Files are limited to 20 MiB, 100,000 events, and seven days. The whole file is validated before playback; any error rejects it without changing points. **Download Event JSON Example** generates an editable file from the selected station's existing points.

The event `type` does not have to match the point table exactly: an event is accepted when the IOA holds a point of the same type or the same data category (single point, double point, measurement, etc.; the untimestamped variant is preferred within a category). The value is written to that existing point, while the frame sent to the master still carries the type declared in the file.

### 📡 Master — `IEC104Master`

- **IEC 104 client** with TCP and TLS support
- **Per-connection SOCKS5 proxy** — configure the proxy address/port, optional username/password authentication, and local or remote DNS resolution directly in the New Connection dialog
- **Multi-CA per connection** — drive 1..N Common Addresses over a single TCP link. Select a CA explicitly from Commands → General Interrogation / Counter Read (or all CAs), while Clock Sync fans out to configured CAs; connecting does not automatically send GI; data is stored per-CA so colliding IOAs from different stations stay separate
- **Three-level connection tree** for multi-CA setups (Connection → CA badge → category) with independent per-CA counts; single-CA connections keep the classic flat tree
- **Real-time data display** with incremental polling and virtual scrolling
- **Category tree** with live point counts (SP, DP, ST, BO, ME_NA, ME_NB, ME_NC, IT)
- **Custom Control dialog** — pick a CA from the connection's configured list, type any IOA + value; stays open after a successful send for fast iteration and remembers your last CA / IOA / type / value via localStorage
- **Control commands (45–51)** — Direct Execute and Select-before-Operate (SbO, except execute-only bitstring); a right-click on any point routes to its actual source CA in multi-CA setups
- **Value panel** showing selected point details
- **General Interrogation**, **Counter Interrogation** and **Clock Sync** commands — GI and Counter Interrogation require CA selection even on single-CA connections (pick one CA or "all CAs")
- **Deactivation (COT=8)** — send General or Counter Interrogation deactivation requests (per-CA, "all CAs" fan-out, or broadcast); the slave answers with a Deactivation Confirmation (COT=9)
- **Auto-reconnect** — T0 limits one connection attempt; an independent **Channel Retry** value (default 5 s) sets the fixed pause before the next attempt, with no retry limit or exponential backoff
- **Communication log analysis** — TLS handshake events, U/I/S and COT decode, raw hex bytes, RX/TX + frame/Type ID filters, full-text search, resizable columns, auto-follow, and filtered CSV export
- **In-app auto-update** from GitHub Releases (ed25519-signed bundles, 6 h check throttle, "later" snoozes 24 h)

### Workspace save and restore

Both apps save workspace definitions locally after configuration changes and restore them on startup. The Slave restores servers, stations, point definitions, TLS and protocol/remote-operation settings. The Master restores connection definitions (including CA, TLS and SOCKS5 settings); **automatic saving omits the received live point table**. Use **Config → Save Config** to export JSON explicitly: a manual Master save includes the received point snapshot for offline inspection.

**Config → Open Config** replaces the current workspace with the selected JSON. Restored/imported Slave servers are **stopped**, and Master connections are **disconnected**; start/connect them explicitly. A saved Master snapshot is historical data, not a live connection. Certificate files are referenced by path and must remain accessible on the destination computer.

Exported configurations and local workspace storage can contain **SOCKS5 passwords in plain text**, site addresses and certificate paths. Remove credentials and sensitive site details before sharing; do not publish these files or commit private keys.

### TLS and certificates

The Slave needs a PEM server certificate and matching private key. For mutual TLS, enable **Require Client Certificate** and supply the client CA; on the Master, enable TLS and supply the server CA plus a client certificate/key when required. Windows paths pasted with wrapping quotes are normalized. X.509 v1/v3 are certificate formats, independent of TLS 1.2/1.3.

- **Slave:** rustls handles TLS; the compatibility verifier uses OpenSSL for legacy X.509 v1 certificates, including mixed v1/v3 mutual TLS. A v1 client must chain to the configured CA, be valid and prove private-key possession; weak keys/signatures are rejected. v1 lacks SAN/key-usage extensions, so prefer v3 for newly issued certificates. PEM files are not rewritten and Slave keys are not imported into the macOS Keychain.
- **Master:** a configured custom CA without a PKCS#12 identity selects the vendored OpenSSL path; PEM client certificate/key are optional for one-way TLS and required when the peer demands mutual TLS. CA/time/purpose/signature checks remain enabled by default, with system roots alongside the custom CA. Connections without a custom CA, or using a PKCS#12 identity at the core-library level, use native-tls with platform limitations. The desktop connection dialog exposes PEM paths.
- The Master currently disables hostname matching for device certificates, even with certificate validation enabled. **Accept Invalid Certificates** also disables trust validation; keep it off when checking peer trust. TLS support does not imply full IEC 62351 conformance.

To test an existing local certificate directory against the real core Master and Slave, run from the repository root:

```bash
IEC104_TLS_CERT_DIR=/path/to/certs cargo test -p iec104sim-core --lib tls_compat::tests::configured_certificate_directory -- --ignored
```

The directory must contain `ca.crt`, `server.crt`, `server.key`, `client.crt` and `client.key`. The test uses loopback, checks mutual TLS 1.2/1.3, exchanges STARTDT/TESTFR and receives GI data without modifying the certificate files. Never commit private keys or site certificates as fixtures.

## Download

Choose **both apps** for the tutorial from [v1.15.21 Releases](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/tag/v1.15.21). The filenames below match that release; check [Releases](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases) for newer versions.

| Platform / format | Slave | Master |
|---|---|---|
| macOS Apple Silicon | [IEC104Slave_1.15.21_aarch64.dmg](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Slave_1.15.21_aarch64.dmg) | [IEC104Master_1.15.21_aarch64.dmg](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Master_1.15.21_aarch64.dmg) |
| macOS Intel | [IEC104Slave_1.15.21_x64.dmg](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Slave_1.15.21_x64.dmg) | [IEC104Master_1.15.21_x64.dmg](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Master_1.15.21_x64.dmg) |
| Windows x64 · NSIS | [IEC104Slave_1.15.21_x64-setup.exe](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Slave_1.15.21_x64-setup.exe) | [IEC104Master_1.15.21_x64-setup.exe](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Master_1.15.21_x64-setup.exe) |
| Windows x64 · MSI | [IEC104Slave_1.15.21_x64_en-US.msi](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Slave_1.15.21_x64_en-US.msi) | [IEC104Master_1.15.21_x64_en-US.msi](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Master_1.15.21_x64_en-US.msi) |
| Windows x64 · Portable | [IEC104Slave_1.15.21_x64-portable.exe](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Slave_1.15.21_x64-portable.exe) | [IEC104Master_1.15.21_x64-portable.exe](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Master_1.15.21_x64-portable.exe) |
| Windows ARM64 · NSIS | [IEC104Slave_1.15.21_arm64-setup.exe](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Slave_1.15.21_arm64-setup.exe) | [IEC104Master_1.15.21_arm64-setup.exe](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Master_1.15.21_arm64-setup.exe) |
| Windows ARM64 · MSI | [IEC104Slave_1.15.21_arm64_en-US.msi](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Slave_1.15.21_arm64_en-US.msi) | [IEC104Master_1.15.21_arm64_en-US.msi](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Master_1.15.21_arm64_en-US.msi) |
| Windows ARM64 · Portable | [IEC104Slave_1.15.21_arm64-portable.exe](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Slave_1.15.21_arm64-portable.exe) | [IEC104Master_1.15.21_arm64-portable.exe](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Master_1.15.21_arm64-portable.exe) |
| Linux x64 · AppImage | [IEC104Slave_1.15.21_amd64.AppImage](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Slave_1.15.21_amd64.AppImage) | [IEC104Master_1.15.21_amd64.AppImage](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Master_1.15.21_amd64.AppImage) |
| Linux x64 · deb | [IEC104Slave_1.15.21_amd64.deb](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Slave_1.15.21_amd64.deb) | [IEC104Master_1.15.21_amd64.deb](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Master_1.15.21_amd64.deb) |
| Linux x64 · rpm | [IEC104Slave-1.15.21-1.x86_64.rpm](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Slave-1.15.21-1.x86_64.rpm) | [IEC104Master-1.15.21-1.x86_64.rpm](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/download/v1.15.21/IEC104Master-1.15.21-1.x86_64.rpm) |

Windows `-setup.exe` and `.msi` are installers; `-portable.exe` runs without an installer but still needs WebView2. Linux assets in this release are x64 only; AppImage may need `chmod +x <file>.AppImage`. The `.sig`, macOS `.app.tar.gz`, and `latest-master*.json` / `latest-slave*.json` files are updater signatures, bundles and manifests, not manual-install choices.

Both apps **auto-update** from GitHub Releases since v1.0.9. macOS users need [one extra step on first launch](#macos-first-launch).

### China mirror

Users in mainland China may have unstable access to GitHub Releases. Recommended mirror for direct installer downloads:

- <https://ghfast.top/https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/latest>

Since v1.12.10 the in-app updater tries a **self-hosted mainland accelerator** (`gh.carldai.cloud`, a Tencent Cloud node that relays through a Singapore reverse proxy to GitHub and mirrors installer downloads via the mainland front) **first**, then falls back to the GitHub origin automatically — no manual action needed. However, **the very first upgrade from an older version** uses the endpoint compiled into the old binary (github.com only); if the in-app update check fails, please download and install the new version once via the mirror above, after which the updater routes through the self-hosted mirror automatically.

## Build from Source

### Prerequisites

- [Rust](https://rustup.rs/): use current stable, as CI does. The app manifests declare `rust-version = "1.77.2"`; this is not a verified minimum for the complete dependency graph (the workspace does not commit `Cargo.lock`).
- [Node.js](https://nodejs.org/): the locked Vite requires `^20.19.0 || >=22.12.0`, and jsdom requires `^20.19.0 || ^22.13.0 || >=24.0.0`. Use a version satisfying both; Node 18 is insufficient. The [successful v1.15.21 main test run](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/actions/runs/37310280444) used **Node 20.20.2 / npm 10.8.2**; the workflow selects Node 20, not those exact patch versions.
- Tauri CLI **2.x**: `cargo install tauri-cli --version '^2' --locked`.
- A C compiler, Perl and Make (MSVC/NMake on Windows): the compatibility layer builds **vendored OpenSSL** and links it statically. Packaged apps do not require a separate OpenSSL installation.
- macOS: Xcode Command Line Tools (`xcode-select --install`). Windows: Visual Studio C++ build tools and WebView2.
- Linux: install the native Tauri/WebKit dependencies before Rust tests or desktop builds. For Ubuntu 22.04:

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config perl libssl-dev \
  libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
```

### Clone and install dependencies

```bash
git clone https://github.com/Karl-Dai/IEC60870-5-104-Simulator.git
cd IEC60870-5-104-Simulator
# Run from the repository root; each subshell returns here.
(cd frontend && npm ci)
(cd master-frontend && npm ci)
```

### Development: two terminals

Start each terminal in the cloned **repository root**. `cargo tauri dev` starts that app's Vite server automatically; keep both terminals running for the tutorial.

```bash
# Terminal 1, from the repository root: Slave
cd crates/iec104sim-app
cargo tauri dev
```

```bash
# Terminal 2, from the repository root: Master
cd crates/iec104master-app
cargo tauri dev
```

For frontend-only Master browser checks, run `(cd master-frontend && npm run dev:mock)` from the repository root. This mock does not exercise Rust/Tauri or IEC 104 networking; stop the Master dev session first because both use port 5177.

### Packaging

The Tauri configs have no `beforeBuildCommand`, so build **both frontend distributions first**. From the repository root, after `npm ci` above:

```bash
(cd frontend && npm run build)
(cd master-frontend && npm run build)
(cd crates/iec104sim-app && cargo tauri build)
(cd crates/iec104master-app && cargo tauri build)
```

This packages for the current host/target; producing the whole release matrix requires the platform-specific release workflow. Bundles are normally under `target/release/bundle/` (or `target/<target-triple>/release/bundle/` for an explicit target).

## Quick Start (Tutorial)

A full round-trip with the Master driving the simulated Slave — no hardware required. (The screenshots show the Chinese UI; flip to English any time with the **中 / EN** toggle.)

> **Install first:** grab the installer for your platform from the [Releases page](#download), or run from source (`cargo tauri dev`). Open **both** `IEC104Slave` and `IEC104Master`.

### Step 1 · Slave — create a server and add data points

Open **IEC104Slave** and choose **新建 → 新建服务器 (New → New Server)**. For a local-only trial, set the bind address to `127.0.0.1` (default `0.0.0.0` listens on all IPv4 interfaces), port `2404` and CA `1`. Creation adds the first station and starts the server; its default point count is zero. Select that station, then use **批量添加点位 (Batch Add Points)** above the table to add points spanning all 8 monitored types — single/double point, step position, bitstring, normalized, scaled, short-float and integrated totals. Each point carries an IOA, a value and quality flags.

![Slave with a running server and data points](docs/screenshots/tut-1-slave-current-main.png)

**Tip · batch-add**: the **批量添加 (Batch Add)** dialog takes an IOA range (e.g. `1-200`) and an ASDU type, creating hundreds of points in one shot.

**Server settings:** select a server and choose **设置 → 服务器设置 (Settings → Server Settings)**, or use its context menu, to edit the address, port and TLS. A running server offers **停止并编辑 (Stop and Edit)** and explains that clients will disconnect. Saving keeps the server stopped and preserves its stations and points. Disabling TLS retains certificate paths for later reuse. If creation/startup fails, the creation dialog retains its input and shows an inline error; retrying does not leave duplicate servers behind.

### Step 2 · Master — create a connection

Open **IEC104Master** and choose **连接 → 新建连接 (Connection → New Connection)**. The defaults already target the local Slave: address `127.0.0.1`, port `2404`, Common Address `1`.

- **Multi-CA on one link** — to reach several stations over a single TCP connection, list the Common Addresses comma-separated (`1, 2, 3`). The connection tree later expands to **Connection → CA badge → category**, with per-CA point counts so colliding IOAs from different stations never mix.
- **TLS** — tick **启用 TLS (Enable TLS)** and provide the server CA; provide client cert/key paths as well when the Slave requires mutual TLS (see [TLS and certificates](#tls-and-certificates)). Pasted paths with wrapping quotes from Windows *Copy as path* are auto-stripped.

Click **创建 (Create)**, then **连接 (Connect)**.

![New Connection dialog](docs/screenshots/tut-2-master-newconn.png)

### Step 3 · General Interrogation fills the table

Choose **召唤 → 总召唤 (Commands → General Interrogation)**, then select a CA explicitly, even for a single-CA connection; choose **全部 CA (all CAs)** to interrogate multiple stations. The Slave returns that station's monitor points (control points are excluded); the connection tree shows per-category counts and the table fills with the received IOAs, values and quality. Normalized measurements show as the raw NVA integer (i16), matching the wire bytes exactly.

![Master data table after General Interrogation](docs/screenshots/tut-3-master-data.png)

**Counter Interrogation** (累计量召唤) and **Clock Sync** (时钟同步) are also in **Commands**; Counter Read likewise requires CA selection, while Clock Sync targets the configured CA list.

### Step 4 · Control a point from the Master

Choose **召唤 → 自定义控制 (Commands → Custom Control)** (or right-click a data point → **控制** — this routes to the point's actual source CA, so multi-CA setups never send to the wrong station). The **Custom Control dialog** lets you:

- pick a **CA** from the connection's configured list,
- type any **IOA** and value,
- choose a **command type** (single / double / step / setpoint / bitstring),
- choose a **control mode** — **Direct Execute**, **Select-only**, or **Auto SbO** (select-before-operate, persisted for next time); bitstring is execute-only.

The dialog stays open after a successful send for fast iteration, and remembers your last CA / IOA / type / value / mode across opens and restarts.

For a visible write-back, first declare a matching control point on the Slave and map it to a monitor point, or use the legacy same-CA/IOA write-back setting. The tutorial's monitor points alone do not declare every command IOA.

### Step 5 · Mutate values and watch spontaneous updates

Back on the Slave, drive value changes and watch them surface live on the Master:

- **Select point(s) → 模拟设置 (Simulation Settings)** (also available from the context menu) — analog points and counters ramp as a **triangle wave** (set a step and min/max bounds, bounces at the limits; the in-row glyph shows ↑/↓/⇅), discrete points flip. Multiple points mutate concurrently and independently.
- **设置值 (Set Value)** above the point table — type a mix of single IOAs and ranges (e.g. `100, 1000-2000, 5000`), pick a type, write one value to every matching point with a live **matched N · ignored M** preview.
- Changed values are pushed **spontaneously (COT=3)** and appear in the Master's table and log in real time. If the Master's link drops, it **auto-reconnects** indefinitely: T0 bounds each attempt, while **Channel Retry** is the fixed delay between attempts (0 retries immediately).

### Step 6 · Read the wire — decoded frames & raw hex

Expand **通信日志 (Communication Log)** at the bottom (drag the splitter to resize — the height persists). Every U/I/S frame is decoded — frame type, Cause of Transmission, a readable detail and the raw hex side by side. Use the RX/TX, frame-kind or Type ID filters and full-text search to isolate a flow; the visible/total counter updates immediately, and **导出 CSV** exports that filtered view. The master's **auto-reconnect**, TLS handshake steps and **TESTFR** heartbeat are all logged.

Localized log descriptions and validation messages follow the selected UI language. Protocol identifiers and wire-level fields—such as `Type ID`, `COT`, `CA`, `IOA`, `QOI`, `QCC`, `S/E`, `QU/QL`, APDU hex, certificate paths, IP addresses, and OS error codes—are intentionally kept in their standard/original form.

![Communication log with decoded frames and raw hex](docs/screenshots/tut-4-master-log.png)

That's the full round-trip — server, points, interrogation, control, mutation and wire-level inspection, all on your desktop.

## Protocol Support

This is the implemented subset used by the desktop apps, not a claim of complete IEC 104 conformance or coverage of every ASDU/service.

| Capability | Implemented types / scope |
|------------|---------------------------|
| Slave monitor transmission; Master receive/display | M_SP_NA_1 / M_SP_TA_1 / M_SP_TB_1; M_DP_NA_1 / M_DP_TA_1 / M_DP_TB_1; M_ST_NA_1 / M_ST_TA_1 / M_ST_TB_1; M_BO_NA_1 / M_BO_TB_1; M_ME_NA_1 / M_ME_TA_1 / M_ME_TD_1 / M_ME_ND_1; M_ME_NB_1 / M_ME_TB_1 / M_ME_TE_1; M_ME_NC_1 / M_ME_TC_1 / M_ME_TF_1; M_IT_NA_1 / M_IT_TB_1 |
| Master control dialog sends; Slave handles | C_SC_NA_1, C_DC_NA_1, C_RC_NA_1, C_SE_NA_1, C_SE_NB_1, C_SE_NC_1, C_BO_NA_1 (45–51) |
| Slave additionally handles CP56-tagged controls | C_SC_TA_1, C_DC_TA_1, C_RC_TA_1, C_SE_TA_1, C_SE_TB_1, C_SE_TC_1, C_BO_TA_1 (58–64); these are not selectable in the current Master control dialog |
| System commands: Master sends, Slave responds | C_IC_NA_1 (100, GI), C_CI_NA_1 (101, Counter), C_CS_NA_1 (103, Clock Sync); GI/Counter activation and deactivation |
| Common COTs used | Spontaneous(3), Activation(6), ActivationCon(7), Deactivation(8), DeactivationCon(9), ActivationTerm(10), Interrogated(20), CounterInterrogated(37); negative/unknown-request responses where implemented |
| Transport | TCP, TLS (one-way or mutual authentication; see certificate/backend limits above) |

CP24 variants in this subset are Types **2, 4, 6, 10, 12, 14**; `M_ME_ND_1` (21) has no quality descriptor or timestamp. Control points do not participate in GI, cyclic or spontaneous monitor uploads. Bitstring control has no S/E bit, so the Master offers execute-only for it. The current desktop UI has **Parse Frame** for hex inspection, but no raw APDU send entry; internal raw-send code is not a user-facing feature.

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
| Backend | Rust, Tokio; Slave TLS via rustls, Master TLS via native-tls or OpenSSL; vendored OpenSSL certificate compatibility |
| Frontend | Vue 3, TypeScript, Vite |
| Desktop | Tauri 2 |

## Contributing

Issues and pull requests are welcome. From the repository root, after installing the native prerequisites, run the checks corresponding to [.github/workflows/test.yml](.github/workflows/test.yml):

```bash
# REPO_ROOT
(cd frontend && npm ci && npm test && npm run build)
(cd master-frontend && npm ci && npm test && npm run build)
cargo test --workspace
node scripts/prepare-release.mjs verify-current
```

Both frontend builds type-check and create the `frontend/dist` and `master-frontend/dist` required by Tauri during Rust compilation. The Rust CI job creates empty dist directories with `mkdir -p frontend/dist master-frontend/dist` when testing Rust alone; empty directories are not suitable for packaging. CI runs Rust tests on Ubuntu and Windows, and tests/builds both frontends on Ubuntu. The optional certificate-directory test above requires your own local certificates and is not part of the default suite.

## Changelog

See [CHANGELOG.md](CHANGELOG.md) or the [Releases page](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases).

Starting from v1.0.9, both apps check GitHub Releases on startup and prompt to install new versions. Users on v1.0.8 or earlier need to upgrade manually once.

<a id="first-launch-on-macos"></a>

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
