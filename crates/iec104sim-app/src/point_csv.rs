//! Per-station point configuration CSV import/export.
//!
//! JSON remains the authoritative whole-application configuration format. CSV
//! is deliberately scoped to the station selected in the UI and includes the
//! station's runtime point-mutation settings so a spreadsheet round trip does
//! not silently turn simulations off.

use crate::commands::{
    parse_asdu_type, persist_workspace_after, validate_control_point_options,
};
use crate::state::AppState;
use csv::{QuoteStyle, ReaderBuilder, StringRecord, Terminator, Trim, WriterBuilder};
use iec104sim_core::data_point::{ControlTarget, DataPoint, InformationObjectDef};
use iec104sim_core::slave::{
    MutationMode, MutationParams, PointMutationConfig, PointMutationUpdate, ServerState,
    SlaveServer, Station, StationMutationBatchMode,
};
use iec104sim_core::types::{AsduTypeId, DataCategory};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tauri::{AppHandle, State};

const IOA_MAX: u32 = 0x00FF_FFFF;
const PERIOD_MIN_MS: u32 = 50;
const PERIOD_MAX_MS: u32 = 60_000;
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";
// Defense-in-depth wire prefix for spreadsheet-bound free text. OWASP's
// Excel-resistant guidance uses a TAB inside a quoted cell; U+2060 makes the
// prefix uniquely reversible without adding visible text. No CSV encoding is
// universally safe across every spreadsheet application.
const EXCEL_TEXT_GUARD: &str = "\t\u{2060}";

pub(crate) const POINT_CSV_HEADERS: [&str; 15] = [
    "IOA",
    "ASDU Type",
    "Type ID",
    "Name",
    "Comment",
    "QOC/QL",
    "S/E Mode",
    "Mapped CA",
    "Mapped IOA",
    "Mapped Type ID",
    "Sim Mode",
    "Period",
    "Step",
    "Min",
    "Max",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportMode {
    Replace,
    Merge,
}

impl ImportMode {
    fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "replace" => Ok(Self::Replace),
            "merge" => Ok(Self::Merge),
            _ => Err(format!(
                "invalid CSV import mode {raw:?}; expected \"replace\" or \"merge\""
            )),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct CsvMutation {
    params: MutationParams,
    period_ms: u32,
}

#[derive(Debug, Clone)]
struct ParsedPointRow {
    row: usize,
    definition: InformationObjectDef,
    mutation: Option<CsvMutation>,
}

#[derive(Debug, Clone, Copy)]
struct ProjectedPointCandidate {
    row: usize,
    key: Option<(u32, AsduTypeId)>,
    mapping: Option<ControlTarget>,
}

#[derive(Debug)]
struct ParsedPointCsv {
    rows: Vec<ParsedPointRow>,
    candidates: Vec<ProjectedPointCandidate>,
    errors: Vec<ValidationError>,
}

#[derive(Debug, Clone)]
struct ValidationError {
    row: Option<usize>,
    field: String,
    message: String,
}

impl ValidationError {
    fn row(row: usize, field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            row: Some(row),
            field: field.into(),
            message: message.into(),
        }
    }

    fn file(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            row: None,
            field: field.into(),
            message: message.into(),
        }
    }
}

fn format_validation_errors(errors: &[ValidationError]) -> String {
    let mut message = format!("CSV validation failed ({} error(s)):", errors.len());
    for error in errors {
        message.push('\n');
        match error.row {
            Some(row) => {
                message.push_str(&format!("Row {row} [{}]: {}", error.field, error.message))
            }
            None => message.push_str(&format!("File [{}]: {}", error.field, error.message)),
        }
    }
    message
}

fn has_excel_formula_prefix(value: &str) -> bool {
    if matches!(value.chars().next(), Some('\t' | '\r' | '\n')) {
        return true;
    }
    matches!(
        value.chars().find(|character| !character.is_whitespace()),
        Some('=' | '+' | '-' | '@' | '＝' | '＋' | '－' | '＠')
    )
}

/// Encode free text so Excel/Sheets cannot interpret it as a formula while a
/// CSV round trip remains lossless.
///
/// Dangerous text is prefixed with TAB + U+2060 and every exported CSV cell is
/// quoted. A value already beginning with that guard is wrapped again, so
/// decoding exactly one layer preserves the original bytes. Leading apostrophe
/// values are guarded too because spreadsheet editors can otherwise consume the
/// apostrophe as a display-only text marker during a save/reopen cycle.
fn encode_excel_safe_text(value: &str) -> String {
    if has_excel_formula_prefix(value)
        || value.starts_with('\'')
        || value.starts_with(EXCEL_TEXT_GUARD)
    {
        format!("{EXCEL_TEXT_GUARD}{value}")
    } else {
        value.to_string()
    }
}

fn decode_excel_safe_text(value: &str) -> String {
    value
        .strip_prefix(EXCEL_TEXT_GUARD)
        .unwrap_or(value)
        .to_string()
}

fn record_field(record: &StringRecord, index: usize) -> &str {
    record.get(index).unwrap_or("")
}

fn parse_required_u32(
    raw: &str,
    row: usize,
    field: &'static str,
    min: u32,
    max: u32,
    errors: &mut Vec<ValidationError>,
) -> Option<u32> {
    let value = match raw.trim().parse::<u32>() {
        Ok(value) => value,
        Err(_) => {
            errors.push(ValidationError::row(
                row,
                field,
                format!("expected an integer in {min}..={max}, got {raw:?}"),
            ));
            return None;
        }
    };
    if !(min..=max).contains(&value) {
        errors.push(ValidationError::row(
            row,
            field,
            format!("must be in {min}..={max}, got {value}"),
        ));
        return None;
    }
    Some(value)
}

fn parse_optional_u8(
    raw: &str,
    row: usize,
    field: &'static str,
    errors: &mut Vec<ValidationError>,
) -> Option<u8> {
    if raw.trim().is_empty() {
        return None;
    }
    match raw.trim().parse::<u8>() {
        Ok(value) => Some(value),
        Err(_) => {
            errors.push(ValidationError::row(
                row,
                field,
                format!("expected an integer in 0..=255 or blank, got {raw:?}"),
            ));
            None
        }
    }
}

fn parse_optional_f64(
    raw: &str,
    row: usize,
    field: &'static str,
    errors: &mut Vec<ValidationError>,
) -> Option<f64> {
    if raw.trim().is_empty() {
        return None;
    }
    match raw.trim().parse::<f64>() {
        Ok(value) if value.is_finite() => Some(value),
        _ => {
            errors.push(ValidationError::row(
                row,
                field,
                format!("expected a finite number or blank, got {raw:?}"),
            ));
            None
        }
    }
}

fn parse_select_mode(raw: &str, row: usize, errors: &mut Vec<ValidationError>) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "any" => None,
        "direct" | "execute" | "false" | "0" => Some(false),
        "select" | "sbo" | "true" | "1" => Some(true),
        _ => {
            errors.push(ValidationError::row(
                row,
                "S/E Mode",
                format!(
                    "expected blank/any, direct, or select (boolean aliases are accepted), got {raw:?}"
                ),
            ));
            None
        }
    }
}

fn parse_type_id(
    raw: &str,
    row: usize,
    field: &'static str,
    allow_control: bool,
    errors: &mut Vec<ValidationError>,
) -> Option<AsduTypeId> {
    let value = match raw.trim().parse::<u8>() {
        Ok(value) => value,
        Err(_) => {
            errors.push(ValidationError::row(
                row,
                field,
                format!("expected a recognized numeric IEC type ID, got {raw:?}"),
            ));
            return None;
        }
    };
    let Some(asdu_type) = AsduTypeId::from_u8(value) else {
        errors.push(ValidationError::row(
            row,
            field,
            format!("unrecognized IEC type ID {value}"),
        ));
        return None;
    };
    if asdu_type.category() == DataCategory::System {
        errors.push(ValidationError::row(
            row,
            field,
            format!("system type ID {value} cannot be configured as a station point"),
        ));
        return None;
    }
    if !allow_control && asdu_type.is_control() {
        errors.push(ValidationError::row(
            row,
            field,
            format!(
                "mapping target type {} is not monitor-direction",
                asdu_type.name()
            ),
        ));
        return None;
    }
    Some(asdu_type)
}

fn parse_mapping(
    record: &StringRecord,
    row: usize,
    source_type: Option<AsduTypeId>,
    errors: &mut Vec<ValidationError>,
) -> Option<ControlTarget> {
    let ca_raw = record_field(record, 7);
    let ioa_raw = record_field(record, 8);
    let type_raw = record_field(record, 9);
    let populated = [ca_raw, ioa_raw, type_raw]
        .iter()
        .filter(|value| !value.trim().is_empty())
        .count();
    if populated == 0 {
        return None;
    }
    if populated != 3 {
        errors.push(ValidationError::row(
            row,
            "Mapped CA/Mapped IOA/Mapped Type ID",
            "all three mapping fields must be populated together",
        ));
        return None;
    }

    let ca = parse_required_u32(ca_raw, row, "Mapped CA", 1, 65_534, errors)
        .and_then(|value| u16::try_from(value).ok());
    let ioa = parse_required_u32(ioa_raw, row, "Mapped IOA", 0, IOA_MAX, errors);
    let target_type = parse_type_id(type_raw, row, "Mapped Type ID", false, errors);

    if let Some(source_type) = source_type {
        if !source_type.is_control() {
            errors.push(ValidationError::row(
                row,
                "Mapped CA/Mapped IOA/Mapped Type ID",
                "only control-direction points can define a mapping",
            ));
        } else if let Some(target_type) = target_type {
            if !source_type
                .allowed_target_categories()
                .contains(&target_type.category())
            {
                errors.push(ValidationError::row(
                    row,
                    "Mapped Type ID",
                    format!(
                        "{} cannot map to {}",
                        source_type.name(),
                        target_type.name()
                    ),
                ));
            }
        }
    }

    match (ca, ioa, target_type) {
        (Some(common_address), Some(ioa), Some(asdu_type)) => Some(ControlTarget {
            common_address,
            ioa,
            asdu_type,
        }),
        _ => None,
    }
}

fn numeric_mutation_limits(category: DataCategory) -> Option<(f64, f64)> {
    match category {
        DataCategory::NormalizedMeasured => Some((-1.0, 1.0)),
        DataCategory::ScaledMeasured => Some((i16::MIN as f64, i16::MAX as f64)),
        DataCategory::FloatMeasured => Some((f32::MIN as f64, f32::MAX as f64)),
        DataCategory::IntegratedTotals => Some((i32::MIN as f64, i32::MAX as f64)),
        _ => None,
    }
}

fn parse_mutation(
    record: &StringRecord,
    row: usize,
    asdu_type: Option<AsduTypeId>,
    errors: &mut Vec<ValidationError>,
) -> Option<CsvMutation> {
    let mode_raw = record_field(record, 10);
    let period_raw = record_field(record, 11);
    let step_raw = record_field(record, 12);
    let min_raw = record_field(record, 13);
    let max_raw = record_field(record, 14);
    let mode_key = mode_raw.trim().to_ascii_lowercase();

    if matches!(mode_key.as_str(), "" | "off" | "none") {
        if [period_raw, step_raw, min_raw, max_raw]
            .iter()
            .any(|value| !value.trim().is_empty())
        {
            errors.push(ValidationError::row(
                row,
                "Sim Mode",
                "Period/Step/Min/Max must be blank when simulation is off",
            ));
        }
        return None;
    }

    let mode = match mode_key.as_str() {
        "flip" => MutationMode::Flip,
        "increment" => MutationMode::Increment,
        "decrement" => MutationMode::Decrement,
        "random" => MutationMode::Random,
        _ => {
            errors.push(ValidationError::row(
                row,
                "Sim Mode",
                format!("expected off, flip, increment, decrement, or random, got {mode_raw:?}"),
            ));
            return None;
        }
    };

    let period_ms = parse_required_u32(
        period_raw,
        row,
        "Period",
        PERIOD_MIN_MS,
        PERIOD_MAX_MS,
        errors,
    );
    let step = parse_optional_f64(step_raw, row, "Step", errors).unwrap_or(0.0);
    let min = parse_optional_f64(min_raw, row, "Min", errors).unwrap_or(0.0);
    let max = parse_optional_f64(max_raw, row, "Max", errors).unwrap_or(0.0);

    if step < 0.0 {
        errors.push(ValidationError::row(row, "Step", "must be non-negative"));
    }
    if min > max {
        errors.push(ValidationError::row(
            row,
            "Min/Max",
            format!("Min ({min}) must not exceed Max ({max})"),
        ));
    }

    if let Some(asdu_type) = asdu_type {
        let limits = numeric_mutation_limits(asdu_type.category());
        if mode != MutationMode::Flip && limits.is_none() {
            errors.push(ValidationError::row(
                row,
                "Sim Mode",
                format!("{} supports only flip simulation", asdu_type.name()),
            ));
        }
        if matches!(mode, MutationMode::Increment | MutationMode::Decrement)
            && (step_raw.trim().is_empty() || step == 0.0)
        {
            errors.push(ValidationError::row(
                row,
                "Step",
                "a non-zero Step is required for increment/decrement simulation",
            ));
        }
        if mode != MutationMode::Flip && (min_raw.trim().is_empty() || max_raw.trim().is_empty()) {
            errors.push(ValidationError::row(
                row,
                "Min/Max",
                "Min and Max are required for increment/decrement/random simulation",
            ));
        }
        if let Some((low, high)) = limits {
            if min < low || min > high {
                errors.push(ValidationError::row(
                    row,
                    "Min",
                    format!("must be in {low}..={high} for {}", asdu_type.name()),
                ));
            }
            if max < low || max > high {
                errors.push(ValidationError::row(
                    row,
                    "Max",
                    format!("must be in {low}..={high} for {}", asdu_type.name()),
                ));
            }
        }
    }

    period_ms.map(|period_ms| CsvMutation {
        params: MutationParams {
            mode,
            step,
            min,
            max,
        },
        period_ms,
    })
}

fn parse_point_record(
    record: &StringRecord,
    row: usize,
    errors: &mut Vec<ValidationError>,
) -> (Option<ParsedPointRow>, ProjectedPointCandidate) {
    let error_start = errors.len();
    let ioa = parse_required_u32(record_field(record, 0), row, "IOA", 0, IOA_MAX, errors);

    let asdu_name_raw = record_field(record, 1);
    let type_from_name = match parse_asdu_type(asdu_name_raw.trim()) {
        Ok(asdu_type) => Some(asdu_type),
        Err(_) => {
            errors.push(ValidationError::row(
                row,
                "ASDU Type",
                format!("unrecognized station point type {asdu_name_raw:?}"),
            ));
            None
        }
    };
    let type_from_id = parse_type_id(record_field(record, 2), row, "Type ID", true, errors);
    if let (Some(name_type), Some(id_type)) = (type_from_name, type_from_id) {
        if name_type != id_type {
            errors.push(ValidationError::row(
                row,
                "ASDU Type/Type ID",
                format!(
                    "{} is Type ID {}, but the row declares {}",
                    name_type.name(),
                    name_type as u8,
                    id_type as u8
                ),
            ));
        }
    }
    let asdu_type = type_from_name.filter(|name_type| Some(*name_type) == type_from_id);

    let qualifier = parse_optional_u8(record_field(record, 5), row, "QOC/QL", errors);
    let select_before_operate = parse_select_mode(record_field(record, 6), row, errors);
    if let Some(asdu_type) = asdu_type {
        if let Err(message) =
            validate_control_point_options(asdu_type, qualifier, select_before_operate)
        {
            errors.push(ValidationError::row(row, "QOC/QL or S/E Mode", message));
        }
    }

    let mapping = parse_mapping(record, row, asdu_type, errors);
    let mutation = parse_mutation(record, row, asdu_type, errors);
    // Duplicate-key validation is independent from the rest of the row. A
    // valid IOA/type declaration still participates when (for example) Period
    // or QOC is invalid, so users receive the full report in one import attempt.
    let declared_key = ioa.zip(asdu_type);
    let candidate = ProjectedPointCandidate {
        row,
        key: declared_key,
        mapping,
    };

    if errors.len() != error_start {
        return (None, candidate);
    }
    (
        Some(ParsedPointRow {
            row,
            definition: InformationObjectDef {
                ioa: ioa.expect("validated IOA must be present"),
                asdu_type: asdu_type.expect("validated ASDU type must be present"),
                category: asdu_type
                    .expect("validated ASDU type must be present")
                    .category(),
                name: decode_excel_safe_text(record_field(record, 3)),
                comment: decode_excel_safe_text(record_field(record, 4)),
                mapping,
                command_qualifier: qualifier,
                select_before_operate,
            },
            mutation,
        }),
        candidate,
    )
}

fn parse_point_csv_outcome(bytes: &[u8]) -> Result<ParsedPointCsv, String> {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return Err(
            "CSV encoding error: the file is UTF-16. Save it as UTF-8 (UTF-8 with BOM is supported) and retry."
                .to_string(),
        );
    }
    if bytes.contains(&0) {
        return Err(
            "CSV encoding error: the file contains NUL bytes and appears to be UTF-16. Save it as UTF-8 and retry."
                .to_string(),
        );
    }
    let text = std::str::from_utf8(bytes).map_err(|error| {
        format!(
            "CSV encoding error: file is not valid UTF-8 (invalid byte sequence at byte {}). Save the file as UTF-8 and retry.",
            error.valid_up_to()
        )
    })?;
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .flexible(false)
        .trim(Trim::Headers)
        .from_reader(text.as_bytes());

    let headers = reader.headers().map_err(|error| {
        format_validation_errors(&[ValidationError::file("Header", error.to_string())])
    })?;
    if !headers.iter().eq(POINT_CSV_HEADERS.iter().copied()) {
        return Err(format_validation_errors(&[ValidationError::file(
            "Header",
            format!(
                "expected exactly {}, got {}",
                POINT_CSV_HEADERS.join(", "),
                headers.iter().collect::<Vec<_>>().join(", ")
            ),
        )]));
    }

    let mut parsed = Vec::new();
    let mut candidates = Vec::new();
    let mut errors = Vec::new();
    for (index, record) in reader.records().enumerate() {
        let spreadsheet_row = index + 2;
        match record {
            Ok(record) => {
                if record.iter().all(|field| field.trim().is_empty()) {
                    continue;
                }
                let (parsed_row, candidate) =
                    parse_point_record(&record, spreadsheet_row, &mut errors);
                candidates.push(candidate);
                if let Some(row) = parsed_row {
                    parsed.push(row);
                }
            }
            Err(error) => {
                let row = error
                    .position()
                    .map(|position| position.record() as usize + 1)
                    .unwrap_or(spreadsheet_row);
                errors.push(ValidationError::row(row, "CSV syntax", error.to_string()));
            }
        }
    }

    let mut seen = HashMap::<(u32, AsduTypeId), usize>::with_capacity(candidates.len());
    for candidate in &candidates {
        let Some(key) = candidate.key else {
            continue;
        };
        match seen.entry(key) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(candidate.row);
            }
            std::collections::hash_map::Entry::Occupied(entry) => {
                let first_row = *entry.get();
                errors.push(ValidationError::row(
                    candidate.row,
                    "IOA/Type ID",
                    format!(
                        "duplicate (Type ID {}, IOA {}) for the selected station; first declared on row {first_row}",
                        key.1 as u8, key.0
                    ),
                ));
            }
        }
    }

    Ok(ParsedPointCsv {
        rows: parsed,
        candidates,
        errors,
    })
}

#[cfg(test)]
fn parse_point_csv(bytes: &[u8]) -> Result<Vec<ParsedPointRow>, String> {
    let parsed = parse_point_csv_outcome(bytes)?;
    if parsed.errors.is_empty() {
        Ok(parsed.rows)
    } else {
        Err(format_validation_errors(&parsed.errors))
    }
}

fn validate_projected_station_candidates(
    candidates: &[ProjectedPointCandidate],
    stations: &HashMap<u16, Station>,
    common_address: u16,
    mode: ImportMode,
) -> Vec<ValidationError> {
    let Some(current) = stations.get(&common_address) else {
        return vec![ValidationError::file(
            "Station",
            format!("station CA={common_address} not found"),
        )];
    };
    let imported: HashSet<(u32, AsduTypeId)> = candidates
        .iter()
        .filter_map(|candidate| candidate.key)
        .collect();
    let current_keys: HashSet<(u32, AsduTypeId)> = current
        .object_defs
        .iter()
        .map(|definition| (definition.ioa, definition.asdu_type))
        .chain(
            current
                .data_points
                .points
                .values()
                .map(|point| (point.ioa, point.asdu_type)),
        )
        .collect();
    let mut projected = if mode == ImportMode::Merge {
        current_keys.clone()
    } else {
        HashSet::with_capacity(imported.len())
    };
    projected.extend(imported.iter().copied());

    let mut errors = Vec::new();
    if mode == ImportMode::Merge {
        for candidate in candidates {
            let Some((ioa, asdu_type)) = candidate.key else {
                continue;
            };
            if current_keys.contains(&(ioa, asdu_type)) {
                errors.push(ValidationError::row(
                    candidate.row,
                    "IOA/Type ID",
                    format!(
                        "Merge collision at (CA {}, Type ID {}, IOA {}); no rows were imported",
                        common_address, asdu_type as u8, ioa
                    ),
                ));
            }
        }
    }

    for candidate in candidates {
        let Some(target) = candidate.mapping else {
            continue;
        };
        let exists = if target.common_address == common_address {
            projected.contains(&(target.ioa, target.asdu_type))
        } else {
            stations
                .get(&target.common_address)
                .map(|station| station.data_points.contains(target.ioa, target.asdu_type))
                .unwrap_or(false)
        };
        if !exists {
            errors.push(ValidationError::row(
                candidate.row,
                "Mapped CA/Mapped IOA/Mapped Type ID",
                format!(
                    "mapping target not found: CA={} Type ID={} IOA={}",
                    target.common_address, target.asdu_type as u8, target.ioa
                ),
            ));
        }
    }

    // Replacing one station must not break mappings owned by another station.
    if mode == ImportMode::Replace {
        for (source_ca, station) in stations {
            if *source_ca == common_address {
                continue;
            }
            for definition in &station.object_defs {
                if let Some(target) = definition.mapping {
                    if target.common_address == common_address
                        && !projected.contains(&(target.ioa, target.asdu_type))
                    {
                        errors.push(ValidationError::file(
                            "Existing mapping",
                            format!(
                                "CA={} {} IOA={} maps to CA={} Type ID={} IOA={}, which Replace would remove",
                                source_ca,
                                definition.asdu_type.name(),
                                definition.ioa,
                                common_address,
                                target.asdu_type as u8,
                                target.ioa
                            ),
                        ));
                    }
                }
            }
        }
    }

    errors
}

fn validate_projected_station(
    rows: &[ParsedPointRow],
    stations: &HashMap<u16, Station>,
    common_address: u16,
    mode: ImportMode,
) -> Vec<ValidationError> {
    let candidates: Vec<ProjectedPointCandidate> = rows
        .iter()
        .map(|row| ProjectedPointCandidate {
            row: row.row,
            key: Some((row.definition.ioa, row.definition.asdu_type)),
            mapping: row.definition.mapping,
        })
        .collect();
    validate_projected_station_candidates(&candidates, stations, common_address, mode)
}

fn stage_station_import(
    stations: &HashMap<u16, Station>,
    rows: &[ParsedPointRow],
    common_address: u16,
    mode: ImportMode,
) -> Result<Station, String> {
    let validation_errors = validate_projected_station(rows, stations, common_address, mode);
    if !validation_errors.is_empty() {
        return Err(format_validation_errors(&validation_errors));
    }
    let current = stations
        .get(&common_address)
        .ok_or_else(|| format!("station CA={common_address} not found"))?;
    let mut staged = if mode == ImportMode::Replace {
        let mut replacement = Station::new(common_address, current.name.clone());
        replacement.cyclic_config = current.cyclic_config;
        replacement
    } else {
        current.clone()
    };
    // Rows are already unique and collision-checked. Build in O(n) instead of
    // calling Station::add_point, whose metadata replacement path scans the
    // whole object_defs vector for every row.
    staged.object_defs.reserve(rows.len());
    staged.data_points.points.reserve(rows.len());
    for row in rows {
        staged
            .data_points
            .insert(DataPoint::new(row.definition.ioa, row.definition.asdu_type));
        staged.object_defs.push(row.definition.clone());
    }
    Ok(staged)
}

fn mutation_mode_name(mode: MutationMode) -> &'static str {
    match mode {
        MutationMode::Flip => "flip",
        MutationMode::Increment => "increment",
        MutationMode::Decrement => "decrement",
        MutationMode::Random => "random",
    }
}

fn encode_point_csv(
    definitions: &[InformationObjectDef],
    mutations: &HashMap<(u32, AsduTypeId), CsvMutation>,
) -> Result<Vec<u8>, String> {
    let mut sorted: Vec<&InformationObjectDef> = definitions.iter().collect();
    sorted.sort_by_key(|definition| (definition.ioa, definition.asdu_type as u8));

    let mut writer = WriterBuilder::new()
        .terminator(Terminator::CRLF)
        .quote_style(QuoteStyle::Always)
        .from_writer(Vec::new());
    writer
        .write_record(POINT_CSV_HEADERS)
        .map_err(|error| format!("failed to write CSV header: {error}"))?;
    for definition in sorted {
        let mapping = definition.mapping;
        let mutation = mutations.get(&(definition.ioa, definition.asdu_type));
        let mut record = vec![
            definition.ioa.to_string(),
            definition.asdu_type.name().to_string(),
            (definition.asdu_type as u8).to_string(),
            encode_excel_safe_text(&definition.name),
            encode_excel_safe_text(&definition.comment),
            definition
                .command_qualifier
                .map(|value| value.to_string())
                .unwrap_or_default(),
            match definition.select_before_operate {
                Some(true) => "select".to_string(),
                Some(false) => "direct".to_string(),
                None => String::new(),
            },
            mapping
                .map(|target| target.common_address.to_string())
                .unwrap_or_default(),
            mapping
                .map(|target| target.ioa.to_string())
                .unwrap_or_default(),
            mapping
                .map(|target| (target.asdu_type as u8).to_string())
                .unwrap_or_default(),
        ];
        if let Some(mutation) = mutation {
            record.extend([
                mutation_mode_name(mutation.params.mode).to_string(),
                mutation.period_ms.to_string(),
                mutation.params.step.to_string(),
                mutation.params.min.to_string(),
                mutation.params.max.to_string(),
            ]);
        } else {
            record.extend([
                "off".to_string(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ]);
        }
        writer
            .write_record(&record)
            .map_err(|error| format!("failed to write point CSV: {error}"))?;
    }
    let csv = writer
        .into_inner()
        .map_err(|error| format!("failed to finish point CSV: {error}"))?;
    let mut output = Vec::with_capacity(UTF8_BOM.len() + csv.len());
    output.extend_from_slice(UTF8_BOM);
    output.extend_from_slice(&csv);
    Ok(output)
}

fn template_rows(
    common_address: u16,
) -> (
    Vec<InformationObjectDef>,
    HashMap<(u32, AsduTypeId), CsvMutation>,
) {
    let monitor = InformationObjectDef {
        ioa: 1,
        asdu_type: AsduTypeId::MSpNa1,
        category: DataCategory::SinglePoint,
        name: "Example monitor".to_string(),
        comment: "Edit or delete this example row".to_string(),
        mapping: None,
        command_qualifier: None,
        select_before_operate: None,
    };
    let control = InformationObjectDef {
        ioa: 1001,
        asdu_type: AsduTypeId::CScNa1,
        category: DataCategory::SingleCommand,
        name: "Example control".to_string(),
        comment: "Maps to the example monitor".to_string(),
        mapping: Some(ControlTarget {
            common_address,
            ioa: 1,
            asdu_type: AsduTypeId::MSpNa1,
        }),
        command_qualifier: Some(0),
        select_before_operate: Some(false),
    };
    let mut mutations = HashMap::new();
    mutations.insert(
        (monitor.ioa, monitor.asdu_type),
        CsvMutation {
            params: MutationParams::default(),
            period_ms: 1000,
        },
    );
    (vec![monitor, control], mutations)
}

async fn run_blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| format!("CSV worker failed: {error}"))?
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PointCsvImportResult {
    pub imported: usize,
    pub total_points: usize,
    pub mutations_started: usize,
}

fn ensure_csv_import_stopped(server: &SlaveServer) -> Result<(), String> {
    if server.state() != ServerState::Stopped {
        return Err(
            "CSV import requires a stopped server; stop it before importing point definitions"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
async fn apply_point_import(
    server: &SlaveServer,
    rows: &[ParsedPointRow],
    common_address: u16,
    mode: ImportMode,
) -> Result<(usize, usize), String> {
    let candidates: Vec<ProjectedPointCandidate> = rows
        .iter()
        .map(|row| ProjectedPointCandidate {
            row: row.row,
            key: Some((row.definition.ioa, row.definition.asdu_type)),
            mapping: row.definition.mapping,
        })
        .collect();
    apply_point_import_parts(server, rows, &candidates, &[], common_address, mode).await
}

async fn apply_point_import_outcome(
    server: &SlaveServer,
    parsed: &ParsedPointCsv,
    common_address: u16,
    mode: ImportMode,
) -> Result<(usize, usize), String> {
    apply_point_import_parts(
        server,
        &parsed.rows,
        &parsed.candidates,
        &parsed.errors,
        common_address,
        mode,
    )
    .await
}

async fn apply_point_import_parts(
    server: &SlaveServer,
    rows: &[ParsedPointRow],
    candidates: &[ProjectedPointCandidate],
    parse_errors: &[ValidationError],
    common_address: u16,
    mode: ImportMode,
) -> Result<(usize, usize), String> {
    let updates: Vec<PointMutationUpdate> = rows
        .iter()
        .map(|row| PointMutationUpdate {
            ioa: row.definition.ioa,
            asdu_type: row.definition.asdu_type,
            mutation: row.mutation.map(|mutation| PointMutationConfig {
                params: mutation.params,
                period_ms: mutation.period_ms,
            }),
            preserve_from: None,
        })
        .collect();
    let batch_mode = match mode {
        ImportMode::Replace => StationMutationBatchMode::Replace,
        ImportMode::Merge => StationMutationBatchMode::Merge,
    };
    server
        .transact_station_point_mutations(common_address, batch_mode, &updates, |stations| {
            let mut validation_errors = parse_errors.to_vec();
            validation_errors.extend(validate_projected_station_candidates(
                candidates,
                stations,
                common_address,
                mode,
            ));
            if !validation_errors.is_empty() {
                return Err(format_validation_errors(&validation_errors));
            }
            let staged = stage_station_import(stations, rows, common_address, mode)?;
            let total_points = staged.data_points.len();
            stations.insert(common_address, staged);
            Ok(total_points)
        })
        .await
}

#[tauri::command]
pub async fn save_point_config_csv(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    path: String,
) -> Result<usize, String> {
    let (definitions, mutations) = {
        let servers = state.servers.read().await;
        let server = servers
            .get(&server_id)
            .ok_or_else(|| format!("server {server_id} not found"))?;
        let (definitions, active) = server
            .server
            .snapshot_station_with_point_mutations(common_address)
            .await
            .ok_or_else(|| format!("station CA={common_address} not found"))?;
        let mutations = active
            .into_iter()
            .map(|(ioa, asdu_type, config)| {
                (
                    (ioa, asdu_type),
                    CsvMutation {
                        params: config.params,
                        period_ms: config.period_ms,
                    },
                )
            })
            .collect();
        (definitions, mutations)
    };
    let count = definitions.len();
    run_blocking(move || {
        let bytes = encode_point_csv(&definitions, &mutations)?;
        std::fs::write(&path, bytes)
            .map_err(|error| format!("failed to write point CSV {path:?}: {error}"))
    })
    .await?;
    Ok(count)
}

#[tauri::command]
pub async fn save_point_config_csv_template(
    common_address: u16,
    path: String,
) -> Result<(), String> {
    if !(1..=65_534).contains(&common_address) {
        return Err(format!("invalid station common address: {common_address}"));
    }
    run_blocking(move || {
        let (definitions, mutations) = template_rows(common_address);
        let bytes = encode_point_csv(&definitions, &mutations)?;
        std::fs::write(&path, bytes)
            .map_err(|error| format!("failed to write point CSV template {path:?}: {error}"))
    })
    .await
}

#[tauri::command]
pub async fn import_point_config_csv(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    server_id: String,
    common_address: u16,
    path: String,
    mode: String,
) -> Result<PointCsvImportResult, String> {
    let mode = ImportMode::parse(&mode)?;
    {
        let servers = state.servers.read().await;
        let server = servers
            .get(&server_id)
            .ok_or_else(|| format!("server {server_id} not found"))?;
        ensure_csv_import_stopped(&server.server)?;
    }
    let parsed = run_blocking(move || {
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("failed to read point CSV {path:?}: {error}"))?;
        parse_point_csv_outcome(&bytes)
    })
    .await?;
    let imported = parsed.rows.len();

    let (total_points, mutations_started) = {
        let servers = state.servers.read().await;
        let server = servers
            .get(&server_id)
            .ok_or_else(|| format!("server {server_id} not found"))?;
        // Re-check under the read lock held through commit. start_server needs
        // the corresponding application write lock, so it cannot race this
        // authoritative state gate after validation succeeds.
        ensure_csv_import_stopped(&server.server)?;
        apply_point_import_outcome(&server.server, &parsed, common_address, mode).await?
    };

    persist_workspace_after(&state, &app_handle, "importing point CSV").await;

    Ok(PointCsvImportResult {
        imported,
        total_points,
        mutations_started,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use iec104sim_core::slave::SlaveTransportConfig;
    use std::time::{Duration, Instant};

    fn empty_station_map(ca: u16) -> HashMap<u16, Station> {
        HashMap::from([(ca, Station::new(ca, "Test"))])
    }

    fn csv_bytes(records: &[Vec<String>]) -> Vec<u8> {
        let mut writer = WriterBuilder::new()
            .terminator(Terminator::CRLF)
            .from_writer(Vec::new());
        writer.write_record(POINT_CSV_HEADERS).unwrap();
        for record in records {
            writer.write_record(record).unwrap();
        }
        let csv = writer.into_inner().unwrap();
        [UTF8_BOM, csv.as_slice()].concat()
    }

    fn monitor_row(ioa: u32) -> Vec<String> {
        vec![
            ioa.to_string(),
            "M_SP_NA_1".into(),
            "1".into(),
            format!("Point {ioa}"),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "off".into(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ]
    }

    fn active_monitor_row(ioa: u32, period_ms: u32) -> Vec<String> {
        let mut row = monitor_row(ioa);
        row[10] = "flip".into();
        row[11] = period_ms.to_string();
        row
    }

    fn point_definition(ioa: u32) -> InformationObjectDef {
        InformationObjectDef {
            ioa,
            asdu_type: AsduTypeId::MSpNa1,
            category: DataCategory::SinglePoint,
            name: format!("Point {ioa}"),
            comment: String::new(),
            mapping: None,
            command_qualifier: None,
            select_before_operate: None,
        }
    }

    #[test]
    fn export_has_bom_exact_headers_and_round_trips_quoted_utf8() {
        let definition = InformationObjectDef {
            ioa: 7,
            asdu_type: AsduTypeId::MMeNc1,
            category: DataCategory::FloatMeasured,
            name: "温度, \"A\"\nsecond line".into(),
            comment: "中文备注".into(),
            mapping: None,
            command_qualifier: None,
            select_before_operate: None,
        };
        let mutations = HashMap::from([(
            (7, AsduTypeId::MMeNc1),
            CsvMutation {
                params: MutationParams {
                    mode: MutationMode::Increment,
                    step: 0.5,
                    min: -10.0,
                    max: 10.0,
                },
                period_ms: 750,
            },
        )]);

        let bytes = encode_point_csv(&[definition], &mutations).unwrap();
        assert!(bytes.starts_with(UTF8_BOM));
        let rows = parse_point_csv(&bytes).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].definition.name, "温度, \"A\"\nsecond line");
        assert_eq!(rows[0].definition.comment, "中文备注");
        assert_eq!(rows[0].definition.asdu_type, AsduTypeId::MMeNc1);
        let mutation = rows[0].mutation.unwrap();
        assert_eq!(mutation.params.mode, MutationMode::Increment);
        assert_eq!(mutation.period_ms, 750);
        assert_eq!(mutation.params.step, 0.5);
        assert_eq!((mutation.params.min, mutation.params.max), (-10.0, 10.0));
    }

    #[test]
    fn excel_resistant_formula_guards_are_reversible_and_stable_on_reexport() {
        let dangerous = [
            "=SUM(A1:A2)",
            "+cmd|' /C calc'!A0",
            "-2+3",
            "@SUM(1,2)",
            "＝FULLWIDTH",
            "＋FULLWIDTH",
            "－FULLWIDTH",
            "＠FULLWIDTH",
            "   =leading spaces",
            "\t+leading tab",
            "\rleading carriage return",
            "\nleading line feed",
        ];
        let mut definitions = Vec::new();
        for (index, value) in dangerous.iter().enumerate() {
            let mut definition = point_definition(index as u32 + 1);
            definition.name = (*value).to_string();
            definition.comment = dangerous[dangerous.len() - index - 1].to_string();
            definitions.push(definition);
        }
        let apostrophe_marker = "'\u{2060}=literal prefix".to_string();
        let marker_prefix = "\u{2060}ordinary external text".to_string();
        let original_guard = format!("{EXCEL_TEXT_GUARD}=already guarded text");
        let mut special = point_definition(100);
        special.name = apostrophe_marker.clone();
        special.comment = marker_prefix.clone();
        definitions.push(special);
        let mut guarded_special = point_definition(101);
        guarded_special.name = original_guard.clone();
        definitions.push(guarded_special);

        let exported = encode_point_csv(&definitions, &HashMap::new()).unwrap();
        let raw_text = std::str::from_utf8(&exported[UTF8_BOM.len()..]).unwrap();
        assert!(raw_text.starts_with("\"IOA\",\"ASDU Type\",\"Type ID\""));
        assert!(raw_text.contains(&format!("\"{EXCEL_TEXT_GUARD}=SUM(A1:A2)\"")));

        let mut raw_reader = ReaderBuilder::new()
            .has_headers(true)
            .from_reader(&exported[UTF8_BOM.len()..]);
        let raw_records: Vec<StringRecord> =
            raw_reader.records().map(Result::unwrap).collect();
        for record in raw_records.iter().take(dangerous.len()) {
            assert!(record_field(record, 3).starts_with(EXCEL_TEXT_GUARD));
            assert!(record_field(record, 4).starts_with(EXCEL_TEXT_GUARD));
        }
        assert_eq!(
            record_field(raw_records.last().unwrap(), 3),
            format!("{EXCEL_TEXT_GUARD}{original_guard}")
        );

        let parsed = parse_point_csv(&exported).unwrap();
        for (index, value) in dangerous.iter().enumerate() {
            assert_eq!(parsed[index].definition.name, *value);
            assert_eq!(
                parsed[index].definition.comment,
                dangerous[dangerous.len() - index - 1]
            );
        }
        assert_eq!(parsed[dangerous.len()].definition.name, apostrophe_marker);
        assert_eq!(parsed[dangerous.len()].definition.comment, marker_prefix);
        assert_eq!(parsed.last().unwrap().definition.name, original_guard);

        let parsed_definitions: Vec<InformationObjectDef> = parsed
            .iter()
            .map(|row| row.definition.clone())
            .collect();
        let reexported = encode_point_csv(&parsed_definitions, &HashMap::new()).unwrap();
        assert_eq!(reexported, exported);

        // A manually authored U+2060 without the exact TAB+marker guard is not
        // our transport prefix and must remain untouched.
        assert_eq!(decode_excel_safe_text(&marker_prefix), marker_prefix);
    }

    #[test]
    fn rejects_non_utf8_before_csv_parsing() {
        let error = parse_point_csv(&[0xF0, 0x28, 0x8C, 0x28]).unwrap_err();
        assert!(error.contains("not valid UTF-8"));
        assert!(error.contains("byte 0"));
    }

    #[test]
    fn identifies_utf16_bom_and_nul_encoded_files_clearly() {
        let little_endian = parse_point_csv(&[0xFF, 0xFE, b'I', 0]).unwrap_err();
        assert!(little_endian.contains("UTF-16"));
        assert!(little_endian.contains("Save it as UTF-8"));

        let no_bom = parse_point_csv(&[b'I', 0, b'O', 0, b'A', 0]).unwrap_err();
        assert!(no_bom.contains("NUL bytes"));
        assert!(no_bom.contains("UTF-16"));
    }

    #[test]
    fn reports_type_mismatch_ioa_range_and_duplicate_rows_together() {
        let mut bad_type = monitor_row(1);
        bad_type[2] = "3".into();
        let mut bad_ioa = monitor_row(2);
        bad_ioa[0] = (IOA_MAX as u64 + 1).to_string();
        let duplicate_a = monitor_row(9);
        let duplicate_b = monitor_row(9);
        let duplicate_c = monitor_row(9);
        let error = parse_point_csv(&csv_bytes(&[
            bad_type,
            bad_ioa,
            duplicate_a,
            duplicate_b,
            duplicate_c,
        ]))
        .unwrap_err();

        assert!(error.contains("Row 2 [ASDU Type/Type ID]"));
        assert!(error.contains("Row 3 [IOA]"));
        assert!(error.contains("Row 5 [IOA/Type ID]"));
        assert!(error.contains("Row 6 [IOA/Type ID]"));
        assert_eq!(error.matches("first declared on row 4").count(), 2);
        assert!(!error.contains("first declared on row 5"));
    }

    #[test]
    fn duplicate_detection_includes_rows_with_other_validation_errors() {
        let mut first = active_monitor_row(19, 500);
        first[11] = "not-a-period".into();
        let duplicate = monitor_row(19);

        let error = parse_point_csv(&csv_bytes(&[first, duplicate])).unwrap_err();
        assert!(error.contains("Row 2 [Period]"), "unexpected error: {error}");
        assert!(error.contains("Row 3 [IOA/Type ID]"), "unexpected error: {error}");
        assert!(
            error.contains("first declared on row 2"),
            "the invalid row still owns the first valid key declaration: {error}"
        );
    }

    #[test]
    fn validation_report_keeps_every_row_beyond_fifty_errors() {
        let records: Vec<Vec<String>> = (0..51)
            .map(|ioa| {
                let mut row = monitor_row(ioa);
                row[2] = "3".into();
                row
            })
            .collect();
        let error = parse_point_csv(&csv_bytes(&records)).unwrap_err();
        assert!(error.starts_with("CSV validation failed (51 error(s)):"));
        assert!(error.contains("Row 52 [ASDU Type/Type ID]"));
        assert_eq!(error.matches("[ASDU Type/Type ID]").count(), 51);
    }

    #[tokio::test]
    async fn import_combines_parse_and_projected_station_errors_in_one_report() {
        let server = SlaveServer::new(SlaveTransportConfig::default());
        let mut station = Station::new(21, "existing");
        station.add_point(point_definition(1)).unwrap();
        server.add_station(station).await.unwrap();
        server
            .start_point_mutation(
                21,
                1,
                AsduTypeId::MSpNa1,
                700,
                MutationParams::default(),
            )
            .await
            .unwrap();

        let mut unrelated_bad_type = monitor_row(2);
        unrelated_bad_type[2] = "255".into();
        let parsed = parse_point_csv_outcome(&csv_bytes(&[
            monitor_row(1),
            unrelated_bad_type,
        ]))
        .unwrap();
        let error = apply_point_import_outcome(&server, &parsed, 21, ImportMode::Merge)
            .await
            .unwrap_err();
        assert!(error.contains("Row 2 [IOA/Type ID]"), "unexpected: {error}");
        assert!(error.contains("Merge collision"), "unexpected: {error}");
        assert!(error.contains("Row 3 [Type ID]"), "unexpected: {error}");

        let mut bad_period_collision = active_monitor_row(1, 500);
        bad_period_collision[11] = "invalid-period".into();
        let parsed = parse_point_csv_outcome(&csv_bytes(&[bad_period_collision])).unwrap();
        let error = apply_point_import_outcome(&server, &parsed, 21, ImportMode::Merge)
            .await
            .unwrap_err();
        assert!(error.contains("Row 2 [Period]"), "unexpected: {error}");
        assert!(error.contains("Merge collision"), "unexpected: {error}");

        assert_eq!(
            server.list_point_mutations_with_period().await,
            vec![(21, 1, AsduTypeId::MSpNa1, MutationMode::Flip, 700)],
            "a failed combined validation must not disturb active tasks"
        );
        assert_eq!(
            server
                .stations
                .read()
                .await
                .get(&21)
                .unwrap()
                .data_points
                .len(),
            1,
            "a failed combined validation must not mutate station data"
        );
    }

    #[test]
    fn validates_merge_collision_and_mapping_before_staging() {
        let mut stations = empty_station_map(12);
        stations
            .get_mut(&12)
            .unwrap()
            .add_point(InformationObjectDef {
                ioa: 1,
                asdu_type: AsduTypeId::MSpNa1,
                category: DataCategory::SinglePoint,
                name: "existing".into(),
                comment: String::new(),
                mapping: None,
                command_qualifier: None,
                select_before_operate: None,
            })
            .unwrap();

        let collision = parse_point_csv(&csv_bytes(&[monitor_row(1)])).unwrap();
        let error = stage_station_import(&stations, &collision, 12, ImportMode::Merge).unwrap_err();
        assert!(error.contains("Row 2"));
        assert!(error.contains("Merge collision"));
        assert_eq!(
            stations.get(&12).unwrap().object_defs[0].name,
            "existing",
            "validation must not mutate the live station"
        );

        let mut invalid_mapping = vec![
            "10".into(),
            "C_SC_NA_1".into(),
            "45".into(),
            "Control".into(),
            String::new(),
            "0".into(),
            "direct".into(),
            "12".into(),
            "99".into(),
            "1".into(),
            "off".into(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ];
        let rows = parse_point_csv(&csv_bytes(&[invalid_mapping.clone()])).unwrap();
        let error = stage_station_import(&stations, &rows, 12, ImportMode::Replace).unwrap_err();
        assert!(error.contains("Row 2 [Mapped CA/Mapped IOA/Mapped Type ID]"));
        assert!(error.contains("target not found"));

        invalid_mapping[8] = "1".into();
        let rows = parse_point_csv(&csv_bytes(&[monitor_row(1), invalid_mapping])).unwrap();
        let staged = stage_station_import(&stations, &rows, 12, ImportMode::Replace).unwrap();
        assert_eq!(staged.data_points.len(), 2);
    }

    #[test]
    fn template_is_valid_for_its_selected_station() {
        let (definitions, mutations) = template_rows(77);
        let rows = parse_point_csv(&encode_point_csv(&definitions, &mutations).unwrap()).unwrap();
        let staged =
            stage_station_import(&empty_station_map(77), &rows, 77, ImportMode::Replace).unwrap();
        assert_eq!(staged.data_points.len(), 2);
        assert_eq!(rows.iter().filter(|row| row.mutation.is_some()).count(), 1);
        let control = staged
            .object_defs
            .iter()
            .find(|definition| definition.asdu_type == AsduTypeId::CScNa1)
            .unwrap();
        assert_eq!(control.command_qualifier, Some(0));
        assert_eq!(control.select_before_operate, Some(false));
        assert_eq!(
            control.mapping,
            Some(ControlTarget {
                common_address: 77,
                ioa: 1,
                asdu_type: AsduTypeId::MSpNa1,
            })
        );
    }

    #[tokio::test]
    async fn header_only_replace_clears_points_and_tasks_but_empty_merge_is_a_noop() {
        let rows = parse_point_csv(&csv_bytes(&[])).unwrap();
        let server = SlaveServer::new(Default::default());
        let mut station = Station::new(4, "Clear me");
        station.add_point(point_definition(1)).unwrap();
        server.add_station(station).await.unwrap();
        server
            .start_point_mutation(4, 1, AsduTypeId::MSpNa1, 500, MutationParams::default())
            .await
            .unwrap();

        let (total, started) = apply_point_import(&server, &rows, 4, ImportMode::Merge)
            .await
            .unwrap();
        assert_eq!((total, started), (1, 0));
        assert_eq!(server.list_point_mutations().await.len(), 1);

        let (total, started) = apply_point_import(&server, &rows, 4, ImportMode::Replace)
            .await
            .unwrap();
        assert_eq!((total, started), (0, 0));
        let stations = server.stations.read().await;
        assert_eq!(stations.get(&4).unwrap().data_points.len(), 0);
        drop(stations);
        assert!(server.list_point_mutations().await.is_empty());
    }

    #[tokio::test]
    async fn replace_apply_stops_old_tasks_and_restores_only_csv_tasks() {
        let server = SlaveServer::new(Default::default());
        let mut station = Station::new(8, "Replace");
        station.add_point(point_definition(90)).unwrap();
        server.add_station(station).await.unwrap();
        server
            .start_point_mutation(8, 90, AsduTypeId::MSpNa1, 500, MutationParams::default())
            .await
            .unwrap();

        let rows = parse_point_csv(&csv_bytes(&[active_monitor_row(1, 750)])).unwrap();
        let (total, started) = apply_point_import(&server, &rows, 8, ImportMode::Replace)
            .await
            .unwrap();
        assert_eq!((total, started), (1, 1));
        {
            let stations = server.stations.read().await;
            let station = stations.get(&8).unwrap();
            assert!(!station.data_points.contains(90, AsduTypeId::MSpNa1));
            assert!(station.data_points.contains(1, AsduTypeId::MSpNa1));
        }
        let active = server.list_point_mutations_with_params().await;
        assert_eq!(active.len(), 1);
        assert_eq!(
            (active[0].0, active[0].1, active[0].2, active[0].4),
            (8, 1, AsduTypeId::MSpNa1, 750,)
        );
        server.stop_point_mutations_for_station(8).await;
    }

    #[tokio::test]
    async fn stopped_import_survives_restart_and_sends_spontaneous_frame_to_new_client() {
        use iec104sim_core::slave::{SlaveTlsConfig, SlaveTransportConfig};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let transport = SlaveTransportConfig {
            bind_address: "127.0.0.1".into(),
            port,
            tls: SlaveTlsConfig::default(),
        };
        let mut server = SlaveServer::new(transport);
        server
            .add_station(Station::new(8, "Restart"))
            .await
            .unwrap();
        server.start().await.unwrap();
        assert!(ensure_csv_import_stopped(&server)
            .unwrap_err()
            .contains("requires a stopped server"));
        server.stop().await.unwrap();
        ensure_csv_import_stopped(&server).unwrap();

        let rows = parse_point_csv(&csv_bytes(&[active_monitor_row(1, 200)])).unwrap();
        apply_point_import(&server, &rows, 8, ImportMode::Replace)
            .await
            .unwrap();
        assert_eq!(server.list_point_mutations().await.len(), 1);

        server.start().await.unwrap();
        let initial_seq = server
            .stations
            .read()
            .await
            .get(&8)
            .unwrap()
            .data_points
            .current_seq();
        let mut client = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        client
            .write_all(&[0x68, 0x04, 0x07, 0x00, 0x00, 0x00])
            .await
            .unwrap();
        let mut start_confirm = [0u8; 6];
        tokio::time::timeout(
            Duration::from_secs(1),
            client.read_exact(&mut start_confirm),
        )
        .await
        .expect("STARTDT confirmation should arrive")
        .unwrap();
        assert_eq!(start_confirm, [0x68, 0x04, 0x0B, 0x00, 0x00, 0x00]);

        let frame = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let mut header = [0u8; 2];
                client.read_exact(&mut header).await.unwrap();
                assert_eq!(header[0], 0x68);
                let mut frame = vec![0x68, header[1]];
                frame.resize(header[1] as usize + 2, 0);
                client.read_exact(&mut frame[2..]).await.unwrap();
                if frame[2] & 0x01 == 0 {
                    break frame;
                }
            }
        })
        .await
        .expect("an imported mutation should send through the restarted listener");
        assert!(frame.len() >= 16);
        assert_eq!(frame[6], AsduTypeId::MSpNa1 as u8);
        assert_eq!(u16::from_le_bytes([frame[10], frame[11]]), 8);
        assert_eq!(u32::from_le_bytes([frame[12], frame[13], frame[14], 0]), 1);
        assert!(
            server
                .stations
                .read()
                .await
                .get(&8)
                .unwrap()
                .data_points
                .current_seq()
                > initial_seq
        );

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn merge_apply_preserves_unrelated_tasks_and_clears_incoming_orphan() {
        let server = SlaveServer::new(Default::default());
        let mut station = Station::new(9, "Merge");
        station.add_point(point_definition(1)).unwrap();
        station.add_point(point_definition(2)).unwrap();
        server.add_station(station).await.unwrap();
        server
            .start_point_mutation(9, 1, AsduTypeId::MSpNa1, 500, MutationParams::default())
            .await
            .unwrap();
        server
            .start_point_mutation(9, 2, AsduTypeId::MSpNa1, 500, MutationParams::default())
            .await
            .unwrap();
        // Reproduce the pre-existing orphan condition: point removal currently
        // does not own the server's mutation-task registry.
        server
            .stations
            .write()
            .await
            .get_mut(&9)
            .unwrap()
            .remove_point(2, AsduTypeId::MSpNa1)
            .unwrap();

        let rows =
            parse_point_csv(&csv_bytes(&[monitor_row(2), active_monitor_row(3, 900)])).unwrap();
        let (total, started) = apply_point_import(&server, &rows, 9, ImportMode::Merge)
            .await
            .unwrap();
        assert_eq!((total, started), (3, 1));
        let mut active = server.list_point_mutations_with_params().await;
        active.sort_by_key(|(_, ioa, _, _, _)| *ioa);
        assert_eq!(active.len(), 2);
        assert_eq!((active[0].1, active[0].4), (1, 500));
        assert_eq!((active[1].1, active[1].4), (3, 900));
        assert!(
            active.iter().all(|(_, ioa, _, _, _)| *ioa != 2),
            "Sim Mode=off must remove an orphan handle for the incoming key"
        );
        server.stop_point_mutations_for_station(9).await;
    }

    #[tokio::test]
    async fn failed_validation_changes_neither_station_nor_mutation_tasks() {
        let server = SlaveServer::new(Default::default());
        let mut station = Station::new(10, "Transactional");
        station.add_point(point_definition(1)).unwrap();
        server.add_station(station).await.unwrap();
        server
            .start_point_mutation(10, 1, AsduTypeId::MSpNa1, 650, MutationParams::default())
            .await
            .unwrap();

        let invalid_control = vec![
            "20".into(),
            "C_SC_NA_1".into(),
            "45".into(),
            "Broken mapping".into(),
            String::new(),
            "0".into(),
            "direct".into(),
            "10".into(),
            "999".into(),
            "1".into(),
            "off".into(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ];
        let rows = parse_point_csv(&csv_bytes(&[invalid_control])).unwrap();
        let error = apply_point_import(&server, &rows, 10, ImportMode::Replace)
            .await
            .unwrap_err();
        assert!(error.contains("mapping target not found"));
        {
            let stations = server.stations.read().await;
            let station = stations.get(&10).unwrap();
            assert_eq!(station.name, "Transactional");
            assert_eq!(station.data_points.len(), 1);
            assert!(station.data_points.contains(1, AsduTypeId::MSpNa1));
        }
        let active = server.list_point_mutations_with_params().await;
        assert_eq!(active.len(), 1);
        assert_eq!((active[0].1, active[0].4), (1, 650));
        server.stop_point_mutations_for_station(10).await;
    }

    #[test]
    fn merges_more_than_ten_thousand_points_into_a_large_station_in_linear_time() {
        let mut stations = empty_station_map(1);
        let station = stations.get_mut(&1).unwrap();
        station.object_defs.reserve(10_000);
        station.data_points.points.reserve(10_000);
        for ioa in 20_000..30_000 {
            station
                .data_points
                .insert(DataPoint::new(ioa, AsduTypeId::MSpNa1));
            station.object_defs.push(point_definition(ioa));
        }
        let records: Vec<Vec<String>> = (0..10_001).map(monitor_row).collect();
        let bytes = csv_bytes(&records);
        let started = Instant::now();
        let rows = parse_point_csv(&bytes).unwrap();
        let staged = stage_station_import(&stations, &rows, 1, ImportMode::Merge).unwrap();
        assert_eq!(staged.data_points.len(), 20_001);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "10,001-row Merge should avoid quadratic collision validation"
        );
    }
}
