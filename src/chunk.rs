use glam::{IVec3, Vec3};

use crate::coord::{canonical_linear_index, CHUNK_SIZE_USIZE, CHUNK_VOLUME, CHUNK_WORLD_SIZE};
use crate::material::MaterialId;
use crate::voxel::{VoxelBlock, VOXEL_SIZE};

/// Flag invalidation granular untuk chunk
pub mod dirty_flags {
    pub const VOXEL_DIRTY: u16 = 1 << 0;
    pub const MESH_DIRTY: u16 = 1 << 1;
    pub const LIGHTING_DIRTY: u16 = 1 << 2;
    pub const STRUCTURAL_DIRTY: u16 = 1 << 3;
    pub const SAVE_DIRTY: u16 = 1 << 4;
    pub const ALL: u16 = 0xFFFF;
}

/// Struktur data Chunk otoritatif 32x32x32 micro-voxel.
///
/// INVARIANT 2: 32x32x32 = 32,768 voxel per chunk.
/// INVARIANT 3: Memory storage flat & contiguous (128 KiB per chunk).
/// INVARIANT 4: Chunk adalah authoritative world voxel storage (bukan renderer / bukan LOD).
#[derive(Clone)]
pub struct Chunk {
    pub position: IVec3,
    /// Flat array 128 KiB dialokasikan di heap
    pub voxels: Box<[VoxelBlock; CHUNK_VOLUME]>,
    pub non_air_count: u16,
    pub dirty_flags: u16,
    /// Revision counter mutasi untuk stale async job protection
    pub revision: u64,
}

impl Chunk {
    /// Membuat chunk baru yang seluruhnya berupa udara pejal
    pub fn new(position: IVec3) -> Self {
        // Alokasi aman 128 KiB tanpa buffer overflow di stack
        let voxels = vec![VoxelBlock::AIR; CHUNK_VOLUME]
            .into_boxed_slice()
            .try_into()
            .unwrap_or_else(|_| panic!("Gagal mengalokasikan slice berukuran CHUNK_VOLUME"));

        Self {
            position,
            voxels,
            non_air_count: 0,
            dirty_flags: dirty_flags::ALL,
            revision: 0,
        }
    }

    /// Mengambil voxel pada koordinat lokal [0..31]
    #[inline(always)]
    pub fn get_voxel(&self, x: usize, y: usize, z: usize) -> &VoxelBlock {
        &self.voxels[canonical_linear_index(x, y, z)]
    }

    /// Mengambil voxel pada koordinat lokal dengan validasi batas
    #[inline(always)]
    pub fn get_voxel_checked(&self, x: usize, y: usize, z: usize) -> Option<&VoxelBlock> {
        if x < CHUNK_SIZE_USIZE && y < CHUNK_SIZE_USIZE && z < CHUNK_SIZE_USIZE {
            Some(&self.voxels[canonical_linear_index(x, y, z)])
        } else {
            None
        }
    }

    /// Menetapkan voxel pada koordinat lokal [0..31] serta mengelola dirty flags & non_air_count
    #[inline(always)]
    pub fn set_voxel(&mut self, x: usize, y: usize, z: usize, new_block: VoxelBlock) {
        let idx = canonical_linear_index(x, y, z);
        let old_block = self.voxels[idx];

        if old_block == new_block {
            return;
        }

        let was_air = old_block.is_air();
        let is_now_air = new_block.is_air();

        if was_air && !is_now_air {
            self.non_air_count += 1;
        } else if !was_air && is_now_air {
            self.non_air_count = self.non_air_count.saturating_sub(1);
        }

        self.voxels[idx] = new_block;
        self.revision = self.revision.wrapping_add(1);
        self.mark_dirty(
            dirty_flags::VOXEL_DIRTY
                | dirty_flags::MESH_DIRTY
                | dirty_flags::SAVE_DIRTY
                | dirty_flags::STRUCTURAL_DIRTY,
        );
    }

    /// Mengisi seluruh chunk dengan satu material homogen
    pub fn fill_material(&mut self, material: MaterialId) {
        let block = VoxelBlock::new(material);
        self.voxels.fill(block);
        self.non_air_count = if material == MaterialId::AIR {
            0
        } else {
            CHUNK_VOLUME as u16
        };
        self.revision = self.revision.wrapping_add(1);
        self.mark_dirty(dirty_flags::ALL);
    }

    /// Menaikkan nomor revisi secara manual (misal mutasi batch)
    #[inline(always)]
    pub fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// Mengecek apakah chunk kosong secara instan O(1)
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.non_air_count == 0
    }

    /// Menghitung ulang non_air_count secara akurat dari seluruh voxel
    pub fn recount_non_air(&mut self) {
        let count = self.voxels.iter().filter(|v| !v.is_air()).count();
        self.non_air_count = count as u16;
    }

    /// Mengecek apakah chunk padat penuh
    #[inline(always)]
    pub fn is_full(&self) -> bool {
        self.non_air_count == CHUNK_VOLUME as u16
    }

    #[inline(always)]
    pub fn mark_dirty(&mut self, flags: u16) {
        self.dirty_flags |= flags;
    }

    #[inline(always)]
    pub fn clear_dirty(&mut self, flags: u16) {
        self.dirty_flags &= !flags;
    }

    /// Membersihkan dirty flags hanya jika revisi chunk sama persis dengan saat operasi async dimulai
    #[inline(always)]
    pub fn clear_dirty_if_revision_matched(&mut self, flags: u16, expected_revision: u64) -> bool {
        if self.revision == expected_revision {
            self.dirty_flags &= !flags;
            true
        } else {
            false
        }
    }

    #[inline(always)]
    pub fn is_dirty(&self, flags: u16) -> bool {
        (self.dirty_flags & flags) != 0
    }

    /// Mengambil batas AABB dunia (min, max) dalam meter
    pub fn aabb_world(&self) -> (Vec3, Vec3) {
        let min = Vec3::new(
            self.position.x as f32 * CHUNK_WORLD_SIZE,
            self.position.y as f32 * CHUNK_WORLD_SIZE,
            self.position.z as f32 * CHUNK_WORLD_SIZE,
        );
        let max = min + Vec3::splat(CHUNK_WORLD_SIZE);
        (min, max)
    }

    /// Mengonversi koordinat lokal voxel [0..31] ke posisi dunia minimum voxel dalam meter
    #[inline(always)]
    pub fn local_to_world_pos(&self, lx: usize, ly: usize, lz: usize) -> Vec3 {
        let origin = Vec3::new(
            self.position.x as f32 * CHUNK_WORLD_SIZE,
            self.position.y as f32 * CHUNK_WORLD_SIZE,
            self.position.z as f32 * CHUNK_WORLD_SIZE,
        );
        origin
            + Vec3::new(
                lx as f32 * VOXEL_SIZE,
                ly as f32 * VOXEL_SIZE,
                lz as f32 * VOXEL_SIZE,
            )
    }
}
