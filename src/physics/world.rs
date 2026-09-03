use glam::{Quat, Vec3};
use std::collections::{BTreeMap, BTreeSet};

use super::aabb::Aabb;
use super::broadphase::{
    BodyType, BroadphaseError, BroadphasePair, BroadphaseProxy, RigidBodyId, SpatialHashBroadphase,
};
use super::collider::{Collider, ColliderId};
use super::contact::Contact;
use super::integration::IntegrationError;
use super::island::{
    build_islands, IslandState, PhysicsIsland, PhysicsIslandId, SleepConfig, SleepError,
};
use super::narrowphase::{collide, NarrowphaseError};
use super::rigid_body::{MassProperties, RigidBody};
use super::shape::Shape;
use super::solver::{solve_contacts as solve_contacts_fn, SolverConfig, SolverError};
use super::transform::Transform;
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
    /// Konfigurasi solver sequential impulse (Phase 9.5).
    pub solver_config: SolverConfig,
    /// Konfigurasi ambang batas dan durasi sleeping (Phase 9.8).
    pub sleep_config: SleepConfig,
}

impl Default for PhysicsWorldConfig {
    fn default() -> Self {
        Self {
            fixed_dt: 1.0 / 30.0,
            world_gravity: Vec3::new(0.0, -9.81, 0.0),
            broadphase_cell_size: 4.0,
            solver_config: SolverConfig::default(),
            sleep_config: SleepConfig::default(),
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
    pub next_collider_id: u64,
    pub rigid_bodies: BTreeMap<RigidBodyId, RigidBody>,
    pub colliders: BTreeMap<ColliderId, Collider>,
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
            next_collider_id: 1,
            rigid_bodies: BTreeMap::new(),
            colliders: BTreeMap::new(),
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

    /// Menghapus badan dari registri fisik, menghapus seluruh collider miliknya, dan membersihkan broadphase.
    pub fn remove_rigid_body(&mut self, id: RigidBodyId) -> Option<RigidBody> {
        let body = self.rigid_bodies.remove(&id)?;
        self.colliders.retain(|_, c| c.rigid_body_id() != id);
        self.broadphase.remove(id);
        Some(body)
    }

    /// Mendaftarkan `Collider` ke badan kaku dan menyinkronkan AABB dunia turunannya ke broadphase.
    ///
    /// INVARIAN TRANSAKSIONAL:
    /// - Memvalidasi keberadaan `RigidBody` pemilik terlebih dahulu.
    /// - Memvalidasi ketiadaan duplikasi `ColliderId`.
    /// - Menghitung AABB dunia turunan dan memperbarui/memasukkan proksi ke broadphase.
    /// - Hanya jika seluruh langkah di atas berhasil, `Collider` disimpan di registri internal.
    pub fn add_collider(&mut self, collider: Collider) -> Result<ColliderId, BroadphaseError> {
        let body_id = collider.rigid_body_id();
        let body = self
            .rigid_bodies
            .get(&body_id)
            .ok_or(BroadphaseError::BodyNotFound(body_id))?;

        if self.colliders.contains_key(&collider.id()) {
            return Err(BroadphaseError::ColliderAlreadyExists(collider.id()));
        }

        // Hitung AABB dunia dari collider berdasarkan transform RigidBody pemiliknya
        let world_aabb = collider.compute_world_aabb(&body.transform())?;

        // Sinkronisasi broadphase: jika body sudah memiliki proksi AABB, gabungkan (union)
        let unioned_aabb = match self.broadphase.get_proxy(body_id) {
            Some(proxy) => proxy.aabb.union(&world_aabb),
            None => world_aabb,
        };

        if self.broadphase.get_proxy(body_id).is_some() {
            self.broadphase.update(body_id, unioned_aabb)?;
        } else {
            let proxy = BroadphaseProxy::new(body_id, body.body_type(), unioned_aabb);
            self.broadphase.insert(proxy)?;
        }

        // Transaksional: hanya simpan collider jika validasi dan sinkronisasi broadphase berhasil
        let collider_id = collider.id();
        if collider_id.0 >= self.next_collider_id {
            self.next_collider_id = collider_id.0 + 1;
        }
        self.colliders.insert(collider_id, collider);

        Ok(collider_id)
    }

    /// Helper untuk membuat dan menambahkan collider baru dengan ID otomatis.
    pub fn create_collider(
        &mut self,
        rigid_body_id: RigidBodyId,
        shape: Shape,
        local_transform: Transform,
    ) -> Result<ColliderId, BroadphaseError> {
        let id = ColliderId(self.next_collider_id);
        let collider = Collider::new(id, rigid_body_id, shape, local_transform);
        self.add_collider(collider)
    }

    /// Mengambil referensi tidak dapat diubah ke Collider berdasarkan ID.
    #[inline(always)]
    pub fn get_collider(&self, id: ColliderId) -> Option<&Collider> {
        self.colliders.get(&id)
    }

    /// Mengambil referensi mutabel ke Collider berdasarkan ID.
    #[inline(always)]
    pub fn get_collider_mut(&mut self, id: ColliderId) -> Option<&mut Collider> {
        self.colliders.get_mut(&id)
    }

    /// Menghapus collider dan menghitung ulang AABB broadphase dari collider-collider yang tersisa.
    pub fn remove_collider(&mut self, id: ColliderId) -> Option<Collider> {
        let collider = self.colliders.remove(&id)?;
        let body_id = collider.rigid_body_id();

        // Hitung ulang AABB gabungan untuk badan ini dari collider yang tersisa
        let mut combined_aabb: Option<Aabb> = None;
        if let Some(body) = self.rigid_bodies.get(&body_id) {
            let body_transform = body.transform();
            for c in self
                .colliders
                .values()
                .filter(|c| c.rigid_body_id() == body_id)
            {
                if let Ok(aabb) = c.compute_world_aabb(&body_transform) {
                    combined_aabb = Some(match combined_aabb {
                        Some(prev) => prev.union(&aabb),
                        None => aabb,
                    });
                }
            }
        }

        if let Some(new_aabb) = combined_aabb {
            let _ = self.broadphase.update(body_id, new_aabb);
        } else {
            // Jika tidak ada collider tersisa untuk badan ini, bersihkan proksi broadphase
            self.broadphase.remove(body_id);
        }

        Some(collider)
    }

    /// Mengambil iterator untuk seluruh collider yang terpasang pada suatu badan kaku tertentu.
    #[inline(always)]
    pub fn colliders_for_body(&self, body_id: RigidBodyId) -> impl Iterator<Item = &Collider> {
        self.colliders
            .values()
            .filter(move |c| c.rigid_body_id() == body_id)
    }

    /// Jumlah total collider yang terdaftar.
    #[inline(always)]
    pub fn collider_count(&self) -> usize {
        self.colliders.len()
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

    /// Menghasilkan kontak geometris narrowphase untuk seluruh pasangan kandidat broadphase.
    ///
    /// JEMBATAN MULTI-COLLIDER TRANSAKSIONAL:
    /// Broadphase Phase 9.1 tetap berindeks `RigidBodyId`.
    /// Untuk setiap pasangan badan kandidat `(body_a, body_b)` dari broadphase:
    /// - Mengambil seluruh collider milik `body_a` dan `body_b` dari registri otoritatif.
    /// - Menghitung transform dunia masing-masing collider: $T_{\text{world}} = T_{\text{body}} \times T_{\text{local}}$.
    /// - Mengevaluasi `narrowphase::collide(...)` untuk setiap kombinasi produk Cartesian `Collider_A × Collider_B`.
    /// - Mengumpulkan kontak geometris yang valid dengan urutan deterministik.
    pub fn generate_contacts(&self) -> Result<Vec<Contact>, NarrowphaseError> {
        let pairs = self.broadphase.generate_candidate_pairs();
        let mut contacts = Vec::new();

        for pair in pairs {
            let body_a = match self.rigid_bodies.get(&pair.body_a) {
                Some(b) => b,
                None => continue,
            };
            let body_b = match self.rigid_bodies.get(&pair.body_b) {
                Some(b) => b,
                None => continue,
            };

            let transform_body_a = body_a.transform();
            let transform_body_b = body_b.transform();

            for (_, collider_a) in self
                .colliders
                .iter()
                .filter(|(_, c)| c.rigid_body_id() == pair.body_a)
            {
                let transform_world_a =
                    transform_body_a.mul_transform(collider_a.local_transform());

                for (_, collider_b) in self
                    .colliders
                    .iter()
                    .filter(|(_, c)| c.rigid_body_id() == pair.body_b)
                {
                    let transform_world_b =
                        transform_body_b.mul_transform(collider_b.local_transform());

                    if let Some(contact) = collide(
                        collider_a,
                        &transform_world_a,
                        collider_b,
                        &transform_world_b,
                    )? {
                        contacts.push(contact);
                    }
                }
            }
        }

        Ok(contacts)
    }

    /// Menyelesaikan batasan kontak menggunakan Sequential Impulse Contact Solver (Phase 9.5).
    ///
    /// INVARIAN ARSITEKTURAL:
    /// - Hanya memperbarui `linear_velocity` dan `angular_velocity` badan kaku Dinamis.
    /// - **TIDAK PERNAH** memutasi `position` atau `rotation` (integrasi posisi/rotasi ditunda ke Phase 9.6).
    /// - Menggunakan `fixed_dt` dan `solver_config` dari konfigurasi dunia.
    pub fn solve_contacts(&mut self, contacts: &[Contact]) -> Result<(), SolverError> {
        let dt = self.config.fixed_dt;
        let solver_config = self.config.solver_config;
        solve_contacts_fn(&mut self.rigid_bodies, contacts, dt, &solver_config)
    }

    /// Menghitung ulang AABB dunia seluruh collider milik `body_id` dan menyinkronkan proksi broadphase.
    pub fn sync_body_broadphase(&mut self, body_id: RigidBodyId) -> Result<(), BroadphaseError> {
        let body = match self.rigid_bodies.get(&body_id) {
            Some(b) => b,
            None => {
                self.broadphase.remove(body_id);
                return Ok(());
            }
        };

        let body_transform = body.transform();
        let mut combined_aabb: Option<Aabb> = None;
        for c in self
            .colliders
            .values()
            .filter(|c| c.rigid_body_id() == body_id)
        {
            let aabb = c.compute_world_aabb(&body_transform)?;
            combined_aabb = Some(match combined_aabb {
                Some(prev) => prev.union(&aabb),
                None => aabb,
            });
        }

        if let Some(new_aabb) = combined_aabb {
            if self.broadphase.get_proxy(body_id).is_some() {
                self.broadphase.update(body_id, new_aabb)?;
            } else {
                let proxy = BroadphaseProxy::new(body_id, body.body_type(), new_aabb);
                self.broadphase.insert(proxy)?;
            }
        } else {
            self.broadphase.remove(body_id);
        }

        Ok(())
    }

    /// Mengintegrasikan kecepatan linier badan kaku dinamis dari gravitasi dunia (Phase 9.6).
    pub fn integrate_velocities(&mut self) -> Result<(), IntegrationError> {
        let dt = self.config.fixed_dt;
        let gravity = self.config.world_gravity;
        super::integration::integrate_velocities(&mut self.rigid_bodies, dt, gravity)
    }

    /// Mengintegrasikan posisi dan rotasi seluruh badan kaku serta menyinkronkan broadphase (Phase 9.6 & 9.8).
    /// Hanya menyinkronkan broadphase untuk badan non-static yang tidak sedang tidur (non-sleeping).
    pub fn integrate_transforms(&mut self) -> Result<(), IntegrationError> {
        let dt = self.config.fixed_dt;
        super::integration::integrate_transforms(&mut self.rigid_bodies, dt)?;

        // Sinkronisasi proksi broadphase untuk badan yang bergerak (non-static dan non-sleeping)
        let moved_body_ids: Vec<RigidBodyId> = self
            .rigid_bodies
            .iter()
            .filter(|(_, b)| b.body_type() != BodyType::Static && !b.is_sleeping())
            .map(|(id, _)| *id)
            .collect();

        for body_id in moved_body_ids {
            let _ = self.sync_body_broadphase(body_id);
        }

        Ok(())
    }

    /// Melakukan integrasi penuh: kecepatan dari gravitasi lalu transform dan sinkronisasi broadphase.
    pub fn integrate(&mut self) -> Result<(), IntegrationError> {
        self.integrate_velocities()?;
        self.integrate_transforms()?;
        Ok(())
    }

    /// Membangun partisi Physics Island secara deterministik dari kumpulan kontak narrowphase saat ini.
    pub fn build_islands(&self, contacts: &[Contact]) -> Result<Vec<PhysicsIsland>, SleepError> {
        build_islands(&self.rigid_bodies, contacts)
    }

    /// Membangunkan badan kaku dinamis tertentu berdasarkan ID.
    pub fn wake_body(&mut self, id: RigidBodyId) -> bool {
        if let Some(body) = self.rigid_bodies.get_mut(&id) {
            body.wake();
            true
        } else {
            false
        }
    }

    /// Membangunkan seluruh badan kaku dinamis anggota suatu Physics Island.
    pub fn wake_island(&mut self, island: &PhysicsIsland) {
        for &body_id in &island.bodies {
            if let Some(body) = self.rigid_bodies.get_mut(&body_id) {
                body.wake();
            }
        }
    }

    /// Membangunkan seluruh badan kaku dinamis anggota Physics Island berdasarkan ID pulau.
    pub fn wake_island_by_id(
        &mut self,
        island_id: PhysicsIslandId,
        islands: &[PhysicsIsland],
    ) -> bool {
        if let Some(island) = islands.iter().find(|i| i.id == island_id) {
            self.wake_island(island);
            true
        } else {
            false
        }
    }

    /// Mengeksekusi satu fixed step simulasi fisika terpadu (Phase 9.8 Pipeline Orchestration).
    ///
    /// TAHAPAN EKSEKUSI DETERMINISTIK:
    /// 1. Deteksi kontak geometris narrowphase dari pasangan kandidat broadphase.
    /// 2. Konstruksi graf kontak dan partisi ke dalam Physics Island deterministik.
    /// 3. Deteksi pemicu gangguan (wake seeds) dan propagasi status bangun (wake) ke seluruh pulau.
    /// 4. Penyelesaian batasan kontak aktif (Sequential Impulse Solver) HANYA untuk pulau aktif (Awake).
    /// 5. Integrasi kecepatan (gravitasi) dan transform untuk badan aktif (Awake).
    /// 6. Evaluasi kondisi tenang dan akumulasi timer tidur untuk badan dinamis aktif.
    /// 7. Transisi atomik pulau yang seluruh anggotanya settled ke status tidur (Sleeping),
    ///    dengan kanonisasi kecepatan residual ke nol (tanpa memodifikasi transform).
    /// 8. Sinkronisasi broadphase selektif hanya untuk badan yang transform-nya bergerak.
    pub fn step(&mut self) -> Result<StepResult, PhysicsStepError> {
        // Validasi konfigurasi sleep
        self.config.sleep_config.validate()?;

        // Tahap 1: Generasi kontak narrowphase
        let contacts = self.generate_contacts()?;

        // Tahap 2: Konstruksi pulau fisika
        let mut islands = self.build_islands(&contacts)?;

        // Tahap 3: Deteksi pemicu gangguan bangun (wake seeds)
        let mut bodies_to_wake: BTreeSet<RigidBodyId> = BTreeSet::new();
        for contact in &contacts {
            let body_a = match self.rigid_bodies.get(&contact.body_a) {
                Some(b) => b,
                None => continue,
            };
            let body_b = match self.rigid_bodies.get(&contact.body_b) {
                Some(b) => b,
                None => continue,
            };

            // Dynamic <-> Dynamic
            if body_a.is_dynamic() && body_b.is_dynamic() {
                if body_a.is_awake() && body_b.is_sleeping() {
                    let v_sq = body_a.linear_velocity().length_squared()
                        + body_a.angular_velocity().length_squared();
                    if v_sq > 1e-6 || contact.penetration > 1e-4 {
                        bodies_to_wake.insert(contact.body_b);
                    }
                } else if body_b.is_awake() && body_a.is_sleeping() {
                    let v_sq = body_b.linear_velocity().length_squared()
                        + body_b.angular_velocity().length_squared();
                    if v_sq > 1e-6 || contact.penetration > 1e-4 {
                        bodies_to_wake.insert(contact.body_a);
                    }
                }
            }
            // Kinematic <-> Dynamic Sleeping
            else if body_a.is_kinematic() && body_b.is_dynamic() && body_b.is_sleeping() {
                let v_sq = body_a.linear_velocity().length_squared()
                    + body_a.angular_velocity().length_squared();
                if v_sq > 1e-6 || contact.penetration > 1e-4 {
                    bodies_to_wake.insert(contact.body_b);
                }
            } else if body_b.is_kinematic() && body_a.is_dynamic() && body_a.is_sleeping() {
                let v_sq = body_b.linear_velocity().length_squared()
                    + body_b.angular_velocity().length_squared();
                if v_sq > 1e-6 || contact.penetration > 1e-4 {
                    bodies_to_wake.insert(contact.body_a);
                }
            }
        }

        // Propagasi bangun ke seluruh anggota pulau terkait
        for island in &mut islands {
            let has_disturbance = island.bodies.iter().any(|id| bodies_to_wake.contains(id));
            let has_awake = island
                .bodies
                .iter()
                .any(|id| self.rigid_bodies.get(id).is_some_and(|b| b.is_awake()));
            let has_sleeping = island
                .bodies
                .iter()
                .any(|id| self.rigid_bodies.get(id).is_some_and(|b| b.is_sleeping()));

            if has_disturbance || (has_awake && has_sleeping) {
                island.state = IslandState::Awake;
                for &id in &island.bodies {
                    if let Some(b) = self.rigid_bodies.get_mut(&id) {
                        if b.is_sleeping() {
                            b.wake();
                        }
                    }
                }
            }
        }

        // Tahap 4: Integrasi kecepatan (gravitasi) untuk badan aktif (semi-implicit Euler)
        self.integrate_velocities()?;

        // Tahap 5: Penyelesaian batasan kontak aktif (Sequential Impulse Solver)
        let mut active_contacts: Vec<Contact> = Vec::new();
        let mut solved_contact_indices: BTreeSet<usize> = BTreeSet::new();
        for island in &islands {
            if island.state == IslandState::Awake {
                for &contact_idx in &island.contact_indices {
                    if solved_contact_indices.insert(contact_idx) {
                        active_contacts.push(contacts[contact_idx]);
                    }
                }
            }
        }

        if !active_contacts.is_empty() {
            self.solve_contacts(&active_contacts)?;
        }

        // Tahap 6: Integrasi transform untuk badan aktif
        self.integrate_transforms()?;

        // Tahap 6 & 7: Evaluasi kondisi tenang dan transisi tidur pulau
        let dt = self.config.fixed_dt;
        let lin_thresh_sq = self.config.sleep_config.linear_velocity_threshold
            * self.config.sleep_config.linear_velocity_threshold;
        let ang_thresh_sq = self.config.sleep_config.angular_velocity_threshold
            * self.config.sleep_config.angular_velocity_threshold;
        let sleep_duration = self.config.sleep_config.sleep_duration;

        // Perbarui timer tidur untuk badan dinamis yang aktif
        for body in self.rigid_bodies.values_mut() {
            if body.is_dynamic() && body.is_awake() {
                let v_sq = body.linear_velocity().length_squared();
                let w_sq = body.angular_velocity().length_squared();
                let is_quiet = v_sq.is_finite()
                    && w_sq.is_finite()
                    && v_sq <= lin_thresh_sq
                    && w_sq <= ang_thresh_sq;
                body.update_sleep_timer(dt, is_quiet);
            }
        }

        // Transisi atomik pulau yang seluruh anggotanya settled
        for island in &mut islands {
            if island.state == IslandState::Awake {
                let all_eligible = island.bodies.iter().all(|id| {
                    if let Some(b) = self.rigid_bodies.get(id) {
                        let v_sq = b.linear_velocity().length_squared();
                        let w_sq = b.angular_velocity().length_squared();
                        let is_quiet = v_sq.is_finite()
                            && w_sq.is_finite()
                            && v_sq <= lin_thresh_sq
                            && w_sq <= ang_thresh_sq;
                        is_quiet && b.sleep_timer() >= sleep_duration
                    } else {
                        false
                    }
                });

                if all_eligible && !island.bodies.is_empty() {
                    island.state = IslandState::Sleeping;
                    for &id in &island.bodies {
                        if let Some(b) = self.rigid_bodies.get_mut(&id) {
                            b.put_to_sleep();
                        }
                    }
                }
            }
        }

        let awake_islands_count = islands
            .iter()
            .filter(|i| i.state == IslandState::Awake)
            .count();
        let sleeping_islands_count = islands.len() - awake_islands_count;

        Ok(StepResult {
            contacts_generated: contacts.len(),
            active_contacts_solved: active_contacts.len(),
            islands_count: islands.len(),
            awake_islands_count,
            sleeping_islands_count,
        })
    }

    /// Mengosongkan seluruh badan, collider, dan broadphase dari dunia fisika.
    pub fn clear(&mut self) {
        self.rigid_bodies.clear();
        self.colliders.clear();
        self.broadphase.clear();
    }
}

/// Hasil ringkasan langkah simulasi fisika (PhysicsWorld::step).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepResult {
    /// Jumlah total kontak geometris narrowphase yang terdeteksi
    pub contacts_generated: usize,
    /// Jumlah batasan kontak yang diselesaikan oleh solver (hanya pulau aktif)
    pub active_contacts_solved: usize,
    /// Jumlah total Physics Island yang dipartisi
    pub islands_count: usize,
    /// Jumlah pulau berstatus aktif (Awake)
    pub awake_islands_count: usize,
    /// Jumlah pulau berstatus tidur (Sleeping)
    pub sleeping_islands_count: usize,
}

/// Kesalahan dalam eksekusi langkah simulasi fisika terpadu (PhysicsWorld::step).
#[derive(Debug)]
pub enum PhysicsStepError {
    Narrowphase(NarrowphaseError),
    Island(SleepError),
    Solver(SolverError),
    Integration(IntegrationError),
    Broadphase(BroadphaseError),
}

impl std::fmt::Display for PhysicsStepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Narrowphase(e) => write!(f, "Narrowphase error: {}", e),
            Self::Island(e) => write!(f, "Island error: {}", e),
            Self::Solver(e) => write!(f, "Solver error: {}", e),
            Self::Integration(e) => write!(f, "Integration error: {}", e),
            Self::Broadphase(e) => write!(f, "Broadphase error: {}", e),
        }
    }
}

impl std::error::Error for PhysicsStepError {}

impl From<NarrowphaseError> for PhysicsStepError {
    fn from(e: NarrowphaseError) -> Self {
        Self::Narrowphase(e)
    }
}

impl From<SleepError> for PhysicsStepError {
    fn from(e: SleepError) -> Self {
        Self::Island(e)
    }
}

impl From<SolverError> for PhysicsStepError {
    fn from(e: SolverError) -> Self {
        Self::Solver(e)
    }
}

impl From<IntegrationError> for PhysicsStepError {
    fn from(e: IntegrationError) -> Self {
        Self::Integration(e)
    }
}

impl From<BroadphaseError> for PhysicsStepError {
    fn from(e: BroadphaseError) -> Self {
        Self::Broadphase(e)
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
