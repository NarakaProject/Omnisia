use glam::{IVec3, Vec3};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::csg::edit::VoxelEditError;
use crate::csg::transaction::VoxelEditCommitResult;
use crate::material::MaterialId;
use crate::mesh::types::FaceDirection;
pub use crate::modding::definitions::{
    InteractableComponent, InteractableId, SupportRule, ToolCategory, ToolDefinition,
    ToolEffectiveness, ToolId, ToolRequirement,
};
pub use crate::modding::resource_id::ResourceId;
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
    /// Nilai cache runtime turunan dari `HarvestableComponent.required_tool` (Phase 11.5).
    /// INVARIANT GUARDRAIL 2: Derived / runtime cache only, BUKAN sumber kebenaran independen.
    pub required_tool: ToolRequirement,
}

impl ResourceDefinition {
    /// Membuat ResourceDefinition baru secara eksplisit
    pub fn new(resource_id: ResourceId, base_yield: u32) -> Self {
        Self {
            resource_id,
            base_yield,
            harvestable: true,
            source_block: None,
            required_tool: ToolRequirement::None,
        }
    }

    /// Menandai blok sumber asal dari mana resource ini dipetakan
    pub fn with_source_block(mut self, block_id: ResourceId) -> Self {
        self.source_block = Some(block_id);
        self
    }

    /// Menentukan kebutuhan alat turunan untuk resource ini (Phase 11.5)
    pub fn with_required_tool(mut self, required_tool: ToolRequirement) -> Self {
        self.required_tool = required_tool;
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

/// Orientasi spasial diskret balok voxel dalam grid 3D kubik (Phase 11.4).
///
/// Menjamin determinisme dan kekebalan terhadap akumulasi drifting floating-point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BlockOrientation {
    /// Orientasi default standar (isotropik / tanpa orientasi khusus)
    #[default]
    Default,
    /// Menghadap ke salah satu dari 6 sisi kubus kanonikal
    Facing(FaceDirection),
}

impl BlockOrientation {
    /// Mengonstruksi orientasi diskret dari arah pandang kontinu pemain
    /// secara deterministik menggunakan sumbu dominan 3D.
    pub fn from_look_direction(look: Vec3) -> Self {
        let abs_x = look.x.abs();
        let abs_y = look.y.abs();
        let abs_z = look.z.abs();

        if abs_x >= abs_y && abs_x >= abs_z {
            if look.x >= 0.0 {
                Self::Facing(FaceDirection::PosX)
            } else {
                Self::Facing(FaceDirection::NegX)
            }
        } else if abs_y >= abs_x && abs_y >= abs_z {
            if look.y >= 0.0 {
                Self::Facing(FaceDirection::PosY)
            } else {
                Self::Facing(FaceDirection::NegY)
            }
        } else if look.z >= 0.0 {
            Self::Facing(FaceDirection::PosZ)
        } else {
            Self::Facing(FaceDirection::NegZ)
        }
    }

    /// Mengonstruksi orientasi horizontal 4-arah (abaikan sumbu Y)
    pub fn from_horizontal_look(look: Vec3) -> Self {
        let abs_x = look.x.abs();
        let abs_z = look.z.abs();

        if abs_x >= abs_z {
            if look.x >= 0.0 {
                Self::Facing(FaceDirection::PosX)
            } else {
                Self::Facing(FaceDirection::NegX)
            }
        } else if look.z >= 0.0 {
            Self::Facing(FaceDirection::PosZ)
        } else {
            Self::Facing(FaceDirection::NegZ)
        }
    }

    /// Mengonstruksi orientasi eksplisit dari FaceDirection
    #[inline(always)]
    pub const fn from_facing(face: FaceDirection) -> Self {
        Self::Facing(face)
    }

    /// Mengambil sisi hadap jika orientasi bertipe Facing
    #[inline(always)]
    pub fn facing(&self) -> Option<FaceDirection> {
        match self {
            Self::Facing(f) => Some(*f),
            Self::Default => None,
        }
    }

    /// Apakah orientasi merupakan default
    #[inline(always)]
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Default)
    }
}

/// Alasan penolakan proposal atau validasi penempatan balok (Phase 11.4)
#[derive(Debug, Clone, PartialEq)]
pub enum PlacementRejectionReason {
    /// Raycast tidak menabrak voxel solid mana pun
    NoTargetHit,
    /// Chunk koordinat target voxel tidak termuat di memori
    TargetNotResident { coord: IVec3 },
    /// Chunk koordinat tujuan penempatan kandidat tidak termuat di memori
    CandidateNotResident { coord: IVec3 },
    /// Jarak kontak melebihi batas jangkauan interaksi (reach)
    ExceedsReach { distance: f32, max_reach: f32 },
    /// Sisi voxel yang ditarget sudah berupa udara (bukan solid)
    TargetIsAir { coord: IVec3 },
    /// Koordinat kandidat penempatan sudah ditempati oleh voxel solid
    CandidateOccupied {
        coord: IVec3,
        current_material: MaterialId,
    },
    /// Penempatan ditolak karena beririsan dengan kapsul tabrakan pemain
    PlayerCapsuleOverlap { coord: IVec3 },
    /// Material yang dicoba untuk ditempatkan tidak valid (misal AIR)
    InvalidMaterial(String),
    /// Blok membutuhkan penopang fisik namun tidak ada penopang yang memenuhi syarat
    SupportMissing { coord: IVec3, rule: SupportRule },
    /// Koordinat penopang yang disyaratkan berada di chunk yang belum termuat (non-resident)
    SupportNotResident { coord: IVec3 },
    /// Orientasi penempatan tidak valid untuk blok ini
    InvalidOrientation(String),
    /// Cooldown aksi interaksi masih aktif
    CooldownActive { remaining: f32 },
    /// Kesalahan pada lapisan transaksi CSG
    TransactionError(String),
    /// Proposal telah usang dan gagal pada validasi final otoritatif
    StaleProposal { reason: String },
}

impl fmt::Display for PlacementRejectionReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTargetHit => write!(f, "No target hit by placement raycast"),
            Self::TargetNotResident { coord } => {
                write!(f, "Target voxel at {:?} is in a non-resident chunk", coord)
            }
            Self::CandidateNotResident { coord } => {
                write!(
                    f,
                    "Candidate voxel at {:?} is in a non-resident chunk",
                    coord
                )
            }
            Self::ExceedsReach {
                distance,
                max_reach,
            } => {
                write!(
                    f,
                    "Placement distance ({:.2}m) exceeds max reach ({:.2}m)",
                    distance, max_reach
                )
            }
            Self::TargetIsAir { coord } => write!(f, "Placement target at {:?} is air", coord),
            Self::CandidateOccupied {
                coord,
                current_material,
            } => {
                write!(
                    f,
                    "Candidate location at {:?} is already occupied (material={:?})",
                    coord, current_material
                )
            }
            Self::PlayerCapsuleOverlap { coord } => {
                write!(
                    f,
                    "Candidate voxel at {:?} overlaps player collision capsule",
                    coord
                )
            }
            Self::InvalidMaterial(msg) => write!(f, "Invalid placement material: {}", msg),
            Self::SupportMissing { coord, rule } => {
                write!(
                    f,
                    "Support required by rule {:?} is missing for placement at {:?}",
                    rule, coord
                )
            }
            Self::SupportNotResident { coord } => {
                write!(
                    f,
                    "Required support location at {:?} is in a non-resident chunk",
                    coord
                )
            }
            Self::InvalidOrientation(msg) => write!(f, "Invalid placement orientation: {}", msg),
            Self::CooldownActive { remaining } => {
                write!(f, "Placement cooldown active ({:.3}s remaining)", remaining)
            }
            Self::TransactionError(msg) => write!(f, "Placement transaction error: {}", msg),
            Self::StaleProposal { reason } => {
                write!(f, "Placement proposal is stale: {}", reason)
            }
        }
    }
}

/// Status validitas dari proposal penempatan balok (Phase 11.4)
#[derive(Debug, Clone, PartialEq)]
pub enum PlacementValidity {
    /// Proposal valid dan siap diajukan ke validasi transaksi final
    Valid,
    /// Proposal tidak valid beserta alasan penolakannya
    Invalid(PlacementRejectionReason),
}

impl PlacementValidity {
    #[inline(always)]
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid)
    }

    #[inline(always)]
    pub fn rejection_reason(&self) -> Option<&PlacementRejectionReason> {
        match self {
            Self::Invalid(reason) => Some(reason),
            Self::Valid => None,
        }
    }
}

/// Proposal semantik penempatan balok (Phase 11.4).
///
/// INVARIANT:
/// 1. Berstatus DERIVED state (bukan otoritas dunia).
/// 2. Murni in-memory / backend state tanpa ketergantungan renderer GPU atau ghost mesh.
/// 3. Tidak memutasi ChunkStore atau World saat dibuat.
/// 4. Harus divalidasi ulang terhadap state otoritatif sebelum dikomit.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacementProposal {
    /// Koordinat voxel solid yang menjadi target klik
    pub target_voxel: IVec3,
    /// Koordinat kandidat penempatan (target_voxel + normal sisi)
    pub candidate_voxel: IVec3,
    /// Sisi kubus target yang terkena raycast
    pub target_face: FaceDirection,
    /// Orientasi diskret balok yang diajukan
    pub orientation: BlockOrientation,
    /// Material ID dari voxel yang diajukan
    pub material: MaterialId,
    /// ResourceId blok opsional jika berasal dari BlockDefinition
    pub block_id: Option<ResourceId>,
    /// Status validitas semantik proposal saat dievaluasi
    pub validity: PlacementValidity,
}

impl PlacementProposal {
    #[inline(always)]
    pub fn is_valid(&self) -> bool {
        self.validity.is_valid()
    }

    #[inline(always)]
    pub fn rejection_reason(&self) -> Option<&PlacementRejectionReason> {
        self.validity.rejection_reason()
    }
}

/// Kesalahan yang dapat terjadi saat validasi atau eksekusi penempatan balok (Phase 11.4)
#[derive(Debug, Clone, PartialEq)]
pub enum PlacementError {
    NoTargetHit,
    TargetNotResident {
        coord: IVec3,
    },
    CandidateNotResident {
        coord: IVec3,
    },
    ExceedsReach {
        distance: f32,
        max_reach: f32,
    },
    TargetIsAir {
        coord: IVec3,
    },
    CandidateOccupied {
        coord: IVec3,
        current_material: MaterialId,
    },
    PlayerCapsuleOverlap {
        coord: IVec3,
    },
    InvalidMaterial(String),
    SupportMissing {
        coord: IVec3,
        rule: SupportRule,
    },
    SupportNotResident {
        coord: IVec3,
    },
    InvalidOrientation(String),
    CooldownActive {
        remaining: f32,
    },
    TransactionError(VoxelEditError),
    MutationError(InteractionMutationError),
    StaleProposal(String),
}

impl fmt::Display for PlacementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTargetHit => write!(f, "No target hit by placement raycast"),
            Self::TargetNotResident { coord } => {
                write!(f, "Target voxel at {:?} is in a non-resident chunk", coord)
            }
            Self::CandidateNotResident { coord } => {
                write!(
                    f,
                    "Candidate voxel at {:?} is in a non-resident chunk",
                    coord
                )
            }
            Self::ExceedsReach {
                distance,
                max_reach,
            } => {
                write!(
                    f,
                    "Placement distance ({:.2}m) exceeds max reach ({:.2}m)",
                    distance, max_reach
                )
            }
            Self::TargetIsAir { coord } => write!(f, "Placement target at {:?} is air", coord),
            Self::CandidateOccupied {
                coord,
                current_material,
            } => {
                write!(
                    f,
                    "Candidate location at {:?} is already occupied (material={:?})",
                    coord, current_material
                )
            }
            Self::PlayerCapsuleOverlap { coord } => {
                write!(
                    f,
                    "Placement at {:?} overlaps player collision capsule",
                    coord
                )
            }
            Self::InvalidMaterial(msg) => write!(f, "Invalid placement material: {}", msg),
            Self::SupportMissing { coord, rule } => {
                write!(
                    f,
                    "Support required by rule {:?} is missing for placement at {:?}",
                    rule, coord
                )
            }
            Self::SupportNotResident { coord } => {
                write!(
                    f,
                    "Required support location at {:?} is in a non-resident chunk",
                    coord
                )
            }
            Self::InvalidOrientation(msg) => write!(f, "Invalid placement orientation: {}", msg),
            Self::CooldownActive { remaining } => {
                write!(f, "Placement cooldown active ({:.3}s remaining)", remaining)
            }
            Self::TransactionError(err) => write!(f, "Placement transaction error: {}", err),
            Self::MutationError(err) => write!(f, "Placement mutation error: {}", err),
            Self::StaleProposal(msg) => write!(f, "Placement proposal is stale: {}", msg),
        }
    }
}

impl std::error::Error for PlacementError {}

impl From<VoxelEditError> for PlacementError {
    fn from(err: VoxelEditError) -> Self {
        Self::TransactionError(err)
    }
}

impl From<InteractionMutationError> for PlacementError {
    fn from(err: InteractionMutationError) -> Self {
        match err {
            InteractionMutationError::NoTargetHit => Self::NoTargetHit,
            InteractionMutationError::TargetNotResident { coord } => {
                Self::TargetNotResident { coord }
            }
            InteractionMutationError::DestinationNotResident { coord } => {
                Self::CandidateNotResident { coord }
            }
            InteractionMutationError::ExceedsReach {
                distance,
                max_reach,
            } => Self::ExceedsReach {
                distance,
                max_reach,
            },
            InteractionMutationError::PlacementOccupied { coord, current } => {
                Self::CandidateOccupied {
                    coord,
                    current_material: current.material(),
                }
            }
            InteractionMutationError::PlayerCapsuleOverlap { coord } => {
                Self::PlayerCapsuleOverlap { coord }
            }
            InteractionMutationError::RemovalTargetIsAir { coord } => Self::TargetIsAir { coord },
            InteractionMutationError::InvalidMaterial(msg) => Self::InvalidMaterial(msg),
            InteractionMutationError::CooldownActive { remaining } => {
                Self::CooldownActive { remaining }
            }
            InteractionMutationError::TransactionError(e) => Self::TransactionError(e),
        }
    }
}

impl From<PlacementRejectionReason> for PlacementError {
    fn from(reason: PlacementRejectionReason) -> Self {
        match reason {
            PlacementRejectionReason::NoTargetHit => Self::NoTargetHit,
            PlacementRejectionReason::TargetNotResident { coord } => {
                Self::TargetNotResident { coord }
            }
            PlacementRejectionReason::CandidateNotResident { coord } => {
                Self::CandidateNotResident { coord }
            }
            PlacementRejectionReason::ExceedsReach {
                distance,
                max_reach,
            } => Self::ExceedsReach {
                distance,
                max_reach,
            },
            PlacementRejectionReason::TargetIsAir { coord } => Self::TargetIsAir { coord },
            PlacementRejectionReason::CandidateOccupied {
                coord,
                current_material,
            } => Self::CandidateOccupied {
                coord,
                current_material,
            },
            PlacementRejectionReason::PlayerCapsuleOverlap { coord } => {
                Self::PlayerCapsuleOverlap { coord }
            }
            PlacementRejectionReason::InvalidMaterial(msg) => Self::InvalidMaterial(msg),
            PlacementRejectionReason::SupportMissing { coord, rule } => {
                Self::SupportMissing { coord, rule }
            }
            PlacementRejectionReason::SupportNotResident { coord } => {
                Self::SupportNotResident { coord }
            }
            PlacementRejectionReason::InvalidOrientation(msg) => Self::InvalidOrientation(msg),
            PlacementRejectionReason::CooldownActive { remaining } => {
                Self::CooldownActive { remaining }
            }
            PlacementRejectionReason::TransactionError(msg) => {
                Self::StaleProposal(format!("Transaction error: {}", msg))
            }
            PlacementRejectionReason::StaleProposal { reason } => Self::StaleProposal(reason),
        }
    }
}

/// Hasil agregat dari aksi penempatan balok yang berhasil dieksekusi ke dunia (Phase 11.4)
#[derive(Debug, Clone, PartialEq)]
pub struct PlacementResult {
    /// Proposal semantik yang berhasil dieksekusi
    pub proposal: PlacementProposal,
    /// Hasil komit mutasi fisik dan struktural di dunia
    pub mutation: VoxelMutationResult,
}

/// State instans alat aktif yang dibawa oleh pemanggil aksi (Phase 11.5).
/// Mewakili kapabilitas alat aktif yang digunakan dalam aksi interaksi saat ini.
/// INVARIANT GUARDRAIL 3: Hanya menyimpan durabilitas saat ini; durabilitas maksimum
/// bersumber otoritatif dari `ToolDefinition`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolState {
    /// Identitas unik alat
    pub tool_id: ToolId,
    /// Durabilitas saat ini (0 <= current_durability <= tool_def.max_durability)
    pub current_durability: u32,
}

impl ToolState {
    /// Membuat ToolState baru dengan durabilitas saat ini
    pub fn new(tool_id: ToolId, current_durability: u32) -> Self {
        Self {
            tool_id,
            current_durability,
        }
    }

    /// Apakah alat sudah rusak / habis durabilitasnya (durabilitas == 0)
    #[inline(always)]
    pub fn is_broken(&self) -> bool {
        self.current_durability == 0
    }

    /// Apakah alat masih dapat digunakan (durabilitas > 0)
    #[inline(always)]
    pub fn is_usable(&self) -> bool {
        self.current_durability > 0
    }

    /// Mengurangi 1 unit durabilitas secara aman (saturating_sub).
    /// INVARIANT GUARDRAIL 3: Saturating sub digunakan HANYA untuk dekremen pasca-komit yang telah sukses.
    pub fn consume_durability(&mut self) -> bool {
        if self.current_durability > 0 {
            self.current_durability = self.current_durability.saturating_sub(1);
            true
        } else {
            false
        }
    }
}

/// Aksi semantik yang dilakukan menggunakan alat (Phase 11.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolAction {
    /// Aksi pemanenan resource voxel dunia
    Gather,
}

/// Hasil agregat dari aksi pemanenan voxel yang menggunakan alat (Phase 11.5).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolGatheringResult {
    /// Hasil koleksi semantik (source_coord, resource_id, quantity)
    pub collection: CollectionResult,
    /// Hasil mutasi fisik dan struktural di dunia
    pub mutation: VoxelMutationResult,
    /// Identitas alat yang digunakan (None jika tangan kosong / tanpa alat aktif)
    pub tool_id: Option<ToolId>,
    /// Efektivitas alat yang diterapkan pada aksi ini (metadata semantik)
    pub effectiveness: f32,
    /// Jumlah durabilitas yang dikonsumsi (0 jika tanpa alat, 1 jika menggunakan alat)
    pub durability_consumed: u32,
    /// Sisa durabilitas alat setelah aksi dieksekusi (None jika tanpa alat aktif)
    pub remaining_durability: Option<u32>,
}

/// Kesalahan yang dapat terjadi selama validasi atau eksekusi aksi alat (Phase 11.5).
#[derive(Debug, Clone, PartialEq)]
pub enum ToolError {
    /// Aksi membutuhkan alat, namun tidak ada alat aktif yang disediakan (tangan kosong)
    NoTool { required: ToolRequirement },
    /// Alat yang aktif memiliki kategori yang salah untuk resource ini
    WrongToolCategory {
        expected: ToolCategory,
        actual: ToolCategory,
    },
    /// Alat yang aktif bukan alat spesifik yang disyaratkan
    WrongTool { expected: ToolId, actual: ToolId },
    /// Alat yang aktif telah rusak (durabilitas == 0)
    ToolBroken { tool_id: ToolId },
    /// Syarat alat tidak terpenuhi
    ToolRequirementNotMet { reason: String },
    /// Definisi alat tidak ditemukan dalam ToolRegistry
    UnknownTool { tool_id: ToolId },
    /// State alat tidak valid (misal: current_durability melebihi max_durability definisi)
    InvalidToolState { reason: String },
    /// Definisi alat tidak valid (misal: multiplier negatif atau NaN)
    InvalidToolDefinition(String),
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

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTool { required } => {
                write!(f, "A tool is required for this resource ({:?})", required)
            }
            Self::WrongToolCategory { expected, actual } => {
                write!(
                    f,
                    "Wrong tool category: expected {:?}, actual {:?}",
                    expected, actual
                )
            }
            Self::WrongTool { expected, actual } => {
                write!(
                    f,
                    "Wrong specific tool: expected {}, actual {}",
                    expected, actual
                )
            }
            Self::ToolBroken { tool_id } => {
                write!(f, "Tool '{}' is broken (durability is 0)", tool_id)
            }
            Self::ToolRequirementNotMet { reason } => {
                write!(f, "Tool requirement not met: {}", reason)
            }
            Self::UnknownTool { tool_id } => {
                write!(f, "Tool '{}' not found in tool registry", tool_id)
            }
            Self::InvalidToolState { reason } => write!(f, "Invalid tool state: {}", reason),
            Self::InvalidToolDefinition(msg) => write!(f, "Invalid tool definition: {}", msg),
            Self::NoTargetHit => write!(f, "No solid voxel target hit by ray"),
            Self::TargetNotResident { coord } => {
                write!(f, "Target chunk at {:?} is not resident", coord)
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
            Self::TargetIsAir { coord } => write!(f, "Target at {:?} is air", coord),
            Self::NotHarvestable {
                coord,
                material,
                block_id,
            } => {
                write!(
                    f,
                    "Voxel at {:?} (material={:?}, block={:?}) is not harvestable",
                    coord, material, block_id
                )
            }
            Self::CooldownActive { remaining } => {
                write!(
                    f,
                    "Interaction cooldown active ({:.3}s remaining)",
                    remaining
                )
            }
            Self::TransactionError(err) => write!(f, "Transaction error: {}", err),
            Self::MutationError(err) => write!(f, "Mutation error: {}", err),
        }
    }
}

impl std::error::Error for ToolError {}

impl From<VoxelEditError> for ToolError {
    fn from(err: VoxelEditError) -> Self {
        Self::TransactionError(err)
    }
}

impl From<GatheringError> for ToolError {
    fn from(err: GatheringError) -> Self {
        match err {
            GatheringError::NoTargetHit => Self::NoTargetHit,
            GatheringError::TargetNotResident { coord } => Self::TargetNotResident { coord },
            GatheringError::ExceedsReach {
                distance,
                max_reach,
            } => Self::ExceedsReach {
                distance,
                max_reach,
            },
            GatheringError::TargetIsAir { coord } => Self::TargetIsAir { coord },
            GatheringError::NotHarvestable {
                coord,
                material,
                block_id,
            } => Self::NotHarvestable {
                coord,
                material,
                block_id,
            },
            GatheringError::CooldownActive { remaining } => Self::CooldownActive { remaining },
            GatheringError::TransactionError(e) => Self::TransactionError(e),
            GatheringError::MutationError(e) => Self::MutationError(e),
        }
    }
}

impl From<InteractionMutationError> for ToolError {
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

// ============================================================================
// Generic Interactables & Feedback Primitives (Phase 11.6)
// ============================================================================

/// Primitive semantic states for interactable world objects (Phase 11.6).
/// INVARIANT: Small Phase 11.6 data primitive, not a state machine framework.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InteractableState {
    #[default]
    Idle,
    Active,
    Open,
    Closed,
    Disabled,
}

impl fmt::Display for InteractableState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Active => write!(f, "active"),
            Self::Open => write!(f, "open"),
            Self::Closed => write!(f, "closed"),
            Self::Disabled => write!(f, "disabled"),
        }
    }
}

/// Intentionally small, typed set of interactable actions (Phase 11.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractableAction {
    Activate,
    Toggle,
    Open,
    Close,
    Examine,
}

impl fmt::Display for InteractableAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Activate => write!(f, "activate"),
            Self::Toggle => write!(f, "toggle"),
            Self::Open => write!(f, "open"),
            Self::Close => write!(f, "close"),
            Self::Examine => write!(f, "examine"),
        }
    }
}

/// Semantic audio cue for interactable feedback (Phase 11.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioCue {
    Click,
    SwitchToggle,
    DoorOpen,
    DoorClose,
    Thud,
    FailLocked,
}

/// Semantic visual cue for interactable feedback (Phase 11.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualCue {
    Pulse,
    StateTransition,
    ErrorShake,
}

/// Semantic feedback identifier (Phase 11.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackId {
    Activated,
    Deactivated,
    Toggled,
    Opened,
    Closed,
    Examined,
    Locked,
    Disabled,
}

/// Semantic feedback payload (Phase 11.6). Pure data; zero UI copy, zero audio handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InteractionFeedback {
    pub audio_cue: Option<AudioCue>,
    pub visual_cue: Option<VisualCue>,
    pub feedback_id: Option<FeedbackId>,
}

impl InteractionFeedback {
    pub fn new(
        audio_cue: Option<AudioCue>,
        visual_cue: Option<VisualCue>,
        feedback_id: Option<FeedbackId>,
    ) -> Self {
        Self {
            audio_cue,
            visual_cue,
            feedback_id,
        }
    }
}

/// Runtime cache definition of an interactable world object derived from `BlockDefinition`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractableDefinition {
    pub id: InteractableId,
    pub source_block: ResourceId,
    pub expected_material: MaterialId,
    pub allowed_actions: Vec<InteractableAction>,
    pub initial_state: InteractableState,
    pub audio_cue: Option<AudioCue>,
    pub visual_cue: Option<VisualCue>,
}

/// Description of a detected interactable target (Phase 11.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractableTarget {
    pub interactable_id: InteractableId,
    pub source_block: ResourceId,
    pub expected_material: MaterialId,
    pub coord: IVec3,
    pub current_state: InteractableState,
    pub available_actions: Vec<InteractableAction>,
    pub preferred_action: Option<InteractableAction>,
}

/// Validated interaction intent ready for execution (Phase 11.6).
/// INVARIANT: Read-only; generating a proposal never mutates world state or cooldown.
/// Captures expected_material for mandatory TOCTOU revalidation before execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionProposal {
    pub interactable_id: InteractableId,
    pub coord: IVec3,
    pub expected_material: MaterialId,
    pub action: InteractableAction,
    pub previous_state: InteractableState,
    pub target_state: InteractableState,
    pub feedback: InteractionFeedback,
}

/// Final deterministic result of an executed interaction (Phase 11.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionResult {
    pub interactable_id: InteractableId,
    pub coord: IVec3,
    pub action: InteractableAction,
    pub previous_state: InteractableState,
    pub new_state: InteractableState,
    pub feedback: InteractionFeedback,
}

/// Kategori kesalahan saat kueri, validasi, atau eksekusi interaksi generik (Phase 11.6).
#[derive(Debug, Clone, PartialEq)]
pub enum InteractionError {
    NoTargetHit,
    TargetNotResident {
        coord: IVec3,
    },
    ExceedsReach {
        distance: f32,
        max_reach: f32,
    },
    TargetIsAir {
        coord: IVec3,
    },
    NotInteractable {
        coord: IVec3,
    },
    ObjectDisabled {
        interactable_id: InteractableId,
    },
    InvalidActionForState {
        action: InteractableAction,
        state: InteractableState,
    },
    ActionNotAllowed {
        action: InteractableAction,
        interactable_id: InteractableId,
    },
    CooldownActive {
        remaining: f32,
    },
    UnknownInteractable {
        id: InteractableId,
    },
    /// TOCTOU failure: current interactable ID does not match proposal.interactable_id
    InteractableMismatch {
        expected: InteractableId,
        actual: Option<InteractableId>,
    },
    /// TOCTOU failure: current physical material does not match proposal.expected_material
    MaterialMismatch {
        expected: MaterialId,
        actual: MaterialId,
    },
    /// TOCTOU failure: current state does not match proposal.previous_state
    StateMismatch {
        expected: InteractableState,
        actual: InteractableState,
    },
}

impl fmt::Display for InteractionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTargetHit => write!(f, "Raycast meleset, tidak ada voxel yang tertabrak"),
            Self::TargetNotResident { coord } => {
                write!(
                    f,
                    "Chunk target pada koordinat {:?} belum termuat di memori",
                    coord
                )
            }
            Self::ExceedsReach {
                distance,
                max_reach,
            } => {
                write!(
                    f,
                    "Target berjarak {:.2}m, melebihi jangkauan maksimum {:.2}m",
                    distance, max_reach
                )
            }
            Self::TargetIsAir { coord } => {
                write!(f, "Target pada koordinat {:?} berupa udara (air)", coord)
            }
            Self::NotInteractable { coord } => {
                write!(f, "Voxel pada koordinat {:?} bukan objek interaktif", coord)
            }
            Self::ObjectDisabled { interactable_id } => {
                write!(
                    f,
                    "Objek interaktif {} dalam status dinonaktifkan (disabled)",
                    interactable_id
                )
            }
            Self::InvalidActionForState { action, state } => {
                write!(f, "Aksi {:?} tidak valid untuk status {:?}", action, state)
            }
            Self::ActionNotAllowed {
                action,
                interactable_id,
            } => {
                write!(
                    f,
                    "Aksi {:?} tidak diizinkan pada definisi {}",
                    action, interactable_id
                )
            }
            Self::CooldownActive { remaining } => {
                write!(
                    f,
                    "Cooldown interaksi masih aktif ({:.3}s tersisa)",
                    remaining
                )
            }
            Self::UnknownInteractable { id } => {
                write!(f, "Definisi interactable {} tidak terdaftar", id)
            }
            Self::InteractableMismatch { expected, actual } => {
                write!(
                    f,
                    "TOCTOU: Identitas interactable berubah (diharapkan {}, aktual {:?})",
                    expected, actual
                )
            }
            Self::MaterialMismatch { expected, actual } => {
                write!(
                    f,
                    "TOCTOU: Material voxel berubah (diharapkan {:?}, aktual {:?})",
                    expected, actual
                )
            }
            Self::StateMismatch { expected, actual } => {
                write!(
                    f,
                    "TOCTOU: Status interactable berubah (diharapkan {:?}, aktual {:?})",
                    expected, actual
                )
            }
        }
    }
}

impl std::error::Error for InteractionError {}
