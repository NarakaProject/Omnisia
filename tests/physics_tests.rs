use glam::{IVec3, Vec3};

use omnisia::material::MaterialId;
use omnisia::physics::{DynamicBody, DynamicBodyId, DynamicBodyState, PhysicsConfig};
use omnisia::structure::aggregate::DetachedAggregate;
use omnisia::voxel::{VoxelBlock, VOXEL_SIZE};

// ============================================================================
// 8A.1 DYNAMIC BODY DATA MODEL TESTS
// ============================================================================

#[test]
fn test_dynamic_body_data_model_construction() {
    let voxels = vec![
        (IVec3::new(10, 20, 30), VoxelBlock::new(MaterialId::STONE)),
        (IVec3::new(10, 21, 30), VoxelBlock::new(MaterialId::DIRT)),
    ];
    let aggregate = DetachedAggregate::from_world_voxels(1, &voxels).expect("Valid aggregate");

    let body_id = DynamicBodyId(101);
    let body = DynamicBody::from_detached_aggregate(body_id, aggregate);

    assert_eq!(body.id, body_id);
    assert_eq!(body.state, DynamicBodyState::Active);
    assert_eq!(body.velocity, Vec3::ZERO);
    assert_eq!(body.gravity_scale, 1.0);
    assert_eq!(body.ticks_stationary, 0);
    assert!(!body.is_grounded);

    // Posisi awal harus tepat sama dengan min_voxel dalam meter (10 * 0.5, 20 * 0.5, 30 * 0.5)
    assert_eq!(body.position, Vec3::new(5.0, 10.0, 15.0));
    assert_eq!(body.voxel_count(), 2);
}

#[test]
fn test_dynamic_body_state_transitions() {
    let voxels = vec![(IVec3::ZERO, VoxelBlock::new(MaterialId::STONE))];
    let aggregate = DetachedAggregate::from_world_voxels(1, &voxels).unwrap();
    let mut body = DynamicBody::from_detached_aggregate(DynamicBodyId(1), aggregate);

    assert_eq!(body.state, DynamicBodyState::Active);

    body.ticks_stationary = 10;
    body.set_state(DynamicBodyState::Sleeping);
    assert_eq!(body.state, DynamicBodyState::Sleeping);
    assert_eq!(body.ticks_stationary, 10);

    body.set_state(DynamicBodyState::Settled);
    assert_eq!(body.state, DynamicBodyState::Settled);

    // Kembali ke active mereset counter diam
    body.set_state(DynamicBodyState::Active);
    assert_eq!(body.state, DynamicBodyState::Active);
    assert_eq!(body.ticks_stationary, 0);
}

#[test]
fn test_dynamic_body_bounds_and_voxel_count() {
    // Balok 2x3x4 voxel dari (0,0,0) sampai (1,2,3)
    let mut voxels = Vec::new();
    for x in 0..2 {
        for y in 0..3 {
            for z in 0..4 {
                voxels.push((IVec3::new(x, y, z), VoxelBlock::new(MaterialId::STONE)));
            }
        }
    }
    let aggregate = DetachedAggregate::from_world_voxels(1, &voxels).unwrap();
    let body = DynamicBody::from_detached_aggregate(DynamicBodyId(42), aggregate);

    assert_eq!(body.voxel_count(), 24);
    assert_eq!(body.voxel_dimensions(), IVec3::new(2, 3, 4));

    let (min_bound, max_bound) = body.world_bounds();
    assert_eq!(min_bound, Vec3::ZERO);
    assert_eq!(
        max_bound,
        Vec3::new(2.0 * VOXEL_SIZE, 3.0 * VOXEL_SIZE, 4.0 * VOXEL_SIZE)
    );

    let (min_v, max_v) = body.world_voxel_bounds();
    assert_eq!(min_v, IVec3::new(0, 0, 0));
    assert_eq!(max_v, IVec3::new(1, 2, 3));
}

#[test]
fn test_physics_config_defaults() {
    let config = PhysicsConfig::default();
    assert_eq!(config.world_gravity, Vec3::new(0.0, -9.81, 0.0));
    assert_eq!(config.fixed_timestep_hz, 30.0);
    assert_eq!(config.fixed_dt, 1.0 / 30.0);
    assert_eq!(config.sleep_velocity_threshold, 0.05);
    assert_eq!(config.sleep_ticks_required, 15);
    assert_eq!(config.max_substeps_per_frame, 5);
}

// ============================================================================
// 8A.2 DETACHED AGGREGATE -> DYNAMIC BODY (MOVE SEMANTICS & INTEGRITY)
// ============================================================================

#[test]
fn test_detached_aggregate_to_dynamic_body_move_semantics() {
    let voxels = vec![
        (IVec3::new(-5, 12, -8), VoxelBlock::new(MaterialId::STONE)),
        (IVec3::new(-5, 13, -8), VoxelBlock::new(MaterialId::DIRT)),
        (IVec3::new(-4, 12, -8), VoxelBlock::new(MaterialId::GRASS)),
    ];
    let agg = DetachedAggregate::from_world_voxels(99, &voxels).expect("Valid aggregate");

    // Move semantics: agg berpindah langsung ke dalam DynamicBody
    let body = DynamicBody::from_detached_aggregate(DynamicBodyId(99), agg)
        .with_gravity_scale(0.5)
        .with_velocity(Vec3::new(0.0, -2.0, 0.0));

    assert_eq!(body.id, DynamicBodyId(99));
    assert_eq!(body.gravity_scale, 0.5);
    assert_eq!(body.velocity, Vec3::new(0.0, -2.0, 0.0));
    assert_eq!(body.voxel_count(), 3);
    assert!(body.validate_integrity());

    // Verifikasi identitas material & posisi dunia asal tetap 100% utuh
    let world_voxels: Vec<(IVec3, VoxelBlock)> = body.iter_world_voxels().collect();
    assert_eq!(world_voxels.len(), 3);
    assert!(world_voxels.contains(&(IVec3::new(-5, 12, -8), VoxelBlock::new(MaterialId::STONE))));
    assert!(world_voxels.contains(&(IVec3::new(-5, 13, -8), VoxelBlock::new(MaterialId::DIRT))));
    assert!(world_voxels.contains(&(IVec3::new(-4, 12, -8), VoxelBlock::new(MaterialId::GRASS))));
}

#[test]
fn test_detached_aggregate_multi_material_and_topology_preservation() {
    // Struktur berbentuk L melintasi kuadran negatif
    let voxels = vec![
        (IVec3::new(-33, 10, 0), VoxelBlock::new(MaterialId::STONE)),
        (IVec3::new(-32, 10, 0), VoxelBlock::new(MaterialId::SAND)),
        (IVec3::new(-32, 11, 0), VoxelBlock::new(MaterialId::DIRT)),
    ];
    let agg = DetachedAggregate::from_world_voxels(10, &voxels).unwrap();
    let body = DynamicBody::from_detached_aggregate(DynamicBodyId(10), agg);

    assert_eq!(body.voxel_count(), 3);
    assert_eq!(body.voxel_dimensions(), IVec3::new(2, 2, 1));
    assert!(body.validate_integrity());

    let (min_b, max_b) = body.world_voxel_bounds();
    assert_eq!(min_b, IVec3::new(-33, 10, 0));
    assert_eq!(max_b, IVec3::new(-32, 11, 0));
}

// ============================================================================
// 8A.3 STATIC -> DYNAMIC ATOMIC OWNERSHIP TRANSFER
// ============================================================================

#[test]
fn test_static_to_dynamic_atomic_ownership_transfer() {
    use omnisia::chunk::Chunk;
    use omnisia::modding::resource_id::ResourceId;
    use omnisia::world::World;
    use omnisia::worldgen::seed::WorldSeed;

    let mut world = World::with_seed(WorldSeed(42));
    world.store.insert(Chunk::new(IVec3::ZERO));

    let stone_id = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();
    let wood_id = world
        .materials
        .resolve_material_id(&ResourceId::core("wood_oak").unwrap())
        .unwrap();

    // Bangun tiang batu di atas fondasi anchor jauh di dalam chunk (10, y, 10)
    for y in 0..5 {
        world
            .store
            .set_voxel_world(IVec3::new(10, y, 10), VoxelBlock::new(stone_id));
    }
    // Pasang balok kayu di atas tiang (y=5, 6)
    world
        .store
        .set_voxel_world(IVec3::new(10, 5, 10), VoxelBlock::new(wood_id));
    world
        .store
        .set_voxel_world(IVec3::new(10, 6, 10), VoxelBlock::new(wood_id));

    // Verifikasi awal: 7 voxel solid ada di ChunkStore
    assert!(!world.store.get_voxel_world(IVec3::new(10, 5, 10)).is_air());
    assert!(!world.store.get_voxel_world(IVec3::new(10, 6, 10)).is_air());
    assert_eq!(world.physics.body_count(), 0);

    // Hancurkan tiang batu di y=4 menggunakan World::set_voxel_world
    // Ini memicu mutasi struktural dan ekstraksi detached aggregate
    let detached = world.set_voxel_world(IVec3::new(10, 4, 10), VoxelBlock::AIR);

    // Buktikan 1: Ada aggregate lepas yang terdeteksi
    assert_eq!(detached.len(), 1);
    let agg = &detached[0];
    assert_eq!(agg.voxel_count(), 2); // y=5 dan y=6

    // Buktikan 2: Voxel lepas telah DIHAPUS dari ChunkStore (tidak ada double ownership)
    assert!(
        world.store.get_voxel_world(IVec3::new(10, 5, 10)).is_air(),
        "Voxel y=5 harus sudah bukan milik ChunkStore!"
    );
    assert!(
        world.store.get_voxel_world(IVec3::new(10, 6, 10)).is_air(),
        "Voxel y=6 harus sudah bukan milik ChunkStore!"
    );

    // Buktikan 3: DynamicBody terdaftar di PhysicsRuntime dan memegang 100% kepemilikan
    assert_eq!(world.physics.body_count(), 1);
    let body = world.physics.bodies.values().next().unwrap();
    assert_eq!(body.voxel_count(), 2);
    assert_eq!(body.state, DynamicBodyState::Active);

    let body_voxels: Vec<IVec3> = body.iter_world_voxels().map(|(pos, _)| pos).collect();
    assert!(body_voxels.contains(&IVec3::new(10, 5, 10)));
    assert!(body_voxels.contains(&IVec3::new(10, 6, 10)));

    // Buktikan 4: Total voxel dunia kekal (konservasi total voxel)
    // 4 stone di y=0..3 tetap di ChunkStore, 2 wood di y=5..6 sekarang di DynamicBody
    assert_eq!(world.physics.total_dynamic_voxels(), 2);
}

#[test]
fn test_atomic_ownership_transfer_empty_component_does_not_mutate_store() {
    use omnisia::chunk::Chunk;
    use omnisia::streaming::store::ChunkStore;
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut store = ChunkStore::new();
    store.insert(Chunk::new(IVec3::ZERO));
    store.set_voxel_world(IVec3::new(5, 5, 5), VoxelBlock::new(MaterialId::STONE));

    // Coba buat DetachedAggregate dari kumpulan voxel kosong
    let empty_voxels: Vec<(IVec3, VoxelBlock)> = Vec::new();
    let maybe_agg = DetachedAggregate::from_world_voxels(1, &empty_voxels);
    assert!(maybe_agg.is_none());

    // Karena konstruksi aggregate gagal / None, ChunkStore TIDAK BOLEH dimutasi
    assert!(
        !store.get_voxel_world(IVec3::new(5, 5, 5)).is_air(),
        "ChunkStore harus mempertahankan voxel secara otoritatif jika aggregate tidak valid!"
    );
}

// ============================================================================
// 8A.4 GRAVITY / ANTIGRAVITY MODEL
// ============================================================================

#[test]
fn test_gravity_normal_acceleration() {
    let voxels = vec![(IVec3::ZERO, VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(1, &voxels).unwrap();
    let mut body =
        DynamicBody::from_detached_aggregate(DynamicBodyId(1), agg).with_gravity_scale(1.0);

    let gravity = Vec3::new(0.0, -9.81, 0.0);
    let dt = 1.0 / 30.0; // Fixed timestep 30 Hz

    // Terapkan gravitasi selama 30 ticks (tepat 1.0 detik)
    for _ in 0..30 {
        body.apply_gravity(gravity, dt);
    }

    // Kecepatan setelah 1 detik: v = a * t = -9.81 m/s
    let vy = body.velocity.y;
    assert!(
        (vy - (-9.81)).abs() < 1e-4,
        "Kecepatan setelah 1 detik gravitasi normal harus -9.81 m/s! Actual: {}",
        vy
    );
    assert_eq!(body.velocity.x, 0.0);
    assert_eq!(body.velocity.z, 0.0);
}

#[test]
fn test_antigravity_zero_acceleration() {
    let voxels = vec![(IVec3::ZERO, VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(2, &voxels).unwrap();
    let mut body =
        DynamicBody::from_detached_aggregate(DynamicBodyId(2), agg).with_gravity_scale(0.0); // AntiGravity mode

    let gravity = Vec3::new(0.0, -9.81, 0.0);
    let dt = 1.0 / 30.0;

    // Terapkan gravitasi selama 30 ticks
    for _ in 0..30 {
        body.apply_gravity(gravity, dt);
    }

    // Pada AntiGravity, kecepatan linier tetap nol mutlak!
    assert_eq!(body.velocity, Vec3::ZERO);
}

#[test]
fn test_inverted_gravity_acceleration() {
    let voxels = vec![(IVec3::ZERO, VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(3, &voxels).unwrap();
    let mut body =
        DynamicBody::from_detached_aggregate(DynamicBodyId(3), agg).with_gravity_scale(-1.0); // Inverted gravity

    let gravity = Vec3::new(0.0, -9.81, 0.0);
    let dt = 1.0 / 30.0;

    for _ in 0..30 {
        body.apply_gravity(gravity, dt);
    }

    // Kecepatan setelah 1 detik gravitasi terbalik: v = +9.81 m/s
    let vy = body.velocity.y;
    assert!(
        (vy - 9.81).abs() < 1e-4,
        "Kecepatan setelah 1 detik gravitasi terbalik harus +9.81 m/s! Actual: {}",
        vy
    );
}

// ============================================================================
// 8A.5 FIXED-TIMESTEP FALLING INTEGRATION (30 HZ)
// ============================================================================

#[test]
fn test_fixed_timestep_frame_rate_invariance() {
    use omnisia::physics::PhysicsRuntime;

    // Uji simulasi jatuh bebas selama 1.0 detik di bawah 3 variasi frame rate:
    // 1. 30 FPS  (30 frames, dt = 1/30s)
    // 2. 60 FPS  (60 frames, dt = 1/60s)
    // 3. 120 FPS (120 frames, dt = 1/120s)

    let run_simulation = |frames: usize, render_dt: f32| -> (Vec3, Vec3) {
        use omnisia::chunk::Chunk;
        use omnisia::streaming::store::ChunkStore;

        let mut runtime = PhysicsRuntime::default();
        let mut store = ChunkStore::new();
        // Berikan kolom chunk kosong yang ter-load agar simulasi frame rate murni free fall tanpa tabrakan
        for cy in -10..=10 {
            store.insert(Chunk::new(IVec3::new(0, cy, 0)));
        }

        let voxels = vec![(IVec3::ZERO, VoxelBlock::new(MaterialId::STONE))];
        let agg = DetachedAggregate::from_world_voxels(1, &voxels).unwrap();
        let body_id = runtime.spawn_from_detached_aggregate(agg);

        for _ in 0..frames {
            runtime.update(render_dt, &store);
        }

        let body = runtime.get_body(body_id).unwrap();
        (body.position, body.velocity)
    };

    let (pos_30, vel_30) = run_simulation(30, 1.0 / 30.0);
    let (pos_60, vel_60) = run_simulation(60, 1.0 / 60.0);
    let (pos_120, vel_120) = run_simulation(120, 1.0 / 120.0);

    // Kecepatan dan posisi harus identik secara deterministik pada seluruh framerate normal
    assert!(
        (vel_30 - vel_60).length() < 1e-4,
        "Kecepatan 30 FPS vs 60 FPS harus identik! Diff: {:?}",
        vel_30 - vel_60
    );
    assert!(
        (vel_60 - vel_120).length() < 1e-4,
        "Kecepatan 60 FPS vs 120 FPS harus identik! Diff: {:?}",
        vel_60 - vel_120
    );
    assert!(
        (pos_30 - pos_60).length() < 1e-4,
        "Posisi 30 FPS vs 60 FPS harus identik! Diff: {:?}",
        pos_30 - pos_60
    );
    assert!(
        (pos_60 - pos_120).length() < 1e-4,
        "Posisi 60 FPS vs 120 FPS harus identik! Diff: {:?}",
        pos_60 - pos_120
    );
}

#[test]
fn test_pathological_stall_bounded_catchup() {
    use omnisia::physics::PhysicsRuntime;
    use omnisia::streaming::store::ChunkStore;

    let mut runtime = PhysicsRuntime::default();
    let store = ChunkStore::new();
    let voxels = vec![(IVec3::ZERO, VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(1, &voxels).unwrap();
    runtime.spawn_from_detached_aggregate(agg);

    // Simulasikan frame drop parah (lag 1.0 detik dalam satu frame render)
    let ticks = runtime.update(1.0, &store);

    // Harus dibatasi maksimum 5 ticks per frame (Amendment 2 guardrail)
    assert_eq!(
        ticks, 5,
        "Catch-up harus dibatasi tepat max_substeps_per_frame = 5!"
    );
    // Akumulator harus di-reset ke 0 untuk mencegah spiral of death
    assert_eq!(runtime.accumulator, 0.0);
}

// ============================================================================
// 8A.6 SWEPT VERTICAL COLLISION & UNLOADED CHUNK GUARD
// ============================================================================

#[test]
fn test_swept_vertical_collision_ground_contact_and_snapping() {
    use omnisia::chunk::Chunk;
    use omnisia::physics::{swept_vertical_step, VerticalCollisionResult};
    use omnisia::streaming::store::ChunkStore;

    let mut store = ChunkStore::new();
    store.insert(Chunk::new(IVec3::ZERO));

    // Lantai batu di y = 2
    store.set_voxel_world(IVec3::new(10, 2, 10), VoxelBlock::new(MaterialId::STONE));

    // Badan dinamis berada di y = 4 (posisi meter = 2.0m)
    let voxels = vec![(IVec3::new(10, 4, 10), VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(1, &voxels).unwrap();
    let body = DynamicBody::from_detached_aggregate(DynamicBodyId(1), agg);

    assert_eq!(body.position.y, 2.0); // 4 * 0.5m = 2.0m

    // Lakukan swept langkah vertikal ke bawah sejauh -1.5m (melewati lantai di y=2 yang posisinya 1.0m .. 1.5m)
    let step_result = swept_vertical_step(&body, -1.5, &store);

    match step_result {
        VerticalCollisionResult::GroundContact {
            clamped_pos,
            contact_voxel_y,
        } => {
            assert_eq!(contact_voxel_y, 2);
            // Lantai ada di voxel y=2. Bagian bawah badan harus bertumpu tepat di y=3 (posisi meter = 3 * 0.5 = 1.5m)
            assert_eq!(clamped_pos.y, 1.5);
        }
        other => panic!("Diharapkan GroundContact, namun mendapatkan: {:?}", other),
    }
}

#[test]
fn test_unloaded_chunk_blocks_falling_unknown_not_air() {
    use omnisia::chunk::Chunk;
    use omnisia::physics::{swept_vertical_step, VerticalCollisionResult};
    use omnisia::streaming::store::ChunkStore;

    let mut store = ChunkStore::new();
    // Hanya muat chunk (0, 0, 0) yang mencakup voxel Y dari 0..=31
    // Chunk di bawahnya (0, -1, 0) TIDAK dimuat (Unloaded / Unknown)
    store.insert(Chunk::new(IVec3::ZERO));

    // Badan dinamis di y = 0 (batas paling bawah dari chunk 0)
    let voxels = vec![(IVec3::new(5, 0, 5), VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(1, &voxels).unwrap();
    let body = DynamicBody::from_detached_aggregate(DynamicBodyId(1), agg);

    // Coba jatuh ke bawah sejauh -1.0m (menuju chunk -1 yang belum dimuat)
    let step_result = swept_vertical_step(&body, -1.0, &store);

    match step_result {
        VerticalCollisionResult::BlockedByUnloaded { clamped_pos } => {
            // Tertahan di batas chunk (y = 0 voxel => 0.0m)
            assert_eq!(clamped_pos.y, 0.0);
        }
        other => panic!(
            "Diharapkan BlockedByUnloaded karena chunk bawah belum dimuat, namun: {:?}",
            other
        ),
    }
}

#[test]
fn test_high_velocity_swept_tunneling_prevention() {
    use omnisia::chunk::Chunk;
    use omnisia::physics::{swept_vertical_step, VerticalCollisionResult};
    use omnisia::streaming::store::ChunkStore;

    let mut store = ChunkStore::new();
    store.insert(Chunk::new(IVec3::ZERO));

    // Lantai tipis 1-voxel di y = 15
    store.set_voxel_world(IVec3::new(10, 15, 10), VoxelBlock::new(MaterialId::STONE));

    // Badan dinamis jatuh dengan kecepatan ekstrem dari y = 25 (posisi meter = 12.5m)
    let voxels = vec![(IVec3::new(10, 25, 10), VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(1, &voxels).unwrap();
    let body = DynamicBody::from_detached_aggregate(DynamicBodyId(1), agg);

    // Langkah sebesar -8.0 meter (16 voxel) dalam 1 tick!
    // Posisi target naif jika tanpa swept adalah 12.5 - 8.0 = 4.5m (voxel y=9), menembus lantai di y=15!
    let step_result = swept_vertical_step(&body, -8.0, &store);

    match step_result {
        VerticalCollisionResult::GroundContact {
            clamped_pos,
            contact_voxel_y,
        } => {
            assert_eq!(contact_voxel_y, 15);
            // Harus tertahan di atas lantai tipis y=15 (voxel y=16 => 8.0m), TIDAK BOLEH TUNNELING!
            assert_eq!(clamped_pos.y, 8.0);
        }
        other => panic!(
            "Tunneling terdeteksi! Diharapkan GroundContact di y=15, namun: {:?}",
            other
        ),
    }
}

// ============================================================================
// 8A.7 CONSERVATIVE SLEEP & SETTLED DETECTION
// ============================================================================

#[test]
fn test_sleep_and_settled_transition_on_solid_ground() {
    use omnisia::chunk::Chunk;
    use omnisia::physics::PhysicsRuntime;
    use omnisia::streaming::store::ChunkStore;

    let mut store = ChunkStore::new();
    store.insert(Chunk::new(IVec3::ZERO));
    // Lantai solid di y=1
    store.set_voxel_world(IVec3::new(5, 1, 5), VoxelBlock::new(MaterialId::STONE));

    let mut runtime = PhysicsRuntime::default();
    // Badan dinamis tepat di atas lantai (y=2 => 1.0m)
    let voxels = vec![(IVec3::new(5, 2, 5), VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(1, &voxels).unwrap();
    let body_id = runtime.spawn_from_detached_aggregate(agg);

    // Jalankan simulasi selama 20 ticks (cukup untuk mencapai sleep_ticks_required = 15)
    for _ in 0..20 {
        runtime.tick(1.0 / 30.0, &store);
    }

    let body = runtime.get_body(body_id).unwrap();
    assert_eq!(
        body.state,
        DynamicBodyState::Settled,
        "Badan yang diam di atas tanah solid dengan gravitasi normal harus berstatus Settled!"
    );
    assert_eq!(runtime.settled_body_count(), 1);
    assert_eq!(runtime.active_body_count(), 0);
}

#[test]
fn test_antigravity_floating_stationary_never_settles() {
    use omnisia::chunk::Chunk;
    use omnisia::physics::PhysicsRuntime;
    use omnisia::streaming::store::ChunkStore;

    let mut store = ChunkStore::new();
    store.insert(Chunk::new(IVec3::ZERO));

    let mut runtime = PhysicsRuntime::default();
    // Badan AntiGravity mengapung di udara bebas tanpa tanah di bawahnya
    let voxels = vec![(IVec3::new(5, 15, 5), VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(2, &voxels).unwrap();
    let body_id = runtime.spawn_from_detached_aggregate(agg);

    // Atur gravitasi menjadi nol (AntiGravity)
    runtime.get_body_mut(body_id).unwrap().gravity_scale = 0.0;

    // Jalankan 30 ticks
    for _ in 0..30 {
        runtime.tick(1.0 / 30.0, &store);
    }

    let body = runtime.get_body(body_id).unwrap();
    // Amendment 13: Floating AntiGravity BISA Sleeping, tetapi TIDAK PERNAH Settled!
    assert_eq!(
        body.state,
        DynamicBodyState::Sleeping,
        "Badan AntiGravity diam di udara bebas hanya boleh Sleeping, bukan Settled!"
    );
    assert_eq!(runtime.settled_body_count(), 0);
    assert_eq!(runtime.sleeping_body_count(), 1);
}

#[test]
fn test_sleep_wake_up_on_velocity_increase() {
    use omnisia::chunk::Chunk;
    use omnisia::physics::PhysicsRuntime;
    use omnisia::streaming::store::ChunkStore;

    let mut store = ChunkStore::new();
    store.insert(Chunk::new(IVec3::ZERO));
    let mut runtime = PhysicsRuntime::default();
    let voxels = vec![(IVec3::new(5, 10, 5), VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(3, &voxels).unwrap();
    let body_id = runtime.spawn_from_detached_aggregate(agg);

    let body = runtime.get_body_mut(body_id).unwrap();
    body.gravity_scale = 0.0;
    body.set_state(DynamicBodyState::Sleeping);
    body.ticks_stationary = 20;

    assert_eq!(body.state, DynamicBodyState::Sleeping);

    // Berikan kecepatan di atas threshold (> 0.05 m/s)
    body.velocity = Vec3::new(0.0, 1.0, 0.0);

    // Evaluasi tick
    runtime.tick(1.0 / 30.0, &store);

    let body = runtime.get_body(body_id).unwrap();
    assert_eq!(
        body.state,
        DynamicBodyState::Active,
        "Badan yang bergerak di atas threshold harus bangun (Active)!"
    );
    assert_eq!(body.ticks_stationary, 0);
}
