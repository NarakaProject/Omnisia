use glam::{IVec3, Vec3};

use crate::coord::{
    world_pos_to_world_voxel, world_voxel_to_chunk_and_local, world_voxel_to_world_pos,
};
use crate::csg::edit::{VoxelEdit, VoxelEditError};
use crate::csg::policy::DestructionPolicy;
use crate::csg::transaction::VoxelEditTransaction;
use crate::impact::ImpactEvent;
use crate::material::MaterialRegistry;
use crate::streaming::store::ChunkStore;
use crate::voxel::VOXEL_SIZE;

/// Deterministic, bounded Constructive Solid Geometry (CSG) spherical crater generator.
pub struct CraterGenerator;

impl CraterGenerator {
    /// Generates a spherical crater `VoxelEditTransaction` centered at `center` with `radius` (in world-space meters).
    ///
    /// GEOMETRIC CONVENTION:
    /// - Voxel center: `world_voxel_to_world_pos(voxel_pos) + Vec3::splat(VOXEL_SIZE * 0.5)`
    /// - Inclusion criterion: `(voxel_center - center).length_squared() <= radius * radius`
    /// - `radius <= 0.0`: produces zero edits (empty transaction).
    ///
    /// POLICY BEHAVIOR:
    /// - Air voxels: skipped (no edit emitted).
    /// - Indestructible voxels: preserved untouched (no edit emitted).
    /// - Destructible solid voxels: `VoxelEdit::remove(voxel_pos)` emitted.
    ///
    /// BOUNDED COMPLEXITY:
    /// - Scans only the bounded AABB in voxel space: $O(r^3)$ voxels.
    /// - Never scans the entire world.
    ///
    /// OBSERVATIONAL PURITY:
    /// - Takes `&ChunkStore` immutably. Does NOT mutate voxel data or set dirty flags.
    pub fn generate(
        center: Vec3,
        radius: f32,
        policy: &dyn DestructionPolicy,
        materials: &MaterialRegistry,
        store: &ChunkStore,
    ) -> Result<VoxelEditTransaction, VoxelEditError> {
        let mut transaction = VoxelEditTransaction::new();

        if radius <= 0.0 {
            return Ok(transaction);
        }

        let radius_sq = radius * radius;
        let half_voxel = Vec3::splat(VOXEL_SIZE * 0.5);

        // Bounded AABB in voxel coordinates
        let min_pos = center - Vec3::splat(radius);
        let max_pos = center + Vec3::splat(radius);

        let min_voxel = world_pos_to_world_voxel(min_pos);
        let max_voxel = world_pos_to_world_voxel(max_pos);

        let mut edits = Vec::new();

        for z in min_voxel.z..=max_voxel.z {
            for y in min_voxel.y..=max_voxel.y {
                for x in min_voxel.x..=max_voxel.x {
                    let voxel_pos = IVec3::new(x, y, z);
                    let voxel_center = world_voxel_to_world_pos(voxel_pos) + half_voxel;

                    if (voxel_center - center).length_squared() <= radius_sq {
                        let block = match store.get_voxel_world_checked(voxel_pos) {
                            Some(b) => b,
                            None => {
                                let (chunk_coord, _) = world_voxel_to_chunk_and_local(voxel_pos);
                                return Err(VoxelEditError::ChunkNotResident { chunk_coord });
                            }
                        };

                        if !block.is_air() && policy.is_destructible(&block, materials) {
                            edits.push(VoxelEdit::remove(voxel_pos));
                        }
                    }
                }
            }
        }

        // Canonical ordering (x, y, z)
        edits.sort();
        transaction.add_edits(edits);

        Ok(transaction)
    }

    /// Generates a crater transaction from a validated descriptive `ImpactEvent`.
    ///
    /// Translates `impact.position()` and `impact.affected_volume().radius` into
    /// a bounded CSG crater proposal.
    pub fn from_impact(
        impact: &ImpactEvent,
        policy: &dyn DestructionPolicy,
        materials: &MaterialRegistry,
        store: &ChunkStore,
    ) -> Result<VoxelEditTransaction, VoxelEditError> {
        Self::generate(impact.position, impact.radius, policy, materials, store)
    }
}
