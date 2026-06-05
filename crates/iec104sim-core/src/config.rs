//! 配置文件落盘格式 (save/open)。两个应用各自的 JSON 文件 schema,
//! 带 `app` 判别字段防止跨应用误加载。主站连接的 TLS 文件路径与策略一并
//! 写入(不含口令);旧文件缺失这些字段时按「不启用 TLS」处理。

use crate::data_point::{DataPoint, InformationObjectDef};
use crate::slave::{ProtocolTimingConfig, RemoteOperationConfig};
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
    /// 协议时序 (t0/t1/t2/t3/k/w)。旧文件缺失时使用默认值。
    #[serde(default)]
    pub protocol_timing: ProtocolTimingConfig,
    /// 远动运行参数 (应答开关、上送方式、COT 等)。
    #[serde(default)]
    pub remote_ops: RemoteOperationConfig,
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
    // TLS 设置。旧文件缺失这些字段时,serde 默认值等价于「不启用 TLS」,
    // 因此向后兼容;不含任何口令(主站走 PEM 证书/密钥,无 PKCS#12 密码)。
    #[serde(default)]
    pub use_tls: bool,
    #[serde(default)]
    pub ca_file: String,
    #[serde(default)]
    pub cert_file: String,
    #[serde(default)]
    pub key_file: String,
    #[serde(default)]
    pub accept_invalid_certs: bool,
    /// "auto" | "tls12_only" | "tls13_only"。
    #[serde(default = "default_tls_version")]
    pub tls_version: String,
    #[serde(default = "default_broadcast_address")]
    pub broadcast_address: Option<u16>,
    #[serde(default)]
    pub snapshot: Vec<MasterSnapshotPoint>,
}

fn default_broadcast_address() -> Option<u16> { None }
fn default_tls_version() -> String { "auto".to_string() }

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
            protocol_timing: ProtocolTimingConfig::default(),
            remote_ops: RemoteOperationConfig::default(),
        }]);
        let json = file.to_json().unwrap();
        let parsed = SlaveConfigFile::from_json(&json).unwrap();
        assert_eq!(json, parsed.to_json().unwrap());
        assert_eq!(parsed.servers.len(), 1);
        assert_eq!(parsed.servers[0].stations[0].common_address, 1);
    }

    #[test]
    fn slave_file_loads_legacy_without_remote_ops() {
        let legacy = r#"{
            "app": "iec104-slave",
            "version": 1,
            "servers": [
                { "bind_address": "0.0.0.0", "port": 2404,
                  "stations": [{ "common_address": 1, "name": "站1", "object_defs": [] }]
                }
            ]
        }"#;
        let parsed = SlaveConfigFile::from_json(legacy).unwrap();
        let s = &parsed.servers[0];
        assert_eq!(s.protocol_timing.t0, 30);
        assert!(s.remote_ops.answer_general_interrogation);
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
            use_tls: true,
            ca_file: "/etc/ca.pem".to_string(),
            cert_file: "/etc/client.pem".to_string(),
            key_file: "/etc/client-key.pem".to_string(),
            accept_invalid_certs: true,
            tls_version: "tls13_only".to_string(),
            broadcast_address: None,
            snapshot: vec![MasterSnapshotPoint { ca: 1, point }],
        }]);
        let json = file.to_json().unwrap();
        let parsed = MasterConfigFile::from_json(&json).unwrap();
        assert_eq!(json, parsed.to_json().unwrap());
        assert_eq!(parsed.connections[0].snapshot[0].ca, 1);
        assert_eq!(parsed.connections[0].snapshot[0].point.ioa, 100);
        // TLS settings must survive the round trip (regression: previously dropped).
        let c = &parsed.connections[0];
        assert!(c.use_tls);
        assert_eq!(c.ca_file, "/etc/ca.pem");
        assert_eq!(c.cert_file, "/etc/client.pem");
        assert_eq!(c.key_file, "/etc/client-key.pem");
        assert!(c.accept_invalid_certs);
        assert_eq!(c.tls_version, "tls13_only");
    }

    #[test]
    fn master_file_loads_legacy_without_tls() {
        // Files written before TLS was added to the schema must still load,
        // defaulting to TLS disabled.
        let legacy = r#"{
            "app": "iec104-master",
            "version": 1,
            "connections": [
                { "target_address": "10.0.0.1", "port": 2404, "common_addresses": [1],
                  "timeout_ms": 3000, "t0": 30, "t1": 15, "t2": 10, "t3": 20,
                  "k": 12, "w": 8, "default_qoi": 20, "default_qcc": 5,
                  "interrogate_period_s": 0, "counter_interrogate_period_s": 0 }
            ]
        }"#;
        let parsed = MasterConfigFile::from_json(legacy).unwrap();
        let c = &parsed.connections[0];
        assert!(!c.use_tls);
        assert!(c.ca_file.is_empty());
        assert_eq!(c.tls_version, "auto");
    }

    #[test]
    fn master_from_json_rejects_wrong_app() {
        let json = r#"{"app":"iec104-slave","version":1,"connections":[]}"#;
        let err = MasterConfigFile::from_json(json).unwrap_err();
        assert!(err.contains("类型不匹配"), "err was: {err}");
    }
}
