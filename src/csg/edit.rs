use glam::IVec3;
use std::cmp::Ordering;
use std::fmt;

use crate::voxel::VoxelBlock;

/// Explicit operations supported on individual voxels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VoxelEditOperation {
    /// Adds a voxel to a location that must currently be air.
    Add { new_block: VoxelBlock },
    /// Removes a solid voxel, turning it into `VoxelBlock::AIR`.
    Remove,
    /// Replaces an existing voxel.
    /// If `expected` is `Some(expected_block)`, the target voxel MUST match `expected_block`.
    /// If `expected` is `None`, replacement is unconditional.
    Replace {
        expected: Option<VoxelBlock>,
        new_block: VoxelBlock,
    },
}

/// A proposed single-voxel edit at a world voxel coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VoxelEdit {
    pub position: IVec3,
    pub operation: VoxelEditOperation,
}

impl VoxelEdit {
    /// Proposes placing a new solid voxel at an empty/air position.
    pub fn add(position: IVec3, new_block: VoxelBlock) -> Self {
        Self {
            position,
            operation: VoxelEditOperation::Add { new_block },
        }
    }

    /// Proposes removing a solid voxel, replacing it with `VoxelBlock::AIR`.
    pub fn remove(position: IVec3) -> Self {
        Self {
            position,
            operation: VoxelEditOperation::Remove,
        }
    }

    /// Proposes replacing an existing voxel with precondition checking.
    pub fn replace(position: IVec3, expected: VoxelBlock, new_block: VoxelBlock) -> Self {
        Self {
            position,
            operation: VoxelEditOperation::Replace {
                expected: Some(expected),
                new_block,
            },
        }
    }

    /// Proposes replacing an existing voxel unconditionally.
    pub fn replace_unconditional(position: IVec3, new_block: VoxelBlock) -> Self {
        Self {
            position,
            operation: VoxelEditOperation::Replace {
                expected: None,
                new_block,
            },
        }
    }
}

// Canonical spatial ordering by (x, y, z) for deterministic processing
impl PartialOrd for VoxelEdit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for VoxelEdit {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.position.x, self.position.y, self.position.z).cmp(&(
            other.position.x,
            other.position.y,
            other.position.z,
        ))
    }
}

/// A snapshot of a validated voxel state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoxelDelta {
    pub position: IVec3,
    pub old_block: VoxelBlock,
    pub new_block: VoxelBlock,
}

/// The precomputed, inspectable collection of changes proposed by a transaction before commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedDelta {
    pub deltas: Vec<VoxelDelta>,
    pub affected_chunks: Vec<IVec3>,
    pub mesh_invalidation_chunks: Vec<IVec3>,
}

impl ProposedDelta {
    pub fn is_empty(&self) -> bool {
        self.deltas.is_empty()
    }

    pub fn len(&self) -> usize {
        self.deltas.len()
    }
}

/// Errors occurring during voxel edit proposal, validation, or preconditions check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoxelEditError {
    /// The target chunk is not currently loaded/resident in memory.
    ChunkNotResident { chunk_coord: IVec3 },
    /// `Add` operation failed because target voxel is solid rather than air.
    AddTargetNotEmpty {
        position: IVec3,
        current: VoxelBlock,
    },
    /// `Remove` operation failed because target voxel is already air.
    RemoveTargetAlreadyAir { position: IVec3 },
    /// `Replace` operation failed because target voxel did not match expected block.
    PreconditionMismatch {
        position: IVec3,
        expected: VoxelBlock,
        actual: VoxelBlock,
    },
    /// Multiple edits in the transaction targeted the same voxel coordinate.
    ConflictingDuplicateEdit { position: IVec3 },
    /// Generic invalid operation.
    InvalidOperation { reason: String },
}

impl fmt::Display for VoxelEditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ChunkNotResident { chunk_coord } => {
                write!(
                    f,
                    "Target chunk at ({}, {}, {}) is not resident in memory",
                    chunk_coord.x, chunk_coord.y, chunk_coord.z
                )
            }
            Self::AddTargetNotEmpty { position, current } => {
                write!(
                    f,
                    "Add failed at ({}, {}, {}): target voxel is not air (material={:?})",
                    position.x,
                    position.y,
                    position.z,
                    current.material()
                )
            }
            Self::RemoveTargetAlreadyAir { position } => {
                write!(
                    f,
                    "Remove failed at ({}, {}, {}): target voxel is already air",
                    position.x, position.y, position.z
                )
            }
            Self::PreconditionMismatch {
                position,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "Precondition mismatch at ({}, {}, {}): expected material={:?}, actual={:?}",
                    position.x,
                    position.y,
                    position.z,
                    expected.material(),
                    actual.material()
                )
            }
            Self::ConflictingDuplicateEdit { position } => {
                write!(
                    f,
                    "Conflicting duplicate edit detected for voxel at ({}, {}, {})",
                    position.x, position.y, position.z
                )
            }
            Self::InvalidOperation { reason } => {
                write!(f, "Invalid voxel edit operation: {}", reason)
            }
        }
    }
}

impl std::error::Error for VoxelEditError {}
