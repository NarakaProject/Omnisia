use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::material::MaterialId;

/// Ukuran fisik satu micro-voxel dalam meter
pub const VOXEL_SIZE: f32 = 0.5;

/// Volume satu micro-voxel dalam meter kubik (0.5 * 0.5 * 0.5 = 0.125 m^3)
pub const VOXEL_VOLUME: f32 = VOXEL_SIZE * VOXEL_SIZE * VOXEL_SIZE;

/// Flag status bitwise untuk integritas struktural dan state voxel
pub mod voxel_flags {
    pub const SOLID: u8 = 1 << 0;
    pub const GROUNDED: u8 = 1 << 1;
    pub const AG_SUPPORT: u8 = 1 << 2;
    pub const STRUCTURAL_DIRTY: u8 = 1 << 3;
    pub const DETACHED: u8 = 1 << 4;
}

/// Struktur data inti VoxelBlock.
///
/// INVARIANT 1: Tepat 4 bytes (`#[repr(C)]`) untuk memaksimalkan densitas memori
/// dan efisiensi cache L1/L2 pada arsitektur Intel x86_64.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Pod, Zeroable, Serialize, Deserialize)]
pub struct VoxelBlock {
    pub material: MaterialId, // 2 Byte: ID material
    pub flags: u8,            // 1 Byte: Bitflags struktural
    pub light_ao: u8,         // 1 Byte: Derived AO & Light cache
}

// Compile-time assertion untuk memastikan ukuran VoxelBlock selalu tepat 4 bytes
const _: () = assert!(std::mem::size_of::<VoxelBlock>() == 4);
const _: () = assert!(std::mem::align_of::<VoxelBlock>() == 2);

impl VoxelBlock {
    pub const AIR: Self = Self {
        material: MaterialId::AIR,
        flags: 0,
        light_ao: 0,
    };

    #[inline(always)]
    pub const fn new(material: MaterialId) -> Self {
        let flags = if material.0 != 0 {
            voxel_flags::SOLID
        } else {
            0
        };
        Self {
            material,
            flags,
            light_ao: 0,
        }
    }

    #[inline(always)]
    pub const fn with_flags(material: MaterialId, flags: u8) -> Self {
        Self {
            material,
            flags,
            light_ao: 0,
        }
    }

    #[inline(always)]
    pub fn is_air(&self) -> bool {
        self.material == MaterialId::AIR
    }

    #[inline(always)]
    pub fn is_solid(&self) -> bool {
        (self.flags & voxel_flags::SOLID) != 0 && !self.is_air()
    }

    #[inline(always)]
    pub fn is_grounded(&self) -> bool {
        (self.flags & voxel_flags::GROUNDED) != 0
    }

    #[inline(always)]
    pub fn has_ag_support(&self) -> bool {
        (self.flags & voxel_flags::AG_SUPPORT) != 0
    }

    #[inline(always)]
    pub fn set_grounded(&mut self, grounded: bool) {
        if grounded {
            self.flags |= voxel_flags::GROUNDED;
        } else {
            self.flags &= !voxel_flags::GROUNDED;
        }
    }

    #[inline(always)]
    pub fn set_ag_support(&mut self, supported: bool) {
        if supported {
            self.flags |= voxel_flags::AG_SUPPORT;
        } else {
            self.flags &= !voxel_flags::AG_SUPPORT;
        }
    }
}
