use glam::{Quat, Vec3};
use std::collections::BTreeMap;

use super::aabb::Aabb;
use super::broadphase::{
    BodyType, BroadphaseError, BroadphasePair, BroadphaseProxy, RigidBodyId, SpatialHashBroadphase,
};
use super::rigid_body::{MassProperties, RigidBody};
use crate::coord::world_pos_to_world_voxel;
use crate::streaming::store::ChunkStore;
use crate::voxel::VOXEL_SIZE;

/// Konfigurasi global untuk dunia fisika (PhysicsWorld).
#[derive(Debug, Clone)]
pub struct PhysicsWorldConfig {
    /// Durasi satu fixed timestep fisika (detik). Default 1/30s (30 Hz).
    pub fixed_dt: f32,
    /// Vektor gravitasi dunia default (m/s^2). Default (0, -9.81, 0).
    pub world_gravity: Vec3,
    /// Ukuran sel spatial hash broadphase (meter). Default 4.0m.
    pub broadphase_cell_size: f32,
}

impl Default for PhysicsWorldConfig {
    fn default() -> Self {
        Self {
            fixed_dt: 1.0 / 30.0,
            world_gravity: Vec3::new(0.0, -9.81, 0.0),
            broadphase_cell_size: 4.0,
        }
    }
}

/// Abstraksi inti dunia fisika untuk Fase 9 (Rigid Body Physics).
///
/// TANGGUNG JAWAB FASE 9.2:
/// - Otoritas tunggal registri badan kaku (`rigid_bodies: BTreeMap<RigidBodyId, RigidBody>`).
/// - Pendaftaran, pelacakan, dan penghapusan badan kaku (RigidBodyId).
/// - Pembaruan bounding box dunia (Aabb) ke broadphase.
/// - Pengindeksan spasial melalui SpatialHashBroadphase.
/// - Kueri tumpang tindih AABB deterministik.
/// - Generasi pasangan kandidat tabrakan kanonikal.
///
/// INVARIAN ARSITEKTURAL:
/// - PhysicsWorld BUKAN pemilik data voxel dunia (voxel dimiliki ChunkStore dan DynamicBody).
/// - Tidak ada dependensi ke modul render (`wgpu`, mesh, dsb.).
/// - Broadphase tidak pernah memindai voxel di dalam chunk atau badan dinamis.
pub struct PhysicsWorld {
    pub config: PhysicsWorldConfig,
    pub broadphase: SpatialHashBroadphase,
    pub next_body_id: u64,
    pub rigid_bodies: BTreeMap<RigidBodyId, RigidBody>,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new(PhysicsWorldConfig::default())
    }
}

impl PhysicsWorld {
    /// Membuat instance PhysicsWorld baru dengan konfigurasi yang ditentukan.
    pub fn new(config: PhysicsWorldConfig) -> Self {
        let broadphase = SpatialHashBroadphase::new(config.broadphase_cell_size);
        Self {
            config,
            broadphase,
            next_body_id: 1,
            rigid_bodies: BTreeMap::new(),
        }
    }

    /// Mendaftarkan `RigidBody` ke dalam dunia fisika dengan opsi proksi AABB broadphase.
    ///
    /// Menolak duplikasi ID jika badan dengan ID tersebut sudah terdaftar.
    pub fn add_rigid_body(
        &mut self,
        body: RigidBody,
        aabb: Option<Aabb>,
    ) -> Result<RigidBodyId, BroadphaseError> {
        let id = body.id();
        if self.rigid_bodies.contains_key(&id) {
            return Err(BroadphaseError::BodyAlreadyExists(id));
        }

        if let Some(box_bounds) = aabb {
            let proxy = BroadphaseProxy::new(id, body.body_type(), box_bounds);
            self.broadphase.insert(proxy)?;
        }

        if id.0 >= self.next_body_id {
            self.next_body_id = id.0 + 1;
        }

        self.rigid_bodies.insert(id, body);
        Ok(id)
    }

    /// Mengambil referensi tidak dapat diubah ke `RigidBody` berdasarkan ID.
    #[inline(always)]
    pub fn get_rigid_body(&self, id: RigidBodyId) -> Option<&RigidBody> {
        self.rigid_bodies.get(&id)
    }

    /// Mengambil referensi mutabel ke `RigidBody` berdasarkan ID.
    #[inline(always)]
    pub fn get_rigid_body_mut(&mut self, id: RigidBodyId) -> Option<&mut RigidBody> {
        self.rigid_bodies.get_mut(&id)
    }

    /// Menghapus badan dari registri fisik dan broadphase secara atomik.
    pub fn remove_rigid_body(&mut self, id: RigidBodyId) -> Option<RigidBody> {
        let body = self.rigid_bodies.remove(&id)?;
        self.broadphase.remove(id);
        Some(body)
    }

    /// Lapisan kompatibilitas Phase 9.1: mendaftarkan badan fisika baru dengan ID otomatis.
    /// Membangun representasi `RigidBody` minimal sesuai kategori badan dan memasukkannya
    /// ke otoritas tunggal `self.rigid_bodies`.
    pub fn register_body(
        &mut self,
        body_type: BodyType,
        aabb: Aabb,
    ) -> Result<RigidBodyId, BroadphaseError> {
        let id = RigidBodyId(self.next_body_id);
        self.next_body_id += 1;

        if !aabb.is_valid() {
            return Err(BroadphaseError::InvalidAabb(
                super::aabb::AabbError::NonFiniteCoordinates,
            ));
        }

        let center = aabb.center();
        let body = match body_type {
            BodyType::Dynamic => {
                let mass_props =
                    MassProperties::from_box(1.0, aabb.extents().max(Vec3::splat(0.1)))
                        .unwrap_or_else(|_| MassProperties::from_diagonal(1.0, Vec3::ONE).unwrap());
                RigidBody::new(
                    id,
                    BodyType::Dynamic,
                    center,
                    Quat::IDENTITY,
                    Vec3::ZERO,
                    Vec3::ZERO,
                    mass_props,
                )
                .expect("Valid default dynamic body")
            }
            BodyType::Static => RigidBody::new_static(id, center, Quat::IDENTITY)
                .expect("Valid default static body"),
            BodyType::Kinematic => {
                RigidBody::new_kinematic(id, center, Quat::IDENTITY, Vec3::ZERO, Vec3::ZERO)
                    .expect("Valid default kinematic body")
            }
        };

        let proxy = BroadphaseProxy::new(id, body_type, aabb);
        self.broadphase.insert(proxy)?;
        self.rigid_bodies.insert(id, body);

        Ok(id)
    }

    /// Lapisan kompatibilitas Phase 9.1: mendaftarkan badan fisika dengan ID eksplisit tertentu.
    pub fn register_body_with_id(
        &mut self,
        id: RigidBodyId,
        body_type: BodyType,
        aabb: Aabb,
    ) -> Result<(), BroadphaseError> {
        if self.rigid_bodies.contains_key(&id) {
            return Err(BroadphaseError::BodyAlreadyExists(id));
        }

        if !aabb.is_valid() {
            return Err(BroadphaseError::InvalidAabb(
                super::aabb::AabbError::NonFiniteCoordinates,
            ));
        }

        let center = aabb.center();
        let body = match body_type {
            BodyType::Dynamic => {
                let mass_props =
                    MassProperties::from_box(1.0, aabb.extents().max(Vec3::splat(0.1)))
                        .unwrap_or_else(|_| MassProperties::from_diagonal(1.0, Vec3::ONE).unwrap());
                RigidBody::new(
                    id,
                    BodyType::Dynamic,
                    center,
                    Quat::IDENTITY,
                    Vec3::ZERO,
                    Vec3::ZERO,
                    mass_props,
                )
                .expect("Valid default dynamic body")
            }
            BodyType::Static => RigidBody::new_static(id, center, Quat::IDENTITY)
                .expect("Valid default static body"),
            BodyType::Kinematic => {
                RigidBody::new_kinematic(id, center, Quat::IDENTITY, Vec3::ZERO, Vec3::ZERO)
                    .expect("Valid default kinematic body")
            }
        };

        let proxy = BroadphaseProxy::new(id, body_type, aabb);
        self.broadphase.insert(proxy)?;
        self.rigid_bodies.insert(id, body);

        if id.0 >= self.next_body_id {
            self.next_body_id = id.0 + 1;
        }

        Ok(())
    }

    /// Lapisan kompatibilitas Phase 9.1: menghapus badan dari registri fisika dan broadphase.
    pub fn unregister_body(&mut self, id: RigidBodyId) -> bool {
        self.remove_rigid_body(id).is_some()
    }

    /// Memperbarui AABB dunia dari suatu badan terdaftar.
    pub fn update_body_aabb(
        &mut self,
        id: RigidBodyId,
        new_aabb: Aabb,
    ) -> Result<(), BroadphaseError> {
        self.broadphase.update(id, new_aabb)
    }

    /// Mengambil AABB dunia dari suatu badan terdaftar.
    #[inline(always)]
    pub fn get_body_aabb(&self, id: RigidBodyId) -> Option<&Aabb> {
        self.broadphase.get_proxy(id).map(|p| &p.aabb)
    }

    /// Mengambil kategori partisipasi fisik dari suatu badan terdaftar.
    #[inline(always)]
    pub fn get_body_type(&self, id: RigidBodyId) -> Option<BodyType> {
        self.rigid_bodies.get(&id).map(|b| b.body_type())
    }

    /// Memeriksa apakah badan terdaftar dalam dunia fisika.
    #[inline(always)]
    pub fn contains_body(&self, id: RigidBodyId) -> bool {
        self.rigid_bodies.contains_key(&id)
    }

    /// Jumlah total badan yang terdaftar.
    #[inline(always)]
    pub fn body_count(&self) -> usize {
        self.rigid_bodies.len()
    }

    /// Kueri AABB spasial: menemukan seluruh badan yang AABB-nya bertumpukan dengan `aabb`.
    #[inline(always)]
    pub fn query_aabb(&self, aabb: &Aabb) -> Vec<RigidBodyId> {
        self.broadphase.query_aabb(aabb)
    }

    /// Menghasilkan seluruh pasangan kandidat tabrakan kanonikal (`body_a < body_b`).
    #[inline(always)]
    pub fn generate_candidate_pairs(&self) -> Vec<BroadphasePair> {
        self.broadphase.generate_candidate_pairs()
    }

    /// Mengosongkan seluruh badan dari dunia fisika dan broadphase.
    pub fn clear(&mut self) {
        self.rigid_bodies.clear();
        self.broadphase.clear();
    }
}

/// Trait antarmuka kueri tabrakan medan statis (ChunkStore).
///
/// MEMISAHKAN KEPEMILIKAN FISIKA DAN MEDAN STATIS:
/// Fisika tidak pernah menduplikasi seluruh dunia voxel statis ke dalam broadphase.
/// Fisika hanya meminta voxel solid yang berada secara lokal dalam jangkauan AABB kueri.
pub trait StaticTerrainQuery {
    /// Menemukan seluruh voxel solid statis yang berada dalam batas AABB kueri.
    /// Hasil AABB voxel solid dunia dimasukkan ke dalam `results`.
    fn query_static_voxels(&self, aabb: &Aabb, results: &mut Vec<Aabb>);
}

impl StaticTerrainQuery for ChunkStore {
    fn query_static_voxels(&self, aabb: &Aabb, results: &mut Vec<Aabb>) {
        if !aabb.is_valid() {
            return;
        }

        let min_v = world_pos_to_world_voxel(aabb.min);
        let max_v = world_pos_to_world_voxel(aabb.max);

        for vy in min_v.y..=max_v.y {
            for vz in min_v.z..=max_v.z {
                for vx in min_v.x..=max_v.x {
                    let coord = glam::IVec3::new(vx, vy, vz);
                    if let Some(block) = self.get_voxel_world_checked(coord) {
                        if !block.is_air() {
                            let block_min = Vec3::new(
                                vx as f32 * VOXEL_SIZE,
                                vy as f32 * VOXEL_SIZE,
                                vz as f32 * VOXEL_SIZE,
                            );
                            let block_max = block_min + Vec3::splat(VOXEL_SIZE);
                            if let Ok(voxel_aabb) = Aabb::try_new(block_min, block_max) {
                                if voxel_aabb.overlaps(aabb) {
                                    results.push(voxel_aabb);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
