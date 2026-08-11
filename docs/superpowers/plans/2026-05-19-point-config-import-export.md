# 点位配置导入/导出 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为主站、从站两个 Tauri 应用增加手动保存/打开点位配置文件(JSON)的功能。

**Architecture:** 在核心 crate `iec104sim-core` 的 `config.rs` 中定义文件 schema 结构体(含 `from_json`/`to_json` 校验逻辑);两个应用各新增 `save_config` / `load_config` 两个 Tauri 命令(文件读写在 Rust 内完成);前端通过 `tauri-plugin-dialog` 选取路径,Toolbar 新增两个按钮调用命令。配置文件是完整工作区快照:`load_config` 先完整解析和构建候选状态,成功后一次性替换当前工作区;不提供 Merge/追加模式,TLS 配置不写入文件。

**Tech Stack:** Rust / Tauri 2 / `serde_json` / Vue 3 / `tauri-plugin-dialog`

参考设计文档:`docs/superpowers/specs/2026-05-19-point-config-import-export-design.md`

---

## File Structure

- `crates/iec104sim-core/src/config.rs` — **重写**。当前内容(`SlaveServerConfig`/`StationConfig`/`MasterConnectionConfig`/`PersistedAppState`/`PersistedMasterState`)是无任何引用的死代码,整体替换为新的文件 schema 结构体 + 单元测试。
- `crates/iec104sim-app/src/commands.rs` — 移除死代码 `export_app_state`/`import_app_state`/`clear_app_state` 及 `PersistedServer`/`PersistedStation`/`PersistedAppState`,新增 `save_config`/`load_config`。
- `crates/iec104sim-app/src/lib.rs` — 更新 `invoke_handler` 与 plugin 注册。
- `crates/iec104master-app/src/commands.rs` — 新增 `save_config`/`load_config`。
- `crates/iec104master-app/src/lib.rs` — 更新 `invoke_handler` 与 plugin 注册。
- 两个 app 的 `Cargo.toml`、`capabilities/default.json` — 增加 `tauri-plugin-dialog`。
- 两个 frontend 的 `package.json`、`Toolbar.vue`、`i18n/locales/{zh-CN,en-US}.ts` — 增加按钮与文案。

---

## Task 1: 核心文件 schema 结构体

**Files:**
- Rewrite: `crates/iec104sim-core/src/config.rs`

- [ ] **Step 1: 重写 config.rs,写入结构体与校验逻辑**

完整替换 `crates/iec104sim-core/src/config.rs` 为:

```rust
//! 配置文件落盘格式 (save/open)。两个应用各自的 JSON 文件 schema,
//! 带 `app` 判别字段防止跨应用误加载。TLS 不写入文件。

use crate::data_point::{DataPoint, InformationObjectDef};
use serde::{Deserialize, Serialize};

pub const SLAVE_CONFIG_APP: &str = "iec104-slave";
pub const MASTER_CONFIG_APP: &str = "iec104-master";
pub const CONFIG_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// 从站文件 schema
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaveStationConfig {
    pub common_address: u16,
    pub name: String,
    pub object_defs: Vec<InformationObjectDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaveServerConfig {
    pub bind_address: String,
    pub port: u16,
    pub stations: Vec<SlaveStationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaveConfigFile {
    pub app: String,
    pub version: u32,
    pub servers: Vec<SlaveServerConfig>,
}

impl SlaveConfigFile {
    pub fn new(servers: Vec<SlaveServerConfig>) -> Self {
        Self { app: SLAVE_CONFIG_APP.to_string(), version: CONFIG_VERSION, servers }
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| format!("序列化失败: {e}"))
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        let f: SlaveConfigFile =
            serde_json::from_str(s).map_err(|e| format!("配置文件解析失败: {e}"))?;
        if f.app != SLAVE_CONFIG_APP {
            return Err(format!(
                "配置文件类型不匹配:期望从站配置,实际为 \"{}\"",
                f.app
            ));
        }
        if f.version != CONFIG_VERSION {
            return Err(format!("不支持的配置文件版本: {}", f.version));
        }
        Ok(f)
    }
}

// ---------------------------------------------------------------------------
// 主站文件 schema
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterSnapshotPoint {
    pub ca: u16,
    pub point: DataPoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterConnectionConfig {
    pub target_address: String,
    pub port: u16,
    pub common_addresses: Vec<u16>,
    pub timeout_ms: u64,
    pub t0: u32,
    pub t1: u32,
    pub t2: u32,
    pub t3: u32,
    pub k: u16,
    pub w: u16,
    pub default_qoi: u8,
    pub default_qcc: u8,
    pub interrogate_period_s: u32,
    pub counter_interrogate_period_s: u32,
    #[serde(default)]
    pub snapshot: Vec<MasterSnapshotPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterConfigFile {
    pub app: String,
    pub version: u32,
    pub connections: Vec<MasterConnectionConfig>,
}

impl MasterConfigFile {
    pub fn new(connections: Vec<MasterConnectionConfig>) -> Self {
        Self { app: MASTER_CONFIG_APP.to_string(), version: CONFIG_VERSION, connections }
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| format!("序列化失败: {e}"))
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        let f: MasterConfigFile =
            serde_json::from_str(s).map_err(|e| format!("配置文件解析失败: {e}"))?;
        if f.app != MASTER_CONFIG_APP {
            return Err(format!(
                "配置文件类型不匹配:期望主站配置,实际为 \"{}\"",
                f.app
            ));
        }
        if f.version != CONFIG_VERSION {
            return Err(format!("不支持的配置文件版本: {}", f.version));
        }
        Ok(f)
    }
}
```

- [ ] **Step 2: 在 config.rs 末尾追加单元测试**

在上述内容末尾追加:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_point::DataPoint;
    use crate::types::AsduTypeId;

    #[test]
    fn slave_file_round_trip() {
        let file = SlaveConfigFile::new(vec![SlaveServerConfig {
            bind_address: "0.0.0.0".to_string(),
            port: 2404,
            stations: vec![SlaveStationConfig {
                common_address: 1,
                name: "站1".to_string(),
                object_defs: vec![],
            }],
        }]);
        let json = file.to_json().unwrap();
        let parsed = SlaveConfigFile::from_json(&json).unwrap();
        // 二次序列化字符串相等即视为往返保真
        assert_eq!(json, parsed.to_json().unwrap());
        assert_eq!(parsed.servers.len(), 1);
        assert_eq!(parsed.servers[0].stations[0].common_address, 1);
    }

    #[test]
    fn slave_from_json_rejects_wrong_app() {
        let json = r#"{"app":"iec104-master","version":1,"servers":[]}"#;
        let err = SlaveConfigFile::from_json(json).unwrap_err();
        assert!(err.contains("类型不匹配"), "err was: {err}");
    }

    #[test]
    fn slave_from_json_rejects_bad_version() {
        let json = r#"{"app":"iec104-slave","version":999,"servers":[]}"#;
        let err = SlaveConfigFile::from_json(json).unwrap_err();
        assert!(err.contains("版本"), "err was: {err}");
    }

    #[test]
    fn slave_from_json_rejects_corrupt() {
        let err = SlaveConfigFile::from_json("not json").unwrap_err();
        assert!(err.contains("解析失败"), "err was: {err}");
    }

    #[test]
    fn master_file_round_trip_with_snapshot() {
        let point = DataPoint::new(100, AsduTypeId::MSpNa1);
        let file = MasterConfigFile::new(vec![MasterConnectionConfig {
            target_address: "127.0.0.1".to_string(),
            port: 2404,
            common_addresses: vec![1, 2],
            timeout_ms: 3000,
            t0: 30, t1: 15, t2: 10, t3: 20, k: 12, w: 8,
            default_qoi: 20, default_qcc: 5,
            interrogate_period_s: 0,
            counter_interrogate_period_s: 0,
            snapshot: vec![MasterSnapshotPoint { ca: 1, point }],
        }]);
        let json = file.to_json().unwrap();
        let parsed = MasterConfigFile::from_json(&json).unwrap();
        assert_eq!(json, parsed.to_json().unwrap());
        assert_eq!(parsed.connections[0].snapshot[0].ca, 1);
        assert_eq!(parsed.connections[0].snapshot[0].point.ioa, 100);
    }

    #[test]
    fn master_from_json_rejects_wrong_app() {
        let json = r#"{"app":"iec104-slave","version":1,"connections":[]}"#;
        let err = MasterConfigFile::from_json(json).unwrap_err();
        assert!(err.contains("类型不匹配"), "err was: {err}");
    }
}
```

注意:`AsduTypeId::MSpNa1` 是单点遥信类型的枚举名 —— 执行前用 `grep -n "M.*=.*1," crates/iec104sim-core/src/types.rs` 或查看 `pub enum AsduTypeId` 确认实际变体名(如 `MSpNa1` / `M_SP_NA_1`),用真实存在的变体替换。

- [ ] **Step 3: 运行测试验证通过**

Run: `cargo test -p iec104sim-core config::tests`
Expected: 6 个测试全部 PASS。

- [ ] **Step 4: Commit**

```bash
git add crates/iec104sim-core/src/config.rs
git commit -m "feat(core): 配置文件 schema 结构体与校验"
```

---

## Task 2: 从站后端 save_config / load_config

**Files:**
- Modify: `crates/iec104sim-app/src/commands.rs`(移除死代码 + 新增命令)
- Modify: `crates/iec104sim-app/src/lib.rs`

- [ ] **Step 1: 移除 commands.rs 中的死代码**

删除 `crates/iec104sim-app/src/commands.rs` 中以下整段(标题注释 `State Persistence Commands` 之下):`PersistedServer`、`PersistedStation`、`PersistedAppState` 三个结构体定义,以及 `export_app_state`、`import_app_state`、`clear_app_state` 三个 `#[tauri::command]` 函数。保留该区块的标题注释横线。

- [ ] **Step 2: 在 commands.rs 同一位置新增 save_config / load_config**

在 `State Persistence Commands` 标题注释之下写入:

```rust
#[tauri::command]
pub async fn save_config(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    use iec104sim_core::config::{SlaveConfigFile, SlaveServerConfig, SlaveStationConfig};

    let servers = state.servers.read().await;
    let mut out = Vec::new();
    for (_id, srv_state) in servers.iter() {
        let stations = srv_state.server.stations.read().await;
        let mut st = Vec::new();
        for (_ca, station) in stations.iter() {
            st.push(SlaveStationConfig {
                common_address: station.common_address,
                name: station.name.clone(),
                object_defs: station.object_defs.clone(),
            });
        }
        out.push(SlaveServerConfig {
            bind_address: srv_state.server.transport.bind_address.clone(),
            port: srv_state.server.transport.port,
            stations: st,
        });
    }
    let json = SlaveConfigFile::new(out).to_json()?;
    std::fs::write(&path, json).map_err(|e| format!("写入文件失败: {e}"))
}

#[tauri::command]
pub async fn load_config(
    state: State<'_, AppState>,
    path: String,
) -> Result<usize, String> {
    use iec104sim_core::config::SlaveConfigFile;

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取文件失败: {e}"))?;
    let file = SlaveConfigFile::from_json(&content)?;

    let mut imported = 0usize;
    for srv in file.servers {
        let id = {
            let mut counter = state.next_server_id.write().await;
            let id = format!("server_{}", *counter);
            *counter += 1;
            id
        };
        let transport = SlaveTransportConfig {
            bind_address: srv.bind_address,
            port: srv.port,
            tls: Default::default(),
        };
        let log_collector = Arc::new(LogCollector::new());
        let server = SlaveServer::new(transport).with_log_collector(log_collector.clone());
        for st in srv.stations {
            let mut station = Station::new(st.common_address, st.name);
            for def in st.object_defs {
                let _ = station.add_point(def);
            }
            let _ = server.add_station(station).await;
        }
        state.servers.write().await.insert(
            id,
            SlaveServerState { server, log_collector },
        );
        imported += 1;
    }
    Ok(imported)
}
```

- [ ] **Step 3: 更新 lib.rs 的 invoke_handler**

在 `crates/iec104sim-app/src/lib.rs` 的 `invoke_handler` 中,把这三行:

```rust
            // State persistence commands
            commands::export_app_state,
            commands::import_app_state,
            commands::clear_app_state,
```

替换为:

```rust
            // Config file save/open
            commands::save_config,
            commands::load_config,
```

- [ ] **Step 4: 编译验证**

Run: `cargo build -p iec104sim-app`
Expected: 编译成功,无 `export_app_state` 等未定义引用错误。

- [ ] **Step 5: Commit**

```bash
git add crates/iec104sim-app/src/commands.rs crates/iec104sim-app/src/lib.rs
git commit -m "feat(slave): save_config/load_config 命令"
```

---

## Task 3: 主站后端 save_config / load_config

**Files:**
- Modify: `crates/iec104master-app/src/commands.rs`
- Modify: `crates/iec104master-app/src/lib.rs`

- [ ] **Step 1: 在 commands.rs 末尾新增 save_config / load_config**

在 `crates/iec104master-app/src/commands.rs` 文件末尾追加:

```rust
// ---------------------------------------------------------------------------
// Config file save/open
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn save_config(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    use iec104sim_core::config::{MasterConfigFile, MasterConnectionConfig, MasterSnapshotPoint};

    let connections = state.connections.read().await;
    let mut out = Vec::new();
    for (_id, cs) in connections.iter() {
        let cfg = &cs.connection.config;
        let data = cs.connection.received_data.read().await;
        let snapshot: Vec<MasterSnapshotPoint> = data
            .all_sorted()
            .into_iter()
            .map(|(ca, p)| MasterSnapshotPoint { ca, point: p.clone() })
            .collect();
        out.push(MasterConnectionConfig {
            target_address: cfg.target_address.clone(),
            port: cfg.port,
            common_addresses: cs.common_addresses.clone(),
            timeout_ms: cfg.timeout_ms,
            t0: cfg.t0,
            t1: cfg.t1,
            t2: cfg.t2,
            t3: cfg.t3,
            k: cfg.k,
            w: cfg.w,
            default_qoi: cfg.default_qoi,
            default_qcc: cfg.default_qcc,
            interrogate_period_s: cfg.interrogate_period_s,
            counter_interrogate_period_s: cfg.counter_interrogate_period_s,
            snapshot,
        });
    }
    let json = MasterConfigFile::new(out).to_json()?;
    std::fs::write(&path, json).map_err(|e| format!("写入文件失败: {e}"))
}

#[tauri::command]
pub async fn load_config(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    path: String,
) -> Result<usize, String> {
    use iec104sim_core::config::MasterConfigFile;

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取文件失败: {e}"))?;
    let file = MasterConfigFile::from_json(&content)?;

    let mut imported = 0usize;
    for conn in file.connections {
        let request = CreateConnectionRequest {
            target_address: conn.target_address,
            port: conn.port,
            common_addresses: Some(conn.common_addresses),
            common_address: None,
            timeout_ms: Some(conn.timeout_ms),
            use_tls: None,
            ca_file: None,
            cert_file: None,
            key_file: None,
            accept_invalid_certs: None,
            tls_version: None,
            t0: Some(conn.t0),
            t1: Some(conn.t1),
            t2: Some(conn.t2),
            t3: Some(conn.t3),
            k: Some(conn.k),
            w: Some(conn.w),
            default_qoi: Some(conn.default_qoi),
            default_qcc: Some(conn.default_qcc),
            interrogate_period_s: Some(conn.interrogate_period_s),
            counter_interrogate_period_s: Some(conn.counter_interrogate_period_s),
        };
        let info = create_connection(state.clone(), app_handle.clone(), request).await?;

        // 把快照点位预填入新连接的 received_data,使 DataTable 立即可见。
        if !conn.snapshot.is_empty() {
            let connections = state.connections.read().await;
            if let Some(cs) = connections.get(&info.id) {
                let mut data = cs.connection.received_data.write().await;
                for sp in conn.snapshot {
                    data.insert(sp.ca, sp.point);
                }
            }
        }
        imported += 1;
    }
    Ok(imported)
}
```

说明:`State<'_, AppState>` 在 Tauri 2 中是 `Copy`,`state.clone()` 与后续 `state.connections` 复用都成立;若编译器报 `State` 未实现 `Clone`,改为直接传 `state`(`create_connection(state, ...)`)即可,因为 `State` 是 `Copy`。

- [ ] **Step 2: 更新 lib.rs 的 invoke_handler 与导入**

在 `crates/iec104master-app/src/lib.rs` 的 `invoke_handler` 中,`// Tool commands` 之前插入:

```rust
            // Config file save/open
            commands::save_config,
            commands::load_config,
```

- [ ] **Step 3: 编译验证**

Run: `cargo build -p iec104master-app`
Expected: 编译成功。

- [ ] **Step 4: Commit**

```bash
git add crates/iec104master-app/src/commands.rs crates/iec104master-app/src/lib.rs
git commit -m "feat(master): save_config/load_config 命令"
```

---

## Task 4: 接入 tauri-plugin-dialog

**Files:**
- Modify: `crates/iec104sim-app/Cargo.toml`、`crates/iec104master-app/Cargo.toml`
- Modify: `crates/iec104sim-app/src/lib.rs`、`crates/iec104master-app/src/lib.rs`
- Modify: `crates/iec104sim-app/capabilities/default.json`、`crates/iec104master-app/capabilities/default.json`
- Modify: `frontend/package.json`、`master-frontend/package.json`

- [ ] **Step 1: 两个 Cargo.toml 增加依赖**

在 `crates/iec104sim-app/Cargo.toml` 和 `crates/iec104master-app/Cargo.toml` 的 `[dependencies]` 末尾各加一行:

```toml
tauri-plugin-dialog = "2"
```

- [ ] **Step 2: 两个 lib.rs 注册插件**

在 `crates/iec104sim-app/src/lib.rs` 和 `crates/iec104master-app/src/lib.rs` 的 `tauri::Builder::default()` 之后、其它 `.plugin(...)` 旁边各加一行:

```rust
        .plugin(tauri_plugin_dialog::init())
```

- [ ] **Step 3: 两个 capabilities 增加权限**

把 `crates/iec104sim-app/capabilities/default.json` 和 `crates/iec104master-app/capabilities/default.json` 的 `permissions` 数组改为:

```json
  "permissions": [
    "core:default",
    "dialog:allow-open",
    "dialog:allow-save"
  ]
```

- [ ] **Step 4: 两个 frontend 增加 npm 依赖**

分别在 `frontend/` 和 `master-frontend/` 目录执行:

```bash
cd frontend && npm install @tauri-apps/plugin-dialog@^2 && cd ..
cd master-frontend && npm install @tauri-apps/plugin-dialog@^2 && cd ..
```

- [ ] **Step 5: 编译验证**

Run: `cargo build -p iec104sim-app -p iec104master-app`
Expected: 编译成功(`tauri_plugin_dialog` 解析正常)。

- [ ] **Step 6: Commit**

```bash
git add crates/iec104sim-app/Cargo.toml crates/iec104master-app/Cargo.toml \
        crates/iec104sim-app/src/lib.rs crates/iec104master-app/src/lib.rs \
        crates/iec104sim-app/capabilities/default.json crates/iec104master-app/capabilities/default.json \
        frontend/package.json frontend/package-lock.json \
        master-frontend/package.json master-frontend/package-lock.json Cargo.lock
git commit -m "chore: 接入 tauri-plugin-dialog"
```

---

## Task 5: 从站前端 — Toolbar 按钮与文案

**Files:**
- Modify: `frontend/src/components/Toolbar.vue`
- Modify: `frontend/src/i18n/locales/zh-CN.ts`、`frontend/src/i18n/locales/en-US.ts`

- [ ] **Step 1: i18n — zh-CN.ts 增加 type 与值**

在 `frontend/src/i18n/locales/zh-CN.ts` 的 `toolbar` **类型定义块**(`parseFrameInLog: string` 之后)加入两行:

```ts
    saveConfig: string
    openConfig: string
```

在同文件 `toolbar` **值对象块**(`parseFrameInLog: '解析此报文',` 之后)加入:

```ts
    saveConfig: '保存配置',
    openConfig: '打开配置',
```

并在文件中找到承载提示文案的区块(如已有的 `errors` 或顶层),新增以下键值(若没有合适区块,加到 `toolbar` 值对象内即可,key 用 `configSaved` / `configLoaded` / `configSaveFailed` / `configLoadFailed`,并同步加入类型块):

```ts
    configSaved: '配置已保存',
    configLoaded: '已导入 {count} 个服务器',
    configSaveFailed: '保存失败',
    configLoadFailed: '打开失败',
```

- [ ] **Step 2: i18n — en-US.ts 增加对应值**

在 `frontend/src/i18n/locales/en-US.ts` 的 `toolbar` 值对象对应位置加入:

```ts
    saveConfig: 'Save Config',
    openConfig: 'Open Config',
    configSaved: 'Configuration saved',
    configLoaded: 'Imported {count} server(s)',
    configSaveFailed: 'Save failed',
    configLoadFailed: 'Open failed',
```

- [ ] **Step 3: Toolbar.vue — script 增加处理函数**

在 `frontend/src/components/Toolbar.vue` 的 `<script setup>` 中,`import { invoke } from '@tauri-apps/api/core'` 之后加一行:

```ts
import { save, open } from '@tauri-apps/plugin-dialog'
```

在 `addStation` 函数之后加入:

```ts
async function saveConfig() {
  const path = await save({
    filters: [{ name: 'IEC104 Config', extensions: ['json'] }],
    defaultPath: 'iec104-slave-config.json',
  })
  if (!path) return
  try {
    await invoke('save_config', { path })
    await showAlert(t('toolbar.configSaved'))
  } catch (e) {
    await showAlert(`${t('toolbar.configSaveFailed')}: ${e}`)
  }
}

async function openConfig() {
  const path = await open({
    multiple: false,
    filters: [{ name: 'IEC104 Config', extensions: ['json'] }],
  })
  if (!path || typeof path !== 'string') return
  try {
    const count = await invoke<number>('load_config', { path })
    refreshTree()
    await showAlert(t('toolbar.configLoaded', { count }))
  } catch (e) {
    await showAlert(`${t('toolbar.configLoadFailed')}: ${e}`)
  }
}
```

- [ ] **Step 4: Toolbar.vue — template 增加按钮**

在 `frontend/src/components/Toolbar.vue` 的 `<template>` 中,「报文解析」按钮所在 `toolbar-group` 之后、`toolbar-btn-update` 按钮之前,插入:

```html
    <div class="toolbar-divider"></div>
    <div class="toolbar-group">
      <button class="toolbar-btn" @click="saveConfig" :title="t('toolbar.saveConfig')">
        <span class="toolbar-label">{{ t('toolbar.saveConfig') }}</span>
      </button>
      <button class="toolbar-btn" @click="openConfig" :title="t('toolbar.openConfig')">
        <span class="toolbar-label">{{ t('toolbar.openConfig') }}</span>
      </button>
    </div>
```

- [ ] **Step 5: typecheck 与 i18n 测试**

Run: `cd frontend && npm run build && npm test`
Expected: `vue-tsc` 通过;`i18n.spec.ts` 通过(zh-CN/en-US key 一致)。

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/Toolbar.vue frontend/src/i18n/locales/zh-CN.ts frontend/src/i18n/locales/en-US.ts
git commit -m "feat(slave-ui): Toolbar 保存/打开配置按钮"
```

---

## Task 6: 主站前端 — Toolbar 按钮与文案

**Files:**
- Modify: `master-frontend/src/components/Toolbar.vue`
- Modify: `master-frontend/src/i18n/locales/zh-CN.ts`、`master-frontend/src/i18n/locales/en-US.ts`

- [ ] **Step 1: i18n — zh-CN.ts 增加 type 与值**

在 `master-frontend/src/i18n/locales/zh-CN.ts` 的 `toolbar` **类型定义块**(`about: string` 之后)加入:

```ts
    saveConfig: string
    openConfig: string
    configSaved: string
    configLoaded: string
    configSaveFailed: string
    configLoadFailed: string
```

在 `toolbar` **值对象块**(`about: '关于',` 之后)加入:

```ts
    saveConfig: '保存配置',
    openConfig: '打开配置',
    configSaved: '配置已保存',
    configLoaded: '已导入 {count} 个连接',
    configSaveFailed: '保存失败',
    configLoadFailed: '打开失败',
```

- [ ] **Step 2: i18n — en-US.ts 增加对应值**

在 `master-frontend/src/i18n/locales/en-US.ts` 的 `toolbar` 值对象对应位置加入:

```ts
    saveConfig: 'Save Config',
    openConfig: 'Open Config',
    configSaved: 'Configuration saved',
    configLoaded: 'Imported {count} connection(s)',
    configSaveFailed: 'Save failed',
    configLoadFailed: 'Open failed',
```

- [ ] **Step 3: Toolbar.vue — script 增加处理函数**

在 `master-frontend/src/components/Toolbar.vue` 的 `<script setup>` 中,`import { invoke } from '@tauri-apps/api/core'` 之后加一行:

```ts
import { save, open } from '@tauri-apps/plugin-dialog'
```

在 `connectMaster` 函数之前(或 `manualCheckUpdate` 之后)加入:

```ts
async function saveConfig() {
  const path = await save({
    filters: [{ name: 'IEC104 Config', extensions: ['json'] }],
    defaultPath: 'iec104-master-config.json',
  })
  if (!path) return
  try {
    await invoke('save_config', { path })
    await showAlert(t('toolbar.configSaved'))
  } catch (e) {
    await showAlert(`${t('toolbar.configSaveFailed')}: ${e}`)
  }
}

async function openConfig() {
  const path = await open({
    multiple: false,
    filters: [{ name: 'IEC104 Config', extensions: ['json'] }],
  })
  if (!path || typeof path !== 'string') return
  try {
    const count = await invoke<number>('load_config', { path })
    refreshTree()
    refreshData()
    await showAlert(t('toolbar.configLoaded', { count }))
  } catch (e) {
    await showAlert(`${t('toolbar.configLoadFailed')}: ${e}`)
  }
}
```

- [ ] **Step 4: Toolbar.vue — template 增加按钮**

在 `master-frontend/src/components/Toolbar.vue` 的 `<template>` 中,「报文解析」按钮所在 `toolbar-group` 之后、`<div class="toolbar-spacer"></div>` 之前,插入:

```html
    <div class="toolbar-divider"></div>

    <div class="toolbar-group">
      <button class="toolbar-btn" @click="saveConfig">
        {{ t('toolbar.saveConfig') }}
      </button>
      <button class="toolbar-btn" @click="openConfig">
        {{ t('toolbar.openConfig') }}
      </button>
    </div>
```

- [ ] **Step 5: typecheck 与 i18n 测试**

Run: `cd master-frontend && npm run build && npm test`
Expected: `vue-tsc` 通过;`i18n.spec.ts` 通过。

- [ ] **Step 6: Commit**

```bash
git add master-frontend/src/components/Toolbar.vue master-frontend/src/i18n/locales/zh-CN.ts master-frontend/src/i18n/locales/en-US.ts
git commit -m "feat(master-ui): Toolbar 保存/打开配置按钮"
```

---

## Task 7: 端到端手动验证

- [ ] **Step 1: 从站冒烟测试**

Run: `cd frontend && npm run tauri dev`(或项目既有的开发启动方式)
操作:先创建一份与 `a.json` 不同的现有工作区 → 点「打开配置」选 `a.json` → 确认界面仅显示 `a.json` 中的服务器/站/点位,且旧树选择、详情和日志已清空;再打开同一文件,确认数量不变且没有重复项。

- [ ] **Step 2: 主站冒烟测试**

启动主站应用 → 新建连接、连接到一个运行中的从站、总召唤收到点位 → 点「保存配置」存为 `b.json` → 重启 → 「打开配置」选 `b.json` → 确认连接配置恢复且 DataTable 立即显示快照点位(断开态)。

- [ ] **Step 3: 错误路径验证**

在主站应用里「打开配置」选择从站的 `a.json` → 确认弹窗提示"配置文件类型不匹配";手动改坏一个 json 文件再打开 → 确认弹窗提示解析失败。

- [ ] **Step 4: 运行全部自动化测试**

Run: `cargo test && cd frontend && npm test && cd ../master-frontend && npm test`
Expected: 全部 PASS。

---

## 备注

- 全程不要在 `git commit` 中加入 Claude 署名(用户全局规则)。
- 若 `cargo test` 中已有的更新相关测试因新插件失败,检查 `lib.rs` 的 plugin 注册顺序,`tauri_plugin_dialog::init()` 与其它插件并列即可,无顺序要求。
