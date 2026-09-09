//! Import and validate per-station JSON point-event schedules.

use crate::commands::parse_asdu_type;
use crate::state::AppState;
use iec104sim_core::data_point::{DataPoint, DataPointValue};
use iec104sim_core::slave::ScheduledPointEvent;
use iec104sim_core::types::{AsduTypeId, DataCategory};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use tauri::State;

const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_EVENTS: usize = 100_000;
const MAX_TIME_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const MAX_IOA: u32 = 0x00ff_ffff;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPointEvent {
    time_ms: u64,
    #[serde(rename = "type")]
    asdu_type: String,
    ioa: u32,
    value: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StepPositionValue {
    value: i8,
    transient: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IntegratedTotalValue {
    value: i32,
    carry: bool,
    sequence: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PointEventSchedulePreview {
    pub event_count: usize,
    pub point_count: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PointEventScheduleStarted {
    pub task_id: String,
    pub event_count: usize,
    pub point_count: usize,
    pub duration_ms: u64,
}

fn value_error(index: usize, message: impl AsRef<str>) -> String {
    format!("事件 {} [value]: {}", index + 1, message.as_ref())
}

fn integer(value: &Value, index: usize) -> Result<i64, String> {
    value
        .as_i64()
        .ok_or_else(|| value_error(index, "必须是整数"))
}

fn parse_event_value(
    index: usize,
    asdu_type: AsduTypeId,
    value: Value,
) -> Result<DataPointValue, String> {
    let parsed = match asdu_type.category() {
        DataCategory::SinglePoint => DataPointValue::SinglePoint {
            value: value
                .as_bool()
                .ok_or_else(|| value_error(index, "单点值必须是 true 或 false"))?,
        },
        DataCategory::DoublePoint => {
            let value = integer(&value, index)?;
            if !(0..=3).contains(&value) {
                return Err(value_error(index, "双点值范围是 0..3"));
            }
            DataPointValue::DoublePoint { value: value as u8 }
        }
        DataCategory::StepPosition => {
            let value: StepPositionValue = serde_json::from_value(value)
                .map_err(|error| value_error(index, format!("步位置格式错误: {error}")))?;
            if !(-64..=63).contains(&value.value) {
                return Err(value_error(index, "步位置值范围是 -64..63"));
            }
            DataPointValue::StepPosition {
                value: value.value,
                transient: value.transient,
            }
        }
        DataCategory::Bitstring => {
            let value = value
                .as_u64()
                .ok_or_else(|| value_error(index, "位串值必须是非负整数"))?;
            if value > u32::MAX as u64 {
                return Err(value_error(index, "位串值范围是 0..4294967295"));
            }
            DataPointValue::Bitstring {
                value: value as u32,
            }
        }
        DataCategory::NormalizedMeasured => {
            let value = integer(&value, index)?;
            if !(i16::MIN as i64..=i16::MAX as i64).contains(&value) {
                return Err(value_error(index, "归一化原始值范围是 -32768..32767"));
            }
            DataPointValue::Normalized {
                value: value as f32 / 32767.0,
            }
        }
        DataCategory::ScaledMeasured => {
            let value = integer(&value, index)?;
            if !(i16::MIN as i64..=i16::MAX as i64).contains(&value) {
                return Err(value_error(index, "标度化值范围是 -32768..32767"));
            }
            DataPointValue::Scaled {
                value: value as i16,
            }
        }
        DataCategory::FloatMeasured => {
            let value = value
                .as_f64()
                .ok_or_else(|| value_error(index, "短浮点值必须是数字"))?;
            let value = value as f32;
            if !value.is_finite() {
                return Err(value_error(index, "短浮点值超出有限 f32 范围"));
            }
            DataPointValue::ShortFloat { value }
        }
        DataCategory::IntegratedTotals => {
            let value: IntegratedTotalValue = serde_json::from_value(value)
                .map_err(|error| value_error(index, format!("累计量格式错误: {error}")))?;
            if value.sequence > 31 {
                return Err(value_error(index, "累计量 sequence 范围是 0..31"));
            }
            DataPointValue::IntegratedTotal {
                value: value.value,
                carry: value.carry,
                sequence: value.sequence,
            }
        }
        DataCategory::SingleCommand
        | DataCategory::DoubleCommand
        | DataCategory::StepCommand
        | DataCategory::BitstringCommand
        | DataCategory::NormalizedSetpoint
        | DataCategory::ScaledSetpoint
        | DataCategory::FloatSetpoint
        | DataCategory::System => {
            return Err(format!(
                "事件 {} [type]: 不支持控制或系统类型 {}",
                index + 1,
                asdu_type.name(),
            ));
        }
    };
    Ok(parsed)
}

fn parse_schedule_bytes(bytes: &[u8]) -> Result<Vec<ScheduledPointEvent>, String> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(format!(
            "JSON 文件不能超过 {} MiB",
            MAX_FILE_BYTES / 1024 / 1024
        ));
    }
    let root: Value =
        serde_json::from_slice(bytes).map_err(|error| format!("JSON 解析失败: {error}"))?;
    let values = root
        .as_array()
        .ok_or_else(|| "JSON 顶层必须是事件数组".to_string())?;
    if values.is_empty() {
        return Err("JSON 事件数组不能为空".to_string());
    }
    if values.len() > MAX_EVENTS {
        return Err(format!("事件数量不能超过 {MAX_EVENTS}"));
    }

    let mut events = Vec::with_capacity(values.len());
    for (index, value) in values.iter().cloned().enumerate() {
        let raw: RawPointEvent = serde_json::from_value(value)
            .map_err(|error| format!("事件 {} 格式错误: {error}", index + 1))?;
        if raw.time_ms > MAX_TIME_MS {
            return Err(format!(
                "事件 {} [time_ms]: 不能超过 {MAX_TIME_MS}",
                index + 1,
            ));
        }
        if raw.ioa > MAX_IOA {
            return Err(format!("事件 {} [ioa]: 范围是 0..{MAX_IOA}", index + 1,));
        }
        let asdu_type = parse_asdu_type(&raw.asdu_type)
            .map_err(|error| format!("事件 {} [type]: {error}", index + 1))?;
        let value = parse_event_value(index, asdu_type, raw.value)?;
        events.push(ScheduledPointEvent {
            time_ms: raw.time_ms,
            ioa: raw.ioa,
            asdu_type,
            value,
        });
    }
    // Stable sorting preserves file order for records with the same time.
    events.sort_by_key(|event| event.time_ms);
    Ok(events)
}

async fn read_schedule(path: String) -> Result<Vec<ScheduledPointEvent>, String> {
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|error| format!("读取事件 JSON 失败 {path:?}: {error}"))?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(format!(
            "JSON 文件不能超过 {} MiB",
            MAX_FILE_BYTES / 1024 / 1024
        ));
    }
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|error| format!("读取事件 JSON 失败 {path:?}: {error}"))?;
    parse_schedule_bytes(&bytes)
}

fn preview(events: &[ScheduledPointEvent]) -> PointEventSchedulePreview {
    let point_count = events
        .iter()
        .map(|event| (event.ioa, event.asdu_type))
        .collect::<HashSet<_>>()
        .len();
    PointEventSchedulePreview {
        event_count: events.len(),
        point_count,
        duration_ms: events.last().map(|event| event.time_ms).unwrap_or(0),
    }
}

#[tauri::command]
pub async fn inspect_point_event_schedule(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    path: String,
) -> Result<PointEventSchedulePreview, String> {
    let events = read_schedule(path).await?;
    let servers = state.servers.read().await;
    let server = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {server_id} not found"))?;
    let resolved_types = server
        .server
        .validate_point_event_schedule(common_address, &events)
        .await
        .map_err(|error| error.to_string())?;
    let mut result = preview(&events);
    // 点位数按解析后的实际目标统计:多个声明类型可能退化到同一个点位。
    result.point_count = events
        .iter()
        .zip(&resolved_types)
        .map(|(event, target_type)| (event.ioa, *target_type))
        .collect::<HashSet<_>>()
        .len();
    Ok(result)
}

#[tauri::command]
pub async fn start_point_event_schedule(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    path: String,
) -> Result<PointEventScheduleStarted, String> {
    let events = read_schedule(path).await?;
    let schedule_preview = preview(&events);
    let servers = state.servers.read().await;
    let server = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {server_id} not found"))?;
    // point_count 用解析后的目标数,与 inspect 的预览口径一致。
    let (task_id, point_count) = server
        .server
        .start_point_event_schedule(common_address, events)
        .await
        .map_err(|error| error.to_string())?;
    Ok(PointEventScheduleStarted {
        task_id,
        event_count: schedule_preview.event_count,
        point_count,
        duration_ms: schedule_preview.duration_ms,
    })
}

fn point_value_json(point: &DataPoint) -> Value {
    match &point.value {
        DataPointValue::SinglePoint { value } => Value::Bool(*value),
        DataPointValue::DoublePoint { value } => Value::from(*value),
        DataPointValue::StepPosition { value, transient } => {
            serde_json::json!({ "value": value, "transient": transient })
        }
        DataPointValue::Bitstring { value } => Value::from(*value),
        DataPointValue::Normalized { value } => Value::from((*value * 32767.0).round() as i16),
        DataPointValue::Scaled { value } => Value::from(*value),
        DataPointValue::ShortFloat { value } => {
            Value::from(if value.is_finite() { *value } else { 0.0 })
        }
        DataPointValue::IntegratedTotal {
            value,
            carry,
            sequence,
        } => serde_json::json!({
            "value": value,
            "carry": carry,
            "sequence": sequence,
        }),
    }
}

fn alternative_point_value(point: &DataPoint) -> DataPointValue {
    match &point.value {
        DataPointValue::SinglePoint { value } => DataPointValue::SinglePoint { value: !value },
        DataPointValue::DoublePoint { value } => DataPointValue::DoublePoint {
            value: if *value == 2 { 1 } else { 2 },
        },
        DataPointValue::StepPosition { value, transient } => DataPointValue::StepPosition {
            value: if *value < 63 { *value + 1 } else { *value - 1 },
            transient: *transient,
        },
        DataPointValue::Bitstring { value } => DataPointValue::Bitstring { value: value ^ 1 },
        DataPointValue::Normalized { value } => {
            let raw = (*value * 32767.0).round() as i16;
            let next = if raw == i16::MAX { raw - 1 } else { raw + 1 };
            DataPointValue::Normalized {
                value: next as f32 / 32767.0,
            }
        }
        DataPointValue::Scaled { value } => DataPointValue::Scaled {
            value: if *value == i16::MAX {
                value - 1
            } else {
                value + 1
            },
        },
        DataPointValue::ShortFloat { value } => DataPointValue::ShortFloat {
            value: if value.is_finite() { value + 1.0 } else { 0.0 },
        },
        DataPointValue::IntegratedTotal {
            value,
            carry,
            sequence,
        } => DataPointValue::IntegratedTotal {
            value: value.saturating_add(1),
            carry: *carry,
            sequence: *sequence,
        },
    }
}

fn example_event(time_ms: u64, point: &DataPoint, value: Value) -> Value {
    serde_json::json!({
        "time_ms": time_ms,
        "type": point.asdu_type.name(),
        "ioa": point.ioa,
        "value": value,
    })
}

#[tauri::command]
pub async fn save_point_event_schedule_example(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    path: String,
) -> Result<usize, String> {
    let examples = {
        let servers = state.servers.read().await;
        let server = servers
            .get(&server_id)
            .ok_or_else(|| format!("server {server_id} not found"))?;
        let stations = server.server.stations.read().await;
        let station = stations
            .get(&common_address)
            .ok_or_else(|| format!("station CA={common_address} not found"))?;
        let mut categories = HashSet::new();
        station
            .data_points
            .all_sorted()
            .into_iter()
            .filter(|point| {
                !point.asdu_type.is_control()
                    && point.asdu_type.category() != DataCategory::System
                    && categories.insert(point.asdu_type.category())
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    let Some(first) = examples.first() else {
        return Err("当前站没有可用于事件示例的监视点".to_string());
    };

    let mut output = vec![
        example_event(0, first, point_value_json(first)),
        example_event(
            0,
            first,
            point_value_json(&DataPoint {
                value: alternative_point_value(first),
                ..first.clone()
            }),
        ),
    ];
    for (index, point) in examples.iter().skip(1).enumerate() {
        output.push(example_event(
            (index as u64 + 1) * 1000,
            point,
            point_value_json(point),
        ));
    }
    let mut bytes = serde_json::to_vec_pretty(&output)
        .map_err(|error| format!("生成事件 JSON 示例失败: {error}"))?;
    bytes.push(b'\n');
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|error| format!("写入事件 JSON 示例失败 {path:?}: {error}"))?;
    Ok(output.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_duplicate_ioa_and_stably_sorts_time() {
        let events = parse_schedule_bytes(
            br#"[
                {"time_ms":100,"type":"M_SP_NA_1","ioa":1,"value":false},
                {"time_ms":0,"type":"M_SP_NA_1","ioa":1,"value":true},
                {"time_ms":0,"type":"M_SP_NA_1","ioa":1,"value":false}
            ]"#,
        )
        .unwrap();
        assert_eq!(
            events.iter().map(|event| event.time_ms).collect::<Vec<_>>(),
            vec![0, 0, 100]
        );
        assert!(matches!(
            events[0].value,
            DataPointValue::SinglePoint { value: true }
        ));
        assert!(matches!(
            events[1].value,
            DataPointValue::SinglePoint { value: false }
        ));
    }

    #[test]
    fn parses_all_monitor_value_shapes() {
        let events = parse_schedule_bytes(
            br#"[
                {"time_ms":0,"type":"M_SP_NA_1","ioa":1,"value":true},
                {"time_ms":0,"type":"M_DP_NA_1","ioa":1,"value":3},
                {"time_ms":0,"type":"M_ST_NA_1","ioa":1,"value":{"value":-64,"transient":true}},
                {"time_ms":0,"type":"M_BO_NA_1","ioa":1,"value":4294967295},
                {"time_ms":0,"type":"M_ME_NA_1","ioa":1,"value":-32768},
                {"time_ms":0,"type":"M_ME_NB_1","ioa":1,"value":32767},
                {"time_ms":0,"type":"M_ME_NC_1","ioa":1,"value":1.25},
                {"time_ms":0,"type":"M_IT_NA_1","ioa":1,"value":{"value":7,"carry":true,"sequence":31}}
            ]"#,
        )
        .unwrap();
        assert_eq!(events.len(), 8);
    }

    #[test]
    fn rejects_unknown_fields_and_control_types() {
        let unknown = parse_schedule_bytes(
            br#"[{"time_ms":0,"type":"M_SP_NA_1","ioa":1,"value":true,"typo":1}]"#,
        )
        .unwrap_err();
        assert!(unknown.contains("unknown field"));

        let control =
            parse_schedule_bytes(br#"[{"time_ms":0,"type":"C_SC_NA_1","ioa":1,"value":true}]"#)
                .unwrap_err();
        assert!(control.contains("不支持控制或系统类型"));
    }

    #[test]
    fn rejects_value_and_schedule_limits() {
        assert!(parse_schedule_bytes(b"[]")
            .unwrap_err()
            .contains("不能为空"));
        assert!(
            parse_schedule_bytes(&vec![b' '; MAX_FILE_BYTES as usize + 1])
                .unwrap_err()
                .contains("20 MiB")
        );
        let too_many = serde_json::to_vec(&vec![Value::Null; MAX_EVENTS + 1]).unwrap();
        assert!(parse_schedule_bytes(&too_many)
            .unwrap_err()
            .contains("事件数量"));
        assert!(parse_schedule_bytes(
            br#"[{"time_ms":604800001,"type":"M_SP_NA_1","ioa":1,"value":true}]"#,
        )
        .unwrap_err()
        .contains("time_ms"));
        assert!(parse_schedule_bytes(
            br#"[{"time_ms":0,"type":"M_IT_NA_1","ioa":1,"value":{"value":7,"carry":false,"sequence":32}}]"#,
        )
        .unwrap_err()
        .contains("sequence"));
    }
}
