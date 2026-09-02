use crate::coord::CHUNK_VOLUME;
use crate::voxel::VoxelBlock;

/// Ukuran memori raw voxel data untuk satu chunk (128 KiB)
pub const CHUNK_RAW_VOXEL_BYTES: usize = CHUNK_VOLUME * std::mem::size_of::<VoxelBlock>();

/// Estimasi overhead metadata per chunk (struct Chunk, HashMap overhead, revision, flags)
pub const CHUNK_METADATA_BYTES: usize = 512;

/// Akuntansi penggunaan memori riil untuk residency chunk
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemoryUsage {
    pub resident_chunk_count: usize,
    pub raw_voxel_bytes: usize,
    pub metadata_bytes: usize,
    pub cpu_mesh_bytes: usize,
}

impl MemoryUsage {
    pub fn new(resident_count: usize, cpu_mesh_bytes: usize) -> Self {
        let raw_voxel_bytes = resident_count * CHUNK_RAW_VOXEL_BYTES;
        let metadata_bytes = resident_count * CHUNK_METADATA_BYTES;
        Self {
            resident_chunk_count: resident_count,
            raw_voxel_bytes,
            metadata_bytes,
            cpu_mesh_bytes,
        }
    }

    pub fn total_bytes(&self) -> usize {
        self.raw_voxel_bytes + self.metadata_bytes + self.cpu_mesh_bytes
    }

    pub fn total_megabytes(&self) -> f32 {
        self.total_bytes() as f32 / (1024.0 * 1024.0)
    }
}

/// Batasan anggaran memori untuk residency chunk
#[derive(Debug, Clone, Copy)]
pub struct MemoryBudget {
    pub max_resident_chunks: usize,
    pub max_memory_bytes: usize,
}

impl Default for MemoryBudget {
    fn default() -> Self {
        // Default aman untuk baseline MacBook Pro 2018: 512 chunks (~64 MiB voxel raw) atau maks 256 MiB total
        Self {
            max_resident_chunks: 512,
            max_memory_bytes: 256 * 1024 * 1024,
        }
    }
}

impl MemoryBudget {
    pub fn with_chunk_limit(max_chunks: usize) -> Self {
        Self {
            max_resident_chunks: max_chunks,
            max_memory_bytes: max_chunks
                * (CHUNK_RAW_VOXEL_BYTES + CHUNK_METADATA_BYTES + 32 * 1024),
        }
    }

    pub fn is_over_budget(&self, usage: &MemoryUsage) -> bool {
        usage.resident_chunk_count > self.max_resident_chunks
            || usage.total_bytes() > self.max_memory_bytes
    }

    pub fn excess_chunks(&self, usage: &MemoryUsage) -> usize {
        usage
            .resident_chunk_count
            .saturating_sub(self.max_resident_chunks)
    }
}
