use std::collections::{HashSet, VecDeque};
use std::time::Instant;

use glam::{IVec3, Vec3};
use omnisia::camera::Camera;
use omnisia::chunk::Chunk;
use omnisia::coord::{canonical_linear_index, CHUNK_VOLUME};
use omnisia::material::MaterialId;
use omnisia::mesh::ao::calculate_face_ao;
use omnisia::mesh::culled::generate_culled_mesh;
use omnisia::mesh::greedy::generate_greedy_mesh;
use omnisia::mesh::types::{FaceDirection, MeshData};
use omnisia::modding::discovery::ModDiscovery;
use omnisia::modding::runtime::ContentRuntime;
use omnisia::storage::{decompress_and_deserialize_chunk, serialize_and_compress_chunk};
use omnisia::streaming::generator::ChunkGenerator;
use omnisia::voxel::VoxelBlock;
use omnisia::worldgen::config::WorldGenConfig;
use omnisia::worldgen::noise::sample_fbm_3d;
use omnisia::worldgen::pipeline::ProceduralWorldGenerator;
use omnisia::worldgen::seed::WorldSeed;

fn main() {
    println!("============================================================");
    println!("     OMNISIA ENGINE ARCHITECTURE BENCHMARK SUITE           ");
    println!("     Phase 6: Vegetation & Performance Stabilization       ");
    println!("     Target Baseline: MacBook Pro 2018 (Intel x86_64)      ");
    println!("============================================================");

    let resolved = ContentRuntime::build_runtime("content/core", "mods")
        .expect("Gagal memuat Core Content & Mods untuk benchmark");
    let registry = resolved.materials;

    // 1. Benchmark Chunk Indexing
    {
        let start = Instant::now();
        let iterations = 10_000_000;
        let mut sum = 0usize;
        for i in 0..iterations {
            let x = (i % 32) as usize;
            let y = ((i / 32) % 32) as usize;
            let z = ((i / 1024) % 32) as usize;
            sum = sum.wrapping_add(canonical_linear_index(x, y, z));
        }
        let elapsed = start.elapsed();
        let ns_per_op = elapsed.as_nanos() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 1] Chunk Indexing: {:.2} ns/op (Total: {:?}, Iterations: {}, sum: {})",
            ns_per_op, elapsed, iterations, sum
        );
    }

    // 2. Benchmark Chunk Fill & Mutation
    {
        let mut chunk = Chunk::new(IVec3::ZERO);
        let start = Instant::now();
        let iterations = 10_000;
        for _ in 0..iterations {
            chunk.fill_material(MaterialId::STONE);
            chunk.fill_material(MaterialId::AIR);
        }
        let elapsed = start.elapsed();
        let us_per_chunk_fill = elapsed.as_micros() as f64 / (iterations * 2) as f64;
        println!(
            "[BENCHMARK 2] Chunk Fill (32k voxels): {:.2} µs/chunk (Total: {:?})",
            us_per_chunk_fill, elapsed
        );
    }

    // Siapkan 1 Chunk Terrain Prosedural Nyata (Phase 6 dengan 3D features & vegetasi)
    let worldgen = ProceduralWorldGenerator::new(WorldGenConfig::new(WorldSeed::from_u64(1337)));
    let terrain_chunk = worldgen.generate_chunk(IVec3::new(0, 0, 0), &registry);

    // 3. Benchmark Culled Face Meshing 32³
    let mut culled_mesh = MeshData::new();
    {
        let start = Instant::now();
        let iterations = 500;
        for _ in 0..iterations {
            generate_culled_mesh(&terrain_chunk, &registry, &mut culled_mesh);
        }
        let elapsed = start.elapsed();
        let ms_per_chunk = elapsed.as_secs_f64() * 1000.0 / iterations as f64;
        println!(
            "[BENCHMARK 3] Culled Meshing 32³: {:.3} ms/chunk (Vertices: {}, Indices: {}, Quads: {})",
            ms_per_chunk,
            culled_mesh.vertex_count(),
            culled_mesh.index_count(),
            culled_mesh.quad_count()
        );
    }

    // 4. Benchmark Greedy Meshing
    let mut greedy_mesh = MeshData::new();
    {
        let start = Instant::now();
        let iterations = 500;
        for _ in 0..iterations {
            generate_greedy_mesh(&terrain_chunk, &registry, &mut greedy_mesh);
        }
        let elapsed = start.elapsed();
        let ms_per_chunk = elapsed.as_secs_f64() * 1000.0 / iterations as f64;
        let reduction_ratio =
            culled_mesh.quad_count() as f64 / greedy_mesh.quad_count().max(1) as f64;
        println!(
            "[BENCHMARK 4] Greedy Meshing 32³: {:.3} ms/chunk (Vertices: {}, Indices: {}, Quads: {})",
            ms_per_chunk,
            greedy_mesh.vertex_count(),
            greedy_mesh.index_count(),
            greedy_mesh.quad_count()
        );
        println!(
            "  -> Greedy Quad Reduction Ratio: {:.2}x vs Culled",
            reduction_ratio
        );
    }

    // 5. Benchmark Ambient Occlusion (AO) per face
    {
        let start = Instant::now();
        let iterations = 500_000;
        let mut sum_ao = 0.0f32;
        for i in 0..iterations {
            let x = (i % 30) + 1;
            let y = ((i / 30) % 30) + 1;
            let z = ((i / 900) % 30) + 1;
            let ao = calculate_face_ao(&terrain_chunk, x, y, z, FaceDirection::PosY);
            sum_ao += ao[0] as f32;
        }
        let elapsed = start.elapsed();
        let ns_per_face = elapsed.as_nanos() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 5] AO Calculation: {:.2} ns/face (Total: {:?}, sum: {:.1})",
            ns_per_face, elapsed, sum_ao
        );
    }

    // 6. Benchmark Rayon Parallel Meshing Across 100 Real Chunks
    {
        use rayon::prelude::*;
        let start = Instant::now();
        let chunks: Vec<Chunk> = (0..100)
            .into_par_iter()
            .map(|i| {
                let coord = IVec3::new(i % 10, 0, i / 10);
                worldgen.generate_chunk(coord, &registry)
            })
            .collect();

        let meshes: Vec<MeshData> = chunks
            .par_iter()
            .map(|c| {
                let mut m = MeshData::new();
                generate_greedy_mesh(c, &registry, &mut m);
                m
            })
            .collect();

        let elapsed = start.elapsed();
        let total_verts: usize = meshes.iter().map(|m| m.vertex_count()).sum();
        println!(
            "[BENCHMARK 6] 100 Procedural Chunk Parallel Meshing (Rayon): {:?} ({:.2} ms/100 chunks, Total Vertices: {})",
            elapsed,
            elapsed.as_secs_f64() * 1000.0,
            total_verts
        );
    }

    // 7. Benchmark 1,000 Chunks Parallel Meshing (Synthetic Stress Test)
    {
        use rayon::prelude::*;
        let mut sample_chunk = Chunk::new(IVec3::ZERO);
        for z in 0..32 {
            for x in 0..32 {
                for y in 0..16 {
                    sample_chunk.set_voxel(x, y, z, VoxelBlock::new(MaterialId::DIRT));
                }
            }
        }
        let chunk_ref = &sample_chunk;

        let start = Instant::now();
        let count = 1000;
        let total_quads: usize = (0..count)
            .into_par_iter()
            .map(|_| {
                let mut m = MeshData::new();
                generate_greedy_mesh(chunk_ref, &registry, &mut m);
                m.quad_count()
            })
            .sum();
        let elapsed = start.elapsed();
        println!(
            "[BENCHMARK 7] 1,000 Chunk Synthetic Meshing (Rayon): {:?} ({:.2} ms/1000 chunks, Total Quads: {})",
            elapsed,
            elapsed.as_secs_f64() * 1000.0,
            total_quads
        );
    }

    // 8 & 9. Benchmark Zstd Palette Chunk Compression
    let compressed_bytes = {
        let start = Instant::now();
        let compressed =
            serialize_and_compress_chunk(&terrain_chunk, &registry).expect("Kompresi chunk gagal");
        let elapsed = start.elapsed();
        println!(
            "[BENCHMARK 8 & 9] Chunk Palette Zstd Compress: {:?} | Raw: {} bytes -> Compressed: {} bytes ({:.1}x ratio)",
            elapsed,
            CHUNK_VOLUME * std::mem::size_of::<VoxelBlock>(),
            compressed.len(),
            (CHUNK_VOLUME * std::mem::size_of::<VoxelBlock>()) as f64 / compressed.len() as f64
        );
        compressed
    };

    // 10. Benchmark Zstd Palette Chunk Decompression
    {
        let start = Instant::now();
        let loaded = decompress_and_deserialize_chunk(&compressed_bytes, &registry)
            .expect("Dekompresi chunk gagal");
        let elapsed = start.elapsed();
        println!(
            "[BENCHMARK 10] Chunk Palette Zstd Decompress: {:?} (Valid non_air: {})",
            elapsed, loaded.non_air_count
        );
    }

    // 11. Benchmark Connectivity BFS
    {
        let start = Instant::now();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut traversed = 0;

        let start_coord = IVec3::new(0, 0, 0);
        queue.push_back(start_coord);
        visited.insert(start_coord);

        let neighbors = [
            IVec3::X,
            IVec3::NEG_X,
            IVec3::Y,
            IVec3::NEG_Y,
            IVec3::Z,
            IVec3::NEG_Z,
        ];

        while let Some(current) = queue.pop_front() {
            traversed += 1;
            for offset in &neighbors {
                let next = current + *offset;
                if next.x >= 0
                    && next.x < 32
                    && next.y >= 0
                    && next.y < 32
                    && next.z >= 0
                    && next.z < 32
                    && !visited.contains(&next)
                {
                    let block =
                        terrain_chunk.get_voxel(next.x as usize, next.y as usize, next.z as usize);
                    if !block.is_air() {
                        visited.insert(next);
                        queue.push_back(next);
                    }
                }
            }
        }
        let elapsed = start.elapsed();
        println!(
            "[BENCHMARK 11] Localized Connectivity BFS ({} voxels traversed): {:?}",
            traversed, elapsed
        );
    }

    // 12. Benchmark Mod Discovery & Manifest Parsing
    {
        let start = Instant::now();
        let iterations = 1000;
        let mut count = 0;
        for _ in 0..iterations {
            let (discovered, _) = ModDiscovery::discover_from_dir("mods");
            count = discovered.len();
        }
        let elapsed = start.elapsed();
        let us_per_run = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 12] Mod Discovery & Manifest Parsing: {:.2} µs/run (Mods found: {}, Total: {:?})",
            us_per_run, count, elapsed
        );
    }

    // 13. Benchmark MaterialRegistry Hot Path Lookup (MaterialId)
    {
        let start_index = Instant::now();
        let iterations = 10_000_000;
        let mut sum_density = 0.0f32;
        let mat_id = MaterialId::STONE;
        for _ in 0..iterations {
            if let Some(def) = registry.get(mat_id) {
                sum_density += def.density_kg_m3;
            }
        }
        let elapsed_index = start_index.elapsed();
        let ns_per_index_lookup = elapsed_index.as_nanos() as f64 / iterations as f64;

        println!(
            "[BENCHMARK 13] Registry Voxel Hot Path Lookup (MaterialId): {:.2} ns/op (Zero overhead, sum: {})",
            ns_per_index_lookup, sum_density as usize
        );
    }

    // 14. Benchmark Noise 3D fBm Sampling Throughput (1,000,000 samples)
    {
        let start = Instant::now();
        let iterations = 1_000_000;
        let mut sum = 0.0f32;
        for i in 0..iterations {
            let x = (i % 100) as f32;
            let y = ((i / 100) % 100) as f32;
            let z = (i / 10000) as f32;
            sum += sample_fbm_3d(x, y, z, 1337, 3, 0.5, 2.0, 0.02);
        }
        let elapsed = start.elapsed();
        let ns_per_noise = elapsed.as_nanos() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 14] Noise 3D fBm Sampling (1M samples): {:.2} ns/sample (Total: {:?}, sum: {:.1})",
            ns_per_noise, elapsed, sum
        );
    }

    // 15. Benchmark 3D Cave Density & Worm Tunnel Sampling (100,000 points)
    {
        let caves = worldgen.caves();
        let start = Instant::now();
        let iterations = 100_000;
        let mut cave_count = 0;
        for i in 0..iterations {
            let x = (i % 100) as f32;
            let y = ((i / 100) % 100) as f32 - 50.0;
            let z = (i / 1000) as f32;
            if caves.is_cave(x, y, z, 20.0) {
                cave_count += 1;
            }
        }
        let elapsed = start.elapsed();
        let ns_per_point = elapsed.as_nanos() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 15] 3D Cave & Worm Tunnel Sampling (100k points): {:.2} ns/point (Total: {:?}, caves found: {})",
            ns_per_point, elapsed, cave_count
        );
    }

    // 16. Benchmark 3D Overhang & Feature Evaluation (100,000 points)
    {
        let overhangs = worldgen.overhangs();
        let start = Instant::now();
        let iterations = 100_000;
        let mut sum_density = 0.0f32;
        for i in 0..iterations {
            let x = (i % 100) as f32;
            let y = ((i / 100) % 100) as f32;
            let z = (i / 1000) as f32;
            sum_density += overhangs.sample_density(
                x,
                y,
                z,
                25.0,
                omnisia::worldgen::biome::BiomeType::Mountains,
            );
        }
        let elapsed = start.elapsed();
        let ns_per_point = elapsed.as_nanos() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 16] 3D Overhang & Feature Evaluation (100k points): {:.2} ns/point (Total: {:?}, sum density: {:.1})",
            ns_per_point, elapsed, sum_density
        );
    }

    // 17. Benchmark Phase 6 Procedural Chunk Generation (3D Features + Canonical Vegetation Stamping)
    {
        let start = Instant::now();
        let iterations = 200;
        for i in 0..iterations {
            let coord = IVec3::new(i % 10, 0, i / 10);
            let _ = worldgen.generate_chunk(coord, &registry);
        }
        let elapsed = start.elapsed();
        let ms_per_chunk = elapsed.as_secs_f64() * 1000.0 / iterations as f64;
        println!(
            "[BENCHMARK 17] Phase 6 Procedural Chunk Generation (Vegetation + 3D): {:.3} ms/chunk ({:.1} chunks/sec)",
            ms_per_chunk,
            1000.0 / ms_per_chunk
        );
    }

    // 18. Benchmark 100 Procedural Chunks Parallel Generation (Rayon)
    {
        use rayon::prelude::*;
        let start = Instant::now();
        let coords: Vec<IVec3> = (0..100).map(|i| IVec3::new(i % 10, 0, i / 10)).collect();
        let chunks: Vec<Chunk> = coords
            .par_iter()
            .map(|&c| worldgen.generate_chunk(c, &registry))
            .collect();
        let elapsed = start.elapsed();
        let total_voxels: usize = chunks.iter().map(|c| c.non_air_count as usize).sum();
        println!(
            "[BENCHMARK 18] 100 Procedural Chunks Parallel Generation (Rayon): {:?} ({:.2} ms total, Total Solid Voxels: {})",
            elapsed,
            elapsed.as_secs_f64() * 1000.0,
            total_voxels
        );
    }

    // 19. Benchmark Frustum Extraction & 1,000 Chunk AABB Intersections
    {
        let camera = Camera::new(Vec3::new(0.0, 30.0, 0.0), -90.0, -10.0);
        let start = Instant::now();
        let frustum = camera.extract_frustum(16.0 / 9.0);
        let iterations = 100_000;
        let mut visible_count = 0;
        for i in 0..iterations {
            let coord = IVec3::new((i % 40) - 20, (i / 40) % 5, (i / 200) - 20);
            if frustum.intersects_chunk(coord) {
                visible_count += 1;
            }
        }
        let elapsed = start.elapsed();
        let ns_per_cull = elapsed.as_nanos() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 19] Frustum Culling Intersection: {:.2} ns/chunk (Total: {:?}, visible: {}/{})",
            ns_per_cull, elapsed, visible_count, iterations
        );
    }

    // 20. Benchmark Event-Driven Localized Structural Connectivity (Gate A)
    {
        use omnisia::streaming::store::ChunkStore;
        use omnisia::structure::anchor::AnchorPolicy;
        use omnisia::structure::connectivity::{check_structural_connectivity, ConnectivityConfig};

        let anchor_policy = AnchorPolicy::from_registries(&registry, &resolved.blocks);
        let mut store = ChunkStore::new();
        let mut chunk = Chunk::new(IVec3::ZERO);
        let stone_id = registry
            .resolve_material_id(&omnisia::modding::resource_id::ResourceId::core("stone").unwrap())
            .unwrap();
        let wood_id = registry
            .resolve_material_id(
                &omnisia::modding::resource_id::ResourceId::core("wood_oak").unwrap(),
            )
            .unwrap();

        // Fondasi stone anchor di dasar (y = 0)
        for z in 0..32 {
            for x in 0..32 {
                chunk.set_voxel(x, 0, z, VoxelBlock::new(stone_id));
            }
        }
        // Tiang kayu 15 voxel ke atas
        for y in 1..=15 {
            chunk.set_voxel(16, y, 16, VoxelBlock::new(wood_id));
        }
        store.insert(chunk);

        let config = ConnectivityConfig::default();
        let iterations = 10_000;
        let mut inspected_total = 0;
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = check_structural_connectivity(
                IVec3::new(16, 15, 16),
                &store,
                &anchor_policy,
                &config,
                Some(&mut inspected_total),
            );
        }
        let elapsed = start.elapsed();
        let us_per_check = elapsed.as_micros() as f64 / iterations as f64;
        let avg_voxels = inspected_total as f64 / iterations as f64;
        println!(
            "[BENCHMARK 20] Localized Structural Connectivity: {:.2} µs/check (Avg voxels scanned: {:.1}, Total: {:?})",
            us_per_check, avg_voxels, elapsed
        );
    }

    // 21. Benchmark Detached Aggregate Extraction Throughput (Gate A)
    {
        use omnisia::structure::aggregate::DetachedAggregate;

        let wood_id = registry
            .resolve_material_id(
                &omnisia::modding::resource_id::ResourceId::core("wood_oak").unwrap(),
            )
            .unwrap();
        // Buat komponen 125 voxel (5x5x5)
        let mut voxels = Vec::new();
        for z in 0..5 {
            for y in 0..5 {
                for x in 0..5 {
                    voxels.push((IVec3::new(x, y, z), VoxelBlock::new(wood_id)));
                }
            }
        }

        let iterations = 20_000;
        let start = Instant::now();
        for i in 0..iterations {
            let agg = DetachedAggregate::from_world_voxels(i as u64, &voxels).unwrap();
            std::hint::black_box(agg);
        }
        let elapsed = start.elapsed();
        let us_per_extraction = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 21] Detached Aggregate Extraction (125 voxels): {:.2} µs/op ({:.1} extractions/sec)",
            us_per_extraction, 1_000_000.0 / us_per_extraction
        );
    }

    // ========================================================================
    // BENCHMARK 22: DynamicBody 30 Hz Physics Tick Throughput
    // ========================================================================
    {
        use omnisia::physics::PhysicsRuntime;
        use omnisia::streaming::store::ChunkStore;
        use omnisia::structure::aggregate::DetachedAggregate;

        let mut store = ChunkStore::new();
        // Muat chunk 3x3 horizontal
        for cz in -1..=1 {
            for cx in -1..=1 {
                let mut chunk = Chunk::new(IVec3::new(cx, 0, cz));
                for x in 0..32 {
                    for z in 0..32 {
                        chunk.set_voxel(x, 0, z, VoxelBlock::new(MaterialId::STONE));
                    }
                }
                store.insert(chunk);
            }
        }

        let mut runtime = PhysicsRuntime::default();
        let body_count = 100;

        // Buat 100 DynamicBody dengan aggregate 8 voxel (2x2x2)
        for i in 0..body_count {
            let mut voxels = Vec::new();
            let bx = (i % 10) * 3 - 15;
            let bz = (i / 10) * 3 - 15;
            for dy in 0..2 {
                for dz in 0..2 {
                    for dx in 0..2 {
                        voxels.push((
                            IVec3::new(bx + dx, 15 + dy, bz + dz),
                            VoxelBlock::new(MaterialId::DIRT),
                        ));
                    }
                }
            }
            let agg = DetachedAggregate::from_world_voxels(i as u64, &voxels).unwrap();
            runtime.spawn_from_detached_aggregate(agg);
        }

        let ticks = 500;
        let start = Instant::now();
        for _ in 0..ticks {
            runtime.tick(1.0 / 30.0, &store);
        }
        let elapsed = start.elapsed();
        let us_per_tick = elapsed.as_micros() as f64 / ticks as f64;
        let us_per_body_step = us_per_tick / body_count as f64;

        println!(
            "[BENCHMARK 22] DynamicBody 30 Hz Physics Tick (100 bodies): {:.2} µs/tick ({:.2} µs/body-step, {:.1} ticks/sec)",
            us_per_tick, us_per_body_step, 1_000_000.0 / us_per_tick
        );
    }

    // ========================================================================
    // BENCHMARK 23: Two-Phase Reintegration Throughput
    // ========================================================================
    {
        use omnisia::physics::{DynamicBodyState, PhysicsRuntime};
        use omnisia::streaming::store::ChunkStore;
        use omnisia::structure::aggregate::DetachedAggregate;

        let iterations = 10_000;
        let start = Instant::now();

        for i in 0..iterations {
            let mut store = ChunkStore::new();
            store.insert(Chunk::new(IVec3::ZERO));
            // Dasar lantai
            store.set_voxel_world(IVec3::new(10, 0, 10), VoxelBlock::new(MaterialId::STONE));

            let mut runtime = PhysicsRuntime::default();
            let voxels = vec![
                (IVec3::new(10, 1, 10), VoxelBlock::new(MaterialId::DIRT)),
                (IVec3::new(10, 2, 10), VoxelBlock::new(MaterialId::DIRT)),
            ];
            let agg = DetachedAggregate::from_world_voxels(i as u64, &voxels).unwrap();
            let body_id = runtime.spawn_from_detached_aggregate(agg);
            runtime
                .get_body_mut(body_id)
                .unwrap()
                .set_state(DynamicBodyState::Settled);

            let reintegrated = runtime.process_settled_reintegration(&mut store);
            std::hint::black_box(reintegrated);
        }

        let elapsed = start.elapsed();
        let us_per_reintegration = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 23] Two-Phase Dynamic Reintegration (Prepare+Commit): {:.2} µs/op ({:.1} ops/sec)",
            us_per_reintegration, 1_000_000.0 / us_per_reintegration
        );
    }

    // ========================================================================
    // BENCHMARK 24: Player Fixed 30Hz Simulation Tick
    // ========================================================================
    {
        use omnisia::player::{PlayerController, PlayerInput};
        use omnisia::streaming::store::ChunkStore;

        let mut store = ChunkStore::new();
        let mut chunk = Chunk::new(IVec3::ZERO);
        for vx in 0..32 {
            for vz in 0..32 {
                chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
        store.insert(chunk);

        let mut player = PlayerController::new(Vec3::new(8.0, 0.5, 8.0));
        player.state.grounded = true;
        player.set_input(PlayerInput::from_raw(
            true, false, false, false, true, false, false,
        ));

        let iterations = 100_000;
        let start = Instant::now();

        for _ in 0..iterations {
            player.step_simulation(1.0 / 30.0, &store, 0.0);
            std::hint::black_box(&player.state);
        }

        let elapsed = start.elapsed();
        let us_per_tick = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 24] Player Fixed 30Hz Simulation Tick: {:.3} µs/tick ({:.1} ticks/sec)",
            us_per_tick,
            1_000_000.0 / us_per_tick
        );
    }

    // ========================================================================
    // BENCHMARK 25: Player Swept Capsule Collision Query
    // ========================================================================
    {
        use omnisia::player::{resolve_swept_step, Capsule};
        use omnisia::streaming::store::ChunkStore;

        let mut store = ChunkStore::new();
        let mut chunk = Chunk::new(IVec3::ZERO);
        // Buat dinding vertikal di x = 16 (vx = 16)
        for vy in 0..10 {
            for vz in 0..32 {
                chunk.set_voxel(16, vy, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
        store.insert(chunk);

        let mut capsule = Capsule::new(Vec3::new(10.0, 0.5, 10.0), 0.30, 1.8);
        let mut velocity = Vec3::new(15.0, -9.81, 5.0);
        let delta = velocity * (1.0 / 30.0);

        let iterations = 100_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let stats = resolve_swept_step(&mut capsule, &mut velocity, delta, &store);
            std::hint::black_box(stats);
        }

        let elapsed = start.elapsed();
        let us_per_query = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 25] Player Swept Capsule Collision Query: {:.3} µs/query ({:.1} queries/sec)",
            us_per_query,
            1_000_000.0 / us_per_query
        );
    }

    // ========================================================================
    // BENCHMARK 26: Player Update in Populated DynamicBody Scene (8C.2 & 8C.4)
    // ========================================================================
    {
        use omnisia::player::PlayerController;
        use omnisia::structure::aggregate::DetachedAggregate;
        use omnisia::world::World;

        let mut world = World::with_seed(WorldSeed(42));
        let mut chunk = Chunk::new(IVec3::ZERO);
        for vx in 0..32 {
            for vz in 0..32 {
                chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
        world.store.insert(chunk);

        // Spawn 50 dynamic bodies in the vicinity
        for i in 0..50 {
            let voxels = vec![(
                IVec3::new((i % 16) * 2, 2 + (i / 16), 4),
                VoxelBlock::new(MaterialId::STONE),
            )];
            let agg = DetachedAggregate::from_world_voxels(100 + i as u64, &voxels).unwrap();
            world.physics.spawn_from_detached_aggregate(agg);
        }

        let mut player = PlayerController::new(Vec3::new(4.0, 0.5, 4.0));
        let iterations = 10_000;
        let start = Instant::now();

        for _ in 0..iterations {
            world.update_player(&mut player, 1.0 / 30.0, 0.0);
            std::hint::black_box(&player.state);
        }

        let elapsed = start.elapsed();
        let us_per_tick = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 26] Player Update with 50 DynamicBodies: {:.3} µs/tick ({:.1} ticks/sec)",
            us_per_tick,
            1_000_000.0 / us_per_tick
        );
    }

    // ========================================================================
    // BENCHMARK 27: Structural Mutation Event Dispatch Under Active Simulation (8C.4)
    // ========================================================================
    {
        use omnisia::world::World;

        let mut world = World::with_seed(WorldSeed(42));
        let mut chunk = Chunk::new(IVec3::ZERO);
        for vx in 0..16 {
            for vz in 0..16 {
                chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
        world.store.insert(chunk);

        let iterations = 2_000;
        let start = Instant::now();

        for i in 0..iterations {
            // Tempatkan lalu hancurkan balok
            let pos = IVec3::new(5, 1, 5);
            world.set_voxel_world(pos, VoxelBlock::new(MaterialId::STONE));
            let detached = world.set_voxel_world(pos, VoxelBlock::AIR);
            std::hint::black_box(detached);
            std::hint::black_box(i);
        }

        let elapsed = start.elapsed();
        let us_per_mutation = elapsed.as_micros() as f64 / (iterations * 2) as f64;
        println!(
            "[BENCHMARK 27] Structural Mutation Event Dispatch: {:.3} µs/mutation ({:.1} mutations/sec)",
            us_per_mutation,
            1_000_000.0 / us_per_mutation
        );
    }

    // ========================================================================
    // BENCHMARK 28: Settled DynamicBody Two-Phase Reintegration (8C.6)
    // ========================================================================
    {
        use omnisia::structure::aggregate::DetachedAggregate;
        use omnisia::world::World;

        let mut world = World::with_seed(WorldSeed(42));
        let chunk = Chunk::new(IVec3::ZERO);
        world.store.insert(chunk);

        let iterations = 10_000;
        let start = Instant::now();

        for i in 0..iterations {
            let voxels = vec![(IVec3::new(4, 5, 4), VoxelBlock::new(MaterialId::STONE))];
            let agg = DetachedAggregate::from_world_voxels(1000 + i as u64, &voxels).unwrap();
            let body_id = world.physics.spawn_from_detached_aggregate(agg);
            world.physics.get_body_mut(body_id).unwrap().state =
                omnisia::physics::DynamicBodyState::Settled;

            let res = world
                .physics
                .process_settled_reintegration(&mut world.store);
            std::hint::black_box(res);
            // Bersihkan kembali untuk iterasi berikutnya
            world
                .store
                .set_voxel_world(IVec3::new(4, 5, 4), VoxelBlock::AIR);
        }

        let elapsed = start.elapsed();
        let us_per_reintegration = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 28] Two-Phase Reintegration Commit: {:.3} µs/op ({:.1} ops/sec)",
            us_per_reintegration,
            1_000_000.0 / us_per_reintegration
        );
    }

    // ========================================================================
    // BENCHMARK 29: World Ownership Audit Routine (8C.5)
    // ========================================================================
    {
        use omnisia::world::World;

        let mut world = World::with_seed(WorldSeed(42));
        for cx in 0..3 {
            for cz in 0..3 {
                let mut chunk = Chunk::new(IVec3::new(cx, 0, cz));
                for vx in 0..32 {
                    for vz in 0..32 {
                        chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
                    }
                }
                world.store.insert(chunk);
            }
        }

        let iterations = 5_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let report = world.audit_world_ownership();
            std::hint::black_box(report);
        }

        let elapsed = start.elapsed();
        let us_per_audit = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 29] World Ownership Audit: {:.3} µs/audit ({:.1} audits/sec)",
            us_per_audit,
            1_000_000.0 / us_per_audit
        );
    }

    // ========================================================================
    // BENCHMARK 30: Auto-Step Traversal Solver (8D.1)
    // ========================================================================
    {
        use omnisia::player::collider::Capsule;
        use omnisia::player::collision::resolve_swept_step_with_stepup;

        let mut store = omnisia::streaming::store::ChunkStore::new();
        let mut chunk = Chunk::new(IVec3::ZERO);
        for vx in 0..32 {
            for vz in 0..32 {
                chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
        // Undakan 1-voxel di x >= 8, y = 1
        for vx in 8..16 {
            for vz in 0..32 {
                chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
        store.insert(chunk);

        let iterations = 20_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let mut capsule = Capsule::new(Vec3::new(3.7, 0.5, 5.0), 1.8, 0.30);
            let mut velocity = Vec3::new(5.0, 0.0, 0.0);
            let delta = velocity * (1.0 / 30.0);
            let stats = resolve_swept_step_with_stepup(
                &mut capsule,
                &mut velocity,
                delta,
                0.55,
                true,
                &store,
                None,
            );
            std::hint::black_box(stats);
            std::hint::black_box(capsule);
        }

        let elapsed = start.elapsed();
        let us_per_step = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 30] Auto-Step Traversal Solver: {:.3} µs/step ({:.1} steps/sec)",
            us_per_step,
            1_000_000.0 / us_per_step
        );
    }

    // ========================================================================
    // BENCHMARK 31: Airborne Glide Physics Simulation (8D.2)
    // ========================================================================
    {
        use omnisia::player::{PlayerController, PlayerInput};

        let mut store = omnisia::streaming::store::ChunkStore::new();
        for cy in 0..=3 {
            let mut chunk = Chunk::new(IVec3::new(0, cy, 0));
            if cy == 0 {
                for vx in 0..32 {
                    for vz in 0..32 {
                        chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
                    }
                }
            }
            store.insert(chunk);
        }

        let mut player = PlayerController::new(Vec3::new(4.0, 50.0, 4.0));
        player.state.grounded = false;
        player.state.airborne_origin = omnisia::player::AirborneOrigin::SprintJump;
        player.state.velocity.y = -1.0;
        let input = PlayerInput {
            move_forward: 1.0,
            sprint: true,
            ..Default::default()
        };
        player.set_input(input);

        let iterations = 20_000;
        let start = Instant::now();

        for _ in 0..iterations {
            player.step_simulation(1.0 / 30.0, &store, 0.0);
            // Jaga ketinggian agar tetap melayang di udara selama iterasi
            if player.state.position.y < 20.0 {
                player.state.position.y = 50.0;
                player.state.grounded = false;
                player.state.airborne_origin = omnisia::player::AirborneOrigin::SprintJump;
                player.state.velocity.y = -1.0;
            }
            std::hint::black_box(player.state.velocity);
        }

        let elapsed = start.elapsed();
        let us_per_tick = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 31] Airborne Glide Simulation: {:.3} µs/tick ({:.1} ticks/sec)",
            us_per_tick,
            1_000_000.0 / us_per_tick
        );
    }

    // ========================================================================
    // BENCHMARK 32 (BM-G1): Ground Detection Flat Ground (8D.4)
    // Target: < 2.0 µs/query
    // ========================================================================
    {
        use omnisia::player::collision::check_ground_support;
        use omnisia::streaming::store::ChunkStore;

        let mut store = ChunkStore::new();
        let mut chunk = Chunk::new(IVec3::ZERO);
        for vx in 0..32 {
            for vz in 0..32 {
                chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
        store.insert(chunk);

        let feet_pos = Vec3::new(4.25, 0.5, 4.25);
        let iterations = 50_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
            std::hint::black_box(res);
        }

        let elapsed = start.elapsed();
        let us_per_query = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 32] Ground Detection Flat Ground (BM-G1): {:.3} µs/query ({:.1} queries/sec)",
            us_per_query,
            1_000_000.0 / us_per_query
        );
    }

    // ========================================================================
    // BENCHMARK 33 (BM-G2): Ground Detection Edge Contact (8D.4)
    // Target: < 5.0 µs/query
    // ========================================================================
    {
        use omnisia::player::collision::check_ground_support;
        use omnisia::streaming::store::ChunkStore;

        let mut store = ChunkStore::new();
        let mut chunk = Chunk::new(IVec3::ZERO);
        chunk.set_voxel(4, 1, 4, VoxelBlock::new(MaterialId::STONE));
        store.insert(chunk);

        // Posisi menjorok di tepi (d = 0.15m)
        let feet_pos = Vec3::new(2.65, 0.96, 2.25);
        let iterations = 50_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
            std::hint::black_box(res);
        }

        let elapsed = start.elapsed();
        let us_per_query = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 33] Ground Detection Edge Contact (BM-G2): {:.3} µs/query ({:.1} queries/sec)",
            us_per_query,
            1_000_000.0 / us_per_query
        );
    }

    // ========================================================================
    // BENCHMARK 34 (BM-G3): Ground Detection Uneven Terrain / Slope (8D.4)
    // Target: < 5.0 µs/query
    // ========================================================================
    {
        use omnisia::player::collision::check_ground_support;
        use omnisia::streaming::store::ChunkStore;

        let mut store = ChunkStore::new();
        let mut chunk = Chunk::new(IVec3::ZERO);
        for vx in 0..10 {
            for vz in 0..10 {
                chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
                if vx >= 5 {
                    chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
                }
            }
        }
        store.insert(chunk);

        // Posisi di lereng undakan
        let feet_pos = Vec3::new(2.55, 1.00, 2.25);
        let iterations = 50_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
            std::hint::black_box(res);
        }

        let elapsed = start.elapsed();
        let us_per_query = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 34] Ground Detection Uneven/Slope (BM-G3): {:.3} µs/query ({:.1} queries/sec)",
            us_per_query,
            1_000_000.0 / us_per_query
        );
    }

    // ========================================================================
    // BENCHMARK 35 (BM-G4): Ground Detection DynamicBody Contact (8D.4)
    // Target: < 10.0 µs/query
    // ========================================================================
    {
        use omnisia::physics::{DynamicBodyState, PhysicsRuntime};
        use omnisia::player::collision::check_ground_support_with_physics;
        use omnisia::streaming::store::ChunkStore;
        use omnisia::structure::aggregate::DetachedAggregate;

        let mut store = ChunkStore::new();
        let mut chunk = Chunk::new(IVec3::ZERO);
        for vx in 0..32 {
            for vz in 0..32 {
                chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
        store.insert(chunk);

        let mut physics = PhysicsRuntime::default();
        let mut voxels = Vec::new();
        for vx in 0..4 {
            for vz in 0..4 {
                voxels.push((IVec3::new(vx, 1, vz), VoxelBlock::new(MaterialId::STONE)));
            }
        }
        let agg = DetachedAggregate::from_world_voxels(999, &voxels).unwrap();
        let body_id = physics.spawn_from_detached_aggregate(agg);
        let body_mut = physics.get_body_mut(body_id).unwrap();
        body_mut.state = DynamicBodyState::Settled;
        body_mut.is_grounded = true;

        let feet_pos = Vec3::new(1.0, 1.0, 1.0);
        let iterations = 50_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let res =
                check_ground_support_with_physics(feet_pos, 0.30, 0.05, &store, Some(&physics));
            std::hint::black_box(res);
        }

        let elapsed = start.elapsed();
        let us_per_query = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 35] Ground Detection DynamicBody Contact (BM-G4): {:.3} µs/query ({:.1} queries/sec)",
            us_per_query,
            1_000_000.0 / us_per_query
        );
    }

    // ========================================================================
    // BENCHMARK 36: Broadphase Update & AABB Query (Phase 9.1)
    // 100 Separated Bodies across Positive and Negative Space
    // ========================================================================
    {
        use omnisia::physics::{
            Aabb, BodyType, BroadphaseProxy, RigidBodyId, SpatialHashBroadphase,
        };

        let mut broadphase = SpatialHashBroadphase::new(4.0);
        let count = 100;
        for i in 0..count {
            let base_x = (i as f32 - 50.0) * 10.0;
            let aabb = Aabb::try_new(
                Vec3::new(base_x, -10.0, -10.0),
                Vec3::new(base_x + 2.0, -8.0, -8.0),
            )
            .unwrap();
            broadphase
                .insert(BroadphaseProxy::new(
                    RigidBodyId(i as u64 + 1),
                    BodyType::Dynamic,
                    aabb,
                ))
                .unwrap();
        }

        let iterations = 10_000;
        let start = Instant::now();

        for iter in 0..iterations {
            let id = RigidBodyId((iter % count) as u64 + 1);
            let base_x = ((iter % count) as f32 - 50.0) * 10.0 + 0.1;
            let new_aabb = Aabb::try_new(
                Vec3::new(base_x, -10.0, -10.0),
                Vec3::new(base_x + 2.0, -8.0, -8.0),
            )
            .unwrap();
            let _ = broadphase.update(id, new_aabb);

            let query_box = Aabb::try_new(
                Vec3::new(base_x - 1.0, -11.0, -11.0),
                Vec3::new(base_x + 3.0, -7.0, -7.0),
            )
            .unwrap();
            let hits = broadphase.query_aabb(&query_box);
            std::hint::black_box(hits);
        }

        let elapsed = start.elapsed();
        let us_per_op = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 36] Broadphase Update & Query (100 separated bodies, +/- coords): {:.3} µs/op ({:.1} ops/sec)",
            us_per_op,
            1_000_000.0 / us_per_op
        );
    }

    // ========================================================================
    // BENCHMARK 37: Broadphase Candidate Pair Generation (Phase 9.1)
    // Dense Cluster of 200 Bodies & Medium 500 Bodies
    // ========================================================================
    {
        use omnisia::physics::{
            Aabb, BodyType, BroadphaseProxy, RigidBodyId, SpatialHashBroadphase,
        };

        let mut broadphase = SpatialHashBroadphase::new(4.0);
        let count = 200;
        for i in 0..count {
            let x = ((i * 7) % 20) as f32 * 0.8;
            let y = ((i * 11) % 15) as f32 * 0.8;
            let z = ((i * 13) % 20) as f32 * 0.8;
            let aabb =
                Aabb::try_new(Vec3::new(x, y, z), Vec3::new(x + 2.0, y + 2.0, z + 2.0)).unwrap();
            let btype = if i % 5 == 0 {
                BodyType::Static
            } else {
                BodyType::Dynamic
            };
            broadphase
                .insert(BroadphaseProxy::new(RigidBodyId(i as u64 + 1), btype, aabb))
                .unwrap();
        }

        let iterations = 1_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let pairs = broadphase.generate_candidate_pairs();
            std::hint::black_box(pairs);
        }

        let elapsed = start.elapsed();
        let us_per_gen = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 37] Broadphase Candidate Pair Generation (200 dense bodies): {:.3} µs/run ({:.1} runs/sec)",
            us_per_gen,
            1_000_000.0 / us_per_gen
        );
    }

    // ========================================================================
    // BENCHMARK 38: Broadphase Large Multi-Cell Spanning AABBs (Phase 9.1)
    // 50 Large Bodies (12m x 12m) Spanning 3x3x3 Cells with Deduplication
    // ========================================================================
    {
        use omnisia::physics::{
            Aabb, BodyType, BroadphaseProxy, RigidBodyId, SpatialHashBroadphase,
        };

        let mut broadphase = SpatialHashBroadphase::new(4.0);
        let count = 50;
        for i in 0..count {
            let x = (i as f32 * 4.0) % 40.0;
            let y = (i as f32 * 2.0) % 20.0;
            let z = (i as f32 * 3.0) % 40.0;
            // AABB 12m melintasi 3 hingga 4 sel di setiap sumbu
            let aabb =
                Aabb::try_new(Vec3::new(x, y, z), Vec3::new(x + 12.0, y + 12.0, z + 12.0)).unwrap();
            broadphase
                .insert(BroadphaseProxy::new(
                    RigidBodyId(i as u64 + 1),
                    BodyType::Dynamic,
                    aabb,
                ))
                .unwrap();
        }

        let iterations = 1_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let pairs = broadphase.generate_candidate_pairs();
            std::hint::black_box(pairs);
        }

        let elapsed = start.elapsed();
        let us_per_gen = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 38] Broadphase Large Multi-Cell Spanning & Deduplication (50 bodies): {:.3} µs/run ({:.1} runs/sec)",
            us_per_gen,
            1_000_000.0 / us_per_gen
        );
    }

    println!("============================================================");
    println!("             BENCHMARK SUITE COMPLETE                       ");
    println!("============================================================");
}
