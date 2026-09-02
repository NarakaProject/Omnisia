use glam::IVec3;

use crate::coord::world_voxel_to_chunk_and_local;
use crate::voxel::VoxelBlock;

/// Tipe mutasi voxel yang memicu event struktural
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuralMutationType {
    /// Voxel baru ditempatkan pada koordinat yang sebelumnya udara
    VoxelPlaced { new_block: VoxelBlock },
    /// Voxel padat dihilangkan (menjadi udara)
    VoxelRemoved { previous_block: VoxelBlock },
    /// Voxel diganti dengan tipe voxel lain
    VoxelReplaced {
        previous_block: VoxelBlock,
        new_block: VoxelBlock,
    },
}

/// Event mutasi struktural pada koordinat voxel dunia otoritatif
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructuralEvent {
    pub world_voxel: IVec3,
    pub chunk_coord: IVec3,
    pub local_voxel: IVec3,
    pub mutation: StructuralMutationType,
}

impl StructuralEvent {
    pub fn new(world_voxel: IVec3, mutation: StructuralMutationType) -> Self {
        let (chunk_coord, local_voxel) = world_voxel_to_chunk_and_local(world_voxel);
        Self {
            world_voxel,
            chunk_coord,
            local_voxel,
            mutation,
        }
    }

    /// Apakah mutasi ini berpotensi memutus sambungan struktural (misal voxel solid dihilangkan)
    pub fn can_cause_detachment(&self) -> bool {
        match self.mutation {
            StructuralMutationType::VoxelRemoved { previous_block } => !previous_block.is_air(),
            StructuralMutationType::VoxelReplaced {
                previous_block,
                new_block,
            } => !previous_block.is_air() && new_block.is_air(),
            StructuralMutationType::VoxelPlaced { .. } => false,
        }
    }
}
