use glam::{IVec3, Vec3};

use crate::material::MaterialId;
use crate::mesh::types::FaceDirection;

/// Konfigurasi default jangkauan interaksi pemain dalam meter (5.0m = 10 voxel)
pub const DEFAULT_INTERACTION_REACH: f32 = 5.0;

/// Informasi detail benturan raycast terhadap voxel solid (Phase 11.1)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoxelHit {
    /// Koordinat integer voxel global yang tertabrak (world voxel)
    pub voxel_coord: IVec3,
    /// Material ID dari voxel yang tertabrak
    pub material: MaterialId,
    /// Titik potong persis ray pada permukaan sisi voxel dalam satuan meter dunia
    pub hit_point: Vec3,
    /// Jarak Euclidean dari origin ray ke titik potong (meters)
    pub distance: f32,
    /// Sisi kubus voxel yang tertabrak
    pub face: FaceDirection,
    /// Vektor normal satuan keluar dari sisi kubus yang tertabrak
    pub normal: Vec3,
}

/// Hasil evaluasi query interaksi raycast voxel terhadap dunia
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VoxelRaycastResult {
    /// Menabrak voxel solid yang resident di memori dalam batas max_reach
    Hit(VoxelHit),
    /// Ray tidak menabrak voxel solid mana pun hingga batas max_reach (semua resident air)
    Miss,
    /// Ray memasuki ruang voxel yang belum termuat (non-resident / unloaded chunk) sebelum menabrak solid
    NonResident {
        /// Koordinat voxel non-resident pertama yang ditemui di sepanjang ray
        voxel_coord: IVec3,
        /// Jarak dari origin ray ke batas voxel non-resident tersebut (meters)
        distance: f32,
        /// Titik potong ray saat memasuki ruang non-resident
        hit_point: Vec3,
        /// Sisi voxel non-resident yang dimasuki
        face: FaceDirection,
    },
}

impl VoxelRaycastResult {
    /// Apakah query menghasilkan tabrakan solid voxel
    #[inline(always)]
    pub fn is_hit(&self) -> bool {
        matches!(self, Self::Hit(_))
    }

    /// Apakah query meleset tanpa menabrak dan tanpa menemui chunk non-resident
    #[inline(always)]
    pub fn is_miss(&self) -> bool {
        matches!(self, Self::Miss)
    }

    /// Apakah ray menemui batas chunk non-resident
    #[inline(always)]
    pub fn is_non_resident(&self) -> bool {
        matches!(self, Self::NonResident { .. })
    }

    /// Mengambil referensi detail tabrakan jika berstatus `Hit`
    #[inline(always)]
    pub fn hit(&self) -> Option<&VoxelHit> {
        match self {
            Self::Hit(h) => Some(h),
            _ => None,
        }
    }

    /// Mengambil koordinat voxel yang terlibat (baik solid hit maupun first non-resident voxel)
    #[inline(always)]
    pub fn voxel_coord(&self) -> Option<IVec3> {
        match self {
            Self::Hit(h) => Some(h.voxel_coord),
            Self::NonResident { voxel_coord, .. } => Some(*voxel_coord),
            Self::Miss => None,
        }
    }

    /// Mengambil titik kontak dalam satuan meter dunia
    #[inline(always)]
    pub fn hit_point(&self) -> Option<Vec3> {
        match self {
            Self::Hit(h) => Some(h.hit_point),
            Self::NonResident { hit_point, .. } => Some(*hit_point),
            Self::Miss => None,
        }
    }

    /// Mengambil jarak Euclidean dari origin ke target
    #[inline(always)]
    pub fn distance(&self) -> Option<f32> {
        match self {
            Self::Hit(h) => Some(h.distance),
            Self::NonResident { distance, .. } => Some(*distance),
            Self::Miss => None,
        }
    }

    /// Mengambil sisi kubus yang terkena
    #[inline(always)]
    pub fn face(&self) -> Option<FaceDirection> {
        match self {
            Self::Hit(h) => Some(h.face),
            Self::NonResident { face, .. } => Some(*face),
            Self::Miss => None,
        }
    }

    /// Mengambil normal permukaan jika berstatus `Hit` atau `NonResident`
    #[inline(always)]
    pub fn normal(&self) -> Option<Vec3> {
        match self {
            Self::Hit(h) => Some(h.normal),
            Self::NonResident { face, .. } => Some(face.normal_vec3()),
            Self::Miss => None,
        }
    }
}
