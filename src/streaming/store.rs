use glam::IVec3;
use std::collections::{HashMap, HashSet};

use crate::chunk::Chunk;
use crate::coord::world_voxel_to_chunk_and_local;
use crate::streaming::memory::MemoryUsage;
use crate::voxel::VoxelBlock;

/// Tempat penyimpanan dan otoritas utama untuk chunk yang sedang resident di memori
pub struct ChunkStore {
    pub resident: HashMap<IVec3, Chunk>,
    pub lifecycle_generations: HashMap<IVec3, u64>,
    pub in_flight_loading: HashSet<IVec3>,
    pub in_flight_generating: HashSet<IVec3>,
    pub in_flight_saving: HashMap<IVec3, (u64, u64)>, // (lifecycle_generation, revision)
    pub in_flight_meshing: HashMap<IVec3, (u64, u64)>, // (lifecycle_generation, revision)
}

impl Default for ChunkStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ChunkStore {
    pub fn new() -> Self {
        Self {
            resident: HashMap::new(),
            lifecycle_generations: HashMap::new(),
            in_flight_loading: HashSet::new(),
            in_flight_generating: HashSet::new(),
            in_flight_saving: HashMap::new(),
            in_flight_meshing: HashMap::new(),
        }
    }

    #[inline(always)]
    pub fn get(&self, coord: &IVec3) -> Option<&Chunk> {
        self.resident.get(coord)
    }

    #[inline(always)]
    pub fn get_mut(&mut self, coord: &IVec3) -> Option<&mut Chunk> {
        self.resident.get_mut(coord)
    }

    #[inline(always)]
    pub fn contains(&self, coord: &IVec3) -> bool {
        self.resident.contains_key(coord)
    }

    #[inline(always)]
    pub fn is_in_flight(&self, coord: &IVec3) -> bool {
        self.in_flight_loading.contains(coord)
            || self.in_flight_generating.contains(coord)
            || self.in_flight_saving.contains_key(coord)
    }

    /// Mengambil lifecycle generation saat ini untuk sebuah koordinat chunk (default 1)
    #[inline(always)]
    pub fn current_lifecycle(&self, coord: &IVec3) -> u64 {
        self.lifecycle_generations.get(coord).copied().unwrap_or(1)
    }

    pub fn insert(&mut self, chunk: Chunk) -> Option<Chunk> {
        let pos = chunk.position;
        self.in_flight_loading.remove(&pos);
        self.in_flight_generating.remove(&pos);
        self.resident.insert(pos, chunk)
    }

    pub fn remove(&mut self, coord: &IVec3) -> Option<Chunk> {
        self.in_flight_loading.remove(coord);
        self.in_flight_generating.remove(coord);
        self.in_flight_saving.remove(coord);
        self.in_flight_meshing.remove(coord);

        // Bump lifecycle generation saat dievict sehingga job asinkron lama otomatis terinvalida
        self.lifecycle_generations
            .entry(*coord)
            .and_modify(|g| *g += 1)
            .or_insert(2);

        self.resident.remove(coord)
    }

    pub fn resident_count(&self) -> usize {
        self.resident.len()
    }

    /// Iterasi seluruh chunk yang sedang berstatus resident (termuat di memori)
    pub fn resident_chunks(&self) -> impl Iterator<Item = &Chunk> {
        self.resident.values()
    }

    pub fn memory_usage(&self, cpu_mesh_bytes: usize) -> MemoryUsage {
        MemoryUsage::new(self.resident.len(), cpu_mesh_bytes)
    }

    pub fn mark_dirty(&mut self, coord: &IVec3, flags: u16) {
        if let Some(chunk) = self.resident.get_mut(coord) {
            chunk.mark_dirty(flags);
        }
    }

    /// Menetapkan voxel pada koordinat global dunia (world voxel)
    pub fn set_voxel_world(&mut self, world_voxel: IVec3, block: VoxelBlock) {
        let (chunk_coord, local_coord) = world_voxel_to_chunk_and_local(world_voxel);
        if let Some(chunk) = self.resident.get_mut(&chunk_coord) {
            chunk.set_voxel(
                local_coord.x as usize,
                local_coord.y as usize,
                local_coord.z as usize,
                block,
            );
        }
    }

    /// Mengambil voxel pada koordinat global dunia
    pub fn get_voxel_world(&self, world_voxel: IVec3) -> VoxelBlock {
        let (chunk_coord, local_coord) = world_voxel_to_chunk_and_local(world_voxel);
        if let Some(chunk) = self.resident.get(&chunk_coord) {
            *chunk.get_voxel(
                local_coord.x as usize,
                local_coord.y as usize,
                local_coord.z as usize,
            )
        } else {
            VoxelBlock::AIR
        }
    }

    /// Mengambil voxel pada koordinat global dunia dengan status residency eksplisit (Amendment 6).
    /// Mengembalikan:
    /// - `Some(block)` jika chunk resident (dapat berupa Air atau Solid).
    /// - `None` jika chunk tidak resident (Unloaded / Unknown).
    pub fn get_voxel_world_checked(&self, world_voxel: IVec3) -> Option<VoxelBlock> {
        let (chunk_coord, local_coord) = world_voxel_to_chunk_and_local(world_voxel);
        self.resident.get(&chunk_coord).map(|chunk| {
            *chunk.get_voxel(
                local_coord.x as usize,
                local_coord.y as usize,
                local_coord.z as usize,
            )
        })
    }

    /// Memeriksa apakah chunk pada koordinat tertentu saat ini resident di memori
    #[inline(always)]
    pub fn is_chunk_resident(&self, coord: &IVec3) -> bool {
        self.resident.contains_key(coord)
    }
}
