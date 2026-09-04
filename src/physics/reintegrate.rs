use glam::IVec3;

use super::body::{DynamicBody, DynamicBodyId};
use crate::streaming::store::ChunkStore;
use crate::voxel::VoxelBlock;

use std::fmt;

/// Kesalahan validasi pada tahap Prepare Reintegration (Amendment 7, 8, dan Phase 9.11)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReintegrationError {
    /// Chunk tujuan tidak resident di memori
    ChunkNotResident(IVec3),
    /// Koordinat tujuan telah terisi oleh voxel solid statis (mencegah overwrite)
    DestinationOccupied {
        pos: IVec3,
        existing_block: VoxelBlock,
    },
    /// Duplikasi internal target voxel (injective mapping violation karena rotasi non-lattice)
    SelfOverlap(IVec3),
    /// Badan dinamis atau rigid body tidak ditemukan di PhysicsWorld
    BodyNotFound(u64),
    /// Transformasi badan kaku memuat nilai non-finite
    NonFiniteTransform,
    /// Aggregate kosong tanpa voxel
    EmptyAggregate,
    /// Orientasi rotasi melebihi batas toleransi snapping grid
    OrientationMisaligned,
}

impl fmt::Display for ReintegrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ChunkNotResident(c) => write!(f, "Chunk tujuan {} belum resident", c),
            Self::DestinationOccupied {
                pos,
                existing_block,
            } => {
                write!(f, "Posisi {} telah terisi blok {:?}", pos, existing_block)
            }
            Self::SelfOverlap(pos) => write!(f, "Duplikasi pemetaan voxel pada koordinat {}", pos),
            Self::BodyNotFound(id) => write!(f, "Badan fisik #{} tidak ditemukan", id),
            Self::NonFiniteTransform => write!(f, "Transformasi memuat nilai non-finite"),
            Self::EmptyAggregate => write!(f, "Aggregate tidak memiliki voxel"),
            Self::OrientationMisaligned => {
                write!(f, "Orientasi rotasi tidak sejajar dengan kisi voxel")
            }
        }
    }
}

impl std::error::Error for ReintegrationError {}

/// Rencana mutasi hasil tahap PREPARE yang telah divalidasi dan siap untuk di-COMMIT
#[derive(Debug, Clone)]
pub struct ReintegrationPlan {
    pub body_id: DynamicBodyId,
    pub voxels: Vec<(IVec3, VoxelBlock)>,
    pub affected_chunks: Vec<IVec3>,
}

impl DynamicBody {
    /// Fase 1: PREPARE & VALIDATE (Amendment 7 & 8)
    /// Menghitung seluruh voxel tujuan dan memvalidasi bahwa seluruh chunk tujuan resident
    /// serta seluruh posisi tujuan saat ini adalah AIR (tidak ada overwrite solid statis).
    pub fn prepare_reintegration(
        &self,
        store: &ChunkStore,
    ) -> Result<ReintegrationPlan, ReintegrationError> {
        let base_voxel = self.current_base_voxel();
        let mut plan_voxels = Vec::with_capacity(self.voxel_count());
        let mut affected_chunks = Vec::new();

        for v in &self.aggregate.voxels {
            let dest_coord = base_voxel + v.relative_coord;
            let (chunk_coord, _) = crate::coord::world_voxel_to_chunk_and_local(dest_coord);

            // Validasi 1: Chunk harus resident (Amendment 6 & 7)
            if !store.is_chunk_resident(&chunk_coord) {
                return Err(ReintegrationError::ChunkNotResident(chunk_coord));
            }

            // Validasi 2: Lokasi tujuan harus AIR (Amendment 8: No silent terrain overwrite)
            match store.get_voxel_world_checked(dest_coord) {
                Some(existing) => {
                    if !existing.is_air() {
                        return Err(ReintegrationError::DestinationOccupied {
                            pos: dest_coord,
                            existing_block: existing,
                        });
                    }
                }
                None => {
                    return Err(ReintegrationError::ChunkNotResident(chunk_coord));
                }
            }

            if !affected_chunks.contains(&chunk_coord) {
                affected_chunks.push(chunk_coord);
            }

            plan_voxels.push((dest_coord, v.block));
        }

        Ok(ReintegrationPlan {
            body_id: self.id,
            voxels: plan_voxels,
            affected_chunks,
        })
    }
}
