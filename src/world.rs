use glam::{IVec3, Vec3};
use std::sync::Arc;

use crate::chunk::Chunk;
use crate::coord::{world_pos_to_world_voxel, CHUNK_SIZE, CHUNK_WORLD_SIZE};
use crate::material::MaterialRegistry;
use crate::modding::registry::BlockRegistry;
use crate::modding::runtime::ContentRuntime;
use crate::renderer::Renderer;
use crate::storage::{MemoryCompressedRegionStore, RegionStore};
use crate::streaming::generator::{ChunkGenerator, DemoChunkGenerator};
use crate::streaming::jobs::{JobPriority, JobType};
use crate::streaming::memory::MemoryBudget;
use crate::streaming::scheduler::ChunkScheduler;
use crate::streaming::store::ChunkStore;
use crate::voxel::VoxelBlock;

/// Representasi dunia runtime sparse dengan streaming asynchronous, chunk scheduling, dan memory management
pub struct World {
    pub store: ChunkStore,
    pub materials: MaterialRegistry,
    pub blocks: BlockRegistry,
    pub storage: Arc<dyn RegionStore>,
    pub generator: Arc<dyn ChunkGenerator>,
    pub scheduler: ChunkScheduler,
    pub budget: MemoryBudget,

    pub simulation_radius: i32,
    pub render_radius: i32,
    pub retain_radius: i32,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// Membuat instance World baru dengan memuat konten melalui ContentRuntime
    pub fn new() -> Self {
        match ContentRuntime::build_runtime("content/core", "mods") {
            Ok(resolved) => Self::with_content(resolved.materials, resolved.blocks),
            Err(e) => {
                log::error!(
                    "Gagal memuat Core Content saat inisialisasi World: {}. Menggunakan fallback minimal.",
                    e
                );
                Self::with_content(MaterialRegistry::new(), BlockRegistry::new())
            }
        }
    }

    /// Membuat instance World dengan MaterialRegistry dan BlockRegistry yang sudah di-resolve
    pub fn with_content(materials: MaterialRegistry, blocks: BlockRegistry) -> Self {
        let num_cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(2, 8);

        Self {
            store: ChunkStore::new(),
            materials,
            blocks,
            storage: Arc::new(MemoryCompressedRegionStore::new()),
            generator: Arc::new(DemoChunkGenerator::new(1337)),
            scheduler: ChunkScheduler::new(num_cpus),
            budget: MemoryBudget::default(),
            simulation_radius: 3,
            render_radius: 5,
            retain_radius: 7,
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

    /// Menetapkan voxel pada koordinat global dunia (world voxel)
    pub fn set_voxel_world(&mut self, world_voxel: IVec3, block: VoxelBlock) {
        self.store.set_voxel_world(world_voxel, block);
    }

    /// Mengambil voxel pada koordinat global dunia
    pub fn get_voxel_world(&self, world_voxel: IVec3) -> VoxelBlock {
        self.store.get_voxel_world(world_voxel)
    }

    /// Update per frame: mengelola streaming radius, integrasi hasil worker, eviksi, dan upload GPU
    pub fn update(
        &mut self,
        camera_world_pos: Vec3,
        _dt: f32,
        mut renderer: Option<&mut Renderer>,
    ) {
        // 1. Integrasi hasil worker dari scheduler
        self.scheduler.update(&mut self.store, &self.materials);

        // 2. Upload mesh baru yang siap ke Renderer GPU
        if let Some(ref mut rend) = renderer {
            for (coord, mesh) in self.scheduler.ready_meshes.drain(..) {
                rend.upload_chunk_mesh(coord, &mesh);
            }
        }

        // 3. Streaming Radius: Permintaan chunk di sekitar posisi kamera
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

                        let priority = if dx.abs() <= 1 && dz.abs() <= 1 {
                            JobPriority::High
                        } else {
                            JobPriority::Normal
                        };

                        self.scheduler.request_job(
                            chunk_coord,
                            JobType::LoadChunk,
                            priority,
                            0,
                            dist_sq,
                        );
                    }
                }
            }
        }

        // 4. Pembatalan kooperatif untuk request yang berada jauh di luar retain radius
        self.scheduler
            .cancel_outside_radius(camera_world_pos, self.retain_radius);

        // 5. Evaluasi Eviksi jika melebihi batas retain radius atau memory budget
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
                        self.scheduler.request_job(
                            coord,
                            JobType::SaveChunk,
                            JobPriority::Low,
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
            if let Some(ref mut rend) = renderer {
                rend.remove_chunk_mesh(&coord);
            }
        }

        // 6. Dispatch pending jobs ke Worker Pool
        self.scheduler.dispatch_pending_jobs(
            &mut self.store,
            &self.materials,
            &self.storage,
            &self.generator,
            32,
        );
    }

    /// Menghasilkan Demo World awal
    pub fn generate_demo_world(&mut self) {
        log::info!("Membangun initial resident chunks Omnisia...");

        for cx in -1..=2 {
            for cz in -1..=2 {
                for cy in 0..=1 {
                    let coord = IVec3::new(cx, cy, cz);
                    let chunk = self.generator.generate_chunk(coord, &self.materials);
                    self.store.insert(chunk);
                    self.scheduler.request_job(
                        coord,
                        JobType::MeshChunk,
                        JobPriority::Critical,
                        0,
                        0.0,
                    );
                }
            }
        }

        log::info!(
            "Initial chunks berhasil dibangun: {} resident chunks",
            self.store.resident_count()
        );
    }
}
