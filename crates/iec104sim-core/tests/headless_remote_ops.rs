//! 远动运行参数 (RemoteOperationConfig) 13 项矩阵的端到端回归。
//! 每个 #[tokio::test] 起一对主子站,对单一开关做 A/B 验证。

mod common;
use common::harness::Pair;
use common::helpers::{
    count_iframes, master_point_value, slave_point_value, wait_for_ioa_count,
    wait_for_log_event, DEFAULT_TIMEOUT, SHORT_TIMEOUT,
};

use iec104sim_core::data_point::{DataPoint, DataPointValue};
use iec104sim_core::log_entry::{Direction, FrameLabel};
use iec104sim_core::slave::{CommandAckCot, RemoteOperationConfig, SyncTbByCategory, UploadMode};
use iec104sim_core::types::AsduTypeId;
use tokio::time::{sleep, Duration};

/// 冒烟:harness 自身能起对子,GI 后 master 应至少看到 1 个点。
#[tokio::test]
async fn smoke_pair_and_gi() {
    let pair = Pair::spawn(RemoteOperationConfig::default()).await;
    pair.master.conn.send_interrogation(1).await.unwrap();
    let count = wait_for_ioa_count(&pair.master.conn, 1, 1, DEFAULT_TIMEOUT)
        .await
        .expect("至少收到 1 个点");
    assert!(count >= 1, "got {} IOAs", count);
    pair.shutdown().await;
}

/// 总召唤应答开关=false 时,master 发 GI 后 SHORT_TIMEOUT 内不应收到任何点。
#[tokio::test]
async fn gi_silenced_when_answer_off() {
    let ops = RemoteOperationConfig { answer_general_interrogation: false, ..Default::default() };
    let pair = Pair::spawn(ops).await;

    let initial_seq = pair.master.conn.received_data.read().await.current_seq();
    pair.master.conn.send_interrogation(1).await.unwrap();

    // 等到 SHORT_TIMEOUT 结束;期间不应有任何新点。
    let res = wait_for_ioa_count(&pair.master.conn, 1, 1, SHORT_TIMEOUT).await;
    assert!(res.is_err(), "应静默,但收到点");
    let final_seq = pair.master.conn.received_data.read().await.current_seq();
    assert_eq!(initial_seq, final_seq, "seq 不应前进 (静默)");

    pair.shutdown().await;
}

/// 累积量召唤应答开关=false 时静默。
#[tokio::test]
async fn ci_silenced_when_answer_off() {
    let ops = RemoteOperationConfig { answer_counter_interrogation: false, ..Default::default() };
    let pair = Pair::spawn(ops).await;

    let before = pair.master.conn.received_data.read().await.current_seq();
    pair.master.conn.send_counter_read(1).await.unwrap();
    sleep(SHORT_TIMEOUT).await;
    let after = pair.master.conn.received_data.read().await.current_seq();
    assert_eq!(before, after, "CI 应静默");

    pair.shutdown().await;
}

/// 遥控应答开关=false:master 发 C_SC 后 slave 值已变,但 master 收不到任何写回帧。
#[tokio::test]
async fn command_silenced_but_slave_value_updated() {
    let ops = RemoteOperationConfig {
        answer_commands: false,
        auto_map_commands: true,
        ..Default::default()
    };
    let pair = Pair::spawn(ops).await;

    // 注意:这里 GI=true (默认),但 answer_commands=false → GI 仍会响应,命令静默。
    pair.master.conn.send_interrogation(1).await.unwrap();
    let _ = wait_for_ioa_count(&pair.master.conn, 1, 1, DEFAULT_TIMEOUT).await;
    // 等到 GI 应答完全停止 (seq 在 400ms 内不增长视为稳定)。
    let mut last = pair.master.conn.received_data.read().await.current_seq();
    loop {
        sleep(Duration::from_millis(400)).await;
        let cur = pair.master.conn.received_data.read().await.current_seq();
        if cur == last { break; }
        last = cur;
    }

    let initial = master_point_value(&pair.master.conn, 1, 1, AsduTypeId::MSpNa1)
        .await
        .unwrap();
    let want = match initial {
        DataPointValue::SinglePoint { value } => !value,
        _ => true,
    };

    // 记录命令前 master 已知的 seq;期间不应再前进 (无应答 + 无自发上送)。
    let seq_before = pair.master.conn.received_data.read().await.current_seq();

    // 发命令(direct execute)
    pair.master.conn
        .send_single_command(1, want, false, 1, 0, 6)
        .await
        .unwrap();
    sleep(SHORT_TIMEOUT).await;

    // 1) slave 值已被修改 (answer_commands=false 不阻止点值更新)
    let slave_val = slave_point_value(&pair.slave.server, 1, 1, AsduTypeId::MSpNa1)
        .await
        .unwrap();
    assert_eq!(slave_val, DataPointValue::SinglePoint { value: want },
        "slave 值应已被命令修改");

    // 2) master.received_data 没有任何更新 (seq 不前进)。
    let seq_after = pair.master.conn.received_data.read().await.current_seq();
    assert_eq!(seq_before, seq_after, "应无任何回送给 master 的数据帧");

    // 3) master 端的 IOA=1 仍是 initial 值。
    let master_val = master_point_value(&pair.master.conn, 1, 1, AsduTypeId::MSpNa1)
        .await
        .unwrap();
    assert_eq!(master_val, initial, "master 端应仍是 initial 值,因为没有 spontaneous 回送");

    pair.shutdown().await;
}

/// gi_include_timestamped=true:GI 响应中 master 应同时收到 NA 和 TB 类型的点。
#[tokio::test]
async fn gi_includes_timestamped() {
    let ops = RemoteOperationConfig { gi_include_timestamped: true, ..Default::default() };
    let pair = Pair::spawn(ops).await;

    pair.master.conn.send_interrogation(1).await.unwrap();
    // 等 GI 完成。with_default_points 每类 5 点 × 8 NA = 40 点(默认不预建 TB);
    // gi_include_timestamped 从每个 NA 派生一份 TB,master 端 IOA=1 应同时有 NA 与派生 TB。
    sleep(DEFAULT_TIMEOUT.min(Duration::from_secs(2))).await;

    let data = pair.master.conn.received_data.read().await;
    let map = data.ca_map(1).expect("CA=1 received");
    // 既要有 NA,也要有 TB。
    assert!(map.get(1, AsduTypeId::MSpNa1).is_some(), "缺 NA");
    assert!(map.get(1, AsduTypeId::MSpTb1).is_some(), "缺 TB");
    drop(data);

    pair.shutdown().await;
}

/// SP 分类同步开关开启:slave 主动改 SP 值,master 应同时收到 type=1 和 type=30 两帧
/// (IOA=1 无显式 TB 点,R1 允许派生)。
#[tokio::test]
async fn sp_sync_with_tb_emits_both_frames() {
    let ops = RemoteOperationConfig {
        sync_tb_by_category: SyncTbByCategory { sp: true, ..Default::default() },
        ..Default::default()
    };
    let pair = Pair::spawn(ops).await;

    // 先 GI 让 master 进入数据传输状态并清掉日志噪声。
    pair.master.conn.send_interrogation(1).await.unwrap();
    let _ = wait_for_ioa_count(&pair.master.conn, 1, 1, DEFAULT_TIMEOUT).await;
    pair.log.clear().await;

    // slave 端改 SP 值后调用 queue_spontaneous。
    {
        let mut stations = pair.slave.server.stations.write().await;
        let st = stations.get_mut(&1).unwrap();
        let p = st.data_points.get_mut(1, AsduTypeId::MSpNa1).unwrap();
        p.value = DataPointValue::SinglePoint { value: true };
        st.data_points.mark_changed(1, AsduTypeId::MSpNa1);
    }
    pair.slave.server
        .queue_spontaneous(1, &[(1, AsduTypeId::MSpNa1)])
        .await;

    sleep(Duration::from_millis(500)).await;
    let na = count_iframes(&pair.log, Direction::Rx, "M_SP_NA_1").await;
    let tb = count_iframes(&pair.log, Direction::Rx, "M_SP_TB_1").await;
    assert!(na >= 1, "应至少 1 帧 NA, got {}", na);
    assert!(tb >= 1, "应至少 1 帧 TB, got {}", tb);

    pair.shutdown().await;
}

/// 单个监视信号的突发上送日志应直接显示当前值，避免测试人员再去点表交叉核对。
#[tokio::test]
async fn spontaneous_monitoring_log_includes_current_value() {
    let pair = Pair::spawn(RemoteOperationConfig::default()).await;
    wait_for_log_event(
        &pair.log,
        |entry| entry.direction == Direction::Tx && entry.frame_label == FrameLabel::UStartCon,
        DEFAULT_TIMEOUT,
    )
    .await
    .expect("slave STARTDT confirmation");
    pair.log.clear().await;

    {
        let mut stations = pair.slave.server.stations.write().await;
        let station = stations.get_mut(&1).unwrap();
        station.data_points.insert(DataPoint::with_value(
            512,
            AsduTypeId::MSpTb1,
            DataPointValue::SinglePoint { value: true },
        ));
    }
    pair.slave.server
        .queue_spontaneous(1, &[(512, AsduTypeId::MSpTb1)])
        .await;

    let event = wait_for_log_event(
        &pair.log,
        |entry| {
            entry.direction == Direction::Tx
                && entry.raw_bytes.is_none()
                && matches!(
                    &entry.frame_label,
                    FrameLabel::IFrame(name) if name == "M_SP_TB_1"
                )
                && entry.detail.contains("突发上送")
        },
        DEFAULT_TIMEOUT,
    )
    .await
    .expect("spontaneous monitoring log");

    assert!(
        event.detail.contains("IOA=512 M_SP_TB_1 val=ON CA=1"),
        "detail should include the signal value: {}",
        event.detail,
    );

    pair.shutdown().await;
}

/// select_ack_cot / execute_ack_cot 可配:把 execute 改成 ActivationCon(7),
/// master 接收应答时 raw_bytes 的 COT 字节(offset 8)应为 7。
#[tokio::test]
async fn command_ack_cot_configurable() {
    let ops = RemoteOperationConfig {
        execute_ack_cot: CommandAckCot::ActivationCon,
        auto_map_commands: true,
        ..Default::default()
    };
    let pair = Pair::spawn(ops).await;

    // 先 GI 填充。
    pair.master.conn.send_interrogation(1).await.unwrap();
    let _ = wait_for_ioa_count(&pair.master.conn, 1, 1, DEFAULT_TIMEOUT).await;
    pair.log.clear().await;

    // 发命令(direct execute)
    pair.master.conn
        .send_single_command(1, true, false, 1, 0, 6)
        .await
        .unwrap();
    sleep(Duration::from_millis(500)).await;

    // 在 Rx 方向 (master 收到 slave 发的应答) 找 C_SC 帧,COT 字节应为 7。
    let entries = pair.log.get_all().await;
    let mut found_cot7 = false;
    for e in entries {
        if e.direction != Direction::Rx {
            continue;
        }
        if !matches!(e.frame_label, iec104sim_core::log_entry::FrameLabel::IFrame(ref s) if s.contains("C_SC")) {
            continue;
        }
        if let Some(bytes) = e.raw_bytes {
            // I-frame:[0x68, len, ns_lo, ns_hi, nr_lo, nr_hi, type, vsq, cot, ...]
            // COT 在 offset 8。
            if bytes.len() > 8 && bytes[8] == 7 {
                found_cot7 = true;
                break;
            }
        }
    }
    assert!(found_cot7, "未在应答帧中找到 COT=7");

    pair.shutdown().await;
}

/// upload_mode_untimestamped=Continuous + auto_packing:连续 IOA 的同类型点合并成单 ASDU。
/// 这里只验证开关被读到(具体打包字节检查在 headless_packed_sq1.rs)。
#[tokio::test]
async fn upload_mode_continuous_flag_propagates() {
    let ops = RemoteOperationConfig {
        upload_mode_untimestamped: UploadMode::Continuous,
        auto_packing: true,
        ..Default::default()
    };
    let pair = Pair::spawn(ops).await;

    // 读回 ops 确认 set_remote_ops 已生效
    let got = pair.slave.server.get_remote_ops().await;
    assert_eq!(got.upload_mode_untimestamped, UploadMode::Continuous);
    assert!(got.auto_packing);

    pair.shutdown().await;
}
