//! 主站连接的状态督导任务(state supervisor)。
//!
//! 在 `create_connection` 里随每个连接 spawn 一个本任务,职责有二:
//!   1. 把 core 的 `MasterState` 变化转发给前端(`emit`);
//!   2. 一旦连接**建立过之后**掉线(Disconnected/Error),按独立的
//!      Channel Retry 固定间隔自动重连(`on_drop`,内部封装等待 + connect)。
//!
//! 督导只在**首次 Connected 之后**才武装重连:首次连接失败(填错 IP/端口)
//! 仍按 Error 暴露给用户,不静默无限重试。武装之后,任何进入 Disconnected/
//! Error 都触发重连(含用户手动断开 —— 唯一停止办法是删除连接)。
//!
//! 决策逻辑与 Tauri 解耦,纯靠注入的 `emit`/`on_drop` 闭包驱动,便于无头测试。

use iec104sim_core::master::MasterState;
use std::future::Future;
use tokio::sync::watch;

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
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::mpsc;

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
