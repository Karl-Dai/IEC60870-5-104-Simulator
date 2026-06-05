# 主站归一化设定值改用原始整数 (NVA i16) 设计

日期：2026-06-05
状态：已确认，待写实施计划

## 背景与问题

主站「控制命令」对话框下发归一化设定值（C_SE_NA_1, TI=48）时，当前强制用户输入
`[-1, 1)` 的**归一化小数**（滑块 + 小数框，步进 0.001）。后端再把该小数
`× 32767` 得到线上真正传输的 16 位有符号整数 NVA。

接收方向同理：主站收到归一化测量值（M_ME_NA_1 / M_ME_TD_1 / M_ME_ND_1）后存成
`nva / 32767.0` 的小数，数据表按 `{:.4}` 显示小数。

工程现场习惯直接面对线上的**原始 NVA 整数 `[-32768, 32767]`**（即 `小数 × 32767`
的结果），而非 `[-1, 1)` 的小数。本设计把**主站侧**所有归一化值的展示与输入统一
改为原始整数。

## 目标与范围

**目标**：主站侧凡是归一化值出现的地方，一律用原始 16 位整数 `[-32768, 32767]`
（线上真实 NVA）展示与输入，取代 `[-1, 1)` 小数。

**「非归一化值」的确切定义**：原始 NVA 16 位有符号整数 `[-32768, 32767]`，等于
`归一化小数 × 32767`。**不是**按量程映射的工程量值（无需任何量程/工程单位配置）。

**范围内**（均为主站 iec104master-app / master-frontend）：

1. 下发对话框归一化设定值的输入控件
2. 下发成功后的发送日志/报文文本
3. 主站数据表接收到的归一化测量值显示

**范围外（刻意排除）**：

- 子站 iec104sim-app / `frontend/` 的归一化展示与内部表示**不动**，仍用小数。
- core `iec104sim-core` 的 `DataPointValue::Normalized { value: f32 }` 类型与
  共用的 `DataPointValue::display()` **不动**（子站也在用，改它会越界影响子站）。

**已知的可接受后果**：改完后**主站显示原始整数、子站显示小数**，两端不一致。
这是「仅主站」范围的必然结果；若日后要子站一致，属于另一个独立改动。

## 现状代码落点（改前）

- 前端输入：`master-frontend/src/components/ControlDialog.vue:300-305`
  —— 滑块 `min=-1 max=1 step=0.001` + 小数框，`normalizedValue` 默认 `'0.0'`。
- 后端直发（select=false）：`crates/iec104master-app/src/commands.rs:536-540`
  —— `request.value.parse::<f32>()` → core `send_setpoint_normalized(f32)`。
- 后端 SBO 两步（select=true）：`crates/iec104master-app/src/commands.rs:617-628`
  —— `parse::<f32>()` + `build_control_frames_setpoint_norm`（同文件 `:787-797`）。
- core 编码：`crates/iec104sim-core/src/master.rs:1102`（`send_setpoint_normalized`）
  与 `:2201-2215`（`build_setpoint_normalized`），均 `(value * 32767.0) as i16`。
- 发送日志：`crates/iec104master-app/src/commands.rs:627` —— `val={:.4}`。
- 接收存储：`crates/iec104sim-core/src/master.rs:2037` —— `nva as f32 / 32767.0`。
- 主站显示字符串：`crates/iec104master-app/src/commands.rs:825-832` 的 `point_to_info`
  —— `value: p.value.display()`，归一化走 core `data_point.rs:45-59` 的 `{:.4}`。

## 设计

### 一、发送侧：输入直接当 i16 上线，全程不做浮点换算

口径从「小数 × 32767」改为「输入即原始 NVA i16」，去掉所有 `× 32767`。

| 位置 | 现状 | 改为 |
|---|---|---|
| `ControlDialog.vue:300-305` | 滑块(-1~1)+小数框 | 纯整数框 `type=number min=-32768 max=32767 step=1`，加范围 label |
| `ControlDialog.vue:68` | `normalizedValue` 默认 `'0.0'` | 默认 `'0'`（旧持久化值在加载时取整兜底，见下） |
| `commands.rs:537`（直发） | `parse::<f32>()` | `parse::<i16>()` |
| `commands.rs:618`（两步） | `parse::<f32>()` | `parse::<i16>()` |
| core `send_setpoint_normalized`（master.rs:1102） | 入参 `value: f32` | 入参 `nva: i16`（唯一调用方是 commands.rs:537） |
| core `build_setpoint_normalized`（master.rs:2201） | `(value*32767) as i16` | 直接用 `nva: i16` 装帧 |
| `build_control_frames_setpoint_norm`（commands.rs:787） | `(value*32767) as i16` | 直接用 `value: i16` 装帧 |
| 发送日志 `commands.rs:627` | `val={:.4}` | `val={}`（整数） |

**关键约束**：绝不能保留 core 现有 `(v * 32767.0) as i16` 的浮点截断回环——
`as i16` 向零截断会丢 ±1（例如 `16384` 经小数回环后可能变 `16383`）。因此把入参
类型直接定为 `i16`，端到端不出现浮点。

**前端旧持久化兜底**：`ControlDialog.vue` 用 `localStorage` 持久化 `normalizedValue`，
历史值可能是 `"0.5"` 这类小数。整数框加载时对该字段做 `Math.round(Number(x))` 兜底
（或解析失败时落回 `0`），避免小数残留。`currentValueStr` 既有逻辑已 `String()` 强转，
无需改。

### 二、接收/监视显示侧：仅主站特判，不碰 core

core `DataPointValue::Normalized { value: f32 }` 与 `display()` 主从共用，**不动**。
只在主站独有的 `point_to_info`（`commands.rs:825-832`）里特判归一化变体：

```rust
value: match &p.value {
    iec104sim_core::data_point::DataPointValue::Normalized { value } =>
        ((value * 32767.0).round() as i16).to_string(),
    _ => p.value.display(),
},
```

`round(value × 32767)` 可**无损**还原原始 NVA：`nva as f32` 对 `|nva| ≤ 32767` 精确，
`/32767.0` 再 `×32767.0` 的 f32 相对误差约 6e-8，绝对误差 < 0.002 ≪ 0.5，
四舍五入后必然回到原始整数。影响 M_ME_NA_1 / M_ME_TD_1 / M_ME_ND_1 三种归一化
测量值在主站数据表的显示。

### 三、i18n

为整数输入框新增一条 label（如 `control.valueRangeNormalized`：
「归一化值 (原始整数 -32768~32767)」），中英文各一条，参照既有
`control.valueRangeScaled`。

## 测试（无头 + 后台）

**Rust 单测**：

1. `build_control_frames_setpoint_norm` / core `build_setpoint_normalized`：
   输入 `16384` → NVA 两字节 == `16384i16.to_le_bytes()`；输入 `-32768`、`32767`
   边界正确。
2. 更新 `master.rs:2488` 旧断言（原 `assert_eq!(nva, (0.5_f32*32767.0) as i16)`
   不再成立）。
3. `point_to_info` 对 `Normalized` 的整数还原回环：构造 `nva` → 解码成
   `Normalized { value: nva/32767.0 }` → 经 `point_to_info` 得到的字符串 == `nva`。

**前端**（按项目既有约束）：

- Playwright 真实浏览器实测对话框：归一化类型显示整数框、`min/max/step` 正确、
  发送的 `value` 为整数串；而非 jsdom。
- 跑 `npm run build`（master-frontend）确保类型与构建通过，而非 `vue-tsc --noEmit`。

## 影响文件清单

- `master-frontend/src/components/ControlDialog.vue`
- `master-frontend/src/i18n/locales/zh-CN.ts`、`en-US.ts`
- `crates/iec104master-app/src/commands.rs`
- `crates/iec104sim-core/src/master.rs`
- 相关 Rust 测试文件（core 内联测试 + 如有 master-app 测试）

## 非目标

- 不引入量程 / 工程单位 / min-max 映射配置。
- 不改子站归一化的内部表示与显示。
- 不改标度化（C_SE_NB_1）/ 短浮点（C_SE_NC_1）等其它设定值类型。
