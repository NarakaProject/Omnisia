use glam::{IVec3, Vec3};
use std::collections::VecDeque;
use std::sync::Arc;

use crate::chunk::Chunk;
use crate::coord::{world_pos_to_world_voxel, CHUNK_SIZE, CHUNK_WORLD_SIZE};
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

/// Representasi dunia runtime sparse dengan streaming asynchronous, chunk scheduling, memory management, dan structural connectivity
pub struct World {
    pub store: ChunkStore,
    pub materials: MaterialRegistry,
    pub blocks: BlockRegistry,
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

        Self {
            store: ChunkStore::new(),
            materials,
            blocks,
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
            max_uploads_per_frame: 32,
            last_uploads_count: 0,
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

        // 5. Masukkan mesh baru yang siap dari scheduler ke upload_queue
        for (coord, mesh) in self.scheduler.ready_meshes.drain(..) {
            // Hindari duplikasi jika koordinat sama sudah ada di antrean
            self.upload_queue.retain(|(c, _)| *c != coord);
            self.upload_queue.push_back((coord, mesh));
        }

        // 4. Prioritaskan upload berdasarkan jarak terdekat ke kamera
        if self.upload_queue.len() > 1 {
            let cam_pos = camera_world_pos;
            let mut items: Vec<(IVec3, MeshData)> = self.upload_queue.drain(..).collect();
            items.sort_unstable_by(|(c1, _), (c2, _)| {
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
            self.upload_queue = items.into();
        }

        // 5. Upload dengan batas kuota per frame (GPU Upload Budget)
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

        // 6. Streaming Radius: Permintaan chunk di sekitar posisi kamera
        let camera_voxel = world_pos_to_world_voxel(camera_world_pos);
        let center_chunk = IVec3::new(
            camera_voxel.x.div_euclid(CHUNK_SIZE),
            camera_voxel.y.div_euclid(CHUNK_SIZE),
            camera_voxel.z.div_euclid(CHUNK_SIZE),
        );

        let r = self.render_radius;
        for dy in -2..=2 {
            for dz in -r..=r {
                for dx in -r..=r {
                    let chunk_coord = center_chunk + IVec3::new(dx, dy, dz);
                    if !self.store.contains(&chunk_coord) && !self.store.is_in_flight(&chunk_coord)
                    {
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

        // 7. Pembatalan kooperatif untuk request yang berada jauh di luar retain radius
        self.scheduler
            .cancel_outside_radius(camera_world_pos, self.retain_radius);

        // 8. Evaluasi Eviksi jika melebihi batas retain radius atau memory budget
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

        for coord in to_evict_clean {
            self.store.remove(&coord);
            self.upload_queue.retain(|(c, _)| *c != coord);
            if let Some(ref mut rend) = renderer {
                rend.remove_chunk_mesh(&coord);
            }
        }

        // Sinkronisasi pembersihan GPU mesh agar tidak ada monotonic leak
        if let Some(ref mut rend) = renderer {
            let active_set = self.store.resident.keys().cloned().collect();
            rend.retain_only(&active_set);
        }

        // 9. Dispatch pending jobs ke Worker Pool
        self.scheduler.dispatch_pending_jobs(
            &mut self.store,
            &self.materials,
            &self.storage,
            &self.generator,
            32,
        );
    }
}
