use glam::IVec3;
use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::chunk::dirty_flags;
use crate::coord::world_voxel_to_chunk_and_local;
use crate::csg::edit::{ProposedDelta, VoxelDelta, VoxelEdit, VoxelEditError, VoxelEditOperation};
use crate::streaming::store::ChunkStore;
use crate::structure::events::{StructuralEvent, StructuralMutationType};
use crate::voxel::VoxelBlock;

/// Policy for handling duplicate edits targeting the same voxel within a transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DuplicateEditPolicy {
    /// Strictly rejects any conflicting duplicate edits targeting the same voxel coordinate.
    #[default]
    RejectDuplicates,
    /// Last edit in canonical transaction order wins.
    LastWriteWins,
}

/// Snapshot of authoritative chunk state prior to transaction mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkPreState {
    pub chunk_coord: IVec3,
    pub dirty_flags: u16,
    pub revision: u64,
    pub non_air_count: u16,
}

/// The result returned upon successfully committing a `VoxelEditTransaction`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoxelEditCommitResult {
    /// Resident chunk coordinates directly mutated by the transaction (canonical sorted, deduped).
    pub affected_chunks: Vec<IVec3>,
    /// All chunk coordinates requiring mesh rebuilding, including border neighbors (canonical sorted, deduped).
    pub mesh_invalidation_chunks: Vec<IVec3>,
    /// Structural invalidation events emitted by the committed edits (for downstream consumption).
    pub structural_events: Vec<StructuralEvent>,
    /// Snapshot of the committed deltas.
    pub delta: ProposedDelta,
    /// Pre-commit snapshots of all resident chunks touched or invalidated by the transaction.
    pub chunk_pre_states: Vec<ChunkPreState>,
}

impl VoxelEditCommitResult {
    /// Reverts the authoritative `ChunkStore` state touched by this committed transaction
    /// to the captured pre-commit state, assuming the same `ChunkStore` authority remains
    /// unchanged between commit and revert.
    ///
    /// PREFLIGHT SAFETY:
    /// Prior to making any modifications, this method verifies that every captured chunk
    /// remains resident in `store`. If any chunk is no longer resident, it returns
    /// `Err(VoxelEditError::ChunkNotResident)` and aborts immediately without partial mutation.
    ///
    /// STATE RESTORATION:
    /// 1. Voxel deltas are restored in reverse order via `store.set_voxel_world(d.position, d.old_block)`.
    /// 2. Captured pre-commit metadata (`non_air_count`, `dirty_flags`, `revision`) is restored
    ///    directly for every chunk in `chunk_pre_states`, overwriting any transient mutation side effects.
    pub fn revert(&self, store: &mut ChunkStore) -> Result<(), VoxelEditError> {
        // Preflight safety: verify all captured chunks remain resident before mutating anything.
        for pre_state in &self.chunk_pre_states {
            if !store.is_chunk_resident(&pre_state.chunk_coord) {
                return Err(VoxelEditError::ChunkNotResident {
                    chunk_coord: pre_state.chunk_coord,
                });
            }
        }

        // 1. Restore voxel deltas in reverse delta order
        for d in self.delta.deltas.iter().rev() {
            store.set_voxel_world(d.position, d.old_block);
        }

        // 2. Restore exact pre-commit metadata
        for pre_state in &self.chunk_pre_states {
            if let Some(chunk) = store.get_mut(&pre_state.chunk_coord) {
                chunk.dirty_flags = pre_state.dirty_flags;
                chunk.revision = pre_state.revision;
                chunk.non_air_count = pre_state.non_air_count;
            }
        }

        Ok(())
    }
}

/// An atomic, transactional collection of voxel edit proposals.
#[derive(Debug, Clone, Default)]
pub struct VoxelEditTransaction {
    edits: Vec<VoxelEdit>,
    duplicate_policy: DuplicateEditPolicy,
}

impl VoxelEditTransaction {
    /// Creates a new empty transaction with default `RejectDuplicates` policy.
    pub fn new() -> Self {
        Self {
            edits: Vec::new(),
            duplicate_policy: DuplicateEditPolicy::RejectDuplicates,
        }
    }

    /// Sets the duplicate edit policy for this transaction.
    pub fn with_duplicate_policy(mut self, policy: DuplicateEditPolicy) -> Self {
        self.duplicate_policy = policy;
        self
    }

    /// Adds a single edit to the transaction.
    pub fn add_edit(&mut self, edit: VoxelEdit) {
        self.edits.push(edit);
    }

    /// Adds multiple edits to the transaction.
    pub fn add_edits(&mut self, edits: impl IntoIterator<Item = VoxelEdit>) {
        self.edits.extend(edits);
    }

    /// Returns the number of edits in the transaction.
    pub fn len(&self) -> usize {
        self.edits.len()
    }

    /// Returns `true` if the transaction contains zero edits.
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    /// Returns the proposed edits as a slice.
    pub fn edits(&self) -> &[VoxelEdit] {
        &self.edits
    }

    /// Pure observational validation against `ChunkStore`.
    ///
    /// GUARANTEE: This method takes `&ChunkStore` immutably.
    /// It validates that:
    /// 1. All target chunks are resident in memory.
    /// 2. All operation preconditions hold (`Add` on air, `Remove` on solid, `Replace` matching expected).
    /// 3. Duplicate edits are resolved deterministically according to `DuplicateEditPolicy`.
    ///
    /// It computes and returns `ProposedDelta` without mutating ANY world state or flags.
    pub fn validate(&self, store: &ChunkStore) -> Result<ProposedDelta, VoxelEditError> {
        if self.edits.is_empty() {
            return Ok(ProposedDelta {
                deltas: Vec::new(),
                affected_chunks: Vec::new(),
                mesh_invalidation_chunks: Vec::new(),
            });
        }

        // 1. Sort edits canonically by spatial coordinate (x, y, z) for deterministic processing
        let mut sorted_edits = self.edits.clone();
        sorted_edits.sort();

        // 2. Resolve duplicate edits deterministically
        let resolved_edits: Vec<VoxelEdit> = match self.duplicate_policy {
            DuplicateEditPolicy::RejectDuplicates => {
                for i in 1..sorted_edits.len() {
                    if sorted_edits[i].position == sorted_edits[i - 1].position {
                        return Err(VoxelEditError::ConflictingDuplicateEdit {
                            position: sorted_edits[i].position,
                        });
                    }
                }
                sorted_edits
            }
            DuplicateEditPolicy::LastWriteWins => {
                let mut map: BTreeMap<(i32, i32, i32), VoxelEdit> = BTreeMap::new();
                for edit in self.edits.iter().cloned() {
                    map.insert((edit.position.x, edit.position.y, edit.position.z), edit);
                }
                map.into_values().collect()
            }
        };

        // 3. Validate preconditions and compute proposed deltas
        let mut deltas = Vec::with_capacity(resolved_edits.len());
        let mut affected_chunks = BTreeSet::new();
        let mut mesh_invalidation_chunks = BTreeSet::new();

        for edit in &resolved_edits {
            let (chunk_coord, local) = world_voxel_to_chunk_and_local(edit.position);

            let current_block = store
                .get_voxel_world_checked(edit.position)
                .ok_or(VoxelEditError::ChunkNotResident { chunk_coord })?;

            let new_block = match edit.operation {
                VoxelEditOperation::Add { new_block } => {
                    if !current_block.is_air() {
                        return Err(VoxelEditError::AddTargetNotEmpty {
                            position: edit.position,
                            current: current_block,
                        });
                    }
                    if new_block.is_air() {
                        return Err(VoxelEditError::InvalidOperation {
                            reason: "Cannot perform Add with VoxelBlock::AIR; use Remove instead"
                                .to_string(),
                        });
                    }
                    new_block
                }
                VoxelEditOperation::Remove => {
                    if current_block.is_air() {
                        return Err(VoxelEditError::RemoveTargetAlreadyAir {
                            position: edit.position,
                        });
                    }
                    VoxelBlock::AIR
                }
                VoxelEditOperation::Replace {
                    expected,
                    new_block,
                } => {
                    if let Some(expected_block) = expected {
                        if current_block != expected_block {
                            return Err(VoxelEditError::PreconditionMismatch {
                                position: edit.position,
                                expected: expected_block,
                                actual: current_block,
                            });
                        }
                    }
                    new_block
                }
            };

            deltas.push(VoxelDelta {
                position: edit.position,
                old_block: current_block,
                new_block,
            });

            affected_chunks.insert((chunk_coord.x, chunk_coord.y, chunk_coord.z));
            mesh_invalidation_chunks.insert((chunk_coord.x, chunk_coord.y, chunk_coord.z));

            // Compute neighboring chunk mesh invalidation for border edits
            if local.x == 0 {
                let n = chunk_coord + IVec3::new(-1, 0, 0);
                mesh_invalidation_chunks.insert((n.x, n.y, n.z));
            } else if local.x == 31 {
                let n = chunk_coord + IVec3::new(1, 0, 0);
                mesh_invalidation_chunks.insert((n.x, n.y, n.z));
            }

            if local.y == 0 {
                let n = chunk_coord + IVec3::new(0, -1, 0);
                mesh_invalidation_chunks.insert((n.x, n.y, n.z));
            } else if local.y == 31 {
                let n = chunk_coord + IVec3::new(0, 1, 0);
                mesh_invalidation_chunks.insert((n.x, n.y, n.z));
            }

            if local.z == 0 {
                let n = chunk_coord + IVec3::new(0, 0, -1);
                mesh_invalidation_chunks.insert((n.x, n.y, n.z));
            } else if local.z == 31 {
                let n = chunk_coord + IVec3::new(0, 0, 1);
                mesh_invalidation_chunks.insert((n.x, n.y, n.z));
            }
        }

        // Canonical ordering (y, z, x)
        let mut affected_vec: Vec<IVec3> = affected_chunks
            .into_iter()
            .map(|(x, y, z)| IVec3::new(x, y, z))
            .collect();
        affected_vec.sort_by_key(|c| (c.y, c.z, c.x));

        let mut mesh_vec: Vec<IVec3> = mesh_invalidation_chunks
            .into_iter()
            .map(|(x, y, z)| IVec3::new(x, y, z))
            .collect();
        mesh_vec.sort_by_key(|c| (c.y, c.z, c.x));

        Ok(ProposedDelta {
            deltas,
            affected_chunks: affected_vec,
            mesh_invalidation_chunks: mesh_vec,
        })
    }

    /// Atomically commits the transaction to `ChunkStore`.
    ///
    /// GUARANTEE:
    /// 1. `self.validate(store)?` is executed prior to any mutation.
    ///    If ANY validation check fails (unloaded chunk, precondition mismatch, duplicate conflict),
    ///    this method terminates immediately with `Err`. Exactly ZERO voxels or dirty flags are modified.
    /// 2. Once `validate` succeeds:
    ///    - All affected chunks are proven resident.
    ///    - All coordinates are in valid local bounds [0..31] by Euclidean modulus.
    ///    - Pre-states of all affected chunks and resident boundary neighbors are snapshotted BEFORE mutation.
    ///    - `store.set_voxel_world` performs flat array writes to resident chunks in-memory.
    /// 3. The returned `VoxelEditCommitResult` includes `chunk_pre_states` enabling explicit `revert()`
    ///    to restore exact pre-transaction state on demand.
    pub fn commit(&self, store: &mut ChunkStore) -> Result<VoxelEditCommitResult, VoxelEditError> {
        let delta = self.validate(store)?;

        if delta.is_empty() {
            return Ok(VoxelEditCommitResult {
                affected_chunks: Vec::new(),
                mesh_invalidation_chunks: Vec::new(),
                structural_events: Vec::new(),
                delta,
                chunk_pre_states: Vec::new(),
            });
        }

        // Capture pre-states of all resident chunks that will be touched by commit
        // (both directly affected chunks and resident boundary neighbors that receive MESH_DIRTY)
        // BEFORE the first voxel mutation.
        let mut chunk_pre_states = Vec::new();
        let mut recorded_coords = HashSet::new();

        // Snapshot directly affected resident chunks
        for &coord in &delta.affected_chunks {
            if let Some(chunk) = store.get(&coord) {
                recorded_coords.insert(coord);
                chunk_pre_states.push(ChunkPreState {
                    chunk_coord: coord,
                    dirty_flags: chunk.dirty_flags,
                    revision: chunk.revision,
                    non_air_count: chunk.non_air_count,
                });
            }
        }

        // Snapshot resident boundary neighbor chunks that will receive MESH_DIRTY
        for &coord in &delta.mesh_invalidation_chunks {
            if !recorded_coords.contains(&coord) {
                if let Some(chunk) = store.get(&coord) {
                    recorded_coords.insert(coord);
                    chunk_pre_states.push(ChunkPreState {
                        chunk_coord: coord,
                        dirty_flags: chunk.dirty_flags,
                        revision: chunk.revision,
                        non_air_count: chunk.non_air_count,
                    });
                }
            }
        }

        // Apply mutations in canonical order
        for d in &delta.deltas {
            store.set_voxel_world(d.position, d.new_block);

            // Propagate mesh dirty flag to resident boundary neighbors
            let (chunk_coord, local) = world_voxel_to_chunk_and_local(d.position);
            if local.x == 0 {
                let neighbor = chunk_coord + IVec3::new(-1, 0, 0);
                if store.is_chunk_resident(&neighbor) {
                    store.mark_dirty(&neighbor, dirty_flags::MESH_DIRTY);
                }
            } else if local.x == 31 {
                let neighbor = chunk_coord + IVec3::new(1, 0, 0);
                if store.is_chunk_resident(&neighbor) {
                    store.mark_dirty(&neighbor, dirty_flags::MESH_DIRTY);
                }
            }

            if local.y == 0 {
                let neighbor = chunk_coord + IVec3::new(0, -1, 0);
                if store.is_chunk_resident(&neighbor) {
                    store.mark_dirty(&neighbor, dirty_flags::MESH_DIRTY);
                }
            } else if local.y == 31 {
                let neighbor = chunk_coord + IVec3::new(0, 1, 0);
                if store.is_chunk_resident(&neighbor) {
                    store.mark_dirty(&neighbor, dirty_flags::MESH_DIRTY);
                }
            }

            if local.z == 0 {
                let neighbor = chunk_coord + IVec3::new(0, 0, -1);
                if store.is_chunk_resident(&neighbor) {
                    store.mark_dirty(&neighbor, dirty_flags::MESH_DIRTY);
                }
            } else if local.z == 31 {
                let neighbor = chunk_coord + IVec3::new(0, 0, 1);
                if store.is_chunk_resident(&neighbor) {
                    store.mark_dirty(&neighbor, dirty_flags::MESH_DIRTY);
                }
            }
        }

        // Build structural events describing the committed mutations (notifications only, no BFS)
        let mut structural_events = Vec::with_capacity(delta.deltas.len());
        for d in &delta.deltas {
            let mutation = if d.old_block.is_air() && !d.new_block.is_air() {
                StructuralMutationType::VoxelPlaced {
                    new_block: d.new_block,
                }
            } else if !d.old_block.is_air() && d.new_block.is_air() {
                StructuralMutationType::VoxelRemoved {
                    previous_block: d.old_block,
                }
            } else {
                StructuralMutationType::VoxelReplaced {
                    previous_block: d.old_block,
                    new_block: d.new_block,
                }
            };
            structural_events.push(StructuralEvent::new(d.position, mutation));
        }

        Ok(VoxelEditCommitResult {
            affected_chunks: delta.affected_chunks.clone(),
            mesh_invalidation_chunks: delta.mesh_invalidation_chunks.clone(),
            structural_events,
            delta,
            chunk_pre_states,
        })
    }
}
