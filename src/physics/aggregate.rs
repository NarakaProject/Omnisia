use glam::{IVec3, Mat3, Quat, Vec3};
use std::collections::BTreeSet;
use std::fmt;

use super::body::{DynamicBody, DynamicBodyId, DynamicBodyState};
use super::broadphase::{BroadphaseError, RigidBodyId};
use super::collider::{Collider, ColliderId};
use super::reintegrate::ReintegrationError;
use super::rigid_body::{MassProperties, RigidBody, RigidBodyError};
use super::shape::{BoxShape, Shape, ShapeError};
use super::transform::Transform;
use super::world::PhysicsWorld;

use crate::chunk::dirty_flags;
use crate::coord::{
    world_pos_to_world_voxel, world_voxel_to_chunk_and_local, world_voxel_to_world_pos,
};
use crate::material::MaterialRegistry;
use crate::streaming::store::ChunkStore;
use crate::structure::aggregate::{AggregateVoxel, DetachedAggregate};
use crate::voxel::{VoxelBlock, VOXEL_SIZE};

/// Kesalahan dalam physicalization atau integrasi struktural aggregate ke rigid body.
#[derive(Debug, Clone, PartialEq)]
pub enum AggregatePhysicsError {
    /// Aggregate kosong tidak memiliki voxel solid
    EmptyAggregate,
    /// Koordinat memuat komponen non-finite
    NonFiniteCoordinates,
    /// Massa yang dihitung tidak valid (<= 0.0 atau non-finite)
    InvalidMass,
    /// Tensor inersia yang dihitung tidak valid (asimetris, non-positive-definite, atau singular)
    InvalidInertia,
    /// Kegagalan pembuatan RigidBody
    RigidBodyFailed(RigidBodyError),
    /// Kegagalan pendaftaran broadphase
    BroadphaseFailed(BroadphaseError),
    /// Kegagalan bentuk collider
    ShapeFailed(ShapeError),
    /// Badan dengan ID tersebut sudah terdaftar
    BodyAlreadyExists(RigidBodyId),
    /// Aggregate dinamis dengan ID tersebut sudah terdaftar
    AggregateAlreadyExists(DynamicBodyId),
    /// Aggregate dinamis tidak ditemukan
    AggregateNotFound(DynamicBodyId),
    /// Kegagalan validasi reintegrasi
    ReintegrationFailed(ReintegrationError),
}

impl fmt::Display for AggregatePhysicsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyAggregate => write!(f, "Aggregate tidak memiliki voxel"),
            Self::NonFiniteCoordinates => write!(f, "Koordinat memuat nilai non-finite"),
            Self::InvalidMass => write!(f, "Massa aggregate tidak valid"),
            Self::InvalidInertia => write!(f, "Tensor inersia aggregate tidak valid"),
            Self::RigidBodyFailed(err) => write!(f, "Kegagalan RigidBody: {}", err),
            Self::BroadphaseFailed(err) => write!(f, "Kegagalan Broadphase: {:?}", err),
            Self::ShapeFailed(err) => write!(f, "Kegagalan Shape: {}", err),
            Self::BodyAlreadyExists(id) => write!(f, "RigidBody {:?} sudah terdaftar", id),
            Self::AggregateAlreadyExists(id) => write!(f, "DynamicBody {:?} sudah terdaftar", id),
            Self::AggregateNotFound(id) => write!(f, "DynamicBody {:?} tidak ditemukan", id),
            Self::ReintegrationFailed(err) => write!(f, "Kegagalan Reintegrasi: {}", err),
        }
    }
}

impl std::error::Error for AggregatePhysicsError {}

impl From<RigidBodyError> for AggregatePhysicsError {
    fn from(e: RigidBodyError) -> Self {
        Self::RigidBodyFailed(e)
    }
}

impl From<BroadphaseError> for AggregatePhysicsError {
    fn from(e: BroadphaseError) -> Self {
        Self::BroadphaseFailed(e)
    }
}

impl From<ShapeError> for AggregatePhysicsError {
    fn from(e: ShapeError) -> Self {
        Self::ShapeFailed(e)
    }
}

impl From<ReintegrationError> for AggregatePhysicsError {
    fn from(e: ReintegrationError) -> Self {
        Self::ReintegrationFailed(e)
    }
}

/// Properti massa dan tensor inersia fisik hasil komputasi eksak dari gugusan voxel.
#[derive(Debug, Clone, PartialEq)]
pub struct AggregatePhysicsProperties {
    /// Total massa aggregate dalam kilogram
    pub total_mass: f32,
    /// Pusat massa relatif terhadap sudut minimum lokal aggregate (meter)
    pub center_of_mass_local: Vec3,
    /// Posisi pusat massa awal dalam ruang dunia pada saat pelepasan (meter)
    pub center_of_mass_world: Vec3,
    /// Tensor inersia lokal terhadap pusat massa (kg·m²)
    pub local_inertia: Mat3,
    /// Properti massa yang telah divalidasi penuh untuk RigidBody
    pub mass_properties: MassProperties,
}

/// Menghitung massa, pusat massa, dan tensor inersia paralel eksak untuk gugusan voxel.
///
/// FORMULASI MATEMATIKA FISIK:
/// - Skala Voxel: $s = 0.5$ m, volume $V = s^3 = 0.125$ m³.
/// - Setiap voxel memiliki massa $m_v = \rho_v \times V$ (atau default 1.0 kg jika densitas <= 0).
/// - Total Massa: $M = \sum m_v$.
/// - Pusat Massa Lokal: $\vec{C}_{\text{local}} = \frac{1}{M} \sum m_v \vec{p}_v$.
/// - Tensor Inersia Kubus Tunggal terhadap centroidnya: $I_{\text{cube}} = \frac{1}{24} m_v \mathbf{I}_{3\times3}$.
/// - Parallel Axis Theorem:
///   $\mathbf{I}_{\text{total}} = \sum_v \left[ \frac{1}{24} m_v \mathbf{I}_{3\times3} + m_v (|\vec{r}_v|^2 \mathbf{I}_{3\times3} - \vec{r}_v \vec{r}_v^T) \right]$
///   di mana $\vec{r}_v = \vec{p}_v - \vec{C}_{\text{local}}$ dalam meter.
///
/// SIFAT GARANSI INVARIAN:
/// - Simetri: $I_{ij} = I_{ji}$.
/// - Definit Positif: Nilai eigen terkecil $\lambda_{\min} \ge \frac{1}{24} M > 0$.
/// - Non-singular dan selalu memiliki invers yang terdefinisi secara terhingga.
pub fn calculate_aggregate_mass_properties(
    aggregate: &DetachedAggregate,
    materials: Option<&MaterialRegistry>,
) -> Result<AggregatePhysicsProperties, AggregatePhysicsError> {
    let voxel_count = aggregate.voxel_count();
    if voxel_count == 0 {
        return Err(AggregatePhysicsError::EmptyAggregate);
    }

    let voxel_size = VOXEL_SIZE;
    let voxel_volume = voxel_size * voxel_size * voxel_size; // 0.125 m³

    // 1. Hitung total massa dan pusat massa lokal
    let mut total_mass = 0.0f32;
    let mut weighted_pos_sum = Vec3::ZERO;

    for v in &aggregate.voxels {
        let density = materials
            .and_then(|reg| reg.get(v.block.material()))
            .map(|def| def.density_kg_m3)
            .unwrap_or(0.0);

        let voxel_mass = if density > 0.0 {
            density * voxel_volume
        } else {
            1.0 // Default 1.0 kg per solid voxel jika material tidak memiliki densitas
        };

        if !voxel_mass.is_finite() || voxel_mass <= 0.0 {
            return Err(AggregatePhysicsError::InvalidMass);
        }

        // Posisi pusat voxel dalam kerangka lokal aggregate (meter)
        let voxel_center_local = Vec3::new(
            (v.relative_coord.x as f32 + 0.5) * voxel_size,
            (v.relative_coord.y as f32 + 0.5) * voxel_size,
            (v.relative_coord.z as f32 + 0.5) * voxel_size,
        );

        if !voxel_center_local.is_finite() {
            return Err(AggregatePhysicsError::NonFiniteCoordinates);
        }

        total_mass += voxel_mass;
        weighted_pos_sum += voxel_mass * voxel_center_local;
    }

    if !total_mass.is_finite() || total_mass <= 0.0 {
        return Err(AggregatePhysicsError::InvalidMass);
    }

    let center_of_mass_local = weighted_pos_sum / total_mass;
    if !center_of_mass_local.is_finite() {
        return Err(AggregatePhysicsError::NonFiniteCoordinates);
    }

    // Posisi pusat massa awal di ruang dunia
    let base_world_pos = world_voxel_to_world_pos(aggregate.min_voxel);
    let center_of_mass_world = base_world_pos + center_of_mass_local;

    // 2. Hitung tensor inersia lokal terhadap pusat massa (Parallel Axis Theorem)
    let mut ixx = 0.0f32;
    let mut iyy = 0.0f32;
    let mut izz = 0.0f32;
    let mut ixy = 0.0f32;
    let mut ixz = 0.0f32;
    let mut iyz = 0.0f32;

    for v in &aggregate.voxels {
        let density = materials
            .and_then(|reg| reg.get(v.block.material()))
            .map(|def| def.density_kg_m3)
            .unwrap_or(0.0);

        let voxel_mass = if density > 0.0 {
            density * voxel_volume
        } else {
            1.0
        };

        let voxel_center_local = Vec3::new(
            (v.relative_coord.x as f32 + 0.5) * voxel_size,
            (v.relative_coord.y as f32 + 0.5) * voxel_size,
            (v.relative_coord.z as f32 + 0.5) * voxel_size,
        );

        // Vektor lengan tuas r dari pusat massa aggregate ke centroid voxel
        let r = voxel_center_local - center_of_mass_local;

        // Inersia intrinsik kubus seragam terhadap centroidnya: (1/6) * m * s² = (1/24) * m (karena s=0.5)
        let cube_self_inertia = (1.0 / 24.0) * voxel_mass;

        // Kontribusi sumbu paralel: m * ( |r|² I - r rᵀ )
        ixx += cube_self_inertia + voxel_mass * (r.y * r.y + r.z * r.z);
        iyy += cube_self_inertia + voxel_mass * (r.x * r.x + r.z * r.z);
        izz += cube_self_inertia + voxel_mass * (r.x * r.x + r.y * r.y);

        ixy -= voxel_mass * (r.x * r.y);
        ixz -= voxel_mass * (r.x * r.z);
        iyz -= voxel_mass * (r.y * r.z);
    }

    let local_inertia = Mat3::from_cols(
        Vec3::new(ixx, ixy, ixz),
        Vec3::new(ixy, iyy, iyz),
        Vec3::new(ixz, iyz, izz),
    );

    let mass_properties = MassProperties::new_dynamic(total_mass, local_inertia)
        .map_err(AggregatePhysicsError::RigidBodyFailed)?;

    Ok(AggregatePhysicsProperties {
        total_mass,
        center_of_mass_local,
        center_of_mass_world,
        local_inertia,
        mass_properties,
    })
}

/// Strategi pembentukan collider untuk structural aggregate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AggregateColliderStrategy {
    /// Satu box pembungkus tunggal melingkupi bounding box keseluruhan aggregate
    #[default]
    BoundingBox,
    /// Box gabungan non-overlapping hasil greedy merging pada kisi voxel
    CompoundBoxes,
}

/// Representasi boks integer voxel hasil greedy merging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MergedVoxelBox {
    pub min_coord: IVec3,
    pub max_coord: IVec3,
}

/// Algoritma greedy 3D axis-aligned box merging deterministik untuk mereduksi jumlah collider.
pub fn greedy_merge_voxels(aggregate: &DetachedAggregate) -> Vec<MergedVoxelBox> {
    if aggregate.voxels.is_empty() {
        return Vec::new();
    }

    let mut voxel_set = BTreeSet::new();
    for v in &aggregate.voxels {
        voxel_set.insert((v.relative_coord.x, v.relative_coord.y, v.relative_coord.z));
    }

    let mut merged_boxes = Vec::new();

    while let Some(&(sx, sy, sz)) = voxel_set.iter().next() {
        // 1. Perluas sepanjang sumbu +X
        let mut max_x = sx;
        while voxel_set.contains(&(max_x + 1, sy, sz)) {
            max_x += 1;
        }

        // 2. Perluas sepanjang sumbu +Z untuk seluruh bentang X yang sudah ditemukan
        let mut max_z = sz;
        'expand_z: loop {
            let next_z = max_z + 1;
            for x in sx..=max_x {
                if !voxel_set.contains(&(x, sy, next_z)) {
                    break 'expand_z;
                }
            }
            max_z = next_z;
        }

        // 3. Perluas sepanjang sumbu +Y untuk seluruh pelat X-Z
        let mut max_y = sy;
        'expand_y: loop {
            let next_y = max_y + 1;
            for x in sx..=max_x {
                for z in sz..=max_z {
                    if !voxel_set.contains(&(x, next_y, z)) {
                        break 'expand_y;
                    }
                }
            }
            max_y = next_y;
        }

        // 4. Hapus seluruh voxel yang tercakup dari set
        for x in sx..=max_x {
            for y in sy..=max_y {
                for z in sz..=max_z {
                    voxel_set.remove(&(x, y, z));
                }
            }
        }

        merged_boxes.push(MergedVoxelBox {
            min_coord: IVec3::new(sx, sy, sz),
            max_coord: IVec3::new(max_x, max_y, max_z),
        });
    }

    merged_boxes
}

/// Menghasilkan daftar collider untuk structural aggregate berdasarkan strategi yang dipilih.
///
/// Seluruh collider mengacu pada `rigid_body_id` yang sama (prinsip One Aggregate -> One RigidBody -> N Colliders).
pub fn generate_aggregate_colliders(
    rigid_body_id: RigidBodyId,
    aggregate: &DetachedAggregate,
    center_of_mass_local: Vec3,
    strategy: AggregateColliderStrategy,
    next_collider_id: &mut u64,
) -> Result<Vec<Collider>, AggregatePhysicsError> {
    let mut colliders = Vec::new();

    match strategy {
        AggregateColliderStrategy::BoundingBox => {
            let dims = aggregate.max_voxel - aggregate.min_voxel + IVec3::ONE;
            let extents = Vec3::new(
                dims.x as f32 * VOXEL_SIZE,
                dims.y as f32 * VOXEL_SIZE,
                dims.z as f32 * VOXEL_SIZE,
            );
            let half_extents = extents * 0.5;

            // Pusat geometris bounding box lokal
            let box_center_local = half_extents;
            let local_offset = box_center_local - center_of_mass_local;

            let shape = Shape::Box(BoxShape::new(half_extents)?);
            let transform = Transform::new(local_offset, Quat::IDENTITY)?;

            let id = ColliderId(*next_collider_id);
            *next_collider_id += 1;

            colliders.push(Collider::new(id, rigid_body_id, shape, transform));
        }
        AggregateColliderStrategy::CompoundBoxes => {
            let merged = greedy_merge_voxels(aggregate);
            for m in merged {
                let dims = m.max_coord - m.min_coord + IVec3::ONE;
                let extents = Vec3::new(
                    dims.x as f32 * VOXEL_SIZE,
                    dims.y as f32 * VOXEL_SIZE,
                    dims.z as f32 * VOXEL_SIZE,
                );
                let half_extents = extents * 0.5;

                // Pusat box gabungan dalam kerangka lokal aggregate (meter)
                let box_center_local = Vec3::new(
                    (m.min_coord.x as f32 + m.max_coord.x as f32 + 1.0) * (VOXEL_SIZE * 0.5),
                    (m.min_coord.y as f32 + m.max_coord.y as f32 + 1.0) * (VOXEL_SIZE * 0.5),
                    (m.min_coord.z as f32 + m.max_coord.z as f32 + 1.0) * (VOXEL_SIZE * 0.5),
                );

                let local_offset = box_center_local - center_of_mass_local;
                let shape = Shape::Box(BoxShape::new(half_extents)?);
                let transform = Transform::new(local_offset, Quat::IDENTITY)?;

                let id = ColliderId(*next_collider_id);
                *next_collider_id += 1;

                colliders.push(Collider::new(id, rigid_body_id, shape, transform));
            }
        }
    }

    Ok(colliders)
}

/// Kebijakan snapping rotasi untuk reintegrasi struktural aggregate ke dunia kisi statis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OrientationQuantizationPolicy {
    /// Menyelaraskan orientasi rotasi ke salah satu dari 24 orientasi ortogonal kisi kubus (90 derajat)
    #[default]
    NearestLattice,
    /// Proyeksi langsung posisi kontinu dunia ke kisi integer tanpa pembulatan rotasi
    DirectPosition,
}

/// 24 matriks rotasi ortogonal kubus (grup rotasi simetri kubus O).
static CUBE_LATTICE_ROTATIONS: std::sync::LazyLock<[Mat3; 24]> = std::sync::LazyLock::new(|| {
    let mut mats = Vec::with_capacity(24);
    let axes = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z];

    for &c0 in &axes {
        for &c1 in &axes {
            if c0.dot(c1).abs() < 1e-4 {
                let c2 = c0.cross(c1);
                let m = Mat3::from_cols(c0, c1, c2);
                if (m.determinant() - 1.0).abs() < 1e-4 {
                    mats.push(m);
                }
            }
        }
    }
    mats.try_into()
        .expect("Harus menghasilkan tepat 24 matriks")
});

/// Menyelaraskan quaternion rotasi ke orientasi ortogonal kisi terdekat dari 24 simetri kubus.
pub fn snap_to_nearest_lattice_rotation(rot: Quat) -> Quat {
    let rot_mat = Mat3::from_quat(rot);
    let mut best_trace = f32::NEG_INFINITY;
    let mut best_mat = Mat3::IDENTITY;

    for &cand in CUBE_LATTICE_ROTATIONS.iter() {
        // Tr(M_cand^T * M_rot) = 1 + 2*cos(theta), semakin besar semakin kecil sudut perbedaannya
        let m = cand.transpose() * rot_mat;
        let trace = m.x_axis.x + m.y_axis.y + m.z_axis.z;
        if trace > best_trace {
            best_trace = trace;
            best_mat = cand;
        }
    }

    Quat::from_mat3(&best_mat).normalize()
}

/// Rekaman otoritatif runtime untuk structural aggregate yang sedang berada dalam simulasi fisik dinamis.
#[derive(Debug, Clone)]
pub struct DynamicAggregateRecord {
    pub dynamic_body_id: DynamicBodyId,
    /// Identitas struktural persisten yang bertahan melintasi siklus detach -> fisik -> reintegrasi
    pub aggregate_id: u64,
    /// ID representasi fisik kaku otoritatif di PhysicsWorld
    pub rigid_body_id: RigidBodyId,
    /// Payload data voxel dan topologi lokal eksklusif
    pub aggregate: DetachedAggregate,
    /// Titik pusat massa lokal terhadap sudut minimum aggregate (meter)
    pub center_of_mass_local: Vec3,
    /// Daftar ID collider yang terdaftar di PhysicsWorld untuk aggregate ini
    pub collider_ids: Vec<ColliderId>,
    /// Strategi pembentukan collider yang digunakan
    pub collider_strategy: AggregateColliderStrategy,
}

impl DynamicAggregateRecord {
    /// Mengambil posisi pusat massa dunia otoritatif saat ini dari PhysicsWorld
    #[inline(always)]
    pub fn current_world_com(&self, world: &PhysicsWorld) -> Option<Vec3> {
        world
            .get_rigid_body(self.rigid_body_id)
            .map(|b| b.position())
    }

    /// Mengambil orientasi rotasi dunia otoritatif saat ini dari PhysicsWorld
    #[inline(always)]
    pub fn current_rotation(&self, world: &PhysicsWorld) -> Option<Quat> {
        world
            .get_rigid_body(self.rigid_body_id)
            .map(|b| b.rotation())
    }

    /// Mengambil kecepatan linier dunia saat ini dari PhysicsWorld
    #[inline(always)]
    pub fn current_linear_velocity(&self, world: &PhysicsWorld) -> Option<Vec3> {
        world
            .get_rigid_body(self.rigid_body_id)
            .map(|b| b.linear_velocity())
    }

    /// Mengambil kecepatan sudut dunia saat ini dari PhysicsWorld
    #[inline(always)]
    pub fn current_angular_velocity(&self, world: &PhysicsWorld) -> Option<Vec3> {
        world
            .get_rigid_body(self.rigid_body_id)
            .map(|b| b.angular_velocity())
    }

    /// Memeriksa apakah aggregate saat ini sedang tidur (Sleeping) di PhysicsWorld
    #[inline(always)]
    pub fn is_sleeping(&self, world: &PhysicsWorld) -> bool {
        world
            .get_rigid_body(self.rigid_body_id)
            .map(|b| b.is_sleeping())
            .unwrap_or(false)
    }

    /// Menghitung posisi continuous dunia dari suatu voxel tertentu saat ini
    pub fn current_world_voxel_center(
        &self,
        voxel: &AggregateVoxel,
        world: &PhysicsWorld,
    ) -> Option<Vec3> {
        let body = world.get_rigid_body(self.rigid_body_id)?;
        let rot = body.rotation();
        let com = body.position();

        let local_center = Vec3::new(
            (voxel.relative_coord.x as f32 + 0.5) * VOXEL_SIZE,
            (voxel.relative_coord.y as f32 + 0.5) * VOXEL_SIZE,
            (voxel.relative_coord.z as f32 + 0.5) * VOXEL_SIZE,
        );
        let r_local = local_center - self.center_of_mass_local;
        Some(com + rot.mul_vec3(r_local))
    }

    /// Menghasilkan snapshot `DynamicBody` yang tersinkronisasi dari state fisik otoritatif `RigidBody`
    pub fn to_dynamic_body(&self, world: &PhysicsWorld) -> Option<DynamicBody> {
        let body = world.get_rigid_body(self.rigid_body_id)?;
        let rot = body.rotation();
        let com = body.position();

        // Posisi sudut minimum lokal dunia: p = CoM - R * C_local
        let min_corner_pos = com - rot.mul_vec3(self.center_of_mass_local);

        let mut dyn_body =
            DynamicBody::new(self.dynamic_body_id, self.aggregate.clone(), min_corner_pos)
                .with_velocity(body.linear_velocity())
                .with_rigid_body_id(self.rigid_body_id);

        if body.is_sleeping() {
            dyn_body.state = DynamicBodyState::Sleeping;
        } else {
            dyn_body.state = DynamicBodyState::Active;
        }

        Some(dyn_body)
    }
}

/// Rencana mutasi reintegrasi aggregate ke dunia statis ChunkStore.
#[derive(Debug, Clone)]
pub struct AggregateReintegrationPlan {
    pub dynamic_body_id: DynamicBodyId,
    pub aggregate_id: u64,
    pub rigid_body_id: RigidBodyId,
    pub voxels: Vec<(IVec3, VoxelBlock)>,
    pub affected_chunks: Vec<IVec3>,
}

/// Fase 1: Validasi dan persiapan rencana reintegrasi struktural ke ChunkStore.
///
/// INVARIAN TRANSAKSIONAL:
/// 1. Evaluasi posisi target dunia untuk setiap voxel menggunakan kebijakan rotasi yang dipilih.
/// 2. Verifikasi pemetaan injektif: tidak ada 2 voxel aggregate yang memproyeksikan ke koordinat dunia yang sama.
/// 3. Validasi seluruh chunk tujuan telah resident di ChunkStore.
/// 4. Validasi seluruh koordinat tujuan di ChunkStore adalah AIR (tidak ada overwrite voxel solid statis).
pub fn prepare_aggregate_reintegration(
    record: &DynamicAggregateRecord,
    rigid_body: &RigidBody,
    store: &ChunkStore,
    policy: OrientationQuantizationPolicy,
) -> Result<AggregateReintegrationPlan, ReintegrationError> {
    let com = rigid_body.position();
    let rot = match policy {
        OrientationQuantizationPolicy::NearestLattice => {
            snap_to_nearest_lattice_rotation(rigid_body.rotation())
        }
        OrientationQuantizationPolicy::DirectPosition => rigid_body.rotation(),
    };

    if !com.is_finite() || !rot.is_finite() {
        return Err(ReintegrationError::NonFiniteTransform);
    }

    let mut planned_voxels = Vec::with_capacity(record.aggregate.voxel_count());
    let mut affected_chunks = Vec::new();
    let mut occupied_targets = BTreeSet::new();

    for v in &record.aggregate.voxels {
        let local_center = Vec3::new(
            (v.relative_coord.x as f32 + 0.5) * VOXEL_SIZE,
            (v.relative_coord.y as f32 + 0.5) * VOXEL_SIZE,
            (v.relative_coord.z as f32 + 0.5) * VOXEL_SIZE,
        );
        let r_local = local_center - record.center_of_mass_local;
        let world_center = com + rot.mul_vec3(r_local);
        let target_voxel = world_pos_to_world_voxel(world_center);

        // Validasi 1: Injective mapping (tidak boleh ada overlap antar-voxel aggregate sendiri)
        if !occupied_targets.insert((target_voxel.x, target_voxel.y, target_voxel.z)) {
            return Err(ReintegrationError::SelfOverlap(target_voxel));
        }

        let (chunk_coord, _) = world_voxel_to_chunk_and_local(target_voxel);

        // Validasi 2: Chunk harus resident
        if !store.is_chunk_resident(&chunk_coord) {
            return Err(ReintegrationError::ChunkNotResident(chunk_coord));
        }

        // Validasi 3: Lokasi tujuan di ChunkStore harus AIR
        match store.get_voxel_world_checked(target_voxel) {
            Some(existing) => {
                if !existing.is_air() {
                    return Err(ReintegrationError::DestinationOccupied {
                        pos: target_voxel,
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

        planned_voxels.push((target_voxel, v.block));
    }

    Ok(AggregateReintegrationPlan {
        dynamic_body_id: record.dynamic_body_id,
        aggregate_id: record.aggregate_id,
        rigid_body_id: record.rigid_body_id,
        voxels: planned_voxels,
        affected_chunks,
    })
}

/// Fase 2: Eksekusi atomik penulisan voxel ke ChunkStore dan pembaruan dirty flags.
pub fn commit_aggregate_reintegration(plan: &AggregateReintegrationPlan, store: &mut ChunkStore) {
    for &(pos, block) in &plan.voxels {
        store.set_voxel_world(pos, block);

        let (chunk_coord, local) = world_voxel_to_chunk_and_local(pos);
        if local.x == 0 {
            store.mark_dirty(
                &(chunk_coord + IVec3::new(-1, 0, 0)),
                dirty_flags::MESH_DIRTY,
            );
        } else if local.x == 31 {
            store.mark_dirty(
                &(chunk_coord + IVec3::new(1, 0, 0)),
                dirty_flags::MESH_DIRTY,
            );
        }
        if local.y == 0 {
            store.mark_dirty(
                &(chunk_coord + IVec3::new(0, -1, 0)),
                dirty_flags::MESH_DIRTY,
            );
        } else if local.y == 31 {
            store.mark_dirty(
                &(chunk_coord + IVec3::new(0, 1, 0)),
                dirty_flags::MESH_DIRTY,
            );
        }
        if local.z == 0 {
            store.mark_dirty(
                &(chunk_coord + IVec3::new(0, 0, -1)),
                dirty_flags::MESH_DIRTY,
            );
        } else if local.z == 31 {
            store.mark_dirty(
                &(chunk_coord + IVec3::new(0, 0, 1)),
                dirty_flags::MESH_DIRTY,
            );
        }
    }

    for &chunk_coord in &plan.affected_chunks {
        store.mark_dirty(
            &chunk_coord,
            dirty_flags::VOXEL_DIRTY | dirty_flags::MESH_DIRTY | dirty_flags::SAVE_DIRTY,
        );
    }
}

/// Laporan audit integritas kepemilikan voxel antara ChunkStore dan PhysicsWorld (Phase 9.11).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AggregateOwnershipReport {
    pub total_static_voxels: usize,
    pub total_dynamic_voxels: usize,
    pub total_world_voxels: usize,
    pub dynamic_aggregate_count: usize,
    pub duplicate_detections: usize,
    pub inconsistent_records: usize,
}

/// Mengaudit konsistensi kepemilikan voxel dunia lintas sistem ChunkStore dan PhysicsWorld.
///
/// INVARIAN: `duplicate_detections == 0` dan `inconsistent_records == 0`.
pub fn audit_aggregate_ownership(
    store: &ChunkStore,
    world: &PhysicsWorld,
) -> AggregateOwnershipReport {
    let mut total_static = 0;
    for chunk in store.resident_chunks() {
        total_static += chunk.non_air_count as usize;
    }

    let mut total_dynamic = 0;
    let mut duplicate_detections = 0;
    let mut inconsistent_records = 0;
    let mut occupied_coords = BTreeSet::new();

    for record in world.dynamic_aggregates.values() {
        let body = match world.get_rigid_body(record.rigid_body_id) {
            Some(b) => b,
            None => {
                inconsistent_records += 1;
                continue;
            }
        };

        // Verifikasi seluruh collider miliknya terdaftar
        for &col_id in &record.collider_ids {
            if !world.colliders.contains_key(&col_id) {
                inconsistent_records += 1;
            }
        }

        let rot = snap_to_nearest_lattice_rotation(body.rotation());
        let com = body.position();

        for v in &record.aggregate.voxels {
            total_dynamic += 1;

            let local_center = Vec3::new(
                (v.relative_coord.x as f32 + 0.5) * VOXEL_SIZE,
                (v.relative_coord.y as f32 + 0.5) * VOXEL_SIZE,
                (v.relative_coord.z as f32 + 0.5) * VOXEL_SIZE,
            );
            let r_local = local_center - record.center_of_mass_local;
            let world_center = com + rot.mul_vec3(r_local);
            let coord = world_pos_to_world_voxel(world_center);

            // 1. Periksa apakah ChunkStore statis secara ilegal juga memegang voxel di koordinat ini
            if !store.get_voxel_world(coord).is_air() {
                duplicate_detections += 1;
            }

            // 2. Periksa apakah ada benturan duplikasi antar aggregate dinamis
            if !occupied_coords.insert((coord.x, coord.y, coord.z)) {
                duplicate_detections += 1;
            }
        }
    }

    AggregateOwnershipReport {
        total_static_voxels: total_static,
        total_dynamic_voxels: total_dynamic,
        total_world_voxels: total_static + total_dynamic,
        dynamic_aggregate_count: world.dynamic_aggregates.len(),
        duplicate_detections,
        inconsistent_records,
    }
}
