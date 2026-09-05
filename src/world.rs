use glam::{IVec3, Vec3};
use std::collections::VecDeque;
use std::sync::Arc;

use crate::chunk::Chunk;
use crate::coord::{world_pos_to_world_voxel, CHUNK_SIZE, CHUNK_WORLD_SIZE};
use crate::csg::{VoxelEditCommitResult, VoxelEditError, VoxelEditTransaction};
use crate::interaction::gathering::ResourceGatheringRegistry;
use crate::interaction::placement::BuildRuleRegistry;
use crate::material::MaterialRegistry;
use crate::mesh::types::MeshData;
use crate::modding::registry::BlockRegistry;
use crate::modding::runtime::ContentRuntime;
use crate::physics::PhysicsRuntime;
use crate::player::PlayerController;
use crate::renderer::Renderer;
use crate::storage::{MemoryCompressedRegionStore, RegionStore};
use crate::streaming::generator::ChunkGenerator;
use crate::streaming::jobs::{JobPriority, JobType};
use crate::streaming::memory::MemoryBudget;
use crate::streaming::scheduler::ChunkScheduler;
use crate::streaming::store::ChunkStore;
use crate::structure::aggregate::DetachedAggregate;
use crate::structure::anchor::AnchorPolicy;
use crate::structure::events::{StructuralEvent, StructuralMutationType};
use crate::structure::manager::StructuralSystem;
use crate::voxel::VoxelBlock;
use crate::worldgen::config::WorldGenConfig;
use crate::worldgen::pipeline::ProceduralWorldGenerator;
use crate::worldgen::seed::WorldSeed;

/// Laporan audit integritas kepemilikan voxel dunia lintas sistem (8C.5 & Section 39).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OwnershipAuditReport {
    /// Jumlah total voxel solid non-air di ChunkStore (dunia statis)
    pub total_static_voxels: usize,
    /// Jumlah total voxel solid yang dimiliki oleh seluruh DynamicBody di PhysicsRuntime
    pub total_dynamic_voxels: usize,
    /// Total keseluruhan voxel dalam simulasi dunia
    pub total_world_voxels: usize,
    /// Jumlah deteksi duplikasi kepemilikan voxel antar-sistem (HARUS 0)
    pub duplicate_detections: usize,
    /// Jumlah badan dinamis aktif di runtime fisika
    pub active_bodies_count: usize,
}

/// Representasi dunia runtime sparse dengan streaming asynchronous, chunk scheduling, memory management, dan structural connectivity
pub struct World {
    pub store: ChunkStore,
    pub materials: MaterialRegistry,
    pub blocks: BlockRegistry,
    pub resources: ResourceGatheringRegistry,
    pub build_rules: BuildRuleRegistry,
    pub storage: Arc<dyn RegionStore>,
    pub generator: Arc<dyn ChunkGenerator>,
    pub scheduler: ChunkScheduler,
    pub budget: MemoryBudget,
    pub structure: StructuralSystem,
    pub physics: PhysicsRuntime,

    pub simulation_radius: i32,
    pub render_radius: i32,
    pub retain_radius: i32,

    // GPU Upload Discipline
    pub upload_queue: VecDeque<(IVec3, MeshData)>,
    pub max_uploads_per_frame: usize,
    pub last_uploads_count: usize,

    // Streaming Discovery Cache
    pub last_center_chunk: Option<IVec3>,
    pub streaming_radius_satisfied: bool,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// Membuat instance World baru dengan memuat konten melalui ContentRuntime
    pub fn new() -> Self {
        Self::with_seed(WorldSeed::default())
    }

    /// Mencoba membuat instance World dengan seed tertentu, mengembalikan error jika Core Content gagal dimuat
    pub fn try_with_seed(seed: WorldSeed) -> Result<Self, String> {
        let config = WorldGenConfig::new(seed);
        let resolved = ContentRuntime::build_runtime("content/core", "mods")
            .map_err(|e| format!("Gagal memuat Core Content saat inisialisasi World: {}", e))?;
        Ok(Self::with_content_and_config(
            resolved.materials,
            resolved.blocks,
            config,
        ))
    }

    /// Membuat instance World dengan seed tertentu (panic secara eksplisit jika Core Content hilang/gagal)
    pub fn with_seed(seed: WorldSeed) -> Self {
        Self::try_with_seed(seed).expect("Inisialisasi World gagal karena Core Content tidak valid atau hilang (tidak ada silent minimal fallback)")
    }

    /// Membuat instance World dengan MaterialRegistry, BlockRegistry, dan WorldGenConfig yang ditentukan
    pub fn with_content_and_config(
        materials: MaterialRegistry,
        blocks: BlockRegistry,
        config: WorldGenConfig,
    ) -> Self {
        let num_cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(2, 8);

        let anchor_policy = AnchorPolicy::from_registries(&materials, &blocks);
        let structure = StructuralSystem::new(anchor_policy);
        let resources = ResourceGatheringRegistry::from_registries(&materials, &blocks);
        let build_rules = BuildRuleRegistry::from_registries(&materials, &blocks);

        Self {
            store: ChunkStore::new(),
            materials,
            blocks,
            resources,
            build_rules,
            storage: Arc::new(MemoryCompressedRegionStore::new()),
            generator: Arc::new(ProceduralWorldGenerator::new(config)),
            scheduler: ChunkScheduler::new(num_cpus),
            budget: MemoryBudget::default(),
            structure,
            physics: PhysicsRuntime::default(),
            simulation_radius: 3,
            render_radius: 5,
            retain_radius: 7,
            upload_queue: VecDeque::new(),
            max_uploads_per_frame: 4,
            last_uploads_count: 0,
            last_center_chunk: None,
            streaming_radius_satisfied: false,
        }
    }

    #[inline(always)]
    pub fn get_chunk(&self, coord: &IVec3) -> Option<&Chunk> {
        self.store.get(coord)
    }

    #[inline(always)]
    pub fn get_chunk_mut(&mut self, coord: &IVec3) -> Option<&mut Chunk> {
        self.store.get_mut(coord)
    }

    /// Menetapkan voxel pada koordinat global dunia (world voxel), memancarkan StructuralEvent,
    /// dan mengekstrak gugusan yang terlepas secara langsung ke PhysicsRuntime.
    ///
    /// GUARDRAIL 1: StructuralEvent terintegrasi langsung di production pipeline.
    pub fn set_voxel_world(
        &mut self,
        world_voxel: IVec3,
        block: VoxelBlock,
    ) -> Vec<DetachedAggregate> {
        let previous_block = self.store.get_voxel_world(world_voxel);
        if previous_block == block {
            return Vec::new();
        }

        // Mutasi otoritatif pada ChunkStore
        self.store.set_voxel_world(world_voxel, block);

        // Pancarkan StructuralEvent
        let mutation = if previous_block.is_air() && !block.is_air() {
            StructuralMutationType::VoxelPlaced { new_block: block }
        } else if !previous_block.is_air() && block.is_air() {
            StructuralMutationType::VoxelRemoved { previous_block }
        } else {
            StructuralMutationType::VoxelReplaced {
                previous_block,
                new_block: block,
            }
        };

        let event = StructuralEvent::new(world_voxel, mutation);
        let newly_detached = self.structure.process_event(&event, &mut self.store);

        // Daftarkan gugusan yang lepas langsung ke runtime fisika (DynamicBody)
        for agg in &newly_detached {
            self.physics.spawn_from_detached_aggregate(agg.clone());
        }

        // Bangunkan badan yang sedang resting jika voxel tumpuannya hancur (8C.4 & Section 21)
        self.physics.handle_static_terrain_mutation(&self.store);

        newly_detached
    }

    /// Mengeksekusi transaksi mutasi voxel (CSG / Interaksi) secara atomik,
    /// merekonsiliasi sistem struktural untuk mengekstrak gugusan yang terlepas,
    /// mendaftarkannya ke runtime fisika (DynamicBody), dan menandai remesh.
    pub fn commit_voxel_transaction(
        &mut self,
        transaction: &VoxelEditTransaction,
    ) -> Result<(VoxelEditCommitResult, Vec<DetachedAggregate>), VoxelEditError> {
        let commit_result = transaction.commit(&mut self.store)?;

        let mut newly_detached = Vec::new();
        for event in &commit_result.structural_events {
            let detached = self.structure.process_event(event, &mut self.store);
            newly_detached.extend(detached);
        }

        for agg in &newly_detached {
            self.physics.spawn_from_detached_aggregate(agg.clone());
        }

        if !commit_result.delta.is_empty() {
            self.physics.handle_static_terrain_mutation(&self.store);
        }

        Ok((commit_result, newly_detached))
    }

    /// Mengambil voxel pada koordinat global dunia
    pub fn get_voxel_world(&self, world_voxel: IVec3) -> VoxelBlock {
        self.store.get_voxel_world(world_voxel)
    }

    /// Memperbarui simulasi Player Controller terintegrasi terhadap dunia statis (8C.1)
    /// dan badan dinamis (8C.2).
    pub fn update_player(&self, player: &mut PlayerController, dt: f32, camera_yaw_deg: f32) {
        player.update_fixed_time_with_physics(dt, &self.store, Some(&self.physics), camera_yaw_deg);
    }

    /// Menemukan titik spawn yang valid di atas permukaan tanah solid dunia statis (8C.1).
    pub fn spawn_player_at_valid_ground(
        &self,
        player: &mut PlayerController,
        center_x: f32,
        center_z: f32,
        search_min_y: f32,
        search_max_y: f32,
    ) -> bool {
        player.spawn_at_valid_ground(center_x, center_z, search_min_y, search_max_y, &self.store)
    }

    /// Mengaudit integritas dan konsistensi kepemilikan voxel di seluruh dunia (8C.5 & Section 39).
    /// Memastikan prinsip invarian bahwa setiap voxel hanya dimiliki oleh tepat 1 otoritas:
    /// ChunkStore ATAU DynamicBody, tidak pernah keduanya, tidak pernah duplikat, dan tidak ada voxel yang hilang.
    pub fn audit_world_ownership(&self) -> OwnershipAuditReport {
        let mut total_static: usize = 0;
        for chunk in self.store.resident_chunks() {
            total_static += chunk.non_air_count as usize;
        }

        let mut total_dynamic = 0;
        let mut duplicate_detections = 0;
        let mut occupied_dynamic_coords = std::collections::HashSet::new();

        for body in self.physics.bodies.values() {
            total_dynamic += body.voxel_count();
            for (world_coord, _) in body.iter_world_voxels() {
                // 1. Cek duplikasi dengan ChunkStore statis
                if !self.store.get_voxel_world(world_coord).is_air() {
                    duplicate_detections += 1;
                }
                // 2. Cek duplikasi antar DynamicBody
                if !occupied_dynamic_coords.insert(world_coord) {
                    duplicate_detections += 1;
                }
            }
        }

        OwnershipAuditReport {
            total_static_voxels: total_static,
            total_dynamic_voxels: total_dynamic,
            total_world_voxels: total_static + total_dynamic,
            duplicate_detections,
            active_bodies_count: self.physics.active_body_count(),
        }
    }

    /// Jumlah mesh yang menunggu dalam antrean upload GPU
    pub fn upload_backlog(&self) -> usize {
        self.upload_queue.len()
    }

    /// Jumlah job yang sedang mengantre di scheduler
    pub fn pending_jobs_count(&self) -> usize {
        self.scheduler.pending_jobs_count()
    }

    /// Update per frame: mengelola streaming radius, integrasi hasil worker, eviksi, upload GPU, dan pending structural checks
    pub fn update(&mut self, camera_world_pos: Vec3, dt: f32, mut renderer: Option<&mut Renderer>) {
        // 1. Integrasi hasil worker dari scheduler
        self.scheduler.update(&mut self.store, &self.materials);

        // 2. Proses antrean pending structural connectivity checks saat chunk baru telah selesai dimuat
        let newly_detached = self.structure.process_pending_checks(&mut self.store);
        for agg in newly_detached {
            self.physics.spawn_from_detached_aggregate(agg);
        }

        // 3. Perbarui simulasi fisika DynamicBody (fixed-timestep 30 Hz dengan deteksi tabrakan)
        self.physics.update(dt, &self.store);

        // 4. Reintegrasi dua fase untuk badan dinamis yang telah Settled (Amendment 7 & 8)
        let _ = self.physics.process_settled_reintegration(&mut self.store);

        // 5. Drain dirty mesh chunks dan jadwalkan remesh (High Priority)
        for dirty_coord in std::mem::take(&mut self.store.dirty_mesh_chunks) {
            if self.store.contains(&dirty_coord) && !self.store.is_in_flight_meshing(&dirty_coord) {
                let chunk_center = Vec3::new(
                    (dirty_coord.x as f32 + 0.5) * CHUNK_WORLD_SIZE,
                    (dirty_coord.y as f32 + 0.5) * CHUNK_WORLD_SIZE,
                    (dirty_coord.z as f32 + 0.5) * CHUNK_WORLD_SIZE,
                );
                let dist_sq = camera_world_pos.distance_squared(chunk_center);
                let lifecycle = self.store.current_lifecycle(&dirty_coord);
                let revision = self
                    .store
                    .get(&dirty_coord)
                    .map(|c| c.revision)
                    .unwrap_or(0);
                self.scheduler.request_job(
                    dirty_coord,
                    JobType::MeshChunk,
                    JobPriority::High,
                    lifecycle,
                    revision,
                    dist_sq,
                );
            }
        }

        // 6. Masukkan mesh baru yang siap dari scheduler ke upload_queue
        let mut has_new_ready = false;
        for (coord, mesh) in self.scheduler.ready_meshes.drain(..) {
            // Hindari duplikasi jika koordinat sama sudah ada di antrean
            self.upload_queue.retain(|(c, _)| *c != coord);
            self.upload_queue.push_back((coord, mesh));
            has_new_ready = true;
        }

        // 7. Prioritaskan upload berdasarkan jarak terdekat ke kamera tanpa alokasi heap baru,
        // hanya diurutkan ulang saat mesh baru tiba
        if has_new_ready && self.upload_queue.len() > 1 {
            let cam_pos = camera_world_pos;
            self.upload_queue
                .make_contiguous()
                .sort_unstable_by(|(c1, _), (c2, _)| {
                    let p1 = Vec3::new(
                        (c1.x as f32 + 0.5) * CHUNK_WORLD_SIZE,
                        (c1.y as f32 + 0.5) * CHUNK_WORLD_SIZE,
                        (c1.z as f32 + 0.5) * CHUNK_WORLD_SIZE,
                    );
                    let p2 = Vec3::new(
                        (c2.x as f32 + 0.5) * CHUNK_WORLD_SIZE,
                        (c2.y as f32 + 0.5) * CHUNK_WORLD_SIZE,
                        (c2.z as f32 + 0.5) * CHUNK_WORLD_SIZE,
                    );
                    let d1 = cam_pos.distance_squared(p1);
                    let d2 = cam_pos.distance_squared(p2);
                    d1.partial_cmp(&d2).unwrap_or(std::cmp::Ordering::Equal)
                });
        }

        // 8. Upload dengan batas kuota per frame (GPU Upload Budget: 4 uploads/frame)
        let mut uploaded_count = 0;
        if let Some(ref mut rend) = renderer {
            while uploaded_count < self.max_uploads_per_frame {
                if let Some((coord, mesh)) = self.upload_queue.pop_front() {
                    // Hanya upload jika chunk masih ada di store (belum dievict)
                    if self.store.contains(&coord) {
                        rend.upload_chunk_mesh(coord, &mesh);
                        uploaded_count += 1;
                    }
                } else {
                    break;
                }
            }
        } else {
            // Dalam mode headless / tanpa renderer GPU aktif, kosongkan antrean upload agar tidak terjadi akumulasi memori tak terpakai
            self.upload_queue.clear();
        }
        self.last_uploads_count = uploaded_count;

        // 9. Streaming Radius: Permintaan chunk di sekitar posisi kamera
        let camera_voxel = world_pos_to_world_voxel(camera_world_pos);
        let center_chunk = IVec3::new(
            camera_voxel.x.div_euclid(CHUNK_SIZE),
            camera_voxel.y.div_euclid(CHUNK_SIZE),
            camera_voxel.z.div_euclid(CHUNK_SIZE),
        );

        if self.last_center_chunk != Some(center_chunk) {
            self.last_center_chunk = Some(center_chunk);
            self.streaming_radius_satisfied = false;
        }

        if !self.streaming_radius_satisfied {
            let mut missing_count = 0;
            let r = self.render_radius;
            for dy in -2..=2 {
                for dz in -r..=r {
                    for dx in -r..=r {
                        let chunk_coord = center_chunk + IVec3::new(dx, dy, dz);
                        if !self.store.contains(&chunk_coord)
                            && !self.store.is_in_flight(&chunk_coord)
                            && !self.scheduler.is_queued(&chunk_coord, JobType::LoadChunk)
                        {
                            missing_count += 1;
                            let chunk_center = Vec3::new(
                                (chunk_coord.x as f32 + 0.5) * CHUNK_WORLD_SIZE,
                                (chunk_coord.y as f32 + 0.5) * CHUNK_WORLD_SIZE,
                                (chunk_coord.z as f32 + 0.5) * CHUNK_WORLD_SIZE,
                            );
                            let dist_sq = camera_world_pos.distance_squared(chunk_center);

                            let priority = if dx.abs() <= self.simulation_radius
                                && dz.abs() <= self.simulation_radius
                            {
                                JobPriority::High
                            } else {
                                JobPriority::Normal
                            };

                            let lifecycle = self.store.current_lifecycle(&chunk_coord);
                            self.scheduler.request_job(
                                chunk_coord,
                                JobType::LoadChunk,
                                priority,
                                lifecycle,
                                0,
                                dist_sq,
                            );
                        }
                    }
                }
            }
            if missing_count == 0 {
                self.streaming_radius_satisfied = true;
            }
        }

        // 10. Pembatalan kooperatif untuk request yang berada jauh di luar retain radius
        self.scheduler
            .cancel_outside_radius(camera_world_pos, self.retain_radius);

        // 11. Evaluasi Eviksi jika melebihi batas retain radius atau memory budget
        let retain_radius_sq = (self.retain_radius as f32 * CHUNK_WORLD_SIZE).powi(2);
        let mut to_evict_clean = Vec::new();

        for (&coord, chunk) in self.store.resident.iter() {
            let chunk_center = Vec3::new(
                (coord.x as f32 + 0.5) * CHUNK_WORLD_SIZE,
                (coord.y as f32 + 0.5) * CHUNK_WORLD_SIZE,
                (coord.z as f32 + 0.5) * CHUNK_WORLD_SIZE,
            );
            let dist_sq = camera_world_pos.distance_squared(chunk_center);

            if dist_sq > retain_radius_sq {
                if chunk.is_dirty(crate::chunk::dirty_flags::SAVE_DIRTY) {
                    // Chunk kotor: WAJIB dijadwalkan save sebelum dievict!
                    if !self.store.in_flight_saving.contains_key(&coord) {
                        let lifecycle = self.store.current_lifecycle(&coord);
                        self.scheduler.request_job(
                            coord,
                            JobType::SaveChunk,
                            JobPriority::Low,
                            lifecycle,
                            chunk.revision,
                            dist_sq,
                        );
                    }
                } else {
                    to_evict_clean.push(coord);
                }
            }
        }

        if !to_evict_clean.is_empty() {
            self.streaming_radius_satisfied = false;
        }

        for coord in to_evict_clean {
            self.store.remove(&coord);
            self.upload_queue.retain(|(c, _)| *c != coord);
            if let Some(ref mut rend) = renderer {
                rend.remove_chunk_mesh(&coord);
            }
        }

        // 12. Dispatch pending jobs ke Worker Pool
        self.scheduler.dispatch_pending_jobs(
            &mut self.store,
            &self.materials,
            &self.storage,
            &self.generator,
            32,
        );
    }

    /// Menghasilkan kawah bola terikat (CSG Crater) pada koordinat dunia,
    /// menerapkan transaksi secara atomik, dan menandai chunk yang terdampak untuk remesh & save.
    pub fn apply_crater(&mut self, center: Vec3, radius: f32) -> Result<Vec<IVec3>, String> {
        let policy = crate::csg::DefaultDestructionPolicy;
        let tx = crate::csg::CraterGenerator::generate(
            center,
            radius,
            &policy,
            &self.materials,
            &self.store,
        )
        .map_err(|e| format!("Gagal menghasilkan kawah: {:?}", e))?;
        let commit_result = tx
            .commit(&mut self.store)
            .map_err(|e| format!("Gagal menerapkan transaksi kawah: {:?}", e))?;
        for coord in &commit_result.affected_chunks {
            self.store
                .mark_dirty(coord, crate::chunk::dirty_flags::SAVE_DIRTY);
        }
        Ok(commit_result.affected_chunks)
    }
}
