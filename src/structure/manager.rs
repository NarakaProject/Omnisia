use std::collections::VecDeque;

use glam::IVec3;

use super::adjacency::ADJACENCY_OFFSETS_6;
use super::aggregate::DetachedAggregate;
use super::anchor::AnchorPolicy;
use super::connectivity::{check_structural_connectivity, ConnectivityConfig, ConnectivityStatus};
use super::events::StructuralEvent;
use crate::streaming::store::ChunkStore;
use crate::voxel::VoxelBlock;

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
