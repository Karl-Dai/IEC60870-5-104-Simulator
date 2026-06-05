//! Repro for: master with TLS enabled fails on the *second* connect after a
//! connect → disconnect cycle. No packet capture needed — pure protocol path.

use iec104sim_core::master::{
    MasterConfig, MasterConnection, MasterState, TlsConfig, TlsVersionPolicy,
};
use iec104sim_core::slave::{SlaveServer, SlaveTlsConfig, SlaveTransportConfig, Station};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use tokio::time::{sleep, Duration};

mod common;
use common::cert_gen;

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// connect → disconnect → connect, with a configurable pause between the
/// disconnect and the reconnect. Returns the error string of whichever step
/// failed so the test name + message pinpoint the failing version policy.
async fn run_reconnect(version: TlsVersionPolicy, gap_ms: u64) -> Result<(), String> {
    let port = free_port();
    let certs = cert_gen::generate();
    let tmp = tempfile::tempdir().unwrap();
    let paths = cert_gen::write_to_dir(&certs, tmp.path());

    let transport = SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        tls: SlaveTlsConfig {
            enabled: true,
            cert_file: String::new(),
            key_file: String::new(),
            ca_file: String::new(),
            require_client_cert: false,
            pkcs12_file: paths.server_pkcs12.to_str().unwrap().to_string(),
            pkcs12_password: cert_gen::PKCS12_PASS.to_string(),
        },
    };
    let mut slave = SlaveServer::new(transport);
    slave
        .add_station(Station::with_default_points(1, "TLS Test", 2))
        .await
        .unwrap();
    slave.start().await.unwrap();
    sleep(Duration::from_millis(300)).await;

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
            version,
        },
        ..Default::default()
    };
    let mut master = MasterConnection::new(config);

    // First connect
    master
        .connect()
        .await
        .map_err(|e| format!("FIRST connect failed: {:?}", e))?;
    if master.state() != MasterState::Connected {
        return Err(format!("after 1st connect state = {:?}", master.state()));
    }
    sleep(Duration::from_millis(300)).await;

    // Disconnect
    master
        .disconnect()
        .await
        .map_err(|e| format!("disconnect failed: {:?}", e))?;
    if master.state() != MasterState::Disconnected {
        return Err(format!("after disconnect state = {:?}", master.state()));
    }
    sleep(Duration::from_millis(gap_ms)).await;

    // Second connect — this is the one the user reports as broken.
    master
        .connect()
        .await
        .map_err(|e| format!("SECOND connect failed: {:?}", e))?;
    if master.state() != MasterState::Connected {
        return Err(format!("after 2nd connect state = {:?}", master.state()));
    }
    sleep(Duration::from_millis(200)).await;

    master.disconnect().await.ok();
    slave.stop().await.ok();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tls_reconnect_auto() {
    if let Err(e) = run_reconnect(TlsVersionPolicy::Auto, 300).await {
        panic!("Auto reconnect failed: {e}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tls_reconnect_tls12_only() {
    if let Err(e) = run_reconnect(TlsVersionPolicy::Tls12Only, 300).await {
        panic!("Tls12Only reconnect failed: {e}");
    }
}

// Skipped on Apple platforms for the SAME reason as
// `tls_version_negotiation.rs::master_tls13_only_handshakes_with_tls13_server`:
// native-tls 0.2 on macOS uses SecureTransport, whose TLS 1.3 *client*
// handshake is unreliable and returns alert `illegal_parameter` on the FIRST
// connect (before any reconnect logic runs). That is a documented platform
// limitation, not a reconnect defect — so this case only exercises the
// reconnect path under TLS 1.3 on Linux (OpenSSL) / Windows (SChannel).
#[cfg_attr(target_vendor = "apple", ignore = "native-tls 0.2 TLS 1.3 client unreliable on macOS")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tls_reconnect_tls13_only() {
    if let Err(e) = run_reconnect(TlsVersionPolicy::Tls13Only, 300).await {
        panic!("Tls13Only reconnect failed: {e}");
    }
}

/// Rapid reconnect with no human-scale pause — surfaces races where the old
/// receiver thread / socket hasn't fully torn down yet.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tls_reconnect_rapid() {
    if let Err(e) = run_reconnect(TlsVersionPolicy::Auto, 0).await {
        panic!("rapid reconnect failed: {e}");
    }
}

/// Decisive repro for a real single-connection device: after `disconnect()`,
/// does the master actually CLOSE the TCP socket (FIN / TLS close_notify)
/// promptly? The project's concurrent slave hides this — it just accepts a
/// second connection regardless. A *single-connection* server, which only
/// accepts conn2 after conn1's read returns EOF, exposes whether the socket
/// was really released.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tls_disconnect_closes_socket_single_conn() {
    let port = free_port();
    let certs = cert_gen::generate();
    let tmp = tempfile::tempdir().unwrap();
    let paths = cert_gen::write_to_dir(&certs, tmp.path());

    let p12 = std::fs::read(&paths.server_pkcs12).unwrap();
    let identity = native_tls::Identity::from_pkcs12(&p12, cert_gen::PKCS12_PASS).unwrap();
    let acceptor = native_tls::TlsAcceptor::new(identity).unwrap();
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();

    let (tx, rx) = mpsc::channel::<String>();
    let server = std::thread::spawn(move || {
        let startdt_con = [0x68u8, 0x04, 0x0b, 0x00, 0x00, 0x00];

        // ---- conn 1 ----
        let (tcp1, _) = listener.accept().unwrap();
        let mut s1 = match acceptor.accept(tcp1) {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(format!("conn1 handshake err: {e}"));
                return;
            }
        };
        let _ = tx.send("conn1 handshake ok".into());
        let mut buf = [0u8; 1024];
        let _ = s1.read(&mut buf); // STARTDT ACT
        let _ = s1.write_all(&startdt_con); // STARTDT CON
        s1.get_ref()
            .set_read_timeout(Some(std::time::Duration::from_millis(200)))
            .ok();
        let mut eof = false;
        for _ in 0..30 {
            match s1.read(&mut buf) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(_) => continue, // STOPDT etc.
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    continue
                }
                Err(_) => {
                    eof = true;
                    break;
                }
            }
        }
        let _ = tx.send(format!("conn1 eof={eof}"));

        // ---- conn 2 (the reconnect) ----
        match listener.accept() {
            Ok((tcp2, _)) => match acceptor.accept(tcp2) {
                Ok(_s2) => {
                    let _ = tx.send("conn2 handshake ok".into());
                }
                Err(e) => {
                    let _ = tx.send(format!("conn2 handshake err: {e}"));
                }
            },
            Err(e) => {
                let _ = tx.send(format!("conn2 accept err: {e}"));
            }
        }
    });

    sleep(Duration::from_millis(300)).await;

    let mk_config = || MasterConfig {
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
            version: TlsVersionPolicy::Auto,
        },
        ..Default::default()
    };

    let mut master = MasterConnection::new(mk_config());
    master.connect().await.expect("first connect");
    sleep(Duration::from_millis(500)).await;
    master.disconnect().await.expect("disconnect");

    // Give the single-connection server time to observe EOF on conn1.
    sleep(Duration::from_millis(2500)).await;

    let second = master.connect().await;
    sleep(Duration::from_millis(800)).await;
    master.disconnect().await.ok();
    drop(master);

    let _ = server.join();
    let mut events = Vec::new();
    while let Ok(m) = rx.try_recv() {
        events.push(m);
    }
    eprintln!("SERVER EVENTS: {events:?}");
    eprintln!("second connect result: {second:?}");

    assert!(
        events.iter().any(|e| e == "conn1 eof=true"),
        "master did NOT close the socket after disconnect; events={events:?}"
    );
    assert!(
        events.iter().any(|e| e == "conn2 handshake ok"),
        "reconnect handshake did not complete; events={events:?}"
    );
}

/// THE faithful repro for the user's "second connect just errors" on a real
/// single-connection RTU. The failing path is NOT an explicit disconnect —
/// it's an *unexpected drop* (the link dies / RTU goes silent and t1 fires, or
/// the peer half-closes). In that path `disconnect()` is never called, so the
/// master's receive loop exits and sets state=Disconnected but leaves its TCP
/// socket open inside `tls_stream_mutex`. On the reconnect the old socket is
/// only released *after* the new handshake — so a single-connection RTU, still
/// holding the stale session, refuses the new TLS handshake (WSAETIMEDOUT).
///
/// Here the server half-closes conn1 to simulate the drop, then refuses to
/// accept conn2 until conn1 reaches EOF. The master must close its stale socket
/// as part of reconnecting; otherwise `conn1 eof=true` never fires.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tls_reconnect_after_unexpected_drop_single_conn() {
    let port = free_port();
    let certs = cert_gen::generate();
    let tmp = tempfile::tempdir().unwrap();
    let paths = cert_gen::write_to_dir(&certs, tmp.path());

    let p12 = std::fs::read(&paths.server_pkcs12).unwrap();
    let identity = native_tls::Identity::from_pkcs12(&p12, cert_gen::PKCS12_PASS).unwrap();
    let acceptor = native_tls::TlsAcceptor::new(identity).unwrap();
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();

    let (tx, rx) = mpsc::channel::<String>();
    let server = std::thread::spawn(move || {
        let startdt_con = [0x68u8, 0x04, 0x0b, 0x00, 0x00, 0x00];

        // ---- conn 1 ----
        let (tcp1, _) = listener.accept().unwrap();
        let mut s1 = match acceptor.accept(tcp1) {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(format!("conn1 handshake err: {e}"));
                return;
            }
        };
        let _ = tx.send("conn1 handshake ok".into());
        let mut buf = [0u8; 1024];
        let _ = s1.read(&mut buf); // STARTDT ACT
        let _ = s1.write_all(&startdt_con); // STARTDT CON
        let _ = s1.flush();

        // Simulate an unexpected link drop: half-close so the master reads EOF
        // and tears down its receive loop *without* calling disconnect().
        let _ = s1.get_ref().shutdown(std::net::Shutdown::Write);

        // Wait for the master to actually close conn1 (its FIN -> EOF here).
        // Budget ~3s; if the master leaves the stale socket open this stays
        // false and the reconnect can't be accepted as conn2.
        s1.get_ref()
            .set_read_timeout(Some(std::time::Duration::from_millis(200)))
            .ok();
        let mut eof = false;
        for _ in 0..15 {
            match s1.read(&mut buf) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(_) => continue,
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    continue
                }
                Err(_) => {
                    eof = true;
                    break;
                }
            }
        }
        let _ = tx.send(format!("conn1 eof={eof}"));

        // ---- conn 2 (the reconnect) ----
        match listener.accept() {
            Ok((tcp2, _)) => match acceptor.accept(tcp2) {
                Ok(_s2) => {
                    let _ = tx.send("conn2 handshake ok".into());
                }
                Err(e) => {
                    let _ = tx.send(format!("conn2 handshake err: {e}"));
                }
            },
            Err(e) => {
                let _ = tx.send(format!("conn2 accept err: {e}"));
            }
        }
    });

    sleep(Duration::from_millis(300)).await;

    let mk_config = || MasterConfig {
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
            version: TlsVersionPolicy::Auto,
        },
        ..Default::default()
    };

    let mut master = MasterConnection::new(mk_config());
    master.connect().await.expect("first connect");

    // Wait until the master notices the dropped link (server half-closed).
    let mut dropped = false;
    for _ in 0..40 {
        if master.state() != MasterState::Connected {
            dropped = true;
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    assert!(
        dropped,
        "master never noticed the dropped link; state={:?}",
        master.state()
    );

    // Reconnect. The fix makes connect() force-close the stale socket first.
    let second = master.connect().await;
    sleep(Duration::from_millis(800)).await;
    master.disconnect().await.ok();
    drop(master);
    let _ = server.join();

    let mut events = Vec::new();
    while let Ok(m) = rx.try_recv() {
        events.push(m);
    }
    eprintln!("DROP-RECONNECT EVENTS: {events:?}");
    eprintln!("second connect result: {second:?}");

    assert!(
        events.iter().any(|e| e == "conn1 eof=true"),
        "reconnect did NOT close the stale socket from the dropped link \
         (single-conn RTU would refuse the reconnect); events={events:?}"
    );
    assert!(
        events.iter().any(|e| e == "conn2 handshake ok"),
        "reconnect handshake did not complete; events={events:?}"
    );
    assert!(second.is_ok(), "second connect failed: {second:?}");
}

/// Root-cause repro for the user's reconnect failure:
///   Windows: "TLS 握手失败: ... (os error 10060)"  (WSAETIMEDOUT)
///   macOS:   "TLS 握手失败: the handshake process was interrupted"
///
/// `connect_inner` armed the TCP socket with a 100 ms read_timeout BEFORE the
/// blocking TLS handshake. That timeout exists only to let the *plain-TCP*
/// receive loop tick its timers — the TLS receive loop uses `set_nonblocking`
/// instead — but it was applied unconditionally, so it also bounded every
/// handshake read at 100 ms. A single-connection RTU still tearing down its
/// previous TLS session answers the reconnect handshake a hair late; any round
/// >100 ms makes the handshake read time out. A fast localhost server hides
/// this (every round <100 ms), which is why the existing reconnect tests pass.
/// Here we model the slow peer explicitly: delay the TLS accept past 100 ms.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tls_handshake_survives_server_slower_than_receive_poll() {
    let port = free_port();
    let certs = cert_gen::generate();
    let tmp = tempfile::tempdir().unwrap();
    let paths = cert_gen::write_to_dir(&certs, tmp.path());

    let p12 = std::fs::read(&paths.server_pkcs12).unwrap();
    let identity = native_tls::Identity::from_pkcs12(&p12, cert_gen::PKCS12_PASS).unwrap();
    let acceptor = native_tls::TlsAcceptor::new(identity).unwrap();
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();

    let server = std::thread::spawn(move || -> String {
        let (tcp, _) = match listener.accept() {
            Ok(v) => v,
            Err(e) => return format!("accept err: {e}"),
        };
        // Single-connection RTU still releasing the old session: it answers the
        // new handshake later than the master's 100 ms receive-poll timeout.
        std::thread::sleep(Duration::from_millis(250));
        let mut s = match acceptor.accept(tcp) {
            Ok(s) => s,
            Err(e) => return format!("server handshake err: {e}"),
        };
        let mut buf = [0u8; 1024];
        let _ = s.read(&mut buf); // STARTDT ACT
        let _ = s.write_all(&[0x68u8, 0x04, 0x0b, 0x00, 0x00, 0x00]); // STARTDT CON
        std::thread::sleep(Duration::from_millis(200));
        "ok".into()
    });

    sleep(Duration::from_millis(100)).await;

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
            version: TlsVersionPolicy::Auto,
        },
        ..Default::default()
    };
    let mut master = MasterConnection::new(config);

    let res = master.connect().await;
    assert!(
        res.is_ok(),
        "TLS handshake must survive a peer that answers slower than the 100ms \
         receive-poll timeout (this is the reconnect-against-a-single-conn-RTU \
         case); got: {res:?}"
    );
    assert_eq!(master.state(), MasterState::Connected);

    master.disconnect().await.ok();
    let _ = server.join();
}
