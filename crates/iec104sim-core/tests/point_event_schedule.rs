//! End-to-end coverage for imported event scheduling and SQ=0 duplicate IOAs.

mod common;

use common::harness::Pair;
use common::helpers::collect_iframe_bytes;
use iec104sim_core::data_point::DataPointValue;
use iec104sim_core::log_entry::Direction;
use iec104sim_core::slave::{RemoteOperationConfig, ScheduledPointEvent};
use iec104sim_core::types::AsduTypeId;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn duplicate_ioa_values_share_one_sq0_asdu_and_master_keeps_last_value() {
    let pair = Pair::spawn_with(RemoteOperationConfig::default(), 1).await;
    for _ in 0..40 {
        if pair.slave.server.active_client_connection_count().await > 0 {
            break;
        }
        sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(pair.slave.server.active_client_connection_count().await, 1);
    pair.log.clear().await;

    pair.slave
        .server
        .start_point_event_schedule(
            1,
            vec![
                ScheduledPointEvent {
                    time_ms: 0,
                    ioa: 1,
                    asdu_type: AsduTypeId::MSpNa1,
                    value: DataPointValue::SinglePoint { value: false },
                },
                ScheduledPointEvent {
                    time_ms: 0,
                    ioa: 1,
                    asdu_type: AsduTypeId::MSpNa1,
                    value: DataPointValue::SinglePoint { value: true },
                },
            ],
        )
        .await
        .unwrap();
    sleep(Duration::from_millis(300)).await;

    let frames = collect_iframe_bytes(&pair.log, Direction::Rx).await;
    let frame = frames
        .iter()
        .find(|frame| {
            frame.len() >= 20
                && frame[6] == AsduTypeId::MSpNa1 as u8
                && frame[7] == 2
                && frame[8] == 3
        })
        .expect("master should receive one SQ=0 ASDU with two objects");
    assert_eq!(&frame[12..15], &[1, 0, 0]);
    assert_eq!(frame[15] & 1, 0);
    assert_eq!(&frame[16..19], &[1, 0, 0]);
    assert_eq!(frame[19] & 1, 1);

    let data = pair.master.conn.received_data.read().await;
    let point = data
        .ca_map(1)
        .unwrap()
        .get(1, AsduTypeId::MSpNa1)
        .unwrap();
    assert!(matches!(
        point.value,
        DataPointValue::SinglePoint { value: true }
    ));
    drop(data);

    pair.shutdown().await;
}
