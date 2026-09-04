use std::collections::BTreeSet;

use crate::material::{MaterialId, MaterialRegistry};
use crate::voxel::VoxelBlock;

/// Policy determining whether a voxel or material can be destroyed/removed by CSG operations.
pub trait DestructionPolicy: Send + Sync {
    /// Returns `true` if the voxel block can be destroyed under this policy.
    fn is_destructible(&self, block: &VoxelBlock, materials: &MaterialRegistry) -> bool;
}

/// Default policy where all solid, non-air voxels are destructible.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultDestructionPolicy;

impl DestructionPolicy for DefaultDestructionPolicy {
    fn is_destructible(&self, block: &VoxelBlock, _materials: &MaterialRegistry) -> bool {
        !block.is_air()
    }
}

/// Material-aware destruction policy governed by a deterministic set of indestructible materials.
#[derive(Debug, Clone, Default)]
pub struct MaterialDestructionPolicy {
    indestructible_materials: BTreeSet<MaterialId>,
}

impl MaterialDestructionPolicy {
    /// Creates a new policy with no indestructible materials.
    pub fn new() -> Self {
        Self {
            indestructible_materials: BTreeSet::new(),
        }
    }

    /// Adds an indestructible material to the policy.
    pub fn with_indestructible(mut self, material: MaterialId) -> Self {
        self.indestructible_materials.insert(material);
        self
    }

    /// Adds an indestructible material mutably.
    pub fn add_indestructible(&mut self, material: MaterialId) {
        self.indestructible_materials.insert(material);
    }

    /// Checks if a material is designated as indestructible.
    pub fn is_material_indestructible(&self, material: MaterialId) -> bool {
        self.indestructible_materials.contains(&material)
    }
}

impl DestructionPolicy for MaterialDestructionPolicy {
    fn is_destructible(&self, block: &VoxelBlock, _materials: &MaterialRegistry) -> bool {
        if block.is_air() {
            return false;
        }
        !self.indestructible_materials.contains(&block.material())
    }
}
