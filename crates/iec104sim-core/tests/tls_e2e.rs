// 本文件的验证依赖 tcpdump/tshark 抓包与 SIGTERM(libc::kill),均为 unix 工具链;
// Windows 上没有等价物,整个测试二进制在 Windows 编译为空。
#![cfg(unix)]

use iec104sim_core::data_point::DataPointValue;
use iec104sim_core::master::{MasterConfig, MasterConnection, TlsConfig};
use iec104sim_core::slave::{
    RemoteOperationConfig, SlaveError, SlaveServer, SlaveTlsConfig, SlaveTransportConfig,
    Station,
};
use iec104sim_core::types::AsduTypeId;
use std::process::Command;
use tokio::time::{sleep, Duration};

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

async fn start_slave_on_free_port(
    tls: &SlaveTlsConfig,
    station_name: &str,
    point_count: u32,
) -> (u16, SlaveServer) {
    // free_port() releases its probe listener before SlaveServer binds, so a
    // parallel test can legitimately claim the port in between those calls.
    const MAX_BIND_ATTEMPTS: usize = 10;

    for attempt in 1..=MAX_BIND_ATTEMPTS {
        let port = free_port();
        let transport = SlaveTransportConfig {
            bind_address: "127.0.0.1".to_string(),
            port,
            tls: tls.clone(),
        };
        let mut slave = SlaveServer::new(transport);
        slave
            .add_station(Station::with_default_points(1, station_name, point_count))
            .await
            .unwrap();

        match slave.start().await {
            Ok(()) => return (port, slave),
            Err(SlaveError::BindFailed {
                code: "addr_in_use",
                ..
            }) if attempt < MAX_BIND_ATTEMPTS => continue,
            Err(error) => panic!("failed to start test slave on attempt {attempt}: {error:?}"),
        }
    }

    unreachable!("the final bind attempt returns or panics")
}

// native-tls on macOS imports client PKCS#12 identities through one process-
// global temporary keychain. Parallel mTLS tests can otherwise race inside
// Security.framework and make one import return no identity.
static MTLS_CLIENT_IDENTITY_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// =========================================================================
// Tool availability check
// =========================================================================

fn check_tools_available() -> bool {
    let tcpdump_ok = Command::new("tcpdump").arg("--version").output().is_ok();
    let tshark_ok = Command::new("tshark").arg("--version").output().is_ok();
    if !tcpdump_ok {
        eprintln!("SKIP: tcpdump not found in PATH");
    }
    if !tshark_ok {
        eprintln!("SKIP: tshark not found in PATH. Install with: brew install wireshark");
    }
    tcpdump_ok && tshark_ok
}

// =========================================================================
// Module: cert_gen — Dynamic certificate generation with rcgen
// =========================================================================

mod common;
use common::cert_gen;

// =========================================================================
// Verification test: cert generation
// =========================================================================

#[test]
fn test_cert_generation() {
    let certs = cert_gen::generate();
    assert!(certs.ca_cert_pem.contains("BEGIN CERTIFICATE"));
    assert!(certs.server_cert_pem.contains("BEGIN CERTIFICATE"));
    assert!(certs.server_key_pem.contains("BEGIN PRIVATE KEY"));
    assert!(certs.client_cert_pem.contains("BEGIN CERTIFICATE"));
    assert!(certs.client_key_pem.contains("BEGIN PRIVATE KEY"));

    let tmp = tempfile::tempdir().unwrap();
    let paths = cert_gen::write_to_dir(&certs, tmp.path());
    assert!(paths.ca_cert.exists());
    assert!(paths.server_cert.exists());
    assert!(paths.server_key.exists());
    assert!(paths.server_pkcs12.exists());
    assert!(paths.client_cert.exists());
    assert!(paths.client_key.exists());
    assert!(paths.client_pkcs12.exists());
}

// =========================================================================
// Module: capture — Packet capture with tcpdump + analysis with tshark
// =========================================================================

mod capture {
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};

    pub struct PacketCapture {
        child: Child,
        pub pcap_path: PathBuf,
    }

    /// Start tcpdump capturing on loopback for the given port.
    pub fn start(test_name: &str, port: u16) -> Result<PacketCapture, String> {
        let pcap_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/pcap");
        std::fs::create_dir_all(&pcap_dir).map_err(|e| format!("create pcap dir: {}", e))?;

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let pcap_path = pcap_dir.join(format!("{}_{}.pcap", test_name, timestamp));

        let child = Command::new("tcpdump")
            .args([
                "-i", "lo0",
                "-w", pcap_path.to_str().unwrap(),
                "-s", "0",
                &format!("port {}", port),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("spawn tcpdump: {} (need BPF permissions — try: brew install --cask wireshark)", e))?;

        Ok(PacketCapture { child, pcap_path })
    }

    impl PacketCapture {
        /// Stop capturing. Sends SIGTERM and waits for tcpdump to flush.
        pub fn stop(&mut self) -> Result<(), String> {
            unsafe {
                libc::kill(self.child.id() as i32, libc::SIGTERM);
            }
            self.child.wait().map_err(|e| format!("wait tcpdump: {}", e))?;
            std::thread::sleep(std::time::Duration::from_millis(200));
            Ok(())
        }
    }

    /// Assert that the pcap contains a valid TLS session:
    /// 1. TLS handshake present (ClientHello + ServerHello)
    /// 2. No plaintext IEC 104 frames visible
    /// 3. Encrypted application data present
    pub fn assert_tls_encrypted(pcap_path: &Path, port: u16) {
        let pcap = pcap_path.to_str().unwrap();

        // 1. Check TLS handshake exists
        let output = Command::new("tshark")
            .args(["-r", pcap, "-Y", "tls.handshake", "-T", "fields", "-e", "tls.handshake.type"])
            .output()
            .expect("failed to run tshark");
        let handshake_types = String::from_utf8_lossy(&output.stdout);
        assert!(
            handshake_types.contains("1"),
            "No ClientHello found in pcap: {}\ntshark output: {}",
            pcap, handshake_types
        );
        assert!(
            handshake_types.contains("2"),
            "No ServerHello found in pcap: {}\ntshark output: {}",
            pcap, handshake_types
        );

        // 2. Check no plaintext IEC 104 is visible
        let output = Command::new("tshark")
            .args(["-r", pcap, "-Y", "iec60870_104", "-T", "fields", "-e", "frame.number"])
            .output()
            .expect("failed to run tshark");
        let iec104_frames = String::from_utf8_lossy(&output.stdout).trim().to_string();
        assert!(
            iec104_frames.is_empty(),
            "Plaintext IEC 104 frames leaked through TLS! pcap: {}\nFrame numbers: {}",
            pcap, iec104_frames
        );

        // 3. Check encrypted application data exists
        let output = Command::new("tshark")
            .args([
                "-r", pcap,
                "-Y", &format!("tls.record.content_type == 23 && tcp.port == {}", port),
                "-T", "fields", "-e", "frame.number",
            ])
            .output()
            .expect("failed to run tshark");
        let app_data = String::from_utf8_lossy(&output.stdout).trim().to_string();
        assert!(
            !app_data.is_empty(),
            "No encrypted application data found in pcap: {}",
            pcap
        );

        eprintln!("  TLS assertions passed. pcap: {}", pcap);
    }
}

// =========================================================================
// Test: One-way TLS handshake (server auth only)
// =========================================================================
// multi_thread flavor needed: master.connect() does blocking TLS I/O inside
// the async context; without extra threads the slave's async accept() loop
// would be starved and the handshake would time out.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tls_handshake_one_way() {
    if !check_tools_available() { return; }

    let port = free_port();
    let certs = cert_gen::generate();
    let tmp = tempfile::tempdir().unwrap();
    let paths = cert_gen::write_to_dir(&certs, tmp.path());

    // Start the rustls-backed slave with PEM identity, no client cert required.
    let transport = SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        tls: SlaveTlsConfig {
            enabled: true,
            cert_file: paths.server_cert.to_str().unwrap().to_string(),
            key_file: paths.server_key.to_str().unwrap().to_string(),
            ca_file: String::new(),
            require_client_cert: false,
            pkcs12_file: String::new(),
            pkcs12_password: String::new(),
        },
    };
    let mut slave = SlaveServer::new(transport);
    slave.add_station(Station::with_default_points(1, "TLS Test", 2)).await.unwrap();
    slave.start().await.unwrap();
    sleep(Duration::from_millis(300)).await;

    // Start packet capture
    let mut cap = capture::start("tls_handshake_one_way", port)
        .expect("failed to start capture");
    sleep(Duration::from_millis(500)).await;

    // Connect master with TLS, trusting our CA
    let config = MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        tls: TlsConfig {
            enabled: true,
            ca_file: paths.ca_cert.to_str().unwrap().to_string(),
            cert_file: String::new(),
            key_file: String::new(),
            pkcs12_file: String::new(),
            pkcs12_password: String::new(),
            accept_invalid_certs: false,
            version: iec104sim_core::master::TlsVersionPolicy::default(),
        },
        ..Default::default()
    };
    let mut master = MasterConnection::new(config);
    let connect_result = master.connect().await;
    assert!(connect_result.is_ok(), "TLS connection should succeed: {:?}", connect_result.err());
    sleep(Duration::from_millis(500)).await;

    // Disconnect and stop capture
    master.disconnect().await.unwrap();
    sleep(Duration::from_millis(300)).await;
    slave.stop().await.unwrap();
    sleep(Duration::from_millis(300)).await;
    cap.stop().expect("failed to stop capture");

    // Protocol assertions
    capture::assert_tls_encrypted(&cap.pcap_path, port);
}

// =========================================================================
// Test: Mutual TLS handshake (server + client auth)
// =========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tls_handshake_mtls() {
    let _identity_guard = MTLS_CLIENT_IDENTITY_LOCK.lock().await;
    if !check_tools_available() { return; }

    let port = free_port();
    let certs = cert_gen::generate();
    let tmp = tempfile::tempdir().unwrap();
    let paths = cert_gen::write_to_dir(&certs, tmp.path());

    // Slave with rustls mTLS config (PEM identity + client CA verifier).
    let transport = SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        tls: SlaveTlsConfig {
            enabled: true,
            cert_file: paths.server_cert.to_str().unwrap().to_string(),
            key_file: paths.server_key.to_str().unwrap().to_string(),
            ca_file: paths.ca_cert.to_str().unwrap().to_string(),
            require_client_cert: true,
            pkcs12_file: String::new(),
            pkcs12_password: String::new(),
        },
    };
    let mut slave = SlaveServer::new(transport);
    slave.add_station(Station::with_default_points(1, "mTLS Test", 2)).await.unwrap();
    slave.start().await.unwrap();
    sleep(Duration::from_millis(300)).await;

    let mut cap = capture::start("tls_handshake_mtls", port)
        .expect("failed to start capture");
    sleep(Duration::from_millis(500)).await;

    // Master with client cert via PKCS12 (macOS Security framework requires this for ECDSA)
    let config = MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        tls: TlsConfig {
            enabled: true,
            ca_file: paths.ca_cert.to_str().unwrap().to_string(),
            cert_file: String::new(),
            key_file: String::new(),
            pkcs12_file: paths.client_pkcs12.to_str().unwrap().to_string(),
            pkcs12_password: cert_gen::PKCS12_PASS.to_string(),
            accept_invalid_certs: false,
            version: iec104sim_core::master::TlsVersionPolicy::default(),
        },
        ..Default::default()
    };
    let mut master = MasterConnection::new(config);
    let connect_result = master.connect().await;
    assert!(connect_result.is_ok(), "mTLS connection should succeed: {:?}", connect_result.err());
    sleep(Duration::from_millis(500)).await;

    master.disconnect().await.unwrap();
    sleep(Duration::from_millis(300)).await;
    slave.stop().await.unwrap();
    sleep(Duration::from_millis(300)).await;
    cap.stop().expect("failed to stop capture");

    capture::assert_tls_encrypted(&cap.pcap_path, port);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_rustls_mtls_rejects_client_without_certificate() {
    let certs = cert_gen::generate();
    let tmp = tempfile::tempdir().unwrap();
    let paths = cert_gen::write_to_dir(&certs, tmp.path());

    let tls = SlaveTlsConfig {
        enabled: true,
        cert_file: paths.server_cert.to_str().unwrap().to_string(),
        key_file: paths.server_key.to_str().unwrap().to_string(),
        ca_file: paths.ca_cert.to_str().unwrap().to_string(),
        require_client_cert: true,
        pkcs12_file: String::new(),
        pkcs12_password: String::new(),
    };
    let (port, mut slave) = start_slave_on_free_port(&tls, "mTLS Required", 1).await;
    sleep(Duration::from_millis(200)).await;

    let config = MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        tls: TlsConfig {
            enabled: true,
            ca_file: paths.ca_cert.to_str().unwrap().to_string(),
            cert_file: String::new(),
            key_file: String::new(),
            pkcs12_file: String::new(),
            pkcs12_password: String::new(),
            accept_invalid_certs: false,
            version: iec104sim_core::master::TlsVersionPolicy::default(),
        },
        ..Default::default()
    };
    let mut master = MasterConnection::new(config);

    // Different native-tls backends surface the server alert at different
    // times: Security.framework can fail connect() immediately, while OpenSSL
    // may return Ok and observe the alert on the first application I/O. The
    // authoritative server-side contract is that the unauthenticated session
    // is rejected and removed.
    if master.connect().await.is_ok() {
        let _ = master.send_interrogation(1).await;
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while slave.client_connection_count().await != 0
        && tokio::time::Instant::now() < deadline
    {
        sleep(Duration::from_millis(25)).await;
    }

    assert_eq!(slave.client_connection_count().await, 0);
    let _ = master.disconnect().await;
    slave.stop().await.unwrap();
}

// =========================================================================
// Helper: Run GI + spontaneous + control protocol checks
// =========================================================================

async fn run_protocol_checks(slave: &SlaveServer, master: &MasterConnection) {
    // --- 1. General Interrogation ---
    master.send_interrogation(1).await.unwrap();
    sleep(Duration::from_millis(1000)).await;

    {
        let data = master.received_data.read().await;
        let map = data.ca_map(1).expect("CA=1 map should exist after GI");
        assert!(map.get(100, AsduTypeId::MSpNa1).is_some(), "IOA=100 (SP) should exist after GI");
        assert!(map.get(200, AsduTypeId::MMeNc1).is_some(), "IOA=200 (Float) should exist after GI");
    }

    // --- 2. Spontaneous (Change-of-State) ---
    {
        let mut stations = slave.stations.write().await;
        let st = stations.get_mut(&1).unwrap();
        let point = st.data_points.get_mut(100, AsduTypeId::MSpNa1).unwrap();
        point.value = DataPointValue::SinglePoint { value: true };
    }
    slave.queue_spontaneous(1, &[(100, AsduTypeId::MSpNa1)]).await;
    sleep(Duration::from_millis(1000)).await;

    {
        let data = master.received_data.read().await;
        let map = data.ca_map(1).expect("CA=1 map should exist after spontaneous");
        let point = map.get(100, AsduTypeId::MSpNa1).unwrap();
        assert_eq!(point.value, DataPointValue::SinglePoint { value: true },
            "Master should receive spontaneous update: SP=true");
    }

    // --- 3. Control Command (single point) — QU=0, COT=6 (activation) ---
    master.send_single_command(100, false, false, 1, 0, 6).await.unwrap();
    sleep(Duration::from_millis(1000)).await;

    {
        let stations = slave.stations.read().await;
        let point = stations.get(&1).unwrap().data_points.get(100, AsduTypeId::MSpNa1).unwrap();
        assert_eq!(point.value, DataPointValue::SinglePoint { value: false },
            "Slave data point should be updated by control command");
    }

    {
        let data = master.received_data.read().await;
        let map = data.ca_map(1).expect("CA=1 map should exist after writeback");
        let point = map.get(100, AsduTypeId::MSpNa1).unwrap();
        assert_eq!(point.value, DataPointValue::SinglePoint { value: false },
            "Master should see control writeback via COT=3");
    }
}

// =========================================================================
// Test: Full IEC 104 protocol over one-way TLS
// =========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tls_full_protocol() {
    if !check_tools_available() { return; }

    let port = free_port();
    let certs = cert_gen::generate();
    let tmp = tempfile::tempdir().unwrap();
    let paths = cert_gen::write_to_dir(&certs, tmp.path());

    let transport = SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        tls: SlaveTlsConfig {
            enabled: true,
            cert_file: paths.server_cert.to_str().unwrap().to_string(),
            key_file: paths.server_key.to_str().unwrap().to_string(),
            ca_file: String::new(),
            require_client_cert: false,
            pkcs12_file: String::new(),
            pkcs12_password: String::new(),
        },
    };
    let mut slave = SlaveServer::new(transport);
    slave
        .set_remote_ops(RemoteOperationConfig {
            auto_map_commands: true,
            ..Default::default()
        })
        .await;
    let mut station = Station::new(1, "TLS Protocol Test");
    station.batch_add_points(100, 1, AsduTypeId::MSpNa1, "SP").unwrap();
    station.batch_add_points(200, 1, AsduTypeId::MMeNc1, "FL").unwrap();
    slave.add_station(station).await.unwrap();
    slave.start().await.unwrap();
    sleep(Duration::from_millis(300)).await;

    let mut cap = capture::start("tls_full_protocol", port)
        .expect("failed to start capture");
    sleep(Duration::from_millis(500)).await;

    let config = MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        tls: TlsConfig {
            enabled: true,
            ca_file: paths.ca_cert.to_str().unwrap().to_string(),
            cert_file: String::new(),
            key_file: String::new(),
            pkcs12_file: String::new(),
            pkcs12_password: String::new(),
            accept_invalid_certs: false,
            version: iec104sim_core::master::TlsVersionPolicy::default(),
        },
        ..Default::default()
    };
    let mut master = MasterConnection::new(config);
    master.connect().await.unwrap();
    sleep(Duration::from_millis(500)).await;

    run_protocol_checks(&slave, &master).await;

    master.disconnect().await.unwrap();
    sleep(Duration::from_millis(300)).await;
    slave.stop().await.unwrap();
    sleep(Duration::from_millis(300)).await;
    cap.stop().expect("failed to stop capture");

    capture::assert_tls_encrypted(&cap.pcap_path, port);
}

// =========================================================================
// Test: Full IEC 104 protocol over mutual TLS
// =========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tls_mtls_full_protocol() {
    let _identity_guard = MTLS_CLIENT_IDENTITY_LOCK.lock().await;
    if !check_tools_available() { return; }

    let port = free_port();
    let certs = cert_gen::generate();
    let tmp = tempfile::tempdir().unwrap();
    let paths = cert_gen::write_to_dir(&certs, tmp.path());

    let transport = SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        tls: SlaveTlsConfig {
            enabled: true,
            cert_file: paths.server_cert.to_str().unwrap().to_string(),
            key_file: paths.server_key.to_str().unwrap().to_string(),
            ca_file: paths.ca_cert.to_str().unwrap().to_string(),
            require_client_cert: true,
            pkcs12_file: String::new(),
            pkcs12_password: String::new(),
        },
    };
    let mut slave = SlaveServer::new(transport);
    slave
        .set_remote_ops(RemoteOperationConfig {
            auto_map_commands: true,
            ..Default::default()
        })
        .await;
    let mut station = Station::new(1, "mTLS Protocol Test");
    station.batch_add_points(100, 1, AsduTypeId::MSpNa1, "SP").unwrap();
    station.batch_add_points(200, 1, AsduTypeId::MMeNc1, "FL").unwrap();
    slave.add_station(station).await.unwrap();
    slave.start().await.unwrap();
    sleep(Duration::from_millis(300)).await;

    let mut cap = capture::start("tls_mtls_full_protocol", port)
        .expect("failed to start capture");
    sleep(Duration::from_millis(500)).await;

    let config = MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        tls: TlsConfig {
            enabled: true,
            ca_file: paths.ca_cert.to_str().unwrap().to_string(),
            cert_file: String::new(),
            key_file: String::new(),
            pkcs12_file: paths.client_pkcs12.to_str().unwrap().to_string(),
            pkcs12_password: cert_gen::PKCS12_PASS.to_string(),
            accept_invalid_certs: false,
            version: iec104sim_core::master::TlsVersionPolicy::default(),
        },
        ..Default::default()
    };
    let mut master = MasterConnection::new(config);
    master.connect().await.unwrap();
    sleep(Duration::from_millis(500)).await;

    run_protocol_checks(&slave, &master).await;

    master.disconnect().await.unwrap();
    sleep(Duration::from_millis(300)).await;
    slave.stop().await.unwrap();
    sleep(Duration::from_millis(300)).await;
    cap.stop().expect("failed to stop capture");

    capture::assert_tls_encrypted(&cap.pcap_path, port);
}
