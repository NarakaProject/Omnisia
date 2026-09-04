use std::collections::{BTreeSet, VecDeque};

use glam::IVec3;

use super::adjacency::ADJACENCY_OFFSETS_6;
use super::aggregate::DetachedAggregate;
use super::anchor::AnchorPolicy;
use super::connectivity::{check_structural_connectivity, ConnectivityConfig, ConnectivityStatus};
use super::events::StructuralEvent;
use crate::streaming::store::ChunkStore;
use crate::voxel::VoxelBlock;

/// Snapshot status StructuralSystem untuk rollback transaksional yang persis sama (Option A - Transactional IDs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralTransactionSnapshot {
    pub next_aggregate_id: u64,
    pub detached_ledger_length: usize,
    pub pending_checks: VecDeque<IVec3>,
    pub total_events_processed: usize,
    pub total_connectivity_checks: usize,
    pub total_voxels_inspected: usize,
    pub total_detached_extracted: usize,
    pub pending_unloaded_count: usize,
}

/// Pengelola sistem konektivitas struktural dan siklus hidup detached aggregate
pub struct StructuralSystem {
    pub anchor_policy: AnchorPolicy,
    pub config: ConnectivityConfig,
    pub detached_aggregates: Vec<DetachedAggregate>,
    pub next_aggregate_id: u64,
    pub pending_checks: VecDeque<IVec3>,

    // Metrik telemetri struktural tanpa alokasi per frame
    pub total_events_processed: usize,
    pub total_connectivity_checks: usize,
    pub total_voxels_inspected: usize,
    pub total_detached_extracted: usize,
    pub pending_unloaded_count: usize,
}

impl StructuralSystem {
    pub fn new(anchor_policy: AnchorPolicy) -> Self {
        Self {
            anchor_policy,
            config: ConnectivityConfig::default(),
            detached_aggregates: Vec::new(),
            next_aggregate_id: 1,
            pending_checks: VecDeque::new(),
            total_events_processed: 0,
            total_connectivity_checks: 0,
            total_voxels_inspected: 0,
            total_detached_extracted: 0,
            pending_unloaded_count: 0,
        }
    }

    /// Mengambil snapshot status StructuralSystem untuk rollback transaksional yang persis sama.
    pub fn create_transaction_snapshot(&self) -> StructuralTransactionSnapshot {
        StructuralTransactionSnapshot {
            next_aggregate_id: self.next_aggregate_id,
            detached_ledger_length: self.detached_aggregates.len(),
            pending_checks: self.pending_checks.clone(),
            total_events_processed: self.total_events_processed,
            total_connectivity_checks: self.total_connectivity_checks,
            total_voxels_inspected: self.total_voxels_inspected,
            total_detached_extracted: self.total_detached_extracted,
            pending_unloaded_count: self.pending_unloaded_count,
        }
    }

    /// Memulihkan status StructuralSystem ke kondisi sebelum transaksi (Option A - Transactional IDs).
    pub fn restore_transaction_snapshot(&mut self, snapshot: &StructuralTransactionSnapshot) {
        self.next_aggregate_id = snapshot.next_aggregate_id;
        self.detached_aggregates
            .truncate(snapshot.detached_ledger_length);
        self.pending_checks = snapshot.pending_checks.clone();
        self.total_events_processed = snapshot.total_events_processed;
        self.total_connectivity_checks = snapshot.total_connectivity_checks;
        self.total_voxels_inspected = snapshot.total_voxels_inspected;
        self.total_detached_extracted = snapshot.total_detached_extracted;
        self.pending_unloaded_count = snapshot.pending_unloaded_count;
    }

    /// Mengumpulkan seluruh koordinat tetangga kandidat yang berpotensi lepas
    /// dari sekumpulan event mutasi struktural.
    ///
    /// INVARIAN LOCALITY & FILTERING:
    /// - Hanya event dengan `can_cause_detachment()` yang diperiksa.
    /// - Hanya 6 tetangga ortogonal yang diuji.
    /// - Voxel udara diabaikan (hanya voxel solid pasca-mutasi yang menjadi kandidat).
    /// - Dideduplikasi secara deterministik menggunakan BTreeSet<IVec3>.
    pub fn collect_candidate_seeds(events: &[StructuralEvent], store: &ChunkStore) -> Vec<IVec3> {
        let mut candidate_set = BTreeSet::new();
        for event in events {
            if !event.can_cause_detachment() {
                continue;
            }
            for offset in &ADJACENCY_OFFSETS_6 {
                let neighbor_pos = event.world_voxel + *offset;
                let neighbor_block = store.get_voxel_world(neighbor_pos);
                if !neighbor_block.is_air() {
                    candidate_set.insert((neighbor_pos.x, neighbor_pos.y, neighbor_pos.z));
                }
            }
        }
        candidate_set
            .into_iter()
            .map(|(x, y, z)| IVec3::new(x, y, z))
            .collect()
    }

    /// Merekonsiliasi konektivitas struktural secara batch dari sekumpulan event mutasi
    /// dengan deduplikasi benih kandidat dan ekstraksi deterministik.
    pub fn reconcile_events(
        &mut self,
        events: &[StructuralEvent],
        store: &mut ChunkStore,
    ) -> Vec<DetachedAggregate> {
        self.total_events_processed += events.len();
        let candidate_seeds = Self::collect_candidate_seeds(events, store);
        let mut newly_detached = Vec::new();

        for seed in candidate_seeds {
            let block = store.get_voxel_world(seed);
            if block.is_air() {
                continue;
            }

            self.total_connectivity_checks += 1;
            let status = check_structural_connectivity(
                seed,
                store,
                &self.anchor_policy,
                &self.config,
                Some(&mut self.total_voxels_inspected),
            );

            match status {
                ConnectivityStatus::ConnectedToAnchor => {}
                ConnectivityStatus::PendingUnloadedNeighbor { .. } => {
                    self.pending_unloaded_count += 1;
                    self.pending_checks.push_back(seed);
                }
                ConnectivityStatus::IndeterminateBudgetExceeded { .. } => {
                    self.pending_checks.push_back(seed);
                }
                ConnectivityStatus::Detached { component_voxels } => {
                    if component_voxels.is_empty() {
                        continue;
                    }

                    if let Some(agg) = DetachedAggregate::from_world_voxels(
                        self.next_aggregate_id,
                        &component_voxels,
                    ) {
                        for &(vpos, _) in &component_voxels {
                            store.set_voxel_world(vpos, VoxelBlock::AIR);
                        }

                        self.next_aggregate_id += 1;
                        self.total_detached_extracted += 1;
                        self.detached_aggregates.push(agg.clone());
                        newly_detached.push(agg);
                    }
                }
            }
        }

        newly_detached
    }

    /// Memproses event mutasi struktural dan mengekstrak gugusan yang terputus dari penopang
    ///
    /// INVARIANT LIFECYCLE (GUARDRAIL 5):
    /// Saat aggregate dinyatakan lepas:
    /// detect -> identify complete voxel set -> construct DetachedAggregate
    /// -> remove/transfer voxels from authoritative Chunk state (set to AIR)
    /// -> mark affected chunks dirty -> persistence state becomes dirty
    /// TIDAK BOLEH ADA DOUBLE OWNERSHIP.
    pub fn process_event(
        &mut self,
        event: &StructuralEvent,
        store: &mut ChunkStore,
    ) -> Vec<DetachedAggregate> {
        self.total_events_processed += 1;

        if !event.can_cause_detachment() {
            return Vec::new();
        }

        let mut newly_detached = Vec::new();

        // Evaluasi 6 tetangga dari voxel yang baru saja dihilangkan
        for offset in &ADJACENCY_OFFSETS_6 {
            let neighbor_pos = event.world_voxel + *offset;
            let neighbor_block = store.get_voxel_world(neighbor_pos);

            if neighbor_block.is_air() {
                continue;
            }

            self.total_connectivity_checks += 1;

            let status = check_structural_connectivity(
                neighbor_pos,
                store,
                &self.anchor_policy,
                &self.config,
                Some(&mut self.total_voxels_inspected),
            );

            match status {
                ConnectivityStatus::ConnectedToAnchor => {
                    // Masih terhubung dengan aman ke anchor
                }
                ConnectivityStatus::PendingUnloadedNeighbor { .. } => {
                    self.pending_unloaded_count += 1;
                    self.pending_checks.push_back(neighbor_pos);
                }
                ConnectivityStatus::IndeterminateBudgetExceeded { .. } => {
                    // Budget habis, jangan anggap lepas!
                    self.pending_checks.push_back(neighbor_pos);
                }
                ConnectivityStatus::Detached { component_voxels } => {
                    if component_voxels.is_empty() {
                        continue;
                    }

                    // 1. PREPARE & VALIDATE (Amendment 1): Buat DetachedAggregate terlebih dahulu
                    if let Some(agg) = DetachedAggregate::from_world_voxels(
                        self.next_aggregate_id,
                        &component_voxels,
                    ) {
                        // 2. COMMIT: Hanya rilis kepemilikan dari ChunkStore jika konstruksi aggregate berhasil
                        for &(vpos, _) in &component_voxels {
                            store.set_voxel_world(vpos, VoxelBlock::AIR);
                        }

                        self.next_aggregate_id += 1;
                        self.total_detached_extracted += 1;
                        self.detached_aggregates.push(agg.clone());
                        newly_detached.push(agg);
                    }
                }
            }
        }

        newly_detached
    }

    /// Memproses ulang antrean pending checks saat chunk baru selesai dimuat
    pub fn process_pending_checks(&mut self, store: &mut ChunkStore) -> Vec<DetachedAggregate> {
        let mut newly_detached = Vec::new();
        let count = self.pending_checks.len();

        for _ in 0..count {
            if let Some(pos) = self.pending_checks.pop_front() {
                let block = store.get_voxel_world(pos);
                if block.is_air() {
                    continue;
                }

                self.total_connectivity_checks += 1;
                let status = check_structural_connectivity(
                    pos,
                    store,
                    &self.anchor_policy,
                    &self.config,
                    Some(&mut self.total_voxels_inspected),
                );

                match status {
                    ConnectivityStatus::ConnectedToAnchor => {}
                    ConnectivityStatus::PendingUnloadedNeighbor { .. }
                    | ConnectivityStatus::IndeterminateBudgetExceeded { .. } => {
                        self.pending_checks.push_back(pos);
                    }
                    ConnectivityStatus::Detached { component_voxels } => {
                        if let Some(agg) = DetachedAggregate::from_world_voxels(
                            self.next_aggregate_id,
                            &component_voxels,
                        ) {
                            for &(vpos, _) in &component_voxels {
                                store.set_voxel_world(vpos, VoxelBlock::AIR);
                            }
                            self.next_aggregate_id += 1;
                            self.total_detached_extracted += 1;
                            self.detached_aggregates.push(agg.clone());
                            newly_detached.push(agg);
                        }
                    }
                }
            }
        }

        newly_detached
    }
}
