//! Point simulation is queued before assigning IEC sequence numbers. Each
//! socket writer meters logical point updates, leaving protocol replies free
//! to pass during the delay without reordering already numbered I-frames.
use super::*;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub(super) const QUEUE_CAPACITY: usize = 1024;

pub(super) struct MutationLifetime(Arc<std::sync::atomic::AtomicBool>);

impl Default for MutationLifetime {
    fn default() -> Self {
        Self(Arc::new(std::sync::atomic::AtomicBool::new(true)))
    }
}

impl Drop for MutationLifetime {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

#[derive(Clone)]
pub(super) struct PendingPoint {
    common_address: u16,
    point: DataPoint,
    derive_tb: bool,
    active: Arc<std::sync::atomic::AtomicBool>,
}

pub(super) async fn enqueue(
    stations: &SharedStations,
    connections: &SharedConnections,
    ca: u16,
    ioa: u32,
    asdu_type: AsduTypeId,
    lifetime: &MutationLifetime,
) {
    let pending = {
        let stations = stations.read().await;
        let Some(station) = stations.get(&ca) else {
            return;
        };
        let Some(point) = station.data_points.get(ioa, asdu_type) else {
            return;
        };
        if point.asdu_type.category().is_control() {
            return;
        }
        PendingPoint {
            common_address: ca,
            point: point.clone(),
            derive_tb: should_derive_tb(&station.data_points, asdu_type, ioa),
            active: lifetime.0.clone(),
        }
    };
    let senders = {
        let mut connections = connections.write().await;
        connections
            .values_mut()
            .filter_map(|conn| {
                if !conn.started.load(std::sync::atomic::Ordering::SeqCst) {
                    return None;
                }
                conn.last_sent.insert(ioa, pending.point.value.display());
                Some(conn.simulation_tx.clone())
            })
            .collect::<Vec<_>>()
    };
    // Backpressure bounds queued snapshots. Never retain station/connection
    // locks while waiting; disconnect drops the receiver and wakes producers.
    for sender in senders {
        let _ = sender.send(pending.clone()).await;
    }
}

#[derive(Default)]
struct Pacer {
    config: Option<RandomMutationPacing>,
    sent: usize,
    until: Option<Instant>,
}

impl Pacer {
    fn budget(&mut self, config: RandomMutationPacing, now: Instant) -> usize {
        let config = config.normalized();
        if self.config != Some(config) {
            self.config = Some(config);
            self.sent = 0;
            self.until = None;
        }
        if config.delay_ms == 0 {
            return QUEUE_CAPACITY;
        }
        if let Some(until) = self.until {
            if now < until {
                return 0;
            }
            self.until = None;
            self.sent = 0;
        }
        (config.batch_size as usize - self.sent).min(QUEUE_CAPACITY)
    }

    fn record_sent(&mut self, count: usize, now: Instant) {
        let Some(config) = self.config else { return };
        if config.delay_ms == 0 || count == 0 {
            return;
        }
        self.sent += count;
        if self.sent >= config.batch_size as usize {
            self.until = Some(now + Duration::from_millis(config.delay_ms as u64));
        }
    }
}

pub(super) struct SimulationWriter {
    receiver: mpsc::Receiver<PendingPoint>,
    queue: SharedQueue,
    seq: SharedSeq,
    started: Arc<std::sync::atomic::AtomicBool>,
    ops: SharedRemoteOps,
    log: Option<Arc<LogCollector>>,
    pending_logs: Vec<LogEntry>,
    pacer: Pacer,
}

impl SimulationWriter {
    pub(super) fn new(
        receiver: mpsc::Receiver<PendingPoint>,
        queue: SharedQueue,
        seq: SharedSeq,
        started: Arc<std::sync::atomic::AtomicBool>,
        ops: SharedRemoteOps,
        log: Option<Arc<LogCollector>>,
    ) -> Self {
        Self {
            receiver,
            queue,
            seq,
            started,
            ops,
            log,
            pending_logs: Vec::new(),
            pacer: Pacer::default(),
        }
    }

    pub(super) async fn prepare(&mut self) -> (Vec<u8>, usize) {
        let ops = self.ops.read().await.clone();
        let budget = self.pacer.budget(ops.random_pacing, Instant::now());
        let mut count = 0;
        self.pending_logs.clear();
        // Hold sequence state until the frames are appended, so another sender
        // cannot append a later sequence number ahead of this batch.
        let mut seq = self.seq.lock().await;
        let mut queue = self.queue.lock().await;
        if self.started.load(std::sync::atomic::Ordering::SeqCst) {
            for _ in 0..QUEUE_CAPACITY {
                if count >= budget {
                    break;
                }
                let Ok(pending) = self.receiver.try_recv() else {
                    break;
                };
                if !pending.active.load(std::sync::atomic::Ordering::SeqCst) {
                    continue;
                }
                let p = &pending.point;
                let ca = pending.common_address.to_le_bytes();
                let continuous = if p.asdu_type.is_timestamped() {
                    ops.upload_mode_timestamped == UploadMode::Continuous
                } else {
                    ops.upload_mode_untimestamped == UploadMode::Continuous
                };
                let grouped = if ops.auto_packing && continuous {
                    encode_points_grouped(&[p], 3, &ca, &mut seq, p.asdu_type.is_timestamped())
                } else {
                    None
                };
                if let Some(bytes) = grouped {
                    queue.extend(bytes);
                } else {
                    queue.extend(encode_point_frame_ex(p, 3, &ca, &mut seq, None));
                    if pending.derive_tb
                        && ops.sync_tb_by_category.enabled_for(p.asdu_type.category())
                    {
                        queue.extend(encode_point_frame_ex(p, 3, &ca, &mut seq, Some(true)));
                    }
                }
                // A derived timestamped companion belongs to the same point update.
                count += 1;
                if self.log.is_some() {
                    self.pending_logs.push(LogEntry::new(
                        Direction::Tx,
                        FrameLabel::IFrame(p.asdu_type.name().into()),
                        format!(
                            "仿真上送 (COT=3) IOA={} {} val={} CA={}",
                            p.ioa,
                            p.asdu_type.name(),
                            p.value.display(),
                            pending.common_address
                        ),
                    ));
                }
            }
        } else {
            while self.receiver.try_recv().is_ok() {}
            self.pacer = Pacer::default();
        }
        (std::mem::take(&mut *queue), count)
    }

    pub(super) fn sent(&mut self, count: usize) {
        self.pacer.record_sent(count, Instant::now());
        if let Some(log) = &self.log {
            for entry in self.pending_logs.drain(..) {
                log.try_add(entry);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cumulative_budget_and_live_changes() {
        let now = Instant::now();
        let cfg = RandomMutationPacing {
            batch_size: 3,
            delay_ms: 50,
        };
        let mut pacer = Pacer::default();
        assert_eq!(pacer.budget(cfg, now), 3);
        pacer.record_sent(1, now);
        assert_eq!(pacer.budget(cfg, now), 2);
        pacer.record_sent(2, now);
        assert_eq!(pacer.budget(cfg, now + Duration::from_millis(49)), 0);
        assert_eq!(pacer.budget(cfg, now + Duration::from_millis(50)), 3);
        pacer.record_sent(3, now + Duration::from_millis(50));
        assert_eq!(
            pacer.budget(RandomMutationPacing { delay_ms: 0, ..cfg }, now),
            QUEUE_CAPACITY
        );
        assert_eq!(
            pacer.budget(
                RandomMutationPacing {
                    batch_size: 1,
                    ..cfg
                },
                now
            ),
            1
        );
    }

    #[test]
    fn batches_larger_than_queue_capacity_keep_the_same_counter() {
        let now = Instant::now();
        let cfg = RandomMutationPacing::default();
        let mut pacer = Pacer::default();
        assert_eq!(pacer.budget(cfg, now), 1024);
        pacer.record_sent(1024, now);
        assert_eq!(pacer.budget(cfg, now), 976);
        pacer.record_sent(976, now);
        assert_eq!(pacer.budget(cfg, now), 0);
    }

    #[tokio::test]
    async fn derived_frames_count_as_one_point_and_control_replies_pass_during_delay() {
        let (tx, rx) = mpsc::channel(QUEUE_CAPACITY);
        let queue = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let seq = Arc::new(tokio::sync::Mutex::new(SeqState::default()));
        let started = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let mut ops = RemoteOperationConfig::default();
        ops.auto_packing = false;
        ops.sync_tb_by_category.sp = true;
        ops.random_pacing = RandomMutationPacing {
            batch_size: 2,
            delay_ms: 60_000,
        };
        let ops = Arc::new(RwLock::new(ops));
        let mut writer = SimulationWriter::new(
            rx,
            queue.clone(),
            seq.clone(),
            started.clone(),
            ops.clone(),
            None,
        );
        let lifetime = MutationLifetime::default();
        for ioa in 1..=3 {
            tx.send(PendingPoint {
                common_address: 1,
                point: DataPoint::new(ioa, AsduTypeId::MSpNa1),
                derive_tb: true,
                active: lifetime.0.clone(),
            })
            .await
            .unwrap();
        }
        let (mut bytes, count) = writer.prepare().await;
        assert_eq!(count, 2);
        let mut frames = Vec::new();
        while let Some(frame) = take_next_apdu(&mut bytes) {
            frames.push(frame);
        }
        assert_eq!(
            frames.len(),
            4,
            "two logical updates include their TB companions"
        );
        writer.sent(count);
        let ssn = seq.lock().await.ssn;
        let reply = vec![0x68, 4, 0x83, 0, 0, 0];
        queue.lock().await.extend(&reply);
        assert_eq!(writer.prepare().await, (reply, 0));
        assert_eq!(
            seq.lock().await.ssn,
            ssn,
            "delayed updates must remain unnumbered"
        );
        // Restarting/reconfiguring a point invalidates the previous task's backlog.
        drop(lifetime);
        ops.write().await.random_pacing.delay_ms = 0;
        assert_eq!(writer.prepare().await, (vec![], 0));
        assert_eq!(seq.lock().await.ssn, ssn);
        // STOPDT discards unsent simulation data without advancing SSN.
        let lifetime = MutationLifetime::default();
        tx.send(PendingPoint {
            common_address: 1,
            point: DataPoint::new(4, AsduTypeId::MSpNa1),
            derive_tb: false,
            active: lifetime.0.clone(),
        })
        .await
        .unwrap();
        started.store(false, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(writer.prepare().await, (vec![], 0));
        started.store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(writer.prepare().await, (vec![], 0));
        assert_eq!(seq.lock().await.ssn, ssn);
    }
}
