//! 端到端验证:主站建立连接后被对端掉线,应在 Channel Retry 间隔后自动重连。
//!
//! 这是 `commands.rs::create_connection` 里 state supervisor 接线的纯 Rust
//! 等价版(不依赖 tauri::AppHandle):真实 `MasterConnection` + 真实 TCP server,
//! supervisor 用 `run_state_supervisor` 驱动,on_drop 闭包按 Channel Retry 锁 map 重连。
//!
//! Fake server 接受 conn1(主站到达 Connected)后立刻掉线,再接受 conn2(重连),
//! 断言主站重新回到 Connected 且 server 收到了第二条连接。

use iec104master_app_lib::reconnect::run_state_supervisor;
use iec104sim_core::master::{MasterConfig, MasterConnection, MasterState};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio::time::sleep;

struct MockConnState {
    connection: MasterConnection,
}

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

#[tokio::test]
async fn master_auto_reconnects_after_peer_drop_at_channel_retry_interval() {
    let port = free_port();
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .unwrap();

    let accepts = Arc::new(AtomicUsize::new(0));
    let ac = accepts.clone();
    let server = tokio::spawn(async move {
        let mut buf = [0u8; 64];
        // conn1:接受后读掉 STARTDT,立刻掉线模拟异常断开。
        let (mut s1, _) = listener.accept().await.unwrap();
        ac.fetch_add(1, Ordering::SeqCst);
        let _ = tokio::time::timeout(Duration::from_millis(200), s1.read(&mut buf)).await;
        drop(s1);
        // conn2:重连。持有 socket 撑过断言窗口。
        let (mut s2, _) = listener.accept().await.unwrap();
        ac.fetch_add(1, Ordering::SeqCst);
        let _ = tokio::time::timeout(Duration::from_millis(200), s2.read(&mut buf)).await;
        sleep(Duration::from_secs(10)).await;
        drop(s2);
    });

    sleep(Duration::from_millis(200)).await;

    // T0 保持 30s；Channel Retry=1s → 独立重连间隔 1s。
    let connection = MasterConnection::new(MasterConfig {
        target_address: "127.0.0.1".into(),
        port,
        common_address: 1,
        t0: 30,
        channel_retry_s: 1,
        ..Default::default()
    });
    let state_rx = connection.subscribe_state();

    let connections: Arc<RwLock<HashMap<String, MockConnState>>> =
        Arc::new(RwLock::new(HashMap::new()));
    let id = "conn_test".to_string();
    connections
        .write()
        .await
        .insert(id.clone(), MockConnState { connection });

    // supervisor:on_drop = 读 Channel Retry → 固定 sleep → 锁 map 重连。
    let conns_for_drop = connections.clone();
    let id_for_drop = id.clone();
    let supervisor = tokio::spawn(run_state_supervisor(
        state_rx,
        |_state| {},
        move || {
            let conns = conns_for_drop.clone();
            let id = id_for_drop.clone();
            async move {
                let retry_delay_s = {
                    let g = conns.read().await;
                    match g.get(&id) {
                        Some(c) => c.connection.config().channel_retry_s,
                        None => return,
                    }
                };
                sleep(Duration::from_secs(retry_delay_s as u64)).await;
                let mut g = conns.write().await;
                if let Some(c) = g.get_mut(&id) {
                    if c.connection.state() != MasterState::Connected {
                        let _ = c.connection.connect().await;
                    }
                }
            }
        },
    ));

    // 初次连接(模拟 connect_master)。
    {
        let mut g = connections.write().await;
        g.get_mut(&id).unwrap().connection.connect().await.unwrap();
    }

    // conn1 掉线检测(~100ms 轮询)+ Channel Retry=1s + 余量。
    sleep(Duration::from_secs(3)).await;

    let final_state = {
        let g = connections.read().await;
        g.get(&id).unwrap().connection.state()
    };
    assert_eq!(
        final_state,
        MasterState::Connected,
        "主站掉线后应自动重连回 Connected,实际 = {:?}",
        final_state
    );
    assert!(
        accepts.load(Ordering::SeqCst) >= 2,
        "server 应收到第二条(重连)连接,实际 accept 次数 = {}",
        accepts.load(Ordering::SeqCst)
    );

    // 清理:删连接 → state 通道关闭 → supervisor 退出。
    connections.write().await.remove(&id);
    let _ = tokio::time::timeout(Duration::from_secs(2), supervisor).await;
    server.abort();
}
