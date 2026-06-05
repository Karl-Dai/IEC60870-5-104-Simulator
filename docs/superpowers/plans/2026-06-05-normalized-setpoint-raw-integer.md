# 主站归一化设定值改用原始整数 (NVA i16) 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 主站侧把归一化设定值（C_SE_NA_1, TI=48）的输入与显示，从 `[-1,1)` 小数改为线上真实的原始 NVA 16 位整数 `[-32768, 32767]`，全程不做浮点缩放。

**Architecture:** 发送侧把口径定为「输入即原始 i16，直接装帧」，去掉所有 `× 32767`（core + master-app 两条路径）；接收/显示侧不动主从共用的 core 类型与 `display()`，仅在主站独有的 `point_to_info` 里把归一化变体用 `round(value×32767)` 无损还原成整数字符串；前端对话框把滑块+小数框换成纯整数框。子站不受影响。

**Tech Stack:** Rust（iec104sim-core / iec104master-app，Cargo workspace）、Vue 3 + TypeScript（master-frontend，Vite + vue-tsc）、Playwright（前端无头实测）。

参考设计：`docs/superpowers/specs/2026-06-05-normalized-setpoint-raw-integer-design.md`

---

## File Structure

- `crates/iec104sim-core/src/master.rs` — core 编码 `build_setpoint_normalized` + 公开方法 `send_setpoint_normalized` 改为 i16；更新内联测试。
- `crates/iec104master-app/src/commands.rs` — 直发/两步两条发送路径改 i16；帧构造器 `build_control_frames_setpoint_norm` 改 i16；发送日志改整数；新增显示辅助函数 `normalized_raw_string` 并在 `point_to_info` 特判归一化；新增单测。
- `master-frontend/src/components/ControlDialog.vue` — 归一化输入控件改纯整数框；默认值与旧持久化兜底。
- `master-frontend/src/i18n/locales/zh-CN.ts`、`en-US.ts` — 新增 `valueRangeNormalized` 文案。

> **行号会漂移（iCloud 同步）**：所有编辑以下方代码块的**字符串原样**为锚点定位，勿依赖行号。

---

## Task 1: 发送侧改为原始 i16（Rust，core + master-app 同一原子改动）

整条发送链路必须一起改，否则 workspace 不编译。先让 core 自身测试转绿，再修 master-app 调用方，最后全量编译测试。

**Files:**
- Modify: `crates/iec104sim-core/src/master.rs`（`build_setpoint_normalized`、`send_setpoint_normalized`、`test_build_setpoint_normalized`）
- Modify: `crates/iec104master-app/src/commands.rs`（直发路径、两步路径、`build_control_frames_setpoint_norm`）

- [ ] **Step 1: 先改 core 内联测试为「原始 i16 直传」语义（RED）**

把 `crates/iec104sim-core/src/master.rs` 里的这个测试整体替换：

旧：
```rust
    #[test]
    fn test_build_setpoint_normalized() {
        let frame = build_setpoint_normalized(1, 400, 0.5, false, 0, 6);
        assert_eq!(frame[0], 0x68);
        assert_eq!(frame[6], 48);
        let nva = i16::from_le_bytes([frame[15], frame[16]]);
        assert_eq!(nva, (0.5_f32 * 32767.0) as i16);
        assert_eq!(frame[17], 0x00); // QOS = no select, QL=0

        // With select
        let frame = build_setpoint_normalized(1, 400, -0.5, true, 0, 6);
        assert_eq!(frame[17], 0x80); // QOS = select bit
    }
```
新：
```rust
    #[test]
    fn test_build_setpoint_normalized() {
        // 入参现在是原始 NVA i16，原样写线，不做任何缩放。
        let frame = build_setpoint_normalized(1, 400, 16384, false, 0, 6);
        assert_eq!(frame[0], 0x68);
        assert_eq!(frame[6], 48);
        let nva = i16::from_le_bytes([frame[15], frame[16]]);
        assert_eq!(nva, 16384);
        assert_eq!(frame[17], 0x00); // QOS = no select, QL=0

        // 边界原样透传
        let frame = build_setpoint_normalized(1, 400, -32768, false, 0, 6);
        assert_eq!(i16::from_le_bytes([frame[15], frame[16]]), -32768);
        let frame = build_setpoint_normalized(1, 400, 32767, false, 0, 6);
        assert_eq!(i16::from_le_bytes([frame[15], frame[16]]), 32767);

        // 带 select 位
        let frame = build_setpoint_normalized(1, 400, -16384, true, 0, 6);
        assert_eq!(frame[17], 0x80); // QOS = select bit
    }
```

- [ ] **Step 2: 跑 core 测试确认编译失败（RED）**

Run: `cargo test -p iec104sim-core test_build_setpoint_normalized`
Expected: 编译失败 —— `build_setpoint_normalized(1, 400, 16384, ...)` 与当前 `value: f32` 形参不匹配（mismatched types，期望 f32 得到整数字面量）。

- [ ] **Step 3: 改 core `build_setpoint_normalized` 为 i16 直传**

旧：
```rust
fn build_setpoint_normalized(ca: u16, ioa: u32, value: f32, select: bool, ql: u8, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let nva = (value * 32767.0) as i16;
    let nva_bytes = nva.to_le_bytes();
```
新：
```rust
fn build_setpoint_normalized(ca: u16, ioa: u32, nva: i16, select: bool, ql: u8, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let nva_bytes = nva.to_le_bytes();
```
（函数其余部分用 `nva_bytes` 不变。）

- [ ] **Step 4: 改 core 公开方法 `send_setpoint_normalized` 为 i16**

旧：
```rust
    pub async fn send_setpoint_normalized(&self, ioa: u32, value: f32, select: bool, ca: u16, ql: u8, cot: u8) -> Result<(), MasterError> {
        let frame = build_setpoint_normalized(ca, ioa, value, select, ql, cot);
        let detail = format!("归一化设定值 IOA={} val={:.4} sel={} QL={} COT={}", ioa, value, select, ql, cot);
        let event = crate::log_entry::DetailEvent {
            kind: "setpoint_normalized".to_string(),
            payload: serde_json::json!({ "ioa": ioa, "val": value, "select": select, "ql": ql, "cot": cot }),
        };
```
新：
```rust
    pub async fn send_setpoint_normalized(&self, ioa: u32, nva: i16, select: bool, ca: u16, ql: u8, cot: u8) -> Result<(), MasterError> {
        let frame = build_setpoint_normalized(ca, ioa, nva, select, ql, cot);
        let detail = format!("归一化设定值 IOA={} val={} sel={} QL={} COT={}", ioa, nva, select, ql, cot);
        let event = crate::log_entry::DetailEvent {
            kind: "setpoint_normalized".to_string(),
            payload: serde_json::json!({ "ioa": ioa, "val": nva, "select": select, "ql": ql, "cot": cot }),
        };
```
（方法末尾 `self.send_frame_with_event(...)` 一行不变。）

- [ ] **Step 5: 跑 core 测试确认转绿（GREEN，仅 core）**

Run: `cargo test -p iec104sim-core test_build_setpoint_normalized`
Expected: PASS。（此时 workspace 整体还不编译，master-app 调用方待修，正常。）

- [ ] **Step 6: 改 master-app 直发路径解析为 i16**

`crates/iec104master-app/src/commands.rs`，旧：
```rust
            "setpoint_normalized" => {
                let value = request.value.parse::<f32>().map_err(|e| format!("{}", e))?;
                conn.connection.send_setpoint_normalized(ioa, value, false, ca, qu, cot).await
                    .map_err(|e| format!("failed to send command: {}", e))?;
            }
```
新：
```rust
            "setpoint_normalized" => {
                let value = request.value.parse::<i16>().map_err(|e| format!("{}", e))?;
                conn.connection.send_setpoint_normalized(ioa, value, false, ca, qu, cot).await
                    .map_err(|e| format!("failed to send command: {}", e))?;
            }
```

- [ ] **Step 7: 改 master-app 两步(SBO)路径解析为 i16、日志改整数**

旧：
```rust
        "setpoint_normalized" => {
            let value = request.value.parse::<f32>().map_err(|e| format!("{}", e))?;
            let select_frame = build_control_frames_setpoint_norm(ca, ioa, value, true, qu, cot);
            let execute_frame = build_control_frames_setpoint_norm(ca, ioa, value, false, qu, cot);
            let event = DetailEvent {
                kind: "setpoint_normalized".to_string(),
                payload: serde_json::json!({ "ioa": ioa, "val": value, "ql": qu, "cot": cot }),
            };
            conn.connection.send_control_with_sbo_event(
                select_frame, execute_frame, ioa,
                &format!("归一化设定值 IOA={} val={:.4} QL={} COT={}", ioa, value, qu, cot),
                FrameLabel::SetpointNormalized, ca, Some(event),
```
新：
```rust
        "setpoint_normalized" => {
            let value = request.value.parse::<i16>().map_err(|e| format!("{}", e))?;
            let select_frame = build_control_frames_setpoint_norm(ca, ioa, value, true, qu, cot);
            let execute_frame = build_control_frames_setpoint_norm(ca, ioa, value, false, qu, cot);
            let event = DetailEvent {
                kind: "setpoint_normalized".to_string(),
                payload: serde_json::json!({ "ioa": ioa, "val": value, "ql": qu, "cot": cot }),
            };
            conn.connection.send_control_with_sbo_event(
                select_frame, execute_frame, ioa,
                &format!("归一化设定值 IOA={} val={} QL={} COT={}", ioa, value, qu, cot),
                FrameLabel::SetpointNormalized, ca, Some(event),
```

- [ ] **Step 8: 改 master-app 帧构造器 `build_control_frames_setpoint_norm` 为 i16 直传**

旧：
```rust
fn build_control_frames_setpoint_norm(ca: u16, ioa: u32, value: f32, select: bool, ql: u8, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let nva = (value * 32767.0) as i16;
    let nva_bytes = nva.to_le_bytes();
```
新：
```rust
fn build_control_frames_setpoint_norm(ca: u16, ioa: u32, value: i16, select: bool, ql: u8, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let nva_bytes = value.to_le_bytes();
```
（函数其余部分用 `nva_bytes` 不变。）

- [ ] **Step 9: 全量编译 + 测试转绿**

Run: `cargo test -p iec104sim-core -p iec104master-app`
Expected: PASS，无编译错误，无 `f32`/`32767.0` 残留导致的告警。

- [ ] **Step 10: 提交**

```bash
git add crates/iec104sim-core/src/master.rs crates/iec104master-app/src/commands.rs
git commit --author="Karl-Dai Karl <kelsoprotein@gmail.com>" -m "feat(master): 归一化设定值发送改为原始 NVA 整数直传"
```

---

## Task 2: 接收/监视显示侧改为原始整数（Rust，仅主站）

不碰 core 共用的 `display()`；只在主站 `point_to_info` 里把归一化变体还原成整数字符串，并加无损往返单测。

**Files:**
- Modify: `crates/iec104master-app/src/commands.rs`（新增 `normalized_raw_string`、修改 `point_to_info`、在已有 `#[cfg(test)] mod tests` 加测试）

- [ ] **Step 1: 在已有测试模块里加无损往返单测（RED）**

`crates/iec104master-app/src/commands.rs` 已有 `#[cfg(test)] mod tests { ... }`。在该模块内追加：
```rust
    #[test]
    fn normalized_raw_string_recovers_wire_nva() {
        // 主站把线上 NVA 解码为 `nva as f32 / 32767.0`；显示必须无损还原成原始整数。
        for nva in [-32768i16, -32767, -16384, -1, 0, 1, 16384, 32766, 32767] {
            let decoded = nva as f32 / 32767.0;
            assert_eq!(super::normalized_raw_string(decoded), nva.to_string(), "nva={}", nva);
        }
    }
```

- [ ] **Step 2: 跑测试确认编译失败（RED）**

Run: `cargo test -p iec104master-app normalized_raw_string_recovers_wire_nva`
Expected: 编译失败 —— `cannot find function normalized_raw_string`。

- [ ] **Step 3: 新增辅助函数 `normalized_raw_string`**

在 `crates/iec104master-app/src/commands.rs` 的 `fn point_to_info` 紧邻上方加入：
```rust
/// 主站侧把归一化值显示为线上原始 NVA 整数 (-32768..32767)，而非 [-1,1) 小数。
/// `round(value * 32767)` 可无损还原原始 NVA：f32 往返误差 < 0.002，远小于 0.5。
fn normalized_raw_string(value: f32) -> String {
    ((value * 32767.0).round() as i16).to_string()
}
```

- [ ] **Step 4: 在 `point_to_info` 里特判归一化变体**

旧（`point_to_info` 中的一行）：
```rust
        value: p.value.display(),
```
新：
```rust
        value: match &p.value {
            iec104sim_core::data_point::DataPointValue::Normalized { value } => normalized_raw_string(*value),
            _ => p.value.display(),
        },
```

- [ ] **Step 5: 跑测试 + 全量测试转绿**

Run: `cargo test -p iec104master-app`
Expected: `normalized_raw_string_recovers_wire_nva` PASS，其余测试不回归。

- [ ] **Step 6: 提交**

```bash
git add crates/iec104master-app/src/commands.rs
git commit --author="Karl-Dai Karl <kelsoprotein@gmail.com>" -m "feat(master): 接收归一化测量值在数据表显示为原始 NVA 整数"
```

---

## Task 3: 前端对话框改纯整数框（Vue + i18n）

前端模板小改，按项目约束用 `npm run build` 把关类型/构建，再用 Playwright 真实浏览器实测（非 jsdom）。

**Files:**
- Modify: `master-frontend/src/components/ControlDialog.vue`
- Modify: `master-frontend/src/i18n/locales/zh-CN.ts`、`master-frontend/src/i18n/locales/en-US.ts`

- [ ] **Step 1: 新增 i18n 文案 key `valueRangeNormalized`**

`zh-CN.ts` 接口段，在 `valueRangeScaled: string` 之后加一行：
```ts
    valueRangeNormalized: string
```
`zh-CN.ts` 值段，在 `valueRangeScaled: '值 (-32768 ~ 32767)',` 之后加一行：
```ts
    valueRangeNormalized: '归一化值 (原始整数 -32768 ~ 32767)',
```
`en-US.ts` 值段，在 `valueRangeScaled: 'Value (-32768 ~ 32767)',` 之后加一行：
```ts
    valueRangeNormalized: 'Normalized (raw NVA -32768 ~ 32767)',
```

- [ ] **Step 2: 归一化输入控件由「滑块+小数框」改为「纯整数框」**

`ControlDialog.vue` 模板，旧：
```html
          <!-- Normalized: slider + input -->
          <div v-else-if="commandType === 'setpoint_normalized'" class="slider-control">
            <div class="slider-row">
              <input type="range" class="slider-input" min="-1" max="1" step="0.001" v-model="normalizedValue" />
              <input type="number" class="number-sm" min="-1" max="1" step="0.001" v-model="normalizedValue" />
            </div>
          </div>
```
新：
```html
          <!-- Normalized: raw NVA integer (-32768 ~ 32767), same form as scaled -->
          <label v-else-if="commandType === 'setpoint_normalized'" class="form-label">
            {{ t('control.valueRangeNormalized') }}
            <input v-model="normalizedValue" class="form-input" type="number" min="-32768" max="32767" step="1" />
          </label>
```

- [ ] **Step 3: 默认值改整数 + 旧持久化小数兜底**

`ControlDialog.vue` `<script setup>`，旧：
```ts
const normalizedValue = ref(saved.normalizedValue ?? '0.0')
```
新：
```ts
// 旧版本持久化的是 [-1,1) 小数；现在统一存原始 NVA 整数字符串。
// 加载时取整并夹到 i16 范围，避免历史小数残留导致解析失败。
function toRawNorm(x: string | undefined): string {
  const n = Number(x)
  if (x == null || x === '' || !Number.isFinite(n)) return '0'
  return String(Math.max(-32768, Math.min(32767, Math.round(n))))
}
const normalizedValue = ref(toRawNorm(saved.normalizedValue))
```

- [ ] **Step 4: 清理因移除滑块而变成死代码的 CSS（确认后删除）**

先确认这些类只在刚改掉的归一化块用过：

Run: `grep -nE "slider-control|slider-row|slider-input|number-sm" master-frontend/src/components/ControlDialog.vue`
Expected: 仅 `<style>` 段的定义命中，模板中已无引用。

若确认无其它引用，删除 `ControlDialog.vue` `<style scoped>` 里的 `.slider-control`、`.slider-row`、`.slider-input`、`.number-sm`（及 `.number-sm:focus`）这几条规则。若发现仍有别处引用，则跳过本步、不要删。

- [ ] **Step 5: 构建把关（类型 + 打包）**

Run: `cd master-frontend && npm run build`
Expected: `vue-tsc -b` 无类型错误（特别是 `t('control.valueRangeNormalized')` 的 key 存在于两个 locale），`vite build` 成功产出。

- [ ] **Step 6: Playwright 真实浏览器实测对话框**

启动 dev（后台）：

Run: `cd master-frontend && npm run dev`（后台运行，地址 `http://localhost:5177`）

用 Playwright MCP 浏览器工具验证：
1. `browser_navigate` 打开 `http://localhost:5177`。
2. 打开「控制命令」对话框（工具栏自定义控制入口），命令类型选「归一化设定值」。
3. `browser_snapshot` 断言：该栏是单个数字输入框、label 文案为「归一化值 (原始整数 -32768 ~ 32767)」、**无 range 滑块**。
4. 用 `browser_evaluate` 读该 `input` 的 `min/max/step`，断言为 `-32768`/`32767`/`1`。
5. 输入 `16384`，断言输入框值为字符串 `"16384"`（即提交时 `currentValueStr` 为 `"16384"`，不带小数）。

Expected: 全部断言通过；控制台无报错。完成后停掉后台 dev。

- [ ] **Step 7: 提交**

```bash
git add master-frontend/src/components/ControlDialog.vue master-frontend/src/i18n/locales/zh-CN.ts master-frontend/src/i18n/locales/en-US.ts
git commit --author="Karl-Dai Karl <kelsoprotein@gmail.com>" -m "feat(master-fe): 归一化设定值对话框改为原始整数输入框"
```

---

## Self-Review

**Spec coverage（逐条对照 spec）：**
- 发送侧输入/编码改原始 i16（spec「一、发送侧」整表）→ Task 1 全覆盖（前端框 Task 3 Step 2/3；后端 parse/帧/core/日志 Task 1 Step 3/4/6/7/8）。
- 接收/监视显示改原始整数、不碰 core（spec「二」）→ Task 2 全覆盖。
- i18n 新 label（spec「三」）→ Task 3 Step 1。
- 测试：帧构造无损、旧断言更新、point_to_info 往返、前端 Playwright + build（spec「测试」）→ Task 1 Step 1/5/9 + Task 2 Step 1 + Task 3 Step 5/6。
- 范围边界（子站不动、core 类型/display 不动）→ Task 2 设计遵守，无任务触碰子站或 core `display()`。
- 非目标（不引入量程/不动子站/不动其它设定值类型）→ 计划无相关任务，符合。

**Placeholder scan:** 无 TBD/TODO/「类似上文」；每个代码步骤均给出完整前后代码。

**Type consistency:** `send_setpoint_normalized(ioa, nva: i16, ...)` 与调用处 `send_setpoint_normalized(ioa, value /*i16*/, ...)` 一致；`build_setpoint_normalized(.., nva: i16, ..)` 与 `build_control_frames_setpoint_norm(.., value: i16, ..)` 均 i16；`normalized_raw_string(value: f32) -> String` 定义于 commands 模块、测试以 `super::normalized_raw_string` 引用、`point_to_info` 以 `normalized_raw_string(*value)` 调用，三处一致；i18n key `valueRangeNormalized` 在 zh-CN 接口+值、en-US 值三处齐备，模板 `t('control.valueRangeNormalized')` 对应。
