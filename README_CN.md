<div align="center">

# ⚡ IEC 60870-5-104 Simulator

**跨平台 IEC 60870-5-104 协议仿真工具 —— 从站与主站,一套桌面工具全包。**

[![Release](https://img.shields.io/github/v/release/Karl-Dai/IEC60870-5-104-Simulator?label=release&color=2ea043)](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases)
[![Downloads](https://img.shields.io/github/downloads/Karl-Dai/IEC60870-5-104-Simulator/total?color=1f6feb)](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases)
[![Stars](https://img.shields.io/github/stars/Karl-Dai/IEC60870-5-104-Simulator?color=e3b341)](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/stargazers)
[![License: MIT](https://img.shields.io/badge/License-MIT-lightgrey.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows%20·%20macOS%20·%20Linux-informational)]()

基于 **Rust** · **Tauri 2** · **Vue 3** 构建

[English](README.md) · **中文**

![主站多 CA 树形展示与新建连接对话框](docs/screenshots/master-multi-ca-newconn.png)

</div>

---

## 项目简介

测试 IEC 104 集成往往需要借一台真实 RTU 或主站设备。本项目把**通信两端都搬到你的桌面**:

- 🛰️ **从站与主站同仓** —— 模拟一台变电站设备,或去驱动一台,无需任何外部硬件。
- 🔌 **协议覆盖完整** —— 8 种监视数据类型、全部控制命令、总召/累计量召唤/时钟同步,支持 **TCP 或双向 TLS**。
- 🌐 **单链路多公共地址** —— 一条 TCP 连接同时与多个 Common Address 对话,各站数据互不串扰。
- 🖥️ **原生桌面应用** —— Rust + Tauri 的小体积安装包,覆盖 Windows / macOS / Linux,内置自动更新。
- 🌏 **中英双语界面** —— 完整 English / 简体中文,运行时即时切换。

## 目录

- [应用截图](#应用截图)
- [功能特性](#功能特性)
- [下载安装](#下载安装)
- [从源码构建](#从源码构建)
- [快速开始](#快速开始)
- [协议支持](#协议支持)
- [项目结构](#项目结构)
- [参与贡献](#参与贡献)
- [更新日志](#更新日志)
- [macOS 首次启动](#macos-首次启动)
- [许可证](#许可证)

## 应用截图

**主站 · 一条 TCP 链路上跑多个公共地址**

一个 IEC 104 主站连接可以同时与多个站(Common Address)对话。在"新建连接"对话框里把公共地址填成 `1, 2, 3`,连接树会自动展开为 **连接 → CA 徽章 → 分类** 三层结构,每个 CA 的分类计数独立统计 —— 不同站共用同一个 IOA 也不会在界面上互相覆盖。

![主站多 CA 树形展示与新建连接对话框](docs/screenshots/master-multi-ca-newconn.png)

**主站 · 含 TLS 握手与多 CA 总召的通信日志**

底部通信日志面板完整记录每一步 TLS 握手、U/I/S 帧、传送原因解码、原始 hex 字节。截图里主站依次发送 **GI CA=1** 和 **GI CA=2**,并接收两个站各自的响应数据流。

![主站通信日志含 TLS 与多 CA 总召](docs/screenshots/master-multi-ca-comm-log.png)

## 功能特性

### 🛰️ 从站 —— `IEC104Slave`

- **IEC 104 服务端**,支持 TCP 和 TLS 连接
- **8 种数据类型** —— 单点、双点、步位置、位串、归一化、标度化、短浮点、累计量
- **数据点管理** —— 支持单个添加或批量添加(IOA 范围 + ASDU 类型选择)
- **随机变位** —— 按可配置间隔模拟数据变化
- **自发传送**(COT=3)—— 数据变化后自动向已连接主站上送
- **周期发送** —— 可配置间隔的周期性数据传送
- **总召唤**(GI)和**累计量召唤**响应
- **控制命令处理** —— 单点、双点、步调节、设定值命令
- **通信日志** —— 支持 hex 帧显示和 CSV 导出
- 创建服务器后自动启动

### 📡 主站 —— `IEC104Master`

- **IEC 104 客户端**,支持 TCP 和 TLS 连接
- **一个连接绑定多个公共地址 (CA)** —— 单条 TCP 链路上同时与多个站对话;连接成功后自动 GI / 时钟同步 / 累计量召唤按 CA 列表逐一发送;接收侧按 CA 分桶存储,不同站的同 IOA 不互相覆盖
- **多 CA 三层连接树** —— 连接 → CA 徽章 → 分类,每个 CA 的分类计数独立;单 CA 连接保持原扁平树
- **实时数据显示** —— 增量轮询 + 虚拟滚动
- **分类树** —— 实时显示各类别点数(单点、双点、步位置、位串、归一化、标度化、浮点、累计量)
- **自定义控制对话框** —— CA 字段下拉选当前连接已配置的 CAs,IOA 任意输;发送成功后窗口保留以便连续发命令;CA / IOA / 命令类型 / 值字段持久化到 localStorage,跨打开和重启都记得
- **控制命令** —— 直接执行和选择-执行(SbO);右键控制命令直接路由到数据点自身的 CA(多 CA 场景下不会发错站)
- **值面板** —— 显示选中数据点详情
- **总召唤**、**累计量召唤**、**时钟同步**命令
- **通信日志** —— 含 TLS 握手事件、U/I/S 帧解码、COT 中文化、原始 hex 字节并排显示;支持 CSV 导出
- **应用内自动更新** —— 从 GitHub Releases 推送(ed25519 签名验证、6 小时检查节流、"稍后" 24 小时不重提)

## 下载安装

各平台预编译安装包均在 **[Releases 页面](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases)**。

| 平台 | 安装包 |
|------|--------|
| Windows | `.msi` / `.exe`(NSIS) |
| macOS   | `.dmg`(Apple Silicon 与 Intel) |
| Linux   | `.AppImage` / `.deb` |

两个应用自 v1.0.9 起均支持从 GitHub Releases **自动更新**。macOS 用户首次启动需要[多做一步](#macos-首次启动)。

### 国内镜像 (China mirror)

中国大陆用户访问 GitHub Releases 可能不稳定,推荐通过镜像直接下载安装包:

- <https://ghfast.top/https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/latest>

应用内更新功能从包含本次改动的发布版本起会自动通过多个反代回退,无需手动处理。但**首次从旧版升级**时,旧版二进制中编译进的 endpoint 仍是 github.com,如果检查更新失败,请按上面镜像链接手动下载新版安装一次,后续更新即可自动通过 proxy。

## 从源码构建

### 环境要求

- [Rust](https://rustup.rs/) 1.77+
- [Node.js](https://nodejs.org/) 18+
- [Tauri CLI](https://tauri.app/) —— `cargo install tauri-cli`

### 步骤

```bash
# 安装前端依赖
cd frontend && npm install
cd ../master-frontend && npm install

# 启动从站
cd crates/iec104sim-app && cargo tauri dev

# 启动主站
cd crates/iec104master-app && cargo tauri dev
```

## 快速开始

四步跑通一次完整往返 —— 用主站驱动仿真从站,全程无需硬件。(截图为中文界面,随时可用 **中 / EN** 切换语言。)

### 1 · 从站 —— 新建服务器并配置数据点

打开 **IEC104Slave**,点击 **新建服务器**:自动绑定 `0.0.0.0:2404` 并启动。添加一个站,再批量添加覆盖全部 8 种监视类型的数据点 —— 单点 / 双点 / 步位置 / 位串 / 归一化 / 标度化 / 短浮点 / 累计量。每个点都带 IOA、值和品质位。

![从站:已启动服务器与数据点](docs/screenshots/tut-1-slave.png)

### 2 · 主站 —— 新建连接

打开 **IEC104Master**,点击 **新建连接**。默认值已指向本地从站:目标地址 `127.0.0.1`、端口 `2404`、公共地址 `1`。单链路对多个站时,把公共地址用逗号分隔填成 `1, 2, 3`;需要加密就勾选 **启用 TLS**。点 **创建**,再点 **连接**。

![新建连接对话框](docs/screenshots/tut-2-master-newconn.png)

### 3 · 主站 —— 总召唤,数据表填满

点击 **总召唤**。从站回送全部数据点;连接树显示各分类计数,表格填满接收到的 IOA、值与品质。

![主站总召唤后的数据表](docs/screenshots/tut-3-master-data.png)

### 4 · 看报文 —— 以及实时变位

展开底部 **通信日志**:每一帧 U/I/S 都被解码 —— 帧类型、传送原因(COT)、可读详情与原始 hex 并排显示。回到从站点击 **随机变化** —— 变化的值以突发(COT=3)自动上送,主站表格与日志实时刷新。

![通信日志:解码后的帧与原始 hex](docs/screenshots/tut-4-master-log.png)

## 协议支持

| 功能 | 支持类型 |
|------|---------|
| 监视方向(从站→主站) | M_SP_NA/TB, M_DP_NA/TB, M_ST_NA/TB, M_BO_NA/TB, M_ME_NA/TD, M_ME_NB/TE, M_ME_NC/TF, M_IT_NA/TB |
| 控制方向(主站→从站) | C_SC_NA, C_DC_NA, C_RC_NA, C_SE_NA/NB/NC |
| 系统命令 | C_IC_NA(总召唤)、C_CI_NA(累计量召唤)、C_CS_NA(时钟同步) |
| 传输原因 | 突发(3)、激活(6)、激活确认(7)、激活终止(10)、总召唤(20)、累计量召唤(37) |
| 传输层 | TCP、TLS(支持双向 TLS) |

## 项目结构

```
IEC104Sim/
├── crates/
│   ├── iec104sim-core/     # IEC 104 协议核心库
│   ├── iec104sim-app/      # 从站 Tauri 应用
│   └── iec104master-app/   # 主站 Tauri 应用
├── frontend/               # 从站 Vue 3 前端
├── master-frontend/        # 主站 Vue 3 前端
└── shared-frontend/        # 共享 Vue 组件、i18n、样式
```

| 层 | 技术栈 |
|----|--------|
| 后端 | Rust、Tokio(异步运行时)、native-tls |
| 前端 | Vue 3、TypeScript、Vite |
| 桌面端 | Tauri 2 |

## 参与贡献

欢迎提交 Issue 与 Pull Request。提交代码改动前,请确保 `cargo test --workspace` 与前端 `npm test` 测试套件全部通过。

## 更新日志

最新变更请参见 [CHANGELOG.md](CHANGELOG.md) 或 [Releases 页面](https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases)。

从 v1.0.9 起,两个应用在启动时自动检测 GitHub Releases,发现新版本会弹窗提示安装。v1.0.8 及更早版本的用户需要手动升级一次。

## macOS 首次启动

应用未做 Apple 公证(Notarization)。首次双击 `.app` 时,macOS 会弹窗 *"未打开 IEC104Slave / IEC104Master —— Apple 无法验证…"*,只提供 *完成* 与 *移到废纸篓* 两个按钮。这是 macOS 15 (Sequoia) 起对 ad-hoc 签名应用的标准拦截,**不是软件损坏**。

<details>
<summary><b>放行步骤(任选其一)</b></summary>

**1. 图形界面**

- 双击 `.app`,出现拦截弹窗,点 *完成*。
- 打开 *系统设置 → 隐私与安全性*,滚到底部。
- 看到 *"已阻止 IEC104Slave 的使用…"*,点 *仍要打开* 并输入密码。
- 弹窗变为 *打开*,点击即可,以后双击直接启动。

**2. 终端一行命令**

```bash
xattr -dr com.apple.quarantine "/Applications/IEC104Slave.app"
xattr -dr com.apple.quarantine "/Applications/IEC104Master.app"
```

清掉隔离标记,macOS 不再拦截。

如果你看到 *"已损坏,无法打开"* 而不是上面的对话框,那是 v1.1.1 及更早完全无签名的旧版,请升级到 v1.1.2 以上(应用内"检查更新"也会推过来),或用上面的 `xattr` 命令清掉隔离属性。

</details>

## 许可证

[MIT](LICENSE)
