use glam::IVec3;

use crate::voxel::VoxelBlock;

/// Representasi satu voxel dalam detached aggregate
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AggregateVoxel {
    /// Koordinat relatif terhadap `min_voxel` dari aggregate
    pub relative_coord: IVec3,
    /// Data blok dan material otoritatif
    pub block: VoxelBlock,
}

/// Gugusan struktural yang terputus dari penopang dunia (Detached Structural Aggregate).
///
/// FIREWALL SCOPE PHASE 7:
/// Struktur data ini murni berisi data topologi dan material voxel.
/// TIDAK BOLEH ditambahkan: velocity, gravity, mass, rigid body, atau collision solver (milik Phase 8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedAggregate {
    pub id: u64,
    /// Sudut minimum bounding box koordinat voxel dunia
    pub min_voxel: IVec3,
    /// Sudut maksimum bounding box koordinat voxel dunia
    pub max_voxel: IVec3,
    /// Daftar seluruh voxel yang membentuk aggregate lepas
    pub voxels: Vec<AggregateVoxel>,
}

impl DetachedAggregate {
    /// Membuat DetachedAggregate baru dari kumpulan voxel dunia mutlak
    pub fn from_world_voxels(id: u64, world_voxels: &[(IVec3, VoxelBlock)]) -> Option<Self> {
        if world_voxels.is_empty() {
            return None;
        }

        let mut min_voxel = world_voxels[0].0;
        let mut max_voxel = world_voxels[0].0;

        for (pos, _) in world_voxels {
            min_voxel = min_voxel.min(*pos);
            max_voxel = max_voxel.max(*pos);
        }

        let voxels = world_voxels
            .iter()
            .map(|(pos, block)| AggregateVoxel {
                relative_coord: *pos - min_voxel,
                block: *block,
            })
            .collect();

        Some(Self {
            id,
            min_voxel,
            max_voxel,
            voxels,
        })
    }

    /// Jumlah total voxel solid dalam aggregate
    pub fn voxel_count(&self) -> usize {
        self.voxels.len()
    }

    /// Mengonversi posisi voxel aggregate kembali ke koordinat dunia otoritatif
    #[inline(always)]
    pub fn world_coord_of(&self, v: &AggregateVoxel) -> IVec3 {
        self.min_voxel + v.relative_coord
    }

    /// Iterasi seluruh voxel dalam koordinat dunia mutlak
    pub fn iter_world_voxels(&self) -> impl Iterator<Item = (IVec3, VoxelBlock)> + '_ {
        self.voxels
            .iter()
            .map(move |v| (self.world_coord_of(v), v.block))
    }
}
