use glam::{IVec3, Vec3};

use crate::chunk::{dirty_flags, Chunk};
use crate::coord::CHUNK_WORLD_SIZE;

/// Kandidat eviksi chunk yang dievaluasi dari residency store
#[derive(Debug, Clone)]
pub struct EvictionCandidate {
    pub coord: IVec3,
    pub distance_sq: f32,
    pub is_dirty: bool,
    pub revision: u64,
}

/// Kebijakan pemilihan chunk untuk dieviction dari memori
pub struct EvictionPolicy;

impl EvictionPolicy {
    /// Mencari kandidat chunk yang berada di luar radius retain atau yang memiliki jarak terjauh dari kamera
    pub fn select_candidates(
        resident_chunks: impl Iterator<Item = (&'static IVec3, &'static Chunk)>,
        camera_world_pos: Vec3,
        retain_radius_chunks: i32,
        max_candidates: usize,
    ) -> Vec<EvictionCandidate> {
        let retain_radius_world_sq = (retain_radius_chunks as f32 * CHUNK_WORLD_SIZE).powi(2);
        let mut candidates = Vec::new();

        for (&coord, chunk) in resident_chunks {
            let chunk_center = Vec3::new(
                (coord.x as f32 + 0.5) * CHUNK_WORLD_SIZE,
                (coord.y as f32 + 0.5) * CHUNK_WORLD_SIZE,
                (coord.z as f32 + 0.5) * CHUNK_WORLD_SIZE,
            );
            let dist_sq = camera_world_pos.distance_squared(chunk_center);

            if dist_sq > retain_radius_world_sq {
                candidates.push(EvictionCandidate {
                    coord,
                    distance_sq: dist_sq,
                    is_dirty: chunk.is_dirty(dirty_flags::SAVE_DIRTY),
                    revision: chunk.revision,
                });
            }
        }

        // Urutkan dari yang terjauh ke yang terdekat
        candidates.sort_by(|a, b| {
            b.distance_sq
                .partial_cmp(&a.distance_sq)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if candidates.len() > max_candidates {
            candidates.truncate(max_candidates);
        }

        candidates
    }
}
