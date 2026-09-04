use glam::{IVec3, Vec3};
use std::fmt;

use super::broadphase::RigidBodyId;
use crate::coord::{world_pos_to_world_voxel, world_voxel_to_world_pos};
use crate::structure::aggregate::DetachedAggregate;
use crate::voxel::{VoxelBlock, VOXEL_SIZE};

/// Identifier unik runtime untuk DynamicBody.
/// Terurut secara deterministik (`Ord`, `PartialOrd`) untuk menjamin iterasi deterministik (Amendment 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DynamicBodyId(pub u64);

impl fmt::Display for DynamicBodyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DynBody#{}", self.0)
    }
}

/// Status siklus hidup DynamicBody dalam runtime fisika.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynamicBodyState {
    /// Sedang bergerak atau aktif diproses fisika
    Active,
    /// Gerakan berada di bawah ambang batas kecepatan (inaktif sementara)
    Sleeping,
    /// Terbukti bertumpu stabil pada tumpuan solid dan siap reintegrasi ke dunia statis
    Settled,
}

/// Representasi entitas dinamis (Dynamic Aggregate Body) untuk Phase 8A dan Phase 9.11.
///
/// INVARIANT:
/// - Menyimpan satu `DetachedAggregate` secara eksklusif (kepemilikan tunggal, tidak ada duplikasi).
/// - Transformasi fisika dalam meter: `position` (m) dan `velocity` (m/s).
/// - `position` adalah posisi dunia titik referensi aggregate (sudut minimum lokal).
/// - `rigid_body_id` menghubungkan secara transaksional ke representasi fisik otoritatif di PhysicsWorld (Phase 9.11).
/// - 1 Voxel = 0.5 meter (VOXEL_SIZE).
#[derive(Debug, Clone)]
pub struct DynamicBody {
    pub id: DynamicBodyId,
    pub aggregate: DetachedAggregate,
    /// Posisi sudut minimum lokal aggregate dalam koordinat dunia (meter)
    pub position: Vec3,
    /// Kecepatan linier translasi dalam m/s
    pub velocity: Vec3,
    /// Pengali gravitasi (1.0 = gravitasi normal, 0.0 = AntiGravity, <0 = gravitasi terbalik)
    pub gravity_scale: f32,
    /// State siklus hidup saat ini
    pub state: DynamicBodyState,
    /// Jumlah tick berturut-turut di mana kecepatan di bawah threshold
    pub ticks_stationary: u32,
    /// Apakah badan ini sedang bersentuhan dengan permukaan tanah solid di bawahnya
    pub is_grounded: bool,
    /// ID representasi fisik kaku otoritatif di PhysicsWorld (Phase 9.11)
    pub rigid_body_id: Option<RigidBodyId>,
}

impl DynamicBody {
    /// Membuat DynamicBody baru dari DetachedAggregate yang diekstrak
    pub fn new(id: DynamicBodyId, aggregate: DetachedAggregate, position: Vec3) -> Self {
        Self {
            id,
            aggregate,
            position,
            velocity: Vec3::ZERO,
            gravity_scale: 1.0,
            state: DynamicBodyState::Active,
            ticks_stationary: 0,
            is_grounded: false,
            rigid_body_id: None,
        }
    }

    /// Membuat DynamicBody di posisi awal yang tepat sesuai koordinat dunia asal `min_voxel`
    /// Menggunakan Rust move semantics (zero clone alokasi heap untuk aggregate.voxels)
    pub fn from_detached_aggregate(id: DynamicBodyId, aggregate: DetachedAggregate) -> Self {
        let initial_position = world_voxel_to_world_pos(aggregate.min_voxel);
        Self::new(id, aggregate, initial_position)
    }

    /// Builder untuk mengatur pengali gravitasi (misal 0.0 untuk AntiGravity)
    pub fn with_gravity_scale(mut self, scale: f32) -> Self {
        self.gravity_scale = scale;
        self
    }

    /// Builder untuk mengatur kecepatan awal
    pub fn with_velocity(mut self, velocity: Vec3) -> Self {
        self.velocity = velocity;
        self
    }

    /// Builder untuk mengasosiasikan RigidBodyId otoritatif (Phase 9.11)
    pub fn with_rigid_body_id(mut self, rigid_body_id: RigidBodyId) -> Self {
        self.rigid_body_id = Some(rigid_body_id);
        self
    }

    /// Mengambil ID RigidBody fisik otoritatif jika terhubung
    #[inline(always)]
    pub fn rigid_body_id(&self) -> Option<RigidBodyId> {
        self.rigid_body_id
    }

    /// Memvalidasi integritas internal dari badan dinamis:
    /// - Tidak boleh kosong (voxel_count > 0)
    /// - Seluruh voxel berada dalam rentang [0 .. dimensions]
    pub fn validate_integrity(&self) -> bool {
        if self.voxel_count() == 0 {
            return false;
        }
        let dims = self.voxel_dimensions();
        for v in &self.aggregate.voxels {
            if v.relative_coord.x < 0
                || v.relative_coord.y < 0
                || v.relative_coord.z < 0
                || v.relative_coord.x >= dims.x
                || v.relative_coord.y >= dims.y
                || v.relative_coord.z >= dims.z
            {
                return false;
            }
        }
        true
    }

    /// Jumlah voxel solid dalam badan dinamis
    #[inline(always)]
    pub fn voxel_count(&self) -> usize {
        self.aggregate.voxel_count()
    }

    /// Dimensi ukuran aggregate dalam satuan voxel (dx, dy, dz)
    pub fn voxel_dimensions(&self) -> IVec3 {
        self.aggregate.max_voxel - self.aggregate.min_voxel + IVec3::ONE
    }

    /// Bounding box dunia dalam satuan meter (min_point, max_point)
    pub fn world_bounds(&self) -> (Vec3, Vec3) {
        let dims = self.voxel_dimensions();
        let size_meters = Vec3::new(
            dims.x as f32 * VOXEL_SIZE,
            dims.y as f32 * VOXEL_SIZE,
            dims.z as f32 * VOXEL_SIZE,
        );
        let min_point = self.position;
        let max_point = self.position + size_meters;
        (min_point, max_point)
    }

    /// Koordinat sudut minimum integer voxel dunia saat ini berdasarkan posisi meter
    #[inline(always)]
    pub fn current_base_voxel(&self) -> IVec3 {
        world_pos_to_world_voxel(self.position)
    }

    /// Rentang AABB koordinat integer voxel dunia saat ini (min_voxel, max_voxel)
    pub fn world_voxel_bounds(&self) -> (IVec3, IVec3) {
        let base_voxel = self.current_base_voxel();
        let dims = self.voxel_dimensions();
        let min_v = base_voxel;
        let max_v = base_voxel + dims - IVec3::ONE;
        (min_v, max_v)
    }

    /// Iterasi seluruh voxel dalam posisi dunia saat ini (menggunakan snapping lattice deterministik)
    pub fn iter_world_voxels(&self) -> impl Iterator<Item = (IVec3, VoxelBlock)> + '_ {
        let base_voxel = self.current_base_voxel();
        self.aggregate
            .voxels
            .iter()
            .map(move |v| (base_voxel + v.relative_coord, v.block))
    }

    /// Menghitung percepatan gravitasi efektif yang bekerja pada badan ini
    #[inline(always)]
    pub fn effective_gravity(&self, world_gravity: Vec3) -> Vec3 {
        world_gravity * self.gravity_scale
    }

    /// Menerapkan percepatan gravitasi terhadap kecepatan linier selama durasi `dt` detik
    pub fn apply_gravity(&mut self, world_gravity: Vec3, dt: f32) {
        if self.state == DynamicBodyState::Active && !self.is_grounded {
            let accel = self.effective_gravity(world_gravity);
            self.velocity += accel * dt;
        }
    }

    /// Mengintegrasikan perpindahan translasi posisi linier: p += v * dt
    pub fn integrate_motion(&mut self, dt: f32) {
        if self.state == DynamicBodyState::Active && !self.is_grounded {
            self.position += self.velocity * dt;
        }
    }

    /// Mengubah status siklus hidup
    pub fn set_state(&mut self, new_state: DynamicBodyState) {
        self.state = new_state;
        if new_state == DynamicBodyState::Active {
            self.ticks_stationary = 0;
        }
    }
}
