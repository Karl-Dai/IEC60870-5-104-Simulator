use crate::types::{AsduTypeId, DataCategory, QualityFlags};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Runtime value of a data point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DataPointValue {
    /// Single-point: true=ON, false=OFF
    SinglePoint { value: bool },
    /// Double-point: 0=intermediate, 1=off, 2=on, 3=indeterminate
    DoublePoint { value: u8 },
    /// Step position: -64..+63, with transient flag
    StepPosition { value: i8, transient: bool },
    /// 32-bit bitstring
    Bitstring { value: u32 },
    /// Normalized float: -1.0 .. +1.0
    Normalized { value: f32 },
    /// Scaled integer: -32768 .. +32767
    Scaled { value: i16 },
    /// Short floating point (IEEE 754)
    ShortFloat { value: f32 },
    /// Integrated total (counter)
    IntegratedTotal { value: i32, carry: bool, sequence: u8 },
}

impl DataPointValue {
    /// Create a default value for the given ASDU type.
    pub fn default_for(asdu_type: AsduTypeId) -> Self {
        match asdu_type.category() {
            DataCategory::SinglePoint => Self::SinglePoint { value: false },
            DataCategory::DoublePoint => Self::DoublePoint { value: 1 }, // OFF
            DataCategory::StepPosition => Self::StepPosition { value: 0, transient: false },
            DataCategory::Bitstring => Self::Bitstring { value: 0 },
            DataCategory::NormalizedMeasured => Self::Normalized { value: 0.0 },
            DataCategory::ScaledMeasured => Self::Scaled { value: 0 },
            DataCategory::FloatMeasured => Self::ShortFloat { value: 0.0 },
            DataCategory::IntegratedTotals => Self::IntegratedTotal { value: 0, carry: false, sequence: 0 },
            DataCategory::System => Self::SinglePoint { value: false },
        }
    }

    /// Format value as display string.
    pub fn display(&self) -> String {
        match self {
            Self::SinglePoint { value } => if *value { "ON".to_string() } else { "OFF".to_string() },
            Self::DoublePoint { value } => match value {
                0 => "中间".to_string(),
                1 => "OFF".to_string(),
                2 => "ON".to_string(),
                3 => "不确定".to_string(),
                _ => format!("{}", value),
            },
            Self::StepPosition { value, transient } => {
                if *transient { format!("{} (T)", value) } else { format!("{}", value) }
            }
            Self::Bitstring { value } => format!("0x{:08X}", value),
            Self::Normalized { value } => format!("{:.4}", value),
            Self::Scaled { value } => format!("{}", value),
            Self::ShortFloat { value } => format!("{:.6}", value),
            Self::IntegratedTotal { value, carry, sequence } => {
                let mut s = format!("{}", value);
                if *carry { s.push_str(" [C]"); }
                if *sequence > 0 { s.push_str(&format!(" S{}", sequence)); }
                s
            }
        }
    }
}

/// Definition of an information object (analogous to RegisterDef).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InformationObjectDef {
    /// Information Object Address (0..16777215)
    pub ioa: u32,
    /// ASDU type for this point
    pub asdu_type: AsduTypeId,
    /// Data category (derived from asdu_type)
    pub category: DataCategory,
    /// User-defined name
    #[serde(default)]
    pub name: String,
    /// User-defined comment
    #[serde(default)]
    pub comment: String,
}

/// A single data point with current value, quality, and timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    pub ioa: u32,
    pub asdu_type: AsduTypeId,
    pub value: DataPointValue,
    pub quality: QualityFlags,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
    /// Sequence number stamped on insert/update (for incremental queries).
    #[serde(default)]
    pub update_seq: u64,
}

impl DataPoint {
    pub fn new(ioa: u32, asdu_type: AsduTypeId) -> Self {
        Self {
            ioa,
            asdu_type,
            value: DataPointValue::default_for(asdu_type),
            quality: QualityFlags::good(),
            timestamp: Some(Utc::now()),
            update_seq: 0,
        }
    }

    pub fn with_value(ioa: u32, asdu_type: AsduTypeId, value: DataPointValue) -> Self {
        Self {
            ioa,
            asdu_type,
            value,
            quality: QualityFlags::good(),
            timestamp: Some(Utc::now()),
            update_seq: 0,
        }
    }
}

/// Storage for all data points in a station.
/// Keyed by (IOA, AsduTypeId) so the same IOA can hold different ASDU types.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DataPointMap {
    /// Data points keyed by (IOA, AsduTypeId).
    pub points: HashMap<(u32, AsduTypeId), DataPoint>,
    /// Monotonically increasing sequence counter, stamped on each insert.
    seq_counter: u64,
}

impl DataPointMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, ioa: u32, asdu_type: AsduTypeId) -> Option<&DataPoint> {
        self.points.get(&(ioa, asdu_type))
    }

    pub fn get_mut(&mut self, ioa: u32, asdu_type: AsduTypeId) -> Option<&mut DataPoint> {
        self.points.get_mut(&(ioa, asdu_type))
    }

    /// Find a point at the given IOA whose category matches. Prefers the
    /// untimestamped variant (NA) so control commands have a stable target
    /// when both NA and TB exist for the same IOA.
    pub fn get_by_category(&self, ioa: u32, category: DataCategory) -> Option<&DataPoint> {
        let preferred = preferred_na_for(category);
        if let Some(t) = preferred {
            if let Some(p) = self.points.get(&(ioa, t)) {
                return Some(p);
            }
        }
        self.points.values().find(|p| p.ioa == ioa && p.asdu_type.category() == category)
    }

    /// Find a point (mutable) at the given IOA whose category matches.
    /// Prefers the untimestamped variant (NA) — see `get_by_category`.
    pub fn get_mut_by_category(&mut self, ioa: u32, category: DataCategory) -> Option<&mut DataPoint> {
        let preferred = preferred_na_for(category);
        if let Some(t) = preferred {
            if self.points.contains_key(&(ioa, t)) {
                return self.points.get_mut(&(ioa, t));
            }
        }
        self.points.values_mut().find(|p| p.ioa == ioa && p.asdu_type.category() == category)
    }

    pub fn insert(&mut self, mut point: DataPoint) {
        self.seq_counter += 1;
        point.update_seq = self.seq_counter;
        self.points.insert((point.ioa, point.asdu_type), point);
    }

    pub fn remove(&mut self, ioa: u32, asdu_type: AsduTypeId) -> Option<DataPoint> {
        self.points.remove(&(ioa, asdu_type))
    }

    pub fn contains(&self, ioa: u32, asdu_type: AsduTypeId) -> bool {
        self.points.contains_key(&(ioa, asdu_type))
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Current sequence counter value.
    pub fn current_seq(&self) -> u64 {
        self.seq_counter
    }

    /// Bump the change counter and stamp an exact point, so incremental
    /// queries (`changed_since`) observe value mutations — not only inserts.
    /// A control command or manual edit changes `value` in place via
    /// `get_mut`, which does not touch `update_seq`; callers must follow such
    /// a mutation with `mark_changed`.
    pub fn mark_changed(&mut self, ioa: u32, asdu_type: AsduTypeId) {
        self.seq_counter += 1;
        let seq = self.seq_counter;
        if let Some(p) = self.points.get_mut(&(ioa, asdu_type)) {
            p.update_seq = seq;
        }
    }

    /// Like `mark_changed` but resolves the point by category, mirroring
    /// `get_mut_by_category` — used by control handlers that target a category.
    pub fn mark_changed_by_category(&mut self, ioa: u32, category: DataCategory) {
        self.seq_counter += 1;
        let seq = self.seq_counter;
        if let Some(p) = self.get_mut_by_category(ioa, category) {
            p.update_seq = seq;
        }
    }

    /// Get points changed since the given sequence number, sorted by IOA.
    pub fn changed_since(&self, seq: u64) -> Vec<&DataPoint> {
        let mut pts: Vec<&DataPoint> = self.points.values()
            .filter(|p| p.update_seq > seq)
            .collect();
        pts.sort_by_key(|p| p.ioa);
        pts
    }

    /// Get all points for a given data category, sorted by IOA.
    pub fn by_category(&self, category: DataCategory) -> Vec<&DataPoint> {
        let mut pts: Vec<&DataPoint> = self.points.values()
            .filter(|p| p.asdu_type.category() == category)
            .collect();
        pts.sort_by_key(|p| p.ioa);
        pts
    }

    /// Get all points sorted by IOA.
    pub fn all_sorted(&self) -> Vec<&DataPoint> {
        let mut pts: Vec<&DataPoint> = self.points.values().collect();
        pts.sort_by_key(|p| p.ioa);
        pts
    }
}

/// Map a category to its untimestamped (NA) ASDU representative. Used by
/// `get_by_category` / `get_mut_by_category` to give control commands a
/// stable target when both NA and TB variants exist for the same IOA.
fn preferred_na_for(category: DataCategory) -> Option<AsduTypeId> {
    match category {
        DataCategory::SinglePoint => Some(AsduTypeId::MSpNa1),
        DataCategory::DoublePoint => Some(AsduTypeId::MDpNa1),
        DataCategory::StepPosition => Some(AsduTypeId::MStNa1),
        DataCategory::Bitstring => Some(AsduTypeId::MBoNa1),
        DataCategory::NormalizedMeasured => Some(AsduTypeId::MMeNa1),
        DataCategory::ScaledMeasured => Some(AsduTypeId::MMeNb1),
        DataCategory::FloatMeasured => Some(AsduTypeId::MMeNc1),
        DataCategory::IntegratedTotals => Some(AsduTypeId::MItNa1),
        DataCategory::System => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_point_creation() {
        let dp = DataPoint::new(100, AsduTypeId::MSpNa1);
        assert_eq!(dp.ioa, 100);
        assert!(matches!(dp.value, DataPointValue::SinglePoint { value: false }));
        assert!(!dp.quality.iv);
    }

    #[test]
    fn test_data_point_value_display() {
        assert_eq!(DataPointValue::SinglePoint { value: true }.display(), "ON");
        assert_eq!(DataPointValue::SinglePoint { value: false }.display(), "OFF");
        assert_eq!(DataPointValue::DoublePoint { value: 2 }.display(), "ON");
        assert_eq!(DataPointValue::ShortFloat { value: 25.125 }.display(), "25.125000");
        assert_eq!(DataPointValue::Bitstring { value: 0xFF00 }.display(), "0x0000FF00");
    }

    #[test]
    fn test_data_point_map() {
        let mut map = DataPointMap::new();
        assert!(map.is_empty());

        map.insert(DataPoint::new(100, AsduTypeId::MSpNa1));
        map.insert(DataPoint::new(200, AsduTypeId::MMeNc1));
        map.insert(DataPoint::new(300, AsduTypeId::MSpNa1));

        assert_eq!(map.len(), 3);
        assert!(map.contains(100, AsduTypeId::MSpNa1));
        assert!(!map.contains(999, AsduTypeId::MSpNa1));

        let sp = map.by_category(DataCategory::SinglePoint);
        assert_eq!(sp.len(), 2);
        assert_eq!(sp[0].ioa, 100);
        assert_eq!(sp[1].ioa, 300);

        // Same IOA, different type — should coexist
        map.insert(DataPoint::new(100, AsduTypeId::MMeNc1));
        assert_eq!(map.len(), 4);
        assert!(map.contains(100, AsduTypeId::MSpNa1));
        assert!(map.contains(100, AsduTypeId::MMeNc1));
    }

    #[test]
    fn test_batch_remove_points_idempotent() {
        use crate::slave::Station;
        let mut st = Station::new(1, "");
        st.add_point(InformationObjectDef {
            ioa: 10, asdu_type: AsduTypeId::MSpNa1,
            category: DataCategory::SinglePoint, name: String::new(), comment: String::new(),
        }).unwrap();
        st.add_point(InformationObjectDef {
            ioa: 11, asdu_type: AsduTypeId::MSpNa1,
            category: DataCategory::SinglePoint, name: String::new(), comment: String::new(),
        }).unwrap();
        st.add_point(InformationObjectDef {
            ioa: 12, asdu_type: AsduTypeId::MMeNc1,
            category: DataCategory::FloatMeasured, name: String::new(), comment: String::new(),
        }).unwrap();
        assert_eq!(st.data_points.len(), 3);

        // 删除 10 + 12, 外加一个不存在的 (99) -> 跳过, 仅删 2 个
        let removed = st.remove_points(&[
            (10, AsduTypeId::MSpNa1),
            (12, AsduTypeId::MMeNc1),
            (99, AsduTypeId::MSpNa1),
        ]);
        assert_eq!(removed, 2);
        assert_eq!(st.data_points.len(), 1);
        assert!(st.data_points.contains(11, AsduTypeId::MSpNa1));
        // object_defs 同步剪除
        assert_eq!(st.object_defs.len(), 1);
        assert_eq!(st.object_defs[0].ioa, 11);
    }

    #[test]
    fn test_mark_changed_drives_incremental() {
        let mut map = DataPointMap::new();
        map.insert(DataPoint::new(1, AsduTypeId::MSpNa1));
        map.insert(DataPoint::new(2, AsduTypeId::MSpNa1));
        let baseline = map.current_seq();
        assert!(map.changed_since(baseline).is_empty());

        // A bare get_mut value write must NOT advance the counter.
        if let Some(p) = map.get_mut(1, AsduTypeId::MSpNa1) {
            p.value = DataPointValue::SinglePoint { value: true };
        }
        assert!(
            map.changed_since(baseline).is_empty(),
            "get_mut alone must not be visible to changed_since"
        );

        // mark_changed stamps the point so incremental queries observe it.
        map.mark_changed(1, AsduTypeId::MSpNa1);
        let changed = map.changed_since(baseline);
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].ioa, 1);

        // mark_changed_by_category resolves the point by category.
        map.mark_changed_by_category(2, DataCategory::SinglePoint);
        assert_eq!(map.changed_since(baseline).len(), 2);
    }
}
