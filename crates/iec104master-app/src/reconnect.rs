//! 主站连接的状态督导任务(state supervisor)。
//!
//! 在 `create_connection` 里随每个连接 spawn 一个本任务,职责有二:
//!   1. 把 core 的 `MasterState` 变化转发给前端(`emit`);
//!   2. 一旦连接**建立过之后**掉线(Disconnected/Error),按独立的
//!      Channel Retry 固定间隔自动重连(`on_drop`,内部封装等待 + connect)。
//!
//! 督导只在**首次 Connected 之后**才武装重连:首次连接失败(填错 IP/端口)
//! 仍按 Error 暴露给用户,不静默无限重试。武装之后,Disconnected/Error 会调用
//! on_drop;实际重连还需检查运行时 reconnect_enabled。手动断开立即关闭该开关,
//! 排队中的重连也需在等待结束后复查;再次手动连接成功才重新启用。
//!
//! 决策逻辑与 Tauri 解耦,纯靠注入的 `emit`/`on_drop` 闭包驱动,便于无头测试。

use iec104sim_core::master::MasterState;
use std::future::Future;
use tokio::sync::watch;

/// 等待 Channel Retry 后重连;等待期间不占用连接表锁。
pub(crate) async fn reconnect_after_delay(state: &crate::state::AppState, id: &str) {
    let retry_delay_s = {
        let connections = state.connections.read().await;
        match connections.get(id) {
            Some(c) if c.reconnect_enabled => c.connection.config().channel_retry_s,
            _ => return,
        }
    };
    tokio::time::sleep(std::time::Duration::from_secs(retry_delay_s as u64)).await;
    let mut connections = state.connections.write().await;
    if let Some(c) = connections.get_mut(id) {
        // The user may have disconnected while the retry timer was sleeping.
        // Check intent under the same lock used by the manual commands.
        if c.reconnect_enabled && c.connection.state() != MasterState::Connected {
            let _ = c.connection.connect().await;
        }
    }
}

/// 驱动一个连接的状态督导循环。`state_rx` 关闭(连接被删除)时返回。
pub async fn run_state_supervisor<E, R, RF>(
    mut state_rx: watch::Receiver<MasterState>,
    mut emit: E,
    mut on_drop: R,
) where
    E: FnMut(MasterState),
    R: FnMut() -> RF,
    RF: Future<Output = ()>,
{
    let mut armed = false;
    let mut pending_change = false;
    loop {
        if !pending_change && state_rx.changed().await.is_err() {
            return;
        }
        pending_change = false;

        let state = *state_rx.borrow_and_update();
        emit(state);
        match state {
            MasterState::Connected => armed = true,
            MasterState::Disconnected | MasterState::Error if armed => {
                // Retry sleep/connect must not keep a retired connection's
                // supervisor alive. Only channel closure cancels the retry:
                // connect() itself emits Connecting/Connected and may yield
                // between them, so cancelling on a real change could leave it
                // half-initialized. Remember such changes and keep polling the
                // same retry future until it completes.
                let retry = on_drop();
                tokio::pin!(retry);
                loop {
                    tokio::select! {
                        biased;
                        changed = state_rx.changed() => match changed {
                            Ok(()) => pending_change = true,
                            Err(_) => return,
                        },
                        _ = &mut retry => break,
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, MasterConnectionState};
    use iec104sim_core::log_collector::LogCollector;
    use iec104sim_core::master::{MasterConfig, MasterConnection, MasterError};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::mpsc;

    /// Real TCP plus the same runtime methods and retry callback as the commands.
    struct TestConnection {
        state: Arc<AppState>,
        listener: TcpListener,
        events: mpsc::UnboundedReceiver<MasterState>,
        retries: mpsc::UnboundedReceiver<()>,
        supervisor: tokio::task::JoinHandle<()>,
    }

    impl TestConnection {
        async fn new() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let connection = MasterConnection::new(MasterConfig {
                port: listener.local_addr().unwrap().port(),
                channel_retry_s: 1,
                ..Default::default()
            });
            let state_rx = connection.subscribe_state();
            let state = Arc::new(AppState::new());
            state.connections.write().await.insert(
                "test".into(),
                MasterConnectionState {
                    connection,
                    log_collector: Arc::new(LogCollector::new()),
                    reconnect_enabled: false,
                    common_addresses: vec![1],
                },
            );
            let retry_state = state.clone();
            let (emit_tx, events) = mpsc::unbounded_channel();
            let (retry_tx, retries) = mpsc::unbounded_channel();
            let supervisor = tokio::spawn(run_state_supervisor(
                state_rx,
                move |state| {
                    let _ = emit_tx.send(state);
                },
                move || {
                    let state = retry_state.clone();
                    let retry_tx = retry_tx.clone();
                    async move {
                        let _ = retry_tx.send(());
                        reconnect_after_delay(&state, "test").await;
                    }
                },
            ));
            Self {
                state,
                listener,
                events,
                retries,
                supervisor,
            }
        }

        async fn accept(&mut self) -> TcpStream {
            tokio::time::timeout(Duration::from_secs(3), async {
                let (mut peer, _) = self.listener.accept().await.unwrap();
                let mut startdt = [0; 6];
                peer.read_exact(&mut startdt).await.unwrap();
                assert_eq!(startdt, [0x68, 4, 7, 0, 0, 0]);
                peer.write_all(&[0x68, 4, 0x0b, 0, 0, 0]).await.unwrap();
                peer
            })
            .await
            .expect("master should establish a real TCP session")
        }

        async fn connect(&mut self) -> TcpStream {
            self.state
                .connections
                .write()
                .await
                .get_mut("test")
                .unwrap()
                .connect()
                .await
                .unwrap();
            let peer = self.accept().await;
            self.wait_state(MasterState::Connected).await;
            peer
        }

        async fn disconnect(&self) -> Result<(), MasterError> {
            self.state
                .connections
                .write()
                .await
                .get_mut("test")
                .unwrap()
                .disconnect()
                .await
        }

        async fn wait_state(&mut self, expected: MasterState) {
            tokio::time::timeout(Duration::from_secs(3), async {
                while self
                    .events
                    .recv()
                    .await
                    .expect("supervisor should remain alive")
                    != expected
                {}
            })
            .await
            .expect("supervisor should emit the expected state");
        }

        async fn assert_stays_disconnected(&self) {
            // Observe more than a full Channel Retry period at the TCP boundary.
            assert!(
                tokio::time::timeout(Duration::from_millis(1300), self.listener.accept())
                    .await
                    .is_err(),
                "manual disconnect must not open another TCP connection"
            );
            assert_eq!(
                self.state.connections.read().await["test"]
                    .connection
                    .state(),
                MasterState::Disconnected
            );
        }
    }

    impl Drop for TestConnection {
        fn drop(&mut self) {
            self.supervisor.abort();
        }
    }

    #[tokio::test]
    async fn manual_disconnect_stays_disconnected() {
        let mut test = TestConnection::new().await;
        let _peer = test.connect().await;
        test.disconnect().await.unwrap();
        test.wait_state(MasterState::Disconnected).await;
        test.assert_stays_disconnected().await;
    }

    #[tokio::test]
    async fn manual_disconnect_cancels_pending_retry_after_peer_drop() {
        let mut test = TestConnection::new().await;
        let peer = test.connect().await;
        drop(peer);
        test.wait_state(MasterState::Disconnected).await;
        tokio::time::timeout(Duration::from_secs(1), test.retries.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            test.disconnect().await,
            Err(MasterError::NotConnected)
        ));
        test.assert_stays_disconnected().await;
    }

    #[tokio::test]
    async fn manual_connect_reenables_retry_for_later_peer_drop() {
        let mut test = TestConnection::new().await;
        let first_peer = test.connect().await;
        test.disconnect().await.unwrap();
        drop(first_peer);
        test.wait_state(MasterState::Disconnected).await;
        test.assert_stays_disconnected().await;

        let second_peer = test.connect().await;
        drop(second_peer);
        test.wait_state(MasterState::Disconnected).await;
        let dropped_at = tokio::time::Instant::now();
        let _reconnected_peer = test.accept().await;
        assert!(dropped_at.elapsed() >= Duration::from_millis(900));
        test.wait_state(MasterState::Connected).await;
        test.disconnect().await.unwrap();
    }

    #[tokio::test]
    async fn failed_manual_connect_does_not_reenable_stopped_connection() {
        let mut test = TestConnection::new().await;
        let _peer = test.connect().await;
        test.disconnect().await.unwrap();
        test.wait_state(MasterState::Disconnected).await;
        {
            let mut connections = test.state.connections.write().await;
            let conn = connections.get_mut("test").unwrap();
            // A deterministic failed dial without racing another ephemeral port.
            conn.connection.config.target_address = "invalid-address".into();
            assert!(conn.connect().await.is_err());
            conn.connection.config.target_address = "127.0.0.1".into();
        }
        test.wait_state(MasterState::Error).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(1300), test.listener.accept())
                .await
                .is_err()
        );
        assert_eq!(
            test.state.connections.read().await["test"]
                .connection
                .state(),
            MasterState::Error
        );
    }

    #[tokio::test]
    async fn failed_automatic_retry_remains_enabled_until_manual_disconnect() {
        let mut test = TestConnection::new().await;
        let peer = test.connect().await;
        test.state
            .connections
            .write()
            .await
            .get_mut("test")
            .unwrap()
            .connection
            .config
            .target_address = "invalid-address".into();
        drop(peer);
        test.wait_state(MasterState::Error).await;
        test.state
            .connections
            .write()
            .await
            .get_mut("test")
            .unwrap()
            .connection
            .config
            .target_address = "127.0.0.1".into();
        let _reconnected_peer = test.accept().await;
        test.wait_state(MasterState::Connected).await;
        test.disconnect().await.unwrap();
        test.assert_stays_disconnected().await;
    }

    struct DropMarker(Arc<AtomicBool>);

    impl Drop for DropMarker {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    /// 首次 Connected 之前的 Error 不触发重连;Connected 之后的 Disconnected/
    /// Error 各触发一次;state 通道关闭后督导退出。
    #[tokio::test]
    async fn arms_after_connected_then_reconnects_on_every_drop() {
        let (state_tx, state_rx) = watch::channel(MasterState::Disconnected);
        let (emit_tx, mut emit_rx) = mpsc::unbounded_channel();
        let (drop_tx, mut drop_rx) = mpsc::unbounded_channel();
        let drop_count = Arc::new(AtomicUsize::new(0));

        let dc = drop_count.clone();
        let sup = tokio::spawn(run_state_supervisor(
            state_rx,
            move |s| {
                let _ = emit_tx.send(s);
            },
            move || {
                let dc = dc.clone();
                let drop_tx = drop_tx.clone();
                async move {
                    dc.fetch_add(1, Ordering::SeqCst);
                    let _ = drop_tx.send(());
                }
            },
        ));

        // 首次 Connected 之前的 Error:转发,但不武装重连。
        state_tx.send_replace(MasterState::Error);
        assert_eq!(emit_rx.recv().await.unwrap(), MasterState::Error);
        assert_eq!(drop_count.load(Ordering::SeqCst), 0);

        // 首次成功 Connected 武装督导。
        state_tx.send_replace(MasterState::Connected);
        assert_eq!(emit_rx.recv().await.unwrap(), MasterState::Connected);

        // 掉线触发一次重连。
        state_tx.send_replace(MasterState::Disconnected);
        assert_eq!(emit_rx.recv().await.unwrap(), MasterState::Disconnected);
        drop_rx.recv().await.unwrap();
        assert_eq!(drop_count.load(Ordering::SeqCst), 1);

        // 重连失败落到 Error,再触发一次。
        state_tx.send_replace(MasterState::Error);
        assert_eq!(emit_rx.recv().await.unwrap(), MasterState::Error);
        drop_rx.recv().await.unwrap();
        assert_eq!(drop_count.load(Ordering::SeqCst), 2);

        // 连接被删除(state 通道关闭)→ 督导退出。
        drop(state_tx);
        tokio::time::timeout(Duration::from_secs(1), sup)
            .await
            .expect("supervisor should exit when state channel closes")
            .unwrap();
    }

    #[tokio::test]
    async fn channel_close_cancels_pending_retry_and_exits_promptly() {
        let (state_tx, state_rx) = watch::channel(MasterState::Disconnected);
        let (emit_tx, mut emit_rx) = mpsc::unbounded_channel();
        let (retry_started_tx, mut retry_started_rx) = mpsc::unbounded_channel();
        let retry_dropped = Arc::new(AtomicBool::new(false));

        let dropped = retry_dropped.clone();
        let supervisor = tokio::spawn(run_state_supervisor(
            state_rx,
            move |state| {
                let _ = emit_tx.send(state);
            },
            move || {
                let marker = DropMarker(dropped.clone());
                let retry_started_tx = retry_started_tx.clone();
                async move {
                    let _marker = marker;
                    let _ = retry_started_tx.send(());
                    std::future::pending::<()>().await;
                }
            },
        ));

        state_tx.send_replace(MasterState::Connected);
        assert_eq!(emit_rx.recv().await.unwrap(), MasterState::Connected);
        state_tx.send_replace(MasterState::Disconnected);
        assert_eq!(emit_rx.recv().await.unwrap(), MasterState::Disconnected);
        retry_started_rx
            .recv()
            .await
            .expect("retry future should start");

        drop(state_tx);
        tokio::time::timeout(Duration::from_millis(500), supervisor)
            .await
            .expect("closed state channel should cancel a long retry promptly")
            .unwrap();
        assert!(
            retry_dropped.load(Ordering::SeqCst),
            "cancelled retry future must be dropped"
        );
    }

    #[tokio::test]
    async fn retry_generated_state_change_does_not_cancel_retry() {
        let (state_tx, state_rx) = watch::channel(MasterState::Disconnected);
        let (emit_tx, mut emit_rx) = mpsc::unbounded_channel();
        let (retry_completed_tx, retry_completed_rx) = tokio::sync::oneshot::channel();
        let mut retry_state_tx = Some(state_tx.clone());
        let mut retry_completed_tx = Some(retry_completed_tx);

        let supervisor = tokio::spawn(run_state_supervisor(
            state_rx,
            move |state| {
                let _ = emit_tx.send(state);
            },
            move || {
                let retry_state_tx = retry_state_tx
                    .take()
                    .expect("test should start exactly one retry");
                let retry_completed_tx = retry_completed_tx
                    .take()
                    .expect("test should complete exactly one retry");
                async move {
                    retry_state_tx.send_replace(MasterState::Connecting);
                    tokio::task::yield_now().await;
                    retry_state_tx.send_replace(MasterState::Connected);
                    let _ = retry_completed_tx.send(());
                }
            },
        ));

        state_tx.send_replace(MasterState::Connected);
        assert_eq!(emit_rx.recv().await.unwrap(), MasterState::Connected);
        state_tx.send_replace(MasterState::Disconnected);
        assert_eq!(emit_rx.recv().await.unwrap(), MasterState::Disconnected);

        tokio::time::timeout(Duration::from_millis(500), retry_completed_rx)
            .await
            .expect("self-generated Connecting must not cancel the retry")
            .expect("retry completion sender should remain alive");
        assert_eq!(
            tokio::time::timeout(Duration::from_millis(500), emit_rx.recv())
                .await
                .expect("final retry state should be processed")
                .unwrap(),
            MasterState::Connected
        );

        drop(state_tx);
        tokio::time::timeout(Duration::from_millis(500), supervisor)
            .await
            .expect("supervisor should exit after state channel closes")
            .unwrap();
    }
}
