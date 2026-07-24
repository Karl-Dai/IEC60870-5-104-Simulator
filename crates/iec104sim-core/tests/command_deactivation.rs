//! 回归测试:主站下发「停止激活」(COT=8)时,子站应回复「停止确认」(COT=9,
//! DeactivationCon),而非错误地按激活处理。
//!
//! 覆盖三类 ASDU:
//! - 命令(type 45-50):COT=8 → 回 9,不执行写值、不发终止/突发。
//! - 总召唤 GI(type 100):COT=8 → 回 9,不上送全量数据、不发激活终止。
//! - 计数量召唤(type 101):COT=8 → 回 9,不上送累计量、不发激活终止。
//!
//! 协议依据(IEC 60870-5-101/104):COT=6(激活)→ 回 COT=7(激活确认)+ 数据 + COT=10
//! (激活终止);COT=8(停止激活)→ 仅回 COT=9(停止确认),取消进行中的操作。
//!
//! 本测试用裸 TCP 直接驱动 slave,避免 master 侧封装对 COT 的默认假设。

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use iec104sim_core::log_collector::LogCollector;
use iec104sim_core::log_entry::Direction;
use iec104sim_core::master::{MasterConfig, MasterConnection};
use iec104sim_core::slave::{SlaveServer, SlaveTransportConfig, Station};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// 取 I 帧的 COT(ASDU 第 2 字节,即整帧第 9 字节,索引 8)。
fn iframe_cot(frame: &[u8]) -> u8 {
    assert!(frame.len() >= 10, "frame too short for COT: {:?}", frame);
    frame[8]
}

/// 取 COT 字节的 negative-confirm 位(bit6,0x40),符合 IEC 60870-5-101 §7.2.2.3
/// (与 decode.rs 一致:cot=byte&0x3F,negative=byte&0x40,test=byte&0x80)。
fn iframe_negative(frame: &[u8]) -> bool {
    assert!(
        frame.len() >= 10,
        "frame too short for negative bit: {:?}",
        frame
    );
    frame[8] & 0x40 != 0
}

/// 读一条完整 104 帧,返回 (整帧字节, ctrl1)。帧不完整或读到 EOF 返回 None。
fn read_one_frame(stream: &mut TcpStream) -> Option<(Vec<u8>, u8)> {
    let mut hdr = [0u8; 2];
    if stream.read_exact(&mut hdr).is_err() {
        return None;
    }
    if hdr[0] != 0x68 {
        return None;
    }
    let len = hdr[1] as usize;
    let mut body = vec![0u8; len];
    if stream.read_exact(&mut body).is_err() {
        return None;
    }
    let ctrl1 = body[0];
    let mut full = vec![hdr[0], hdr[1]];
    full.extend_from_slice(&body);
    Some((full, ctrl1))
}

/// 起一个带 1 个站点(每分类 2 点,含累计量)的 slave,返回 (slave, port)。
async fn spawn_slave() -> (SlaveServer, u16) {
    let port = free_port();
    let transport = SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        ..Default::default()
    };
    let mut slave = SlaveServer::new(transport);
    slave
        .add_station(Station::with_default_points(1, "Test", 2))
        .await
        .unwrap();
    slave.start().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    (slave, port)
}

/// 连接 + STARTDT,返回已就绪的 TcpStream。
fn connect_and_startdt(port: u16) -> TcpStream {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(2))).ok();
    stream
        .write_all(&[0x68, 0x04, 0x07, 0x00, 0x00, 0x00])
        .unwrap();
    // 读到 STARTDT CON (U-frame, ctrl1=0x0B)。
    loop {
        match read_one_frame(&mut stream) {
            Some((_frame, ctrl1)) if ctrl1 & 0x03 == 0x03 => {
                assert_eq!(ctrl1, 0x0B, "expected STARTDT CON (0x0B)");
                return stream;
            }
            Some(_) => continue,
            None => panic!("EOF before STARTDT CON"),
        }
    }
}

/// 读到第一条 I 帧(跳过中间的 S 帧),返回整帧字节。
fn first_iframe(stream: &mut TcpStream) -> Vec<u8> {
    loop {
        match read_one_frame(stream) {
            Some((frame, ctrl1)) if ctrl1 & 0x01 == 0 => return frame,
            Some(_) => continue,
            None => panic!("EOF before any I-frame"),
        }
    }
}

/// 发送一帧裸 APDU。
fn send(stream: &mut TcpStream, frame: &[u8]) {
    stream.write_all(frame).unwrap();
}

// =========================================================================
// 命令(type 45):COT=8 → 回 COT=9
// =========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slave_replies_deactivation_con_for_cot8_command() {
    let (mut slave, port) = spawn_slave().await;
    let mut stream = connect_and_startdt(port);

    // C_SC_NA_1: IOA=1, value=true, select=false, QU=0, COT=8
    let ca_bytes = 1u16.to_le_bytes();
    let ioa_bytes = 1u32.to_le_bytes();
    send(
        &mut stream,
        &[
            0x68,
            0x0E,
            0x00,
            0x00,
            0x00,
            0x00,
            45,
            0x01,
            8,
            0x00,
            ca_bytes[0],
            ca_bytes[1],
            ioa_bytes[0],
            ioa_bytes[1],
            ioa_bytes[2],
            0x01,
        ],
    );

    let resp = first_iframe(&mut stream);
    assert_eq!(iframe_cot(&resp), 9, "COT=8 命令应回 COT=9(停止确认)");

    let _ = slave.stop().await;
}

// =========================================================================
// 总召唤 GI(type 100):COT=8 → 只回 COT=9,不上送数据、不发终止
// =========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slave_replies_deactivation_con_for_cot8_gi() {
    let (mut slave, port) = spawn_slave().await;
    let mut stream = connect_and_startdt(port);

    // 先发一次正常 GI(COT=6)确认 slave 会回数据 + 终止,验证「停止激活」下确实少了这些。
    let ca_bytes = 1u16.to_le_bytes();
    send(
        &mut stream,
        &[
            0x68,
            0x0E,
            0x00,
            0x00,
            0x00,
            0x00,
            100,
            0x01,
            6,
            0x00, // type=100, VSQ=1, COT=6(激活)
            ca_bytes[0],
            ca_bytes[1],
            0x00,
            0x00,
            0x00,
            20, // QOI=20(总召唤)
        ],
    );
    // 正常 GI 应收到: ActCon(COT=7) + 若干数据(COT=20) + ActTerm(COT=10)。
    let act_con = first_iframe(&mut stream);
    assert_eq!(iframe_cot(&act_con), 7, "GI 激活应回 COT=7");
    // 排空后续 GI 数据 + 终止帧(给足时间)。
    std::thread::sleep(Duration::from_millis(500));
    while read_one_frame(&mut stream).is_some() {}

    // 现在发 COT=8(停止激活)的 GI。
    send(
        &mut stream,
        &[
            0x68,
            0x0E,
            0x00,
            0x00,
            0x00,
            0x00,
            100,
            0x01,
            8,
            0x00, // COT=8(停止激活)
            ca_bytes[0],
            ca_bytes[1],
            0x00,
            0x00,
            0x00,
            20,
        ],
    );

    let resp = first_iframe(&mut stream);
    assert_eq!(
        iframe_cot(&resp),
        9,
        "GI COT=8(停止激活)应回 COT=9(停止确认),实际回 COT={}",
        iframe_cot(&resp)
    );

    // 关键断言:停止激活后不应再上送数据帧或终止帧。等一段时间确认无后续 I 帧。
    // (仅回单帧 COT=9;若错误地按激活处理,会紧跟数据帧 COT=20 和终止帧 COT=10。)
    stream
        .set_read_timeout(Some(Duration::from_millis(600)))
        .ok();
    let mut extra_iframes = 0u32;
    while let Some((frame, ctrl1)) = read_one_frame(&mut stream) {
        if ctrl1 & 0x01 == 0 {
            extra_iframes += 1;
            // 若出现 I 帧,它不应该是数据(COT=20)或终止(COT=10)。
            let cot = iframe_cot(&frame);
            assert!(
                cot != 20 && cot != 10,
                "GI 停止激活后不应再上送数据(COT=20)或终止(COT=10),但收到 COT={}",
                cot
            );
        }
    }
    assert_eq!(
        extra_iframes, 0,
        "GI 停止激活应只回单帧 COT=9,不应有额外 I 帧(实际 {} 帧)",
        extra_iframes
    );

    let _ = slave.stop().await;
}

// =========================================================================
// 计数量召唤(type 101):COT=8 → 只回 COT=9,不上送累计量、不发终止
// =========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slave_replies_deactivation_con_for_cot8_counter() {
    let (mut slave, port) = spawn_slave().await;
    let mut stream = connect_and_startdt(port);

    let ca_bytes = 1u16.to_le_bytes();
    // C_CI_NA_1: type=101, COT=8(停止激活), QCC=5(总计数 + 不冻结)
    send(
        &mut stream,
        &[
            0x68,
            0x0E,
            0x00,
            0x00,
            0x00,
            0x00,
            101,
            0x01,
            8,
            0x00, // COT=8(停止激活)
            ca_bytes[0],
            ca_bytes[1],
            0x00,
            0x00,
            0x00,
            5, // QCC=5
        ],
    );

    let resp = first_iframe(&mut stream);
    assert_eq!(
        iframe_cot(&resp),
        9,
        "计数量召唤 COT=8 应回 COT=9(停止确认),实际回 COT={}",
        iframe_cot(&resp)
    );

    // 停止激活后不应上送累计量(COT=37)或终止(COT=10)。
    stream
        .set_read_timeout(Some(Duration::from_millis(600)))
        .ok();
    let mut extra_iframes = 0u32;
    while let Some((frame, ctrl1)) = read_one_frame(&mut stream) {
        if ctrl1 & 0x01 == 0 {
            extra_iframes += 1;
            let cot = iframe_cot(&frame);
            assert!(
                cot != 37 && cot != 10,
                "计数量召唤停止激活后不应上送累计量(COT=37)或终止(COT=10),但收到 COT={}",
                cot
            );
        }
    }
    assert_eq!(
        extra_iframes, 0,
        "计数量召唤停止激活应只回单帧 COT=9,不应有额外 I 帧(实际 {} 帧)",
        extra_iframes
    );

    let _ = slave.stop().await;
}

// =========================================================================
// master 侧:send_interrogation_deactivation / send_counter_read_deactivation
// 必须经公共 API 发出 COT=8 帧,slave 收到后回 COT=9。这覆盖 master 发送链路。
// 用 slave 的 log_collector 抓 Rx 原始帧,断言其 COT 字节 == 8。
// =========================================================================

async fn spawn_slave_with_log(port: u16) -> (SlaveServer, std::sync::Arc<LogCollector>) {
    let lc = std::sync::Arc::new(LogCollector::new());
    let mut slave = SlaveServer::new(SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        ..Default::default()
    })
    .with_log_collector(lc.clone());
    slave
        .add_station(Station::with_default_points(1, "Test", 2))
        .await
        .unwrap();
    slave.start().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    (slave, lc)
}

async fn connect_master(port: u16) -> MasterConnection {
    let mut master = MasterConnection::new(MasterConfig {
        target_address: "127.0.0.1".into(),
        port,
        common_address: 1,
        ..Default::default()
    });
    master.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    master
}

/// 在 slave 日志里找第一条 Direction::Rx 且 ASDU 类型 == asdu_type 的原始帧,
/// 返回其 COT(raw_bytes[8])。找不到则 panic。
async fn first_rx_cot(lc: &LogCollector, asdu_type: u8) -> u8 {
    let entries = lc.get_all().await;
    for e in entries.iter() {
        if e.direction != Direction::Rx {
            continue;
        }
        if let Some(ref raw) = e.raw_bytes {
            // I 帧至少 14 字节;ASDU 类型在 raw[6],COT 在 raw[8]。
            if raw.len() >= 14 && raw[6] == asdu_type {
                return raw[8];
            }
        }
    }
    panic!("slave 日志中未找到 type={} 的 Rx I 帧", asdu_type);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn master_emits_cot8_gi_deactivation_via_public_api() {
    let port = free_port();
    let (mut slave, lc) = spawn_slave_with_log(port).await;
    let mut master = connect_master(port).await;

    // master 公共 API 下发停止激活 GI(COT=8)。
    master.send_interrogation_deactivation(1).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // slave 应已收到 type=100 的 Rx 帧,其 COT 字节应为 8(停止激活)。
    let cot = first_rx_cot(&lc, 100).await;
    assert_eq!(
        cot, 8,
        "master send_interrogation_deactivation 应发出 COT=8 的 GI 帧,实际 slave 收到 COT={}",
        cot
    );

    master.disconnect().await.unwrap();
    let _ = slave.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn master_emits_cot8_counter_deactivation_via_public_api() {
    let port = free_port();
    let (mut slave, lc) = spawn_slave_with_log(port).await;
    let mut master = connect_master(port).await;

    // master 公共 API 下发停止激活计数量召唤(COT=8)。
    master.send_counter_read_deactivation(1).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // slave 应已收到 type=101 的 Rx 帧,其 COT 字节应为 8(停止激活)。
    let cot = first_rx_cot(&lc, 101).await;
    assert_eq!(
        cot, 8,
        "master send_counter_read_deactivation 应发出 COT=8 的计数量召唤帧,实际 slave 收到 COT={}",
        cot
    );

    master.disconnect().await.unwrap();
    let _ = slave.stop().await;
}

// =========================================================================
// 时钟同步(type 103):C_CS_NA_1 规约为单次激活型命令(IEC 60870-5-101 §7.2.6.4),
// 无 COT=8 去激活语义,禁止回 COT=9。仅 COT=6(激活)合法→回 COT=7(激活确认);
// 非激活 COT(含 COT=8)属协议错误→按 lib60870 拒收路径回 COT=45(UNKNOWN_COT)
// +negative-confirm(bit6),不执行对时。本节回归该行为。
// =========================================================================

/// 构造一条 C_CS_NA_1(103) 帧的裸 APDU。cot 为 COT 低 6 位值。
/// 布局:0x68 0x14 | 4 控制字节 | 103,0x01,cot,0x00 | CA(2) | IOA(3,=0) | CP56Time2a(7)。
fn build_clock_sync_frame(ca: u16, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    vec![
        0x68,
        0x14,
        0x00,
        0x00,
        0x00,
        0x00,
        103,
        0x01,
        cot,
        0x00,
        ca_bytes[0],
        ca_bytes[1],
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slave_rejects_non_activation_cot_for_clock_sync() {
    let (mut slave, port) = spawn_slave().await;
    let mut stream = connect_and_startdt(port);

    // COT=8(停止激活)对 103 是协议错误:应回 COT=45(UNKNOWN_COT)+negative-confirm(bit6),
    // 而非 COT=9(停止确认),也非 COT=7(激活确认)。
    send(&mut stream, &build_clock_sync_frame(1, 8));

    let resp = first_iframe(&mut stream);
    assert_eq!(
        iframe_cot(&resp) & 0x3F,
        45,
        "103 非激活 COT 应回 COT=45(UNKNOWN_COT),实际 COT 字节={:#04X}",
        resp[8]
    );
    assert!(
        iframe_negative(&resp),
        "103 非激活 COT 应回 negative-confirm(bit6),COT 字节={:#04X}",
        resp[8]
    );

    let _ = slave.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slave_acks_activation_cot_for_clock_sync() {
    let (mut slave, port) = spawn_slave().await;
    let mut stream = connect_and_startdt(port);

    // COT=6(激活)合法:回 COT=7(激活确认),无 negative-confirm。
    send(&mut stream, &build_clock_sync_frame(1, 6));

    let resp = first_iframe(&mut stream);
    assert_eq!(
        iframe_cot(&resp) & 0x3F,
        7,
        "103 激活(COT=6) 应回 COT=7(激活确认),实际 COT 字节={:#04X}",
        resp[8]
    );
    assert!(
        !iframe_negative(&resp),
        "103 激活确认不应带 negative-confirm,COT 字节={:#04X}",
        resp[8]
    );

    let _ = slave.stop().await;
}

// =========================================================================
// COT=8 去激活必须取消在途的召唤扫描任务(IEC 60870-5-101 §7.2.6.1):
// 主站发 GI(COT=6) 触发 slave 后台 spawn run_interrogation,若主站立即发 COT=8,
// slave 应 abort 该任务,使其不再继续上送数据帧、更不发 COT=10 激活终止帧。
// 断言核心:不应收到 COT=10(若 abort 失败,扫描会跑完并发 ActTerm)。
// =========================================================================

/// 起一个带 1 个站点(每分类 count_per_category 点)的 slave,返回 (slave, port)。
async fn spawn_slave_many_points(count_per_category: u32) -> (SlaveServer, u16) {
    let port = free_port();
    let transport = SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        ..Default::default()
    };
    let mut slave = SlaveServer::new(transport);
    slave
        .add_station(Station::with_default_points(1, "Test", count_per_category))
        .await
        .unwrap();
    slave.start().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    (slave, port)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slave_cot8_deactivation_aborts_inflight_gi_scan() {
    // 多点让 run_interrogation 有足够段间 await 点,提高 abort 命中在途任务的概率。
    let (mut slave, port) = spawn_slave_many_points(500).await;
    let mut stream = connect_and_startdt(port);

    let ca_bytes = 1u16.to_le_bytes();

    // 先发正常 GI(COT=6):slave 回 ActCon(7) 并 spawn run_interrogation 扫描任务。
    send(
        &mut stream,
        &[
            0x68,
            0x0E,
            0x00,
            0x00,
            0x00,
            0x00,
            100,
            0x01,
            6,
            0x00,
            ca_bytes[0],
            ca_bytes[1],
            0x00,
            0x00,
            0x00,
            20,
        ],
    );
    // 立即(不等扫描跑完)发 COT=8 停止激活。
    send(
        &mut stream,
        &[
            0x68,
            0x0E,
            0x00,
            0x00,
            0x00,
            0x00,
            100,
            0x01,
            8,
            0x00,
            ca_bytes[0],
            ca_bytes[1],
            0x00,
            0x00,
            0x00,
            20,
        ],
    );

    // 排空所有帧,收集 COT。应至少含 COT=9(停止确认);关键是不应含 COT=10(激活终止)。
    std::thread::sleep(Duration::from_millis(600));
    stream
        .set_read_timeout(Some(Duration::from_millis(600)))
        .ok();
    let mut got_cot9 = false;
    while let Some((frame, ctrl1)) = read_one_frame(&mut stream) {
        if ctrl1 & 0x01 != 0 {
            continue;
        } // 跳过 S 帧
        let cot = iframe_cot(&frame) & 0x3F;
        if cot == 9 {
            got_cot9 = true;
        }
        assert!(
            cot != 10,
            "GI 去激活后不应收到 COT=10(激活终止):abort 失败,在途扫描跑完并发了终止帧。收到 COT={}",
            cot
        );
    }
    assert!(got_cot9, "应收到 COT=9(停止确认)");

    let _ = slave.stop().await;
}

// =========================================================================
// answer_general_interrogation=false 下,COT=8 去激活仍须回 COT=9(停止确认)。
// 去激活确认是管理层回复,不受数据上送抑制开关门控;否则主站 t1 超时。
// =========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slave_replies_deactivation_con_even_when_gi_suppressed() {
    let (mut slave, port) = spawn_slave().await;
    // 关闭 GI 数据上送(answer_general_interrogation=false)。
    let mut ops = slave.get_remote_ops().await;
    ops.answer_general_interrogation = false;
    slave.set_remote_ops(ops).await;

    let mut stream = connect_and_startdt(port);
    let ca_bytes = 1u16.to_le_bytes();
    // COT=8 停止激活:即便数据上送被抑制,仍应回 COT=9(不再静默)。
    send(
        &mut stream,
        &[
            0x68,
            0x0E,
            0x00,
            0x00,
            0x00,
            0x00,
            100,
            0x01,
            8,
            0x00,
            ca_bytes[0],
            ca_bytes[1],
            0x00,
            0x00,
            0x00,
            20,
        ],
    );

    let resp = first_iframe(&mut stream);
    assert_eq!(
        iframe_cot(&resp) & 0x3F,
        9,
        "answer_general_interrogation=false 时 COT=8 仍应回 COT=9,实际 COT 字节={:#04X}",
        resp[8]
    );

    let _ = slave.stop().await;
}

// =========================================================================
// answer_commands=false 下,命令族(45-50)的 COT=8 停止激活仍须回 COT=9。
// answer_commands 抑制的是命令执行(写值/终止/突发),不抑制停止确认。
// =========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slave_replies_deactivation_con_for_command_even_when_commands_suppressed() {
    let (mut slave, port) = spawn_slave().await;
    let mut ops = slave.get_remote_ops().await;
    ops.answer_commands = false;
    slave.set_remote_ops(ops).await;

    let mut stream = connect_and_startdt(port);
    let ca_bytes = 1u16.to_le_bytes();
    let ioa_bytes = 1u32.to_le_bytes();
    // C_SC_NA_1(45): IOA=1, value=true, S/E=execute, QU=0, COT=8(停止激活)
    send(
        &mut stream,
        &[
            0x68,
            0x0E,
            0x00,
            0x00,
            0x00,
            0x00,
            45,
            0x01,
            8,
            0x00,
            ca_bytes[0],
            ca_bytes[1],
            ioa_bytes[0],
            ioa_bytes[1],
            ioa_bytes[2],
            0x01,
        ],
    );

    let resp = first_iframe(&mut stream);
    assert_eq!(
        iframe_cot(&resp) & 0x3F,
        9,
        "answer_commands=false 时 COT=8 命令仍应回 COT=9,实际 COT 字节={:#04X}",
        resp[8]
    );

    let _ = slave.stop().await;
}

// =========================================================================
// cause 掩码:主站叠加 test bit(0x80)/negative bit(0x40)的 COT=8 仍应命中去激活。
// COT 字节布局 cot=byte&0x3F,故 8|0x80=0x88 应识别为 cause=8 → 回 COT=9。
// =========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slave_treats_cot8_with_test_bit_as_deactivation() {
    let (mut slave, port) = spawn_slave().await;
    let mut stream = connect_and_startdt(port);
    let ca_bytes = 1u16.to_le_bytes();
    // COT=8|0x80(test bit)=0x88:cause 低 6 位仍为 8(停止激活)。
    send(
        &mut stream,
        &[
            0x68,
            0x0E,
            0x00,
            0x00,
            0x00,
            0x00,
            100,
            0x01,
            0x88,
            0x00,
            ca_bytes[0],
            ca_bytes[1],
            0x00,
            0x00,
            0x00,
            20,
        ],
    );

    let resp = first_iframe(&mut stream);
    assert_eq!(
        iframe_cot(&resp) & 0x3F,
        9,
        "COT=8|test(0x88) 应按 cause=8 处理回 COT=9,实际 COT 字节={:#04X}",
        resp[8]
    );

    let _ = slave.stop().await;
}

// =========================================================================
// test 位回显:收帧带 test bit(0x80)时,回 COT 确认帧应仍带 test 位(§7.2.2.3)。
// 发 COT=6|0x80(test)=0x86 的 GI 激活,断言回 COT=7 仍带 0x80。
// =========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slave_echoes_test_bit_in_gi_activation_ack() {
    let (mut slave, port) = spawn_slave().await;
    let mut stream = connect_and_startdt(port);
    let ca_bytes = 1u16.to_le_bytes();
    // COT=6|0x80(test)=0x86。
    send(
        &mut stream,
        &[
            0x68,
            0x0E,
            0x00,
            0x00,
            0x00,
            0x00,
            100,
            0x01,
            0x86,
            0x00,
            ca_bytes[0],
            ca_bytes[1],
            0x00,
            0x00,
            0x00,
            20,
        ],
    );

    let resp = first_iframe(&mut stream);
    assert_eq!(
        iframe_cot(&resp) & 0x3F,
        7,
        "GI 激活(test) 应回 COT=7,实际 COT 字节={:#04X}",
        resp[8]
    );
    assert!(
        resp[8] & 0x80 != 0,
        "回帧应回显 test bit(0x80),实际 COT 字节={:#04X}",
        resp[8]
    );

    let _ = slave.stop().await;
}

// =========================================================================
// 103 短帧守卫:帧长 < 22(CP56Time2a 不完整)应按畸形拒收回 COT=44(unknown-type)+negative,
// 而非当合法对时处理回 COT=7。
// =========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slave_rejects_short_clock_sync_frame() {
    let (mut slave, port) = spawn_slave().await;
    let mut stream = connect_and_startdt(port);
    let ca_bytes = 1u16.to_le_bytes();
    // 构造一条短 103 帧:声明 body 长度=14(0x0E),内容含 ASDU 头+CA+部分 IOA 但无完整 CP56(完整 103 帧 body=20/总 22)。
    // body 14 字节 = 控制(4)+ASDU 头(4)+CA(2)+IOA(3)+1 填充,缺 7 字节 CP56Time2a。
    send(
        &mut stream,
        &[
            0x68,
            0x0E,
            0x00,
            0x00,
            0x00,
            0x00,
            103,
            0x01,
            6,
            0x00,
            ca_bytes[0],
            ca_bytes[1],
            0x00,
            0x00,
            0x00,
            0x00,
        ],
    );

    let resp = first_iframe(&mut stream);
    assert_eq!(
        iframe_cot(&resp) & 0x3F,
        44,
        "103 短帧(无 CP56)应按畸形回 COT=44(UNKNOWN_TYPE),实际 COT 字节={:#04X}",
        resp[8]
    );
    assert!(
        iframe_negative(&resp),
        "103 短帧拒收应带 negative-confirm(bit6),COT 字节={:#04X}",
        resp[8]
    );

    let _ = slave.stop().await;
}

// =========================================================================
// master builder cot mask:send_interrogation_with_qoi 传 cot=Some(255),
// 实际发出帧的 COT 字节应被 mask 成 255&0x3F=63(而非裸 255 污染帧)。
// =========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn master_masks_gi_cot_to_cause_field() {
    let port = free_port();
    let (mut slave, lc) = spawn_slave_with_log(port).await;
    let mut master = connect_master(port).await;

    // 传入超出 cause 域的 cot(255),验证 builder mask 到低 6 位(255&0x3F=63)。
    master
        .send_interrogation_with_qoi(1, None, Some(255))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;

    let cot = first_rx_cot(&lc, 100).await;
    assert_eq!(
        cot, 63,
        "cot=255 应被 mask 成 255&0x3F=63,实际 slave 收到 COT={}",
        cot
    );

    master.disconnect().await.unwrap();
    let _ = slave.stop().await;
}

// =========================================================================
// broadcast deactivation:master send_interrogation_deactivation(broadcast_addr)
// 经公共 API 对广播地址发 COT=8。验证发出帧 CA=广播地址、COT=8。
// =========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn master_emits_cot8_broadcast_gi_deactivation() {
    let port = free_port();
    let (mut slave, lc) = spawn_slave_with_log(port).await;
    let mut master = MasterConnection::new(MasterConfig {
        target_address: "127.0.0.1".into(),
        port,
        common_address: 1,
        broadcast_address: 0xFF00,
        ..Default::default()
    });
    master.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 对广播地址(0xFF00)发停止激活 GI。
    master
        .send_interrogation_deactivation(0xFF00)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 找到 type=100 的 Rx 帧,断言 COT=8。
    let mut found = false;
    let entries = lc.get_all().await;
    for e in entries.iter() {
        if e.direction != Direction::Rx {
            continue;
        }
        if let Some(ref raw) = e.raw_bytes {
            if raw.len() >= 14 && raw[6] == 100 {
                assert_eq!(raw[8], 8, "广播 GI 去激活帧 COT 应为 8,实际 {}", raw[8]);
                // CA 在 raw[10..12](小端)。0xFF00 → [0x00, 0xFF]。
                let ca = u16::from_le_bytes([raw[10], raw[11]]);
                assert_eq!(ca, 0xFF00, "广播帧 CA 应为 0xFF00,实际 {:#06X}", ca);
                found = true;
                break;
            }
        }
    }
    assert!(found, "slave 未收到 type=100 的广播去激活帧");

    master.disconnect().await.unwrap();
    let _ = slave.stop().await;
}

// =========================================================================
// issue #28:时钟同步应答开关(answer_clock_sync)与 Actterm 开关(send_act_term)
// =========================================================================

/// answer_clock_sync=false:合法激活对时也应按未知类型拒收(COT=44 + P/N),不执行对时。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn clock_sync_rejected_when_answer_disabled() {
    let (mut slave, port) = spawn_slave().await;
    let mut ops = slave.get_remote_ops().await;
    ops.answer_clock_sync = false;
    slave.set_remote_ops(ops).await;

    let mut stream = connect_and_startdt(port);
    send(&mut stream, &build_clock_sync_frame(1, 6));

    let resp = first_iframe(&mut stream);
    assert_eq!(
        iframe_cot(&resp) & 0x3F,
        44,
        "禁用后 103 应回 COT=44(UNKNOWN_TYPE),实际 COT 字节={:#04X}",
        resp[8]
    );
    assert!(
        iframe_negative(&resp),
        "禁用后 103 拒收应带 negative-confirm(bit6),COT 字节={:#04X}",
        resp[8]
    );

    let _ = slave.stop().await;
}

/// 构造 C_SC_NA_1(45) 执行帧:COT=6,IOA=1,SCO=ON(0x01)。
fn build_sc_execute_frame(ca: u16) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    vec![
        0x68, 0x0E, 0x00, 0x00, 0x00, 0x00,
        45, 0x01, 6, 0x00,
        ca_bytes[0], ca_bytes[1],
        0x01, 0x00, 0x00, // IOA=1
        0x01, // SCO: execute, ON
    ]
}

/// send_act_term=true(默认):执行应答 ACT_CON(7) 后应跟随 ACT_TERM(10)。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn execute_sends_act_term_by_default() {
    let (mut slave, port) = spawn_slave().await;
    let mut stream = connect_and_startdt(port);

    send(&mut stream, &build_sc_execute_frame(1));

    let first = first_iframe(&mut stream);
    assert_eq!(first[6], 45, "首帧应为 type=45 回显");
    assert_eq!(iframe_cot(&first) & 0x3F, 7, "首帧应为 ACT_CON(7)");

    let mut got_term = false;
    while let Some((frame, ctrl1)) = read_one_frame(&mut stream) {
        if ctrl1 & 0x01 != 0 { continue; }
        if frame.len() > 8 && frame[6] == 45 && iframe_cot(&frame) & 0x3F == 10 {
            got_term = true;
            break;
        }
    }
    assert!(got_term, "默认应追加 ACT_TERM(COT=10)");

    let _ = slave.stop().await;
}

/// send_act_term=false(Actterm 关):仅回 ACT_CON(7),不再追加 ACT_TERM。
/// 自动映射产生的突发上送(COT=3,监视类型)不受影响。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn execute_without_act_term_when_disabled() {
    let (mut slave, port) = spawn_slave().await;
    let mut ops = slave.get_remote_ops().await;
    ops.send_act_term = false;
    slave.set_remote_ops(ops).await;

    let mut stream = connect_and_startdt(port);
    send(&mut stream, &build_sc_execute_frame(1));

    let first = first_iframe(&mut stream);
    assert_eq!(first[6], 45, "首帧应为 type=45 回显");
    assert_eq!(iframe_cot(&first) & 0x3F, 7, "首帧应为 ACT_CON(7)");

    // 后续帧(直到 2s 读超时)不得再出现 type=45 的 ACT_TERM(10)。
    while let Some((frame, ctrl1)) = read_one_frame(&mut stream) {
        if ctrl1 & 0x01 != 0 { continue; }
        assert!(
            !(frame.len() > 8 && frame[6] == 45 && iframe_cot(&frame) & 0x3F == 10),
            "Actterm 关闭后不应出现 ACT_TERM 帧: {:02X?}",
            frame
        );
    }

    let _ = slave.stop().await;
}
