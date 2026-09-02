use glam::IVec3;
use std::cmp::Ordering;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;

use crate::chunk::Chunk;
use crate::mesh::types::MeshData;

/// Tingkat prioritas penjadwalan job chunk
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobPriority {
    /// Kritis (misal chunk di bawah kaki pemain / tabrakan fisik mendesak)
    Critical = 0,
    /// Tinggi (chunk terlihat langsung di depan kamera jarak dekat)
    High = 1,
    /// Normal (chunk di sekitar radius pemain)
    Normal = 2,
    /// Rendah (preload di perbatasan radius simulasi, background save)
    Low = 3,
    /// Sangat Rendah (preparasi LOD jauh)
    VeryLow = 4,
}

impl PartialOrd for JobPriority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for JobPriority {
    fn cmp(&self, other: &Self) -> Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

/// Jenis tugas asynchronous yang dieksekusi oleh worker pool
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobType {
    LoadChunk,
    GenerateChunk,
    SaveChunk,
    MeshChunk,
    BuildLOD,
}

/// Permintaan job chunk yang dimasukkan ke priority scheduler
#[derive(Debug, Clone)]
pub struct ChunkJobRequest {
    pub job_id: u64,
    pub coord: IVec3,
    pub job_type: JobType,
    pub priority: JobPriority,
    pub request_revision: u64,
    pub distance_sq: f32,
    pub cancelled: Arc<AtomicBool>,
}

impl ChunkJobRequest {
    pub fn new(
        job_id: u64,
        coord: IVec3,
        job_type: JobType,
        priority: JobPriority,
        request_revision: u64,
        distance_sq: f32,
    ) -> Self {
        Self {
            job_id,
            coord,
            job_type,
            priority,
            request_revision,
            distance_sq,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    #[inline(always)]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(AtomicOrdering::Relaxed)
    }

    #[inline(always)]
    pub fn cancel(&self) {
        self.cancelled.store(true, AtomicOrdering::Relaxed);
    }
}

// Implementasi Ord untuk Priority Queue:
// Prioritas Tertinggi (Critical) diproses pertama, kemudian jarak terdekat, request_id terkecil (FIFO tie-break), lalu koordinat (deterministik).
impl PartialEq for ChunkJobRequest {
    fn eq(&self, other: &Self) -> bool {
        self.job_id == other.job_id
    }
}

impl Eq for ChunkJobRequest {}

impl PartialOrd for ChunkJobRequest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ChunkJobRequest {
    fn cmp(&self, other: &Self) -> Ordering {
        // Karena BinaryHeap adalah Max-Heap, kita balik perbandingan agar nilai prioritas terendah (0/Critical) keluar duluan
        other
            .priority
            .cmp(&self.priority)
            .then_with(|| {
                // Jarak lebih kecil memiliki prioritas lebih tinggi
                other
                    .distance_sq
                    .partial_cmp(&self.distance_sq)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| other.job_id.cmp(&self.job_id))
            .then_with(|| self.coord.x.cmp(&other.coord.x))
            .then_with(|| self.coord.y.cmp(&other.coord.y))
            .then_with(|| self.coord.z.cmp(&other.coord.z))
    }
}

/// Hasil dari eksekusi job worker yang dikirimkan kembali ke main thread
pub enum ChunkJobResult {
    Loaded {
        chunk: Chunk,
        request_revision: u64,
    },
    Generated {
        chunk: Chunk,
        request_revision: u64,
    },
    Saved {
        coord: IVec3,
        saved_revision: u64,
    },
    Meshed {
        coord: IVec3,
        mesh: MeshData,
        mesh_revision: u64,
    },
    Failed {
        coord: IVec3,
        job_type: JobType,
        error: String,
    },
}
