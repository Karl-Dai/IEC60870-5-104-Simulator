use iec104sim_core::slave::{
    MutationMode, MutationParams, RandomMutationPacing, SlaveServer, SlaveTlsConfig,
    SlaveTransportConfig, Station,
};
use iec104sim_core::types::AsduTypeId;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

trait TestStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> TestStream for T {}
type Stream = Box<dyn TestStream>;

async fn frame(stream: &mut Stream) -> Vec<u8> {
    tokio::time::timeout(Duration::from_secs(4), async {
        let mut header = [0; 2];
        stream.read_exact(&mut header).await.unwrap();
        assert_eq!(header[0], 0x68);
        let mut data = vec![0; header[1] as usize + 2];
        data[..2].copy_from_slice(&header);
        stream.read_exact(&mut data[2..]).await.unwrap();
        data
    })
    .await
    .expect("APDU timeout")
}

async fn connect(port: u16, tls: bool) -> Stream {
    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    tcp.set_nodelay(true).unwrap();
    let mut stream: Stream = if tls {
        let connector = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();
        Box::new(
            tokio_native_tls::TlsConnector::from(connector)
                .connect("localhost", tcp)
                .await
                .unwrap(),
        )
    } else {
        Box::new(tcp)
    };
    stream.write_all(&[0x68, 4, 7, 0, 0, 0]).await.unwrap();
    assert_eq!(frame(&mut stream).await[2], 0x0b);
    stream
}

async fn server(tls: bool) -> (SlaveServer, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let tls = if tls {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_path = dir.path().join("cert.pem");
        let key_path = dir.path().join("key.pem");
        std::fs::write(&cert_path, cert.cert.pem()).unwrap();
        std::fs::write(&key_path, cert.key_pair.serialize_pem()).unwrap();
        SlaveTlsConfig {
            enabled: true,
            cert_file: cert_path.to_string_lossy().into(),
            key_file: key_path.to_string_lossy().into(),
            ..Default::default()
        }
    } else {
        SlaveTlsConfig::default()
    };
    for _ in 0..10 {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut server = SlaveServer::new(SlaveTransportConfig {
            bind_address: "127.0.0.1".into(),
            port,
            tls: tls.clone(),
        });
        for ca in [1, 2] {
            server
                .add_station(Station::with_default_points(ca, "test", 4))
                .await
                .unwrap();
        }
        if server.start().await.is_ok() {
            return (server, dir);
        }
    }
    panic!("unable to bind test server")
}

async fn start_modes(server: &SlaveServer) {
    for (i, mode) in [
        MutationMode::Flip,
        MutationMode::Increment,
        MutationMode::Decrement,
        MutationMode::Random,
    ]
    .into_iter()
    .enumerate()
    {
        server
            .start_point_mutation(
                (i % 2 + 1) as u16,
                (i + 1) as u32,
                AsduTypeId::MMeNc1,
                100,
                MutationParams {
                    mode,
                    step: 1.0,
                    min: -100.0,
                    max: 100.0,
                },
            )
            .await
            .unwrap();
    }
}

async fn collect_batches(mut stream: Stream) -> Stream {
    let mut times = Vec::new();
    let mut first_points = Vec::new();
    for index in 0..8 {
        let data = frame(&mut stream).await;
        times.push(Instant::now());
        assert_eq!(data[2] & 1, 0, "expected I-frame");
        assert_eq!(
            u16::from_le_bytes([data[2], data[3]]) >> 1,
            index,
            "sequence must remain contiguous"
        );
        assert_eq!(data[8] & 0x3f, 3);
        assert_eq!(data[7] & 0x7f, 1, "count logical points, including SQ=1");
        if index < 4 {
            first_points.push(data[12]);
        }
        if index % 2 == 1 && index < 7 {
            // TESTFR must be answered during the pacing pause, not after it.
            let start = Instant::now();
            stream.write_all(&[0x68, 4, 0x43, 0, 0, 0]).await.unwrap();
            assert_eq!(frame(&mut stream).await[2], 0x83);
            assert!(start.elapsed() < Duration::from_millis(150));
        }
    }
    first_points.sort();
    assert_eq!(first_points, [1, 2, 3, 4], "all four modes must be sent");
    for boundary in [2, 4, 6] {
        assert!(
            times[boundary].duration_since(times[boundary - 1]) >= Duration::from_millis(180),
            "two-point batch escaped cooldown: {:?}",
            times
        );
    }
    eprintln!(
        "batch gaps: {:?}",
        [2, 4, 6].map(|i| times[i].duration_since(times[i - 1]))
    );
    stream
}

async fn all_modes_and_connections(tls: bool) {
    let (mut server, _certs) = server(tls).await;
    server
        .set_simulation_pacing(RandomMutationPacing {
            batch_size: 2,
            delay_ms: 200,
        })
        .await;
    let a = connect(server.transport.port, tls).await;
    let b = connect(server.transport.port, tls).await;
    start_modes(&server).await;
    let (mut a, mut b) = tokio::join!(collect_batches(a), collect_batches(b));
    // Cancelling a simulation must invalidate its queued snapshots too.
    server.stop_all_point_mutations().await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    for stream in [&mut a, &mut b] {
        stream.write_all(&[0x68, 4, 0x43, 0, 0, 0]).await.unwrap();
        assert_eq!(
            frame(stream).await[2],
            0x83,
            "stopped point still had queued updates"
        );
    }
    server.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tcp_all_modes_multiple_stations_and_masters() {
    all_modes_and_connections(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tls_all_modes_multiple_stations_and_masters() {
    all_modes_and_connections(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_setting_releases_cooldown_without_restart() {
    let (mut server, _certs) = server(false).await;
    let original = server.get_remote_ops().await;
    server
        .set_simulation_pacing(RandomMutationPacing {
            batch_size: 1,
            delay_ms: 60_000,
        })
        .await;
    let mut stream = connect(server.transport.port, false).await;
    start_modes(&server).await;
    assert_eq!(frame(&mut stream).await[2] & 1, 0);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), frame(&mut stream))
            .await
            .is_err()
    );
    server
        .set_simulation_pacing(RandomMutationPacing {
            batch_size: 1,
            delay_ms: 0,
        })
        .await;
    let start = Instant::now();
    for _ in 0..3 {
        assert_eq!(frame(&mut stream).await[2] & 1, 0);
    }
    assert!(start.elapsed() < Duration::from_millis(150));
    let updated = server.get_remote_ops().await;
    let mut expected = serde_json::to_value(original).unwrap();
    expected["random_pacing"] = serde_json::json!({"batch_size": 1, "delay_ms": 0});
    assert_eq!(serde_json::to_value(updated).unwrap(), expected);
    server.stop().await.unwrap();
}
