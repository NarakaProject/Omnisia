use glam::{IVec3, Vec3};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::csg::edit::VoxelEditError;
use crate::csg::transaction::VoxelEditCommitResult;
use crate::material::MaterialId;
use crate::mesh::types::FaceDirection;
use crate::modding::resource_id::ResourceId;
use crate::structure::aggregate::DetachedAggregate;
use crate::voxel::VoxelBlock;

/// Konfigurasi default jangkauan interaksi pemain dalam meter (5.0m = 10 voxel)
pub const DEFAULT_INTERACTION_REACH: f32 = 5.0;

/// Konfigurasi default durasi cooldown debounce interaksi pemain dalam detik (0.20s = 5 aksi/detik)
pub const DEFAULT_INTERACTION_COOLDOWN: f32 = 0.20;

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

/// Tindakan interaksi yang diajukan oleh pemain terhadap dunia voxel (Phase 11.2)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteractionAction {
    /// Menghancurkan / menghapus voxel solid yang sedang ditarget
    RemoveVoxel,
    /// Menempatkan voxel solid baru pada bidang sisi kubus yang sedang ditarget
    PlaceVoxel { material: MaterialId },
}

/// Kesalahan yang dapat terjadi selama validasi atau eksekusi mutasi interaksi (Phase 11.2)
#[derive(Debug, Clone, PartialEq)]
pub enum InteractionMutationError {
    /// Raycast tidak mengenai voxel solid mana pun
    NoTargetHit,
    /// Chunk target voxel tidak resident di memori
    TargetNotResident { coord: IVec3 },
    /// Chunk koordinat penempatan voxel tidak resident di memori
    DestinationNotResident { coord: IVec3 },
    /// Jarak target kontak melampaui batas jangkauan (reach) yang diizinkan
    ExceedsReach { distance: f32, max_reach: f32 },
    /// Koordinat penempatan yang dituju sudah ditempati oleh voxel solid lain
    PlacementOccupied { coord: IVec3, current: VoxelBlock },
    /// Penempatan voxel ditolak karena volume voxel beririsan dengan kapsul tabrakan pemain
    PlayerCapsuleOverlap { coord: IVec3 },
    /// Target penghapusan sudah berupa udara (bukan solid)
    RemovalTargetIsAir { coord: IVec3 },
    /// Operasi material tidak valid (misal: mencoba menempatkan udara)
    InvalidMaterial(String),
    /// Cooldown debounce aksi interaksi pemain masih aktif
    CooldownActive { remaining: f32 },
    /// Kesalahan pada lapisan transaksi CSG bawaan
    TransactionError(VoxelEditError),
}

impl fmt::Display for InteractionMutationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTargetHit => write!(f, "No solid voxel target hit by interaction ray"),
            Self::TargetNotResident { coord } => {
                write!(
                    f,
                    "Target chunk for voxel {:?} is not resident in memory",
                    coord
                )
            }
            Self::DestinationNotResident { coord } => {
                write!(
                    f,
                    "Destination chunk for placement at {:?} is not resident in memory",
                    coord
                )
            }
            Self::ExceedsReach {
                distance,
                max_reach,
            } => {
                write!(
                    f,
                    "Interaction distance ({:.2}m) exceeds max reach ({:.2}m)",
                    distance, max_reach
                )
            }
            Self::PlacementOccupied { coord, current } => {
                write!(
                    f,
                    "Placement location {:?} is already occupied (material={:?})",
                    coord,
                    current.material()
                )
            }
            Self::PlayerCapsuleOverlap { coord } => {
                write!(
                    f,
                    "Placement at {:?} rejected: overlaps player collision capsule",
                    coord
                )
            }
            Self::RemovalTargetIsAir { coord } => {
                write!(f, "Removal target at {:?} is air", coord)
            }
            Self::InvalidMaterial(msg) => write!(f, "Invalid material: {}", msg),
            Self::CooldownActive { remaining } => {
                write!(
                    f,
                    "Interaction cooldown active ({:.3}s remaining)",
                    remaining
                )
            }
            Self::TransactionError(err) => write!(f, "Transaction error: {}", err),
        }
    }
}

impl std::error::Error for InteractionMutationError {}

impl From<VoxelEditError> for InteractionMutationError {
    fn from(err: VoxelEditError) -> Self {
        Self::TransactionError(err)
    }
}

/// Mekanisme debounce / cooldown interaksi pemain untuk mencegah mutasi berganda tak terkontrol
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InteractionCooldown {
    /// Durasi cooldown dasar dalam detik
    pub cooldown_seconds: f32,
    /// Waktu tersisa hingga aksi berikutnya diizinkan
    pub timer: f32,
}

impl InteractionCooldown {
    /// Membuat instance cooldown baru dengan batas durasi tertentu
    pub fn new(cooldown_seconds: f32) -> Self {
        Self {
            cooldown_seconds: cooldown_seconds.max(0.0),
            timer: 0.0,
        }
    }

    /// Memajukan waktu cooldown sebesar `dt` detik
    #[inline(always)]
    pub fn tick(&mut self, dt: f32) {
        if self.timer > 0.0 {
            self.timer = (self.timer - dt).max(0.0);
        }
    }

    /// Apakah pemain saat ini diizinkan untuk melakukan aksi mutasi baru
    #[inline(always)]
    pub fn can_act(&self) -> bool {
        self.timer <= 0.0
    }

    /// Memicu cooldown interaksi, mengunci aksi hingga durasi cooldown berlalu
    #[inline(always)]
    pub fn trigger(&mut self) {
        self.timer = self.cooldown_seconds;
    }

    /// Mereset cooldown secara paksa agar pemain dapat langsung beraksi
    #[inline(always)]
    pub fn reset(&mut self) {
        self.timer = 0.0;
    }
}

impl Default for InteractionCooldown {
    fn default() -> Self {
        Self::new(DEFAULT_INTERACTION_COOLDOWN)
    }
}

/// Hasil eksekusi mutasi interaksi voxel yang berhasil dikomit ke dunia (Phase 11.2)
#[derive(Debug, Clone, PartialEq)]
pub struct VoxelMutationResult {
    /// Hasil komit transaksi CSG (deltas, affected_chunks, mesh_invalidation_chunks, pre-states)
    pub commit_result: VoxelEditCommitResult,
    /// Daftar gugusan struktural yang terlepas menjadi DynamicBody akibat mutasi ini
    pub newly_detached_aggregates: Vec<DetachedAggregate>,
}

/// Definisi semantik resource yang dapat dipanen dari voxel dunia (Phase 11.3)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceDefinition {
    /// Identitas unik semantik resource (misal `core:iron_ore`, `core:stone`)
    pub resource_id: ResourceId,
    /// Kuantitas hasil panen (yield) dasar deterministik per voxel
    pub base_yield: u32,
    /// Apakah resource ini dapat dipanen saat ini
    pub harvestable: bool,
    /// ResourceId blok sumber jika dipetakan dari BlockDefinition
    pub source_block: Option<ResourceId>,
}

impl ResourceDefinition {
    /// Membuat ResourceDefinition baru secara eksplisit
    pub fn new(resource_id: ResourceId, base_yield: u32) -> Self {
        Self {
            resource_id,
            base_yield,
            harvestable: true,
            source_block: None,
        }
    }

    /// Menandai blok sumber asal dari mana resource ini dipetakan
    pub fn with_source_block(mut self, block_id: ResourceId) -> Self {
        self.source_block = Some(block_id);
        self
    }
}

/// Hasil semantik dari aksi pengumpulan/pemanenan resource yang berhasil (Phase 11.3)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionResult {
    /// Koordinat integer voxel global tempat resource dipanen
    pub source_coord: IVec3,
    /// Identitas unik semantik resource yang dikumpulkan
    pub resource_id: ResourceId,
    /// Kuantitas yang dipanen (deterministik)
    pub quantity: u32,
}

/// Hasil agregat dari aksi pemanenan voxel yang memadukan hasil koleksi dan mutasi dunia (Phase 11.3)
#[derive(Debug, Clone, PartialEq)]
pub struct GatheringResult {
    /// Data semantik resource yang berhasil dikumpulkan
    pub collection: CollectionResult,
    /// Hasil mutasi fisik dan struktural di dunia
    pub mutation: VoxelMutationResult,
}

/// Kesalahan yang dapat terjadi selama validasi atau eksekusi pemanenan resource (Phase 11.3)
#[derive(Debug, Clone, PartialEq)]
pub enum GatheringError {
    /// Raycast tidak mengenai target solid mana pun
    NoTargetHit,
    /// Chunk target voxel tidak resident di memori
    TargetNotResident { coord: IVec3 },
    /// Jarak interaksi melampaui batas jangkauan (reach)
    ExceedsReach { distance: f32, max_reach: f32 },
    /// Voxel target adalah udara (AIR)
    TargetIsAir { coord: IVec3 },
    /// Voxel target solid tetapi bukan resource yang dapat dipanen
    NotHarvestable {
        coord: IVec3,
        material: MaterialId,
        block_id: Option<ResourceId>,
    },
    /// Cooldown debounce interaksi masih aktif
    CooldownActive { remaining: f32 },
    /// Kesalahan pada lapisan transaksi CSG bawaan
    TransactionError(VoxelEditError),
    /// Kesalahan mutasi interaksi umum
    MutationError(InteractionMutationError),
}

impl fmt::Display for GatheringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTargetHit => write!(f, "No solid voxel target hit by gathering ray"),
            Self::TargetNotResident { coord } => {
                write!(
                    f,
                    "Target chunk for voxel {:?} is not resident in memory",
                    coord
                )
            }
            Self::ExceedsReach {
                distance,
                max_reach,
            } => {
                write!(
                    f,
                    "Gathering distance ({:.2}m) exceeds max reach ({:.2}m)",
                    distance, max_reach
                )
            }
            Self::TargetIsAir { coord } => {
                write!(f, "Gathering target at {:?} is air", coord)
            }
            Self::NotHarvestable {
                coord,
                material,
                block_id,
            } => {
                write!(
                    f,
                    "Voxel at {:?} (material={:?}, block={:?}) is not a harvestable resource",
                    coord, material, block_id
                )
            }
            Self::CooldownActive { remaining } => {
                write!(f, "Gathering cooldown active ({:.3}s remaining)", remaining)
            }
            Self::TransactionError(err) => write!(f, "Transaction error: {}", err),
            Self::MutationError(err) => write!(f, "Mutation error: {}", err),
        }
    }
}

impl std::error::Error for GatheringError {}

impl From<VoxelEditError> for GatheringError {
    fn from(err: VoxelEditError) -> Self {
        Self::TransactionError(err)
    }
}

impl From<InteractionMutationError> for GatheringError {
    fn from(err: InteractionMutationError) -> Self {
        match err {
            InteractionMutationError::NoTargetHit => Self::NoTargetHit,
            InteractionMutationError::TargetNotResident { coord } => {
                Self::TargetNotResident { coord }
            }
            InteractionMutationError::ExceedsReach {
                distance,
                max_reach,
            } => Self::ExceedsReach {
                distance,
                max_reach,
            },
            InteractionMutationError::RemovalTargetIsAir { coord } => Self::TargetIsAir { coord },
            InteractionMutationError::CooldownActive { remaining } => {
                Self::CooldownActive { remaining }
            }
            InteractionMutationError::TransactionError(e) => Self::TransactionError(e),
            other => Self::MutationError(other),
        }
    }
}
