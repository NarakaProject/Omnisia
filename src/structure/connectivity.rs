use std::collections::{HashSet, VecDeque};

use glam::IVec3;

use super::adjacency::ADJACENCY_OFFSETS_6;
use super::anchor::AnchorPolicy;
use crate::coord::world_voxel_to_chunk_and_local;
use crate::streaming::store::ChunkStore;
use crate::voxel::VoxelBlock;

/// Konfigurasi batas pencarian konektivitas struktural
#[derive(Debug, Clone, Copy)]
pub struct ConnectivityConfig {
    /// Batas maksimum voxel yang diperiksa per komponen sebelum beralih ke status Indeterminate
    pub max_voxels_budget: usize,
}

impl Default for ConnectivityConfig {
    fn default() -> Self {
        Self {
            max_voxels_budget: 10_000,
        }
    }
}

/// Hasil analisis konektivitas untuk suatu gugusan struktural
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectivityStatus {
    /// Terhubung secara valid ke anchor penopang dunia
    ConnectedToAnchor,
    /// Terbukti putus total dari seluruh anchor (seluruh komponen tertutup selesai dijelajahi tanpa menemukan anchor)
    Detached {
        component_voxels: Vec<(IVec3, VoxelBlock)>,
    },
    /// Penelusuran mencapai batas chunk yang belum dimuat; tidak boleh dianggap AIR atau DETACHED
    PendingUnloadedNeighbor {
        unloaded_chunk: IVec3,
        visited_count: usize,
    },
    /// Batas alokasi pencarian (work budget) tercapai sebelum anchor dipastikan; tidak boleh dianggap lepas
    IndeterminateBudgetExceeded { visited_count: usize },
}

/// Menganalisis konektivitas struktural suatu voxel awal (seed_pos) terhadap anchor dunia
///
/// INVARIANTS:
/// 1. 6-connected face adjacency murni.
/// 2. Bounded & Localized: Berhenti begitu menemukan anchor pertama kali.
/// 3. Unloaded Chunk Guard: Jika menyentuh chunk di luar `store`, TIDAK DIANGGAP UDARA dan TIDAK DIANGGAP LEPAS.
///    Jika jalur lain dalam loaded chunks menemukan anchor, struktur tetap ConnectedToAnchor.
/// 4. Budget Guard: Jika batas voxel tercapai dan belum menemukan anchor, TIDAK DIANGGAP LEPAS melainkan `Indeterminate`.
pub fn check_structural_connectivity(
    seed_pos: IVec3,
    store: &ChunkStore,
    anchor_policy: &AnchorPolicy,
    config: &ConnectivityConfig,
    instrumentation_inspected_counter: Option<&mut usize>,
) -> ConnectivityStatus {
    let (seed_chunk, _) = world_voxel_to_chunk_and_local(seed_pos);
    if !store.contains(&seed_chunk) {
        return ConnectivityStatus::PendingUnloadedNeighbor {
            unloaded_chunk: seed_chunk,
            visited_count: 0,
        };
    }

    let seed_block = store.get_voxel_world(seed_pos);
    if seed_block.is_air() {
        return ConnectivityStatus::ConnectedToAnchor;
    }

    // Jika voxel awal itu sendiri merupakan anchor
    if anchor_policy.is_anchor_block(&seed_block) {
        return ConnectivityStatus::ConnectedToAnchor;
    }

    let mut visited: HashSet<IVec3> = HashSet::new();
    let mut queue: VecDeque<IVec3> = VecDeque::new();
    let mut collected: Vec<(IVec3, VoxelBlock)> = Vec::new();

    visited.insert(seed_pos);
    queue.push_back(seed_pos);
    collected.push((seed_pos, seed_block));

    let mut inspected_count = 0;
    let mut encountered_unloaded_chunk: Option<IVec3> = None;
    let mut budget_exceeded = false;

    while let Some(current_pos) = queue.pop_front() {
        inspected_count += 1;

        if visited.len() > config.max_voxels_budget {
            budget_exceeded = true;
            break;
        }

        for offset in &ADJACENCY_OFFSETS_6 {
            let neighbor_pos = current_pos + *offset;

            if visited.contains(&neighbor_pos) {
                continue;
            }

            let (neighbor_chunk, _) = world_voxel_to_chunk_and_local(neighbor_pos);

            // Periksa apakah chunk tetangga dimuat dalam store
            if !store.contains(&neighbor_chunk) {
                // Catat keberadaan chunk yang belum dimuat, namun jangan langsung putuskan
                // jika masih ada jalur lokal lain yang menuju anchor.
                if encountered_unloaded_chunk.is_none() {
                    encountered_unloaded_chunk = Some(neighbor_chunk);
                }
                continue;
            }

            let neighbor_block = store.get_voxel_world(neighbor_pos);
            if neighbor_block.is_air() {
                continue;
            }

            // Jika tetangga adalah anchor, seluruh struktur ini terbukti tertopang!
            if anchor_policy.is_anchor_block(&neighbor_block) {
                if let Some(counter) = instrumentation_inspected_counter {
                    *counter += inspected_count;
                }
                return ConnectivityStatus::ConnectedToAnchor;
            }

            // Tambahkan tetangga solid ke antrean eksplorasi
            visited.insert(neighbor_pos);
            collected.push((neighbor_pos, neighbor_block));
            queue.push_back(neighbor_pos);
        }
    }

    if let Some(counter) = instrumentation_inspected_counter {
        *counter += inspected_count;
    }

    // Jika belum menemukan anchor, evaluasi guardrails:
    // 1. Jika menyentuh chunk yang belum dimuat, status adalah Pending (tidak boleh dianggap lepas!)
    if let Some(unloaded_chunk) = encountered_unloaded_chunk {
        return ConnectivityStatus::PendingUnloadedNeighbor {
            unloaded_chunk,
            visited_count: visited.len(),
        };
    }

    // 2. Jika budget habis tanpa menyentuh anchor, status adalah Indeterminate (tidak boleh dianggap lepas!)
    if budget_exceeded {
        return ConnectivityStatus::IndeterminateBudgetExceeded {
            visited_count: visited.len(),
        };
    }

    // 3. Hanya jika seluruh komponen tertutup telah dieksplorasi penuh tanpa satupun anchor:
    ConnectivityStatus::Detached {
        component_voxels: collected,
    }
}
