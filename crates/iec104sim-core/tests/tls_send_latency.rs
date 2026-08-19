//! Regression test: TLS connection's send_frame should not be blocked by the
//! receive loop holding the stream mutex during a `read()` that's waiting on
//! a silent peer.
//!
//! Before the fix, `receive_loop_mutex` held `stream.lock()` through the
//! whole `read()` call. Because native_tls on macOS doesn't reliably
//! propagate the underlying `set_read_timeout`, the lock could end up held
//! for many seconds, blocking every `send_frame` (counter read, GI, etc.).
//!
//! After the fix, the underlying TcpStream is switched to non-blocking after
//! the TLS handshake; `read()` returns `WouldBlock` immediately when no
//! data is available, the receiver releases the lock, and senders get a
//! window every ~5 ms.

mod common;
use common::cert_gen;

use iec104sim_core::data_point::{DataPoint, DataPointValue, InformationObjectDef};
use iec104sim_core::master::{MasterConfig, MasterConnection, TlsConfig};
use iec104sim_core::slave::{SlaveServer, SlaveTlsConfig, SlaveTransportConfig, Station};
use iec104sim_core::types::{AsduTypeId, DataCategory};
use std::time::Instant;
use tokio::time::{sleep, Duration};

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tls_send_counter_read_returns_under_100ms_when_peer_is_silent() {
    let port = free_port();
    let tmp = tempfile::tempdir().unwrap();
    let paths = cert_gen::write_to_dir(&cert_gen::generate(), tmp.path());

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

    // One IT point so CI has something to ack — but the slave never sends
    // anything spontaneously, so the master's receive loop sits idle in
    // read() between requests. That's exactly the scenario where the old
    // design pinned the stream mutex.
    let mut station = Station::new(1, "silent");
    station.object_defs.push(InformationObjectDef {
        ioa: 100,
        asdu_type: AsduTypeId::MItNa1,
        category: DataCategory::IntegratedTotals,
        name: String::new(),
        comment: String::new(), mapping: None,
        command_qualifier: None, select_before_operate: None
    });
    station.data_points.insert(DataPoint::with_value(
        100,
        AsduTypeId::MItNa1,
        DataPointValue::IntegratedTotal { value: 42, carry: false, sequence: 0 },
    ));
    slave.add_station(station).await.unwrap();
    slave.start().await.unwrap();
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
    master.connect().await.expect("TLS connect failed");

    // Long enough that the receive loop is guaranteed to be parked inside a
    // read() call before we start measuring.
    sleep(Duration::from_millis(2000)).await;

    for round in 0..5 {
        let t0 = Instant::now();
        master
            .send_counter_read(1)
            .await
            .expect("send_counter_read failed");
        let dt = t0.elapsed();
        eprintln!("round {} send_counter_read latency: {:?}", round, dt);
        assert!(
            dt < Duration::from_millis(100),
            "round {}: send_counter_read took {:?}, expected < 100ms — receive loop is blocking the stream mutex",
            round, dt
        );
        sleep(Duration::from_millis(300)).await;
    }

    master.disconnect().await.ok();
    slave.stop().await.ok();
}
