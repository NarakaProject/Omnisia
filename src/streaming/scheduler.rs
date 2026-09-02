use crossbeam_channel::{Receiver, Sender};
use glam::{IVec3, Vec3};
use std::collections::{BinaryHeap, HashMap};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::chunk::dirty_flags;
use crate::coord::CHUNK_WORLD_SIZE;
use crate::material::MaterialRegistry;
use crate::mesh::greedy::generate_greedy_mesh;
use crate::mesh::types::MeshData;
use crate::storage::RegionStore;
use crate::streaming::generator::ChunkGenerator;
use crate::streaming::jobs::{ChunkJobRequest, ChunkJobResult, JobPriority, JobType};
use crate::streaming::store::ChunkStore;

/// Struktur Scheduler untuk mengorkestrasi pekerjaan asynchronous, worker pool, dan integrasi hasil ke ChunkStore
pub struct ChunkScheduler {
    job_counter: AtomicU64,
    queue: BinaryHeap<ChunkJobRequest>,
    queued_jobs: HashMap<(IVec3, JobType), (JobPriority, f32)>,

    // Channels komunikasi thread berbatas (bounded) untuk mencegah unbounded queue growth
    request_tx: Sender<WorkerTask>,
    result_rx: Receiver<ChunkJobResult>,
    result_tx: Sender<ChunkJobResult>,

    // Worker thread handles
    workers: Vec<JoinHandle<()>>,

    // Meshes siap diunggah ke GPU pada frame saat ini
    pub ready_meshes: Vec<(IVec3, MeshData)>,
}

enum WorkerTask {
    Execute(Box<dyn FnOnce() + Send + 'static>),
    Shutdown,
}

impl Drop for ChunkScheduler {
    fn drop(&mut self) {
        for _ in 0..self.workers.len() {
            let _ = self.request_tx.send(WorkerTask::Shutdown);
        }
        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }
    }
}

impl ChunkScheduler {
    pub fn new(num_workers: usize) -> Self {
        // Bounded channels berkapasitas 1,024 untuk memberikan backpressure terukur
        let (request_tx, request_rx) = crossbeam_channel::bounded::<WorkerTask>(1024);
        let (result_tx, result_rx) = crossbeam_channel::bounded::<ChunkJobResult>(1024);

        let mut workers = Vec::with_capacity(num_workers);
        for worker_id in 0..num_workers {
            let req_rx = request_rx.clone();
            let handle = thread::Builder::new()
                .name(format!("omnisia-worker-{}", worker_id))
                .spawn(move || {
                    while let Ok(task) = req_rx.recv() {
                        match task {
                            WorkerTask::Execute(f) => f(),
                            WorkerTask::Shutdown => break,
                        }
                    }
                })
                .expect("Gagal membuat background worker thread");
            workers.push(handle);
        }

        Self {
            job_counter: AtomicU64::new(1),
            queue: BinaryHeap::new(),
            queued_jobs: HashMap::new(),
            request_tx,
            result_rx,
            result_tx,
            workers,
            ready_meshes: Vec::new(),
        }
    }

    /// Mengembalikan jumlah job yang sedang menunggu dalam antrean scheduler
    pub fn pending_jobs_count(&self) -> usize {
        self.queue.len()
    }

    /// Meminta penjadwalan job untuk chunk tertentu dengan koalesi request duplikat dan Priority Escalation
    pub fn request_job(
        &mut self,
        coord: IVec3,
        job_type: JobType,
        priority: JobPriority,
        lifecycle_generation: u64,
        request_revision: u64,
        distance_sq: f32,
    ) {
        let key = (coord, job_type);
        if let Some(&(existing_priority, existing_dist)) = self.queued_jobs.get(&key) {
            // Priority Escalation: jika request baru memiliki prioritas lebih tinggi (nilai enum lebih kecil)
            // atau jarak lebih dekat, kita eskalasi job dalam antrean!
            if priority < existing_priority || distance_sq < existing_dist {
                let effective_priority = priority.min(existing_priority);
                let effective_distance = distance_sq.min(existing_dist);
                self.queued_jobs
                    .insert(key, (effective_priority, effective_distance));

                let job_id = self
                    .job_counter
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let req = ChunkJobRequest::new(
                    job_id,
                    coord,
                    job_type,
                    effective_priority,
                    lifecycle_generation,
                    request_revision,
                    effective_distance,
                );
                self.queue.push(req);
            }
            return;
        }

        let job_id = self
            .job_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let req = ChunkJobRequest::new(
            job_id,
            coord,
            job_type,
            priority,
            lifecycle_generation,
            request_revision,
            distance_sq,
        );

        self.queued_jobs.insert(key, (priority, distance_sq));
        self.queue.push(req);
    }

    /// Membatalkan request dalam antrean yang berada di luar batas radius tertentu
    pub fn cancel_outside_radius(&mut self, center_world: Vec3, retain_radius_chunks: i32) {
        let max_dist_sq = (retain_radius_chunks as f32 * CHUNK_WORLD_SIZE).powi(2);
        for req in self.queue.iter() {
            let chunk_center = Vec3::new(
                (req.coord.x as f32 + 0.5) * CHUNK_WORLD_SIZE,
                (req.coord.y as f32 + 0.5) * CHUNK_WORLD_SIZE,
                (req.coord.z as f32 + 0.5) * CHUNK_WORLD_SIZE,
            );
            if center_world.distance_squared(chunk_center) > max_dist_sq {
                req.cancel();
            }
        }
    }

    /// Mengecek apakah tetangga 6 sisi chunk sudah siap untuk meshing secara asynchronous dan non-blocking
    pub fn is_neighborhood_ready(&self, coord: IVec3, store: &ChunkStore) -> bool {
        for offset in [
            IVec3::new(1, 0, 0),
            IVec3::new(-1, 0, 0),
            IVec3::new(0, 1, 0),
            IVec3::new(0, -1, 0),
            IVec3::new(0, 0, 1),
            IVec3::new(0, 0, -1),
        ] {
            let neighbor_pos = coord + offset;
            // Jika tetangga horizontal berada dalam batas normal dan sedang in-flight loading/generating,
            // tandai bahwa neighborhood belum ready agar meshing ditunda tanpa memblokir thread
            if (store.in_flight_loading.contains(&neighbor_pos)
                || store.in_flight_generating.contains(&neighbor_pos))
                && !store.contains(&neighbor_pos)
            {
                return false;
            }
        }
        true
    }

    /// Mengirimkan job dari Priority Queue ke Worker Pool (maksimal `batch_size` per panggilan)
    pub fn dispatch_pending_jobs(
        &mut self,
        store: &mut ChunkStore,
        registry: &MaterialRegistry,
        storage: &Arc<dyn RegionStore>,
        generator: &Arc<dyn ChunkGenerator>,
        batch_size: usize,
    ) {
        let mut dispatched = 0;

        while dispatched < batch_size && !self.queue.is_empty() {
            let req = match self.queue.pop() {
                Some(r) => r,
                None => break,
            };

            self.queued_jobs.remove(&(req.coord, req.job_type));

            // Jika job telah dibatalkan secara kooperatif, lewati
            if req.is_cancelled() {
                continue;
            }

            let coord = req.coord;
            let lifecycle = req.lifecycle_generation;
            let req_rev = req.request_revision;
            let res_tx = self.result_tx.clone();
            let reg_clone = registry.clone();
            let store_ref = storage.clone();
            let gen_ref = generator.clone();

            match req.job_type {
                JobType::LoadChunk => {
                    if store.contains(&coord) || store.in_flight_loading.contains(&coord) {
                        continue;
                    }
                    store.in_flight_loading.insert(coord);

                    let task = move || match store_ref.load_chunk(coord, &reg_clone) {
                        Ok(Some(chunk)) => {
                            let _ = res_tx.send(ChunkJobResult::Loaded {
                                chunk,
                                lifecycle_generation: lifecycle,
                                request_revision: req_rev,
                            });
                        }
                        Ok(None) => {
                            // Belum ada di disk -> generate secara otomatis
                            let chunk = gen_ref.generate_chunk(coord, &reg_clone);
                            let _ = res_tx.send(ChunkJobResult::Generated {
                                chunk,
                                lifecycle_generation: lifecycle,
                                request_revision: req_rev,
                            });
                        }
                        Err(e) => {
                            let _ = res_tx.send(ChunkJobResult::Failed {
                                coord,
                                lifecycle_generation: lifecycle,
                                job_type: JobType::LoadChunk,
                                error: e.to_string(),
                            });
                        }
                    };

                    let _ = self.request_tx.send(WorkerTask::Execute(Box::new(task)));
                    dispatched += 1;
                }
                JobType::GenerateChunk => {
                    if store.contains(&coord) || store.in_flight_generating.contains(&coord) {
                        continue;
                    }
                    store.in_flight_generating.insert(coord);

                    let task = move || {
                        let chunk = gen_ref.generate_chunk(coord, &reg_clone);
                        let _ = res_tx.send(ChunkJobResult::Generated {
                            chunk,
                            lifecycle_generation: lifecycle,
                            request_revision: req_rev,
                        });
                    };

                    let _ = self.request_tx.send(WorkerTask::Execute(Box::new(task)));
                    dispatched += 1;
                }
                JobType::SaveChunk => {
                    if let Some(chunk) = store.get(&coord) {
                        let chunk_clone = chunk.clone();
                        let rev = chunk.revision;
                        store.in_flight_saving.insert(coord, (lifecycle, rev));

                        let task = move || match store_ref.save_chunk(&chunk_clone, &reg_clone) {
                            Ok(()) => {
                                let _ = res_tx.send(ChunkJobResult::Saved {
                                    coord,
                                    lifecycle_generation: lifecycle,
                                    saved_revision: rev,
                                });
                            }
                            Err(e) => {
                                let _ = res_tx.send(ChunkJobResult::Failed {
                                    coord,
                                    lifecycle_generation: lifecycle,
                                    job_type: JobType::SaveChunk,
                                    error: e.to_string(),
                                });
                            }
                        };

                        let _ = self.request_tx.send(WorkerTask::Execute(Box::new(task)));
                        dispatched += 1;
                    }
                }
                JobType::MeshChunk => {
                    if let Some(chunk) = store.get(&coord) {
                        let chunk_clone = chunk.clone();
                        let rev = chunk.revision;
                        store.in_flight_meshing.insert(coord, (lifecycle, rev));

                        let task = move || {
                            let mut mesh = MeshData::new();
                            generate_greedy_mesh(&chunk_clone, &reg_clone, &mut mesh);
                            let _ = res_tx.send(ChunkJobResult::Meshed {
                                coord,
                                lifecycle_generation: lifecycle,
                                mesh,
                                mesh_revision: rev,
                            });
                        };

                        let _ = self.request_tx.send(WorkerTask::Execute(Box::new(task)));
                        dispatched += 1;
                    }
                }
                JobType::BuildLOD => {
                    // Placeholder arsitektural untuk distant LOD builder
                }
            }
        }
    }

    /// Update frame main thread: menguras hasil worker, memvalidasi stale jobs berbasis lifecycle & revision, dan memperbarui ChunkStore
    pub fn update(&mut self, store: &mut ChunkStore, _registry: &MaterialRegistry) {
        self.ready_meshes.clear();

        while let Ok(result) = self.result_rx.try_recv() {
            match result {
                ChunkJobResult::Loaded {
                    chunk,
                    lifecycle_generation,
                    request_revision: _,
                } => {
                    let pos = chunk.position;
                    store.in_flight_loading.remove(&pos);

                    // Lifecycle Validation: tolak hasil jika residency cycle sudah berubah
                    if store.current_lifecycle(&pos) != lifecycle_generation {
                        continue;
                    }

                    // Proteksi stale overwrite: jika chunk sudah ada dan memiliki revisi lebih tinggi, jangan timpa!
                    if let Some(existing) = store.get(&pos) {
                        if existing.revision >= chunk.revision {
                            continue;
                        }
                    }

                    // Daftarkan chunk dan jadwalkan meshing
                    store.insert(chunk);
                    self.request_job(
                        pos,
                        JobType::MeshChunk,
                        JobPriority::High,
                        lifecycle_generation,
                        0,
                        0.0,
                    );
                }
                ChunkJobResult::Generated {
                    chunk,
                    lifecycle_generation,
                    request_revision: _,
                } => {
                    let pos = chunk.position;
                    store.in_flight_generating.remove(&pos);

                    // Lifecycle Validation: tolak hasil jika residency cycle sudah berubah
                    if store.current_lifecycle(&pos) != lifecycle_generation {
                        continue;
                    }

                    if store.contains(&pos) {
                        continue;
                    }

                    store.insert(chunk);
                    self.request_job(
                        pos,
                        JobType::MeshChunk,
                        JobPriority::High,
                        lifecycle_generation,
                        0,
                        0.0,
                    );
                }
                ChunkJobResult::Saved {
                    coord,
                    lifecycle_generation,
                    saved_revision,
                } => {
                    store.in_flight_saving.remove(&coord);

                    // Lifecycle Validation: tolak hasil save jika chunk telah dievict dan di-resurrect
                    if store.current_lifecycle(&coord) != lifecycle_generation {
                        continue;
                    }

                    if let Some(chunk) = store.get_mut(&coord) {
                        // Bersihkan SAVE_DIRTY hanya jika revisi chunk masih sama dengan saat save dimulai
                        chunk.clear_dirty_if_revision_matched(
                            dirty_flags::SAVE_DIRTY,
                            saved_revision,
                        );
                    }
                }
                ChunkJobResult::Meshed {
                    coord,
                    lifecycle_generation,
                    mesh,
                    mesh_revision,
                } => {
                    store.in_flight_meshing.remove(&coord);

                    // Lifecycle Validation: tolak mesh jika residency cycle sudah berubah
                    if store.current_lifecycle(&coord) != lifecycle_generation {
                        continue;
                    }

                    if let Some(chunk) = store.get_mut(&coord) {
                        // Stale Mesh Protection: Hanya terima jika revisi chunk sama dengan saat meshing dimulai
                        if chunk.revision == mesh_revision {
                            chunk.clear_dirty(dirty_flags::MESH_DIRTY);
                            self.ready_meshes.push((coord, mesh));
                        }
                    }
                }
                ChunkJobResult::Failed {
                    coord,
                    lifecycle_generation: _,
                    job_type,
                    error,
                } => {
                    log::warn!(
                        "Chunk Job {:?} pada koordinat {:?} gagal: {}",
                        job_type,
                        coord,
                        error
                    );
                    match job_type {
                        JobType::LoadChunk => {
                            store.in_flight_loading.remove(&coord);
                        }
                        JobType::GenerateChunk => {
                            store.in_flight_generating.remove(&coord);
                        }
                        JobType::SaveChunk => {
                            store.in_flight_saving.remove(&coord);
                        }
                        JobType::MeshChunk => {
                            store.in_flight_meshing.remove(&coord);
                        }
                        JobType::BuildLOD => {}
                    }
                }
            }
        }
    }

    pub fn pending_job_count(&self) -> usize {
        self.queue.len()
    }
}
