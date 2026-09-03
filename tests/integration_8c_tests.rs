use glam::{IVec3, Vec3};
use omnisia::chunk::Chunk;
use omnisia::material::MaterialId;
use omnisia::player::{PlayerController, PlayerInput};
use omnisia::voxel::VoxelBlock;
use omnisia::world::World;
use omnisia::worldgen::seed::WorldSeed;

// ============================================================================
// PHASE 8C.1: PLAYER <-> STATIC WORLD INTEGRATION TESTS
// ============================================================================

#[test]
fn test_8c1_player_standing_and_gravity_on_static_world() {
    let mut world = World::with_seed(WorldSeed(123));

    // Bersihkan dan setup chunk statis (0, 0, 0)
    let mut chunk = Chunk::new(IVec3::ZERO);
    // Lantai solid di y = 0..3 (permukaan atas y = 4 * 0.5 = 2.0m)
    for vx in 0..16 {
        for vz in 0..16 {
            for vy in 0..4 {
                chunk.set_voxel(vx, vy, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
    }
    world.store.insert(chunk);

    // Spawn pemain di udara di atas lantai statis (y = 6.0m)
    let mut player = PlayerController::new(Vec3::new(4.0, 6.0, 4.0));
    assert!(!player.state.grounded);

    // Simulasikan 60 ticks (2.0 detik) menggunakan World::update_player
    for _ in 0..60 {
        world.update_player(&mut player, 1.0 / 30.0, 0.0);
        if player.state.grounded {
            break;
        }
    }

    // INVARIAN 8C.1: Pemain harus mendarat tepat pada y = 2.0m di atas voxel statis
    assert!(
        player.state.grounded,
        "Pemain harus grounded di atas lantai statis!"
    );
    assert!(
        (player.state.position.y - 2.0).abs() < 1e-3,
        "Posisi kaki pemain harus di permukaan 2.0m, terukur: {}",
        player.state.position.y
    );
    assert_eq!(player.state.velocity.y, 0.0);
}

#[test]
fn test_8c1_player_walking_across_static_terrain() {
    let mut world = World::with_seed(WorldSeed(123));

    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..32 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::GRASS));
        }
    }
    world.store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(2.0, 0.5, 2.0));
    player.state.grounded = true;

    // Input berjalan maju (+X, yaw = 0 deg)
    player.set_input(PlayerInput::from_raw(
        true, false, false, false, false, false, false,
    ));

    // Jalankan 15 tick (0.5 detik) -> jarak teoritis: 3.0 m/s * 0.5s = 1.5m (posisi x: 2.0 + 1.5 = 3.5m)
    for _ in 0..15 {
        world.update_player(&mut player, 1.0 / 30.0, 0.0);
    }

    // INVARIAN 8C.1: Pemain harus maju secara stabil tanpa jatuh atau keluar dari ground
    assert!(
        (player.state.position.x - 3.5).abs() < 0.05,
        "Pemain harus menempuh ~1.5m di sumbu X, terukur x = {}",
        player.state.position.x
    );
    assert_eq!(player.state.position.y, 0.5);
    assert!(player.state.grounded);
}

#[test]
fn test_8c1_player_jumping_from_static_terrain() {
    let mut world = World::with_seed(WorldSeed(123));

    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..16 {
        for vz in 0..16 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    world.store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(4.0, 0.5, 4.0));
    player.state.grounded = true;

    // Frame 1: Memicu lompatan
    player.set_input(PlayerInput::from_raw(
        false, false, false, false, false, false, true,
    ));
    world.update_player(&mut player, 1.0 / 30.0, 0.0);

    // Harus melayang di udara dengan kecepatan awal 6.0 m/s (dikurangi gravitasi 1 tick)
    assert!(!player.state.grounded);
    assert!(player.state.position.y > 0.5);

    // Frame 2..40: Menahan tombol jump sambil jatuh kembali
    player.set_input(PlayerInput::from_raw(
        false, false, false, false, false, false, true,
    ));
    let mut landed = false;
    for _ in 0..40 {
        world.update_player(&mut player, 1.0 / 30.0, 0.0);
        if player.state.grounded && player.state.velocity.y == 0.0 {
            landed = true;
            break;
        }
    }

    // INVARIAN 8C.1: Pemain mendarat kembali ke lantai statis, dan Space ditahan TIDAK memicu lompatan berulang
    assert!(landed, "Pemain harus mendarat kembali!");
    assert_eq!(player.state.position.y, 0.5);
    assert!(player.state.grounded);

    // Tick berikutnya dengan space masih ditekan harus tetap di tanah (bukan lompat lagi)
    world.update_player(&mut player, 1.0 / 30.0, 0.0);
    assert!(player.state.grounded);
    assert_eq!(player.state.velocity.y, 0.0);
}

#[test]
fn test_8c1_player_wall_and_ceiling_collision() {
    let mut world = World::with_seed(WorldSeed(123));

    let mut chunk = Chunk::new(IVec3::ZERO);
    // Lantai di y = 0
    for vx in 0..16 {
        for vz in 0..16 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Dinding di x = 4.0..4.5 (vx = 8)
    for vy in 1..4 {
        for vz in 0..16 {
            chunk.set_voxel(8, vy, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Langit-langit di atas titik spawn (x = 2, voxel y = 5 -> dasar langit-langit y = 2.5m)
    for vx in 0..4 {
        for vz in 0..16 {
            chunk.set_voxel(vx, 5, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    world.store.insert(chunk);

    // 1. Tabrakan Dinding Horizontal
    let mut player = PlayerController::new(Vec3::new(2.5, 0.5, 5.0));
    player.state.grounded = true;
    player.set_input(PlayerInput::from_raw(
        true, false, false, false, true, false, false,
    )); // Sprint maju ke arah dinding (9.0 m/s)

    for _ in 0..10 {
        world.update_player(&mut player, 1.0 / 30.0, 0.0);
    }

    let front_x = player.state.position.x + player.config.capsule_radius;
    assert!(
        front_x <= 4.001,
        "Pemain menembus dinding statis! front_x: {}",
        front_x
    );
    assert_eq!(player.state.velocity.x, 0.0);

    // 2. Tabrakan Langit-langit Vertikal
    let mut ceiling_player = PlayerController::new(Vec3::new(1.0, 0.5, 5.0));
    ceiling_player.state.grounded = true;
    // Input lompat di bawah langit-langit rendah (2.5m)
    ceiling_player.set_input(PlayerInput::from_raw(
        false, false, false, false, false, false, true,
    ));

    let mut hit_ceiling = false;
    for _ in 0..15 {
        world.update_player(&mut ceiling_player, 1.0 / 30.0, 0.0);
        let top_y = ceiling_player.state.position.y + ceiling_player.config.standing_height;
        assert!(
            top_y <= 2.501,
            "Pemain menembus langit-langit statis! top_y: {}",
            top_y
        );
        if ceiling_player.collision_hits_total > 0 {
            hit_ceiling = true;
        }
    }
    assert!(hit_ceiling, "Pemain harus membentur langit-langit statis!");
}

#[test]
fn test_8c1_player_cross_chunk_and_negative_coordinates() {
    let mut world = World::with_seed(WorldSeed(123));

    // Chunk Positif (0, 0, 0)
    let mut pos_chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..32 {
        for vz in 0..10 {
            pos_chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    world.store.insert(pos_chunk);

    // Chunk Negatif (-1, 0, 0)
    let mut neg_chunk = Chunk::new(IVec3::new(-1, 0, 0));
    for vx in 0..32 {
        for vz in 0..10 {
            neg_chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    world.store.insert(neg_chunk);

    // Chunk Negatif (-2, 0, 0)
    let mut neg2_chunk = Chunk::new(IVec3::new(-2, 0, 0));
    for vx in 0..32 {
        for vz in 0..10 {
            neg2_chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    world.store.insert(neg2_chunk);

    // Pemain mulai dari x = 2.0m bergerak ke arah -X menuju koordinat negatif
    let mut player = PlayerController::new(Vec3::new(2.0, 0.5, 5.0));
    player.state.grounded = true;

    // Gerak mundur (-X)
    player.set_input(PlayerInput::from_raw(
        false, true, false, false, false, false, false,
    ));

    // Simulasikan pergerakan melintasi x = 0, x = -1, x = -16 (batas chunk -1), x = -32 (batas chunk -2)
    for _ in 0..250 {
        world.update_player(&mut player, 1.0 / 30.0, 0.0);
    }

    // INVARIAN 8C.1: Pemain harus melintasi batas chunk negatif tanpa jatuh ke void
    assert!(
        player.state.position.x < -20.0,
        "Pemain harus berhasil melintasi batas chunk ke wilayah negatif mendalam!"
    );
    assert_eq!(
        player.state.position.y, 0.5,
        "Pemain tidak boleh jatuh di sambungan chunk negatif!"
    );
    assert!(player.state.grounded);
}

#[test]
fn test_8c1_failure_injection_unloaded_chunk_blocks_movement_unknown_not_air() {
    let mut world = World::with_seed(WorldSeed(123));

    // Hanya muat chunk (0, 0, 0) [x = 0..16m]
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..32 {
        for vz in 0..10 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    world.store.insert(chunk);

    let initial_resident_chunks = world.store.resident_count();

    // Pemain berada di x = 15.6m (dekat batas chunk yang belum dimuat x = 16.0m)
    let mut player = PlayerController::new(Vec3::new(15.6, 0.5, 5.0));
    player.state.grounded = true;

    // Coba bergerak maju ke arah chunk (1, 0, 0) yang belum dimuat
    player.set_input(PlayerInput::from_raw(
        true, false, false, false, true, false, false,
    )); // Sprint maju (9.0 m/s)
    world.update_player(&mut player, 1.0 / 30.0, 0.0);

    // INVARIAN 8C.1 & FAILURE INJECTION A:
    // 1. Gerak pemain diblokir di perbatasan chunk belum dimuat
    let front_x = player.state.position.x + player.config.capsule_radius;
    assert!(
        front_x <= 16.001,
        "Pemain tidak boleh menembus ke chunk yang belum dimuat! front_x: {}",
        front_x
    );
    assert!(player.unknown_blocked_total > 0);

    // 2. Kueri tabrakan TIDAK BOLEH memutasi dunia sama sekali
    assert_eq!(
        world.store.resident_count(),
        initial_resident_chunks,
        "Kueri tabrakan tidak boleh membuat atau memuat chunk secara diam-diam!"
    );
    assert!(
        world.store.get(&IVec3::new(1, 0, 0)).is_none(),
        "Chunk belum dimuat harus tetap berstatus None (Unknown)!"
    );
}

#[test]
fn test_8c1_no_gpu_collision_authority_cpu_chunkstore_only() {
    // Validasi bahwa tabrakan sepenuhnya dievaluasi dari otoritas CPU ChunkStore,
    // tanpa bergantung pada Renderer wgpu ataupun mesh GPU.
    let mut world = World::with_seed(WorldSeed(999));
    let mut chunk = Chunk::new(IVec3::ZERO);
    chunk.set_voxel(5, 5, 5, VoxelBlock::new(MaterialId::STONE));
    world.store.insert(chunk);

    // Renderer tidak diinisialisasi (None)
    let mut player = PlayerController::new(Vec3::new(2.5, 5.0, 2.5));
    world.update_player(&mut player, 1.0 / 30.0, 0.0);

    // Simulasi berjalan sukses tanpa perlu GPU mesh
    assert!(player.state.position.y < 5.0);
}

#[test]
fn test_8c1_spawn_player_at_valid_ground_integration() {
    let mut world = World::with_seed(WorldSeed(123));

    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..10 {
        for vz in 0..10 {
            chunk.set_voxel(vx, 8, vz, VoxelBlock::new(MaterialId::GRASS));
        }
    }
    world.store.insert(chunk);

    let mut player = PlayerController::new(Vec3::ZERO);
    let spawned = world.spawn_player_at_valid_ground(&mut player, 2.5, 2.5, 0.0, 20.0);

    assert!(spawned, "Spawning via World harus berhasil!");
    assert_eq!(player.state.position, Vec3::new(2.5, 4.5, 2.5));
    assert!(player.state.grounded);
}

// ============================================================================
// PHASE 8C.2: PLAYER <-> DYNAMICBODY INTEGRATION TESTS
// ============================================================================

#[test]
fn test_8c2_player_standing_on_dynamic_body() {
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));
    // Muat chunk (0, 0, 0) ke ChunkStore (Loaded + Air) agar bukan Unknown
    world.store.insert(Chunk::new(IVec3::ZERO));

    // Platform dinamis mengambang (AntiGravity, 4x4 voxel di y = 10 -> y_pos = 5.0m)
    // Permukaan atas platform = 5.0m + 0.5m = 5.5m
    let mut voxels = Vec::new();
    for vx in 0..4 {
        for vz in 0..4 {
            voxels.push((
                IVec3::new(vx, 10, vz),
                VoxelBlock::new(MaterialId::OAK_WOOD),
            ));
        }
    }
    let agg = DetachedAggregate::from_world_voxels(1, &voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);
    world.physics.get_body_mut(body_id).unwrap().gravity_scale = 0.0; // Mengambang

    // Pemain spawn di atas badan dinamis (y = 8.0m)
    let mut player = PlayerController::new(Vec3::new(1.0, 8.0, 1.0));
    assert!(!player.state.grounded);

    // Simulasikan pemain jatuh ke atas badan dinamis
    for _ in 0..60 {
        world.update_player(&mut player, 1.0 / 30.0, 0.0);
        if player.state.grounded {
            break;
        }
    }

    // INVARIAN 8C.2: Pemain harus mendarat tepat pada permukaan atas badan dinamis (5.5m)
    assert!(
        player.state.grounded,
        "Pemain harus grounded di atas DynamicBody!"
    );
    assert!(
        (player.state.position.y - 5.5).abs() < 1e-3,
        "Pemain harus berhenti di y = 5.5m di atas DynamicBody, terukur: {}",
        player.state.position.y
    );
    assert_eq!(player.state.velocity.y, 0.0);
}

#[test]
fn test_8c2_player_jumping_from_dynamic_body() {
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));
    world.store.insert(Chunk::new(IVec3::ZERO));

    let mut voxels = Vec::new();
    for vx in 0..4 {
        for vz in 0..4 {
            voxels.push((
                IVec3::new(vx, 10, vz),
                VoxelBlock::new(MaterialId::OAK_WOOD),
            ));
        }
    }
    let agg = DetachedAggregate::from_world_voxels(2, &voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);
    world.physics.get_body_mut(body_id).unwrap().gravity_scale = 0.0;

    // Pemain berdiri di atas DynamicBody pada y = 5.5m
    let mut player = PlayerController::new(Vec3::new(1.0, 5.5, 1.0));
    player.state.grounded = true;

    // Eksekusi lompat
    player.set_input(PlayerInput::from_raw(
        false, false, false, false, false, false, true,
    ));
    world.update_player(&mut player, 1.0 / 30.0, 0.0);

    // Pemain harus melayang di udara di atas platform dinamis
    assert!(!player.state.grounded);
    assert!(player.state.position.y > 5.5);

    // Biarkan jatuh kembali
    player.set_input(PlayerInput::default());
    let mut landed = false;
    for _ in 0..40 {
        world.update_player(&mut player, 1.0 / 30.0, 0.0);
        if player.state.grounded && player.state.velocity.y == 0.0 {
            landed = true;
            break;
        }
    }

    assert!(landed, "Pemain harus mendarat kembali ke DynamicBody!");
    assert_eq!(player.state.position.y, 5.5);
    assert!(player.state.grounded);
}

#[test]
fn test_8c2_player_walking_and_colliding_against_dynamic_body_wall() {
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));

    // Lantai statis di y = 0..1 (surface y = 0.5m)
    let mut floor_chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..16 {
        for vz in 0..16 {
            floor_chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    world.store.insert(floor_chunk);

    // Dinding DynamicBody berdiri di x = 4.0..4.5m (vx = 8) dari y = 1..4 (ketinggian 0.5m .. 2.0m)
    let mut wall_voxels = Vec::new();
    for vy in 1..4 {
        for vz in 0..5 {
            wall_voxels.push((IVec3::new(8, vy, vz), VoxelBlock::new(MaterialId::STONE)));
        }
    }
    let agg = DetachedAggregate::from_world_voxels(3, &wall_voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);
    world.physics.get_body_mut(body_id).unwrap().gravity_scale = 0.0;

    let mut player = PlayerController::new(Vec3::new(2.5, 0.5, 2.0));
    player.state.grounded = true;
    player.set_input(PlayerInput::from_raw(
        true, false, false, false, true, false, false,
    )); // Sprint maju (9.0 m/s) ke arah dinding dinamis di x = 4.0m

    for _ in 0..15 {
        world.update_player(&mut player, 1.0 / 30.0, 0.0);
    }

    // INVARIAN 8C.2: Dinding DynamicBody memblokir gerak pemain tanpa tembus (front_x <= 4.001)
    let front_x = player.state.position.x + player.config.capsule_radius;
    assert!(
        front_x <= 4.001,
        "Pemain menembus dinding DynamicBody! front_x: {}",
        front_x
    );
    assert_eq!(player.state.velocity.x, 0.0);
}

#[test]
fn test_8c2_hollow_dynamic_aggregate_empty_space_not_solid() {
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));

    // Lantai statis
    let mut floor_chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..16 {
        for vz in 0..16 {
            floor_chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    world.store.insert(floor_chunk);

    // Buat badan dinamis berbentuk bingkai U / C dengan rongga kosong di tengah:
    // Dinding kiri di vx = 4 (x = 2.0..2.5)
    // Dinding kanan di vx = 10 (x = 5.0..5.5)
    // Tengah vx = 5, 6, 7, 8, 9 (x = 2.5..5.0) adalah RONGGA KOSONG!
    let mut hollow_voxels = Vec::new();
    for vy in 1..4 {
        hollow_voxels.push((IVec3::new(4, vy, 4), VoxelBlock::new(MaterialId::STONE)));
        hollow_voxels.push((IVec3::new(10, vy, 4), VoxelBlock::new(MaterialId::STONE)));
    }
    let agg = DetachedAggregate::from_world_voxels(4, &hollow_voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);
    world.physics.get_body_mut(body_id).unwrap().gravity_scale = 0.0;

    // Pemain berjalan tepat melintasi bagian tengah yang kosong (x = 3.5m, z = 2.0m -> menuju z = 5.0m)
    // Bounding box AABB badan dinamis adalah [2.0 .. 5.5] di X dan [2.0 .. 2.5] di Z.
    // Pemain berada di x = 3.75m (dalam rentang AABB X dari DynamicBody), namun di rongga kosong!
    let mut player = PlayerController::new(Vec3::new(3.75, 0.5, 1.0));
    player.state.grounded = true;
    player.set_input(PlayerInput::from_raw(
        false, false, false, false, false, false, false,
    ));

    // Clearance kapsul berdiri di dalam rongga DynamicBody HARUS BERHASIL (true)
    let clearance = omnisia::player::check_capsule_clearance_with_physics(
        Vec3::new(3.75, 0.5, 2.0),
        player.config.standing_height,
        player.config.capsule_radius,
        &world.store,
        Some(&world.physics),
    );
    assert!(
        clearance,
        "Rongga kosong di dalam DynamicBody harus dapat dihuni dan tidak boleh dianggap solid!"
    );
}

#[test]
fn test_8c2_player_under_low_dynamic_body_forced_crouch() {
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));

    // Lantai statis di y = 0 (surface y = 0.5m)
    let mut floor_chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..16 {
        for vz in 0..16 {
            floor_chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    world.store.insert(floor_chunk);

    // Atap rendah DynamicBody di y = 4 (ketinggian dasar y = 2.0m). Ruang bebas = 1.5m
    let mut ceiling_voxels = Vec::new();
    for vx in 0..6 {
        for vz in 0..6 {
            ceiling_voxels.push((IVec3::new(vx, 4, vz), VoxelBlock::new(MaterialId::STONE)));
        }
    }
    let agg = DetachedAggregate::from_world_voxels(5, &ceiling_voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);
    world.physics.get_body_mut(body_id).unwrap().gravity_scale = 0.0;

    let mut player = PlayerController::new(Vec3::new(1.5, 0.5, 1.5));
    player.state.grounded = true;

    // 1. Jongkok di bawah atap DynamicBody
    player.set_input(PlayerInput::from_raw(
        false, false, false, false, false, true, false,
    ));
    world.update_player(&mut player, 1.0 / 30.0, 0.0);
    assert!(player.state.crouching);
    assert_eq!(player.current_capsule().height, 1.2);

    // 2. Lepas tombol jongkok: harus dipaksa tetap jongkok (forced_crouch = true)
    player.set_input(PlayerInput::default());
    world.update_player(&mut player, 1.0 / 30.0, 0.0);
    assert!(
        player.state.crouching,
        "Pemain harus tetap jongkok di bawah atap DynamicBody rendah!"
    );
    assert!(player.state.forced_crouch, "forced_crouch harus aktif!");
}

#[test]
fn test_8c2_no_ownership_transfer_during_player_contact() {
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));

    let mut voxels = Vec::new();
    for vx in 0..2 {
        for vz in 0..2 {
            voxels.push((IVec3::new(vx, 5, vz), VoxelBlock::new(MaterialId::STONE)));
        }
    }
    let agg = DetachedAggregate::from_world_voxels(6, &voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);

    // Pemain berdiri dan melompat di atas DynamicBody
    let mut player = PlayerController::new(Vec3::new(0.5, 3.0, 0.5));
    world.update_player(&mut player, 1.0 / 30.0, 0.0);

    // INVARIAN 8C.2:
    // 1. DynamicBody tetap utuh di PhysicsRuntime
    assert!(world.physics.get_body(body_id).is_some());
    assert_eq!(world.physics.bodies.len(), 1);

    // 2. ChunkStore TIDAK memiliki voxel dari DynamicBody (tidak ada duplikasi)
    for (world_coord, _) in &voxels {
        assert!(
            world.store.get_voxel_world(*world_coord).is_air(),
            "Voxel DynamicBody tidak boleh terduplikasi ke ChunkStore selama kontak!"
        );
    }
}

#[test]
fn test_8c2_player_supported_by_falling_dynamic_body() {
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));

    // Lantai statis di y = 0..1 (surface y = 0.5m)
    let mut floor_chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..16 {
        for vz in 0..16 {
            floor_chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    world.store.insert(floor_chunk);

    // Platform dinamis di y = 10 (ketinggian awal 5.0m, permukaan atas 5.5m)
    let mut platform_voxels = Vec::new();
    for vx in 0..4 {
        for vz in 0..4 {
            platform_voxels.push((
                IVec3::new(vx, 10, vz),
                VoxelBlock::new(MaterialId::OAK_WOOD),
            ));
        }
    }
    let agg = DetachedAggregate::from_world_voxels(7, &platform_voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);
    // Platform memiliki gravitasi normal (1.0)
    world.physics.get_body_mut(body_id).unwrap().gravity_scale = 1.0;

    // Pemain mulai berdiri di atas platform dinamis pada y = 5.5m
    let mut player = PlayerController::new(Vec3::new(1.0, 5.5, 1.0));
    player.state.grounded = true;

    // Simulasikan selama 60 tick (2.0 detik) di mana platform dan pemain jatuh bersamaan
    for _ in 0..60 {
        // 1. Update fisika dinamis (platform jatuh)
        world.update(Vec3::ZERO, 1.0 / 30.0, None);
        // 2. Update player controller (pemain jatuh mengikuti dan bertumpu)
        world.update_player(&mut player, 1.0 / 30.0, 0.0);
    }

    // INVARIAN 8C.2:
    // Platform harus telah mendarat di atas lantai statis (y = 0.5m)
    // Permukaan atas platform yang telah mendarat = 0.5m + 0.5m = 1.0m
    // Pemain tidak boleh tembus dan harus grounded di atas platform
    let _body = world.physics.get_body(body_id);
    // Badan dinamis bisa berupa settled di physics atau telah reintegrasi
    assert!(
        player.state.position.y >= 0.999,
        "Pemain tidak boleh menembus platform yang jatuh ke lantai! y = {}",
        player.state.position.y
    );
    assert!(
        player.state.grounded,
        "Pemain harus grounded saat mendarat bersama platform!"
    );
}

// ============================================================================
// PHASE 8C.3: DYNAMICBODY <-> STATIC WORLD INTEGRATION TESTS
// ============================================================================

#[test]
fn test_8c3_dynamic_body_gravity_and_ground_snapping() {
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));

    // Lantai statis di y = 0..1 (surface y = 0.5m)
    let mut floor_chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..16 {
        for vz in 0..16 {
            floor_chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    world.store.insert(floor_chunk);

    // Badan dinamis (2x2x2) di y = 10 (ketinggian awal 5.0m)
    let mut voxels = Vec::new();
    for vx in 0..2 {
        for vy in 0..2 {
            for vz in 0..2 {
                voxels.push((
                    IVec3::new(vx, 10 + vy, vz),
                    VoxelBlock::new(MaterialId::STONE),
                ));
            }
        }
    }
    let agg = DetachedAggregate::from_world_voxels(8, &voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);

    // Simulasikan jatuhnya badan dinamis hingga menyentuh lantai statis
    // Gunakan world.physics.tick untuk memeriksa grounding dan snapping sebelum auto-reintegrasi
    for _ in 0..60 {
        world.physics.tick(1.0 / 30.0, &world.store);
        if let Some(body) = world.physics.get_body(body_id) {
            if body.is_grounded {
                break;
            }
        }
    }

    let body = world.physics.get_body(body_id).unwrap();
    // INVARIAN 8C.3: Badan harus bertumpu tepat pada integer grid (y = 0.5m)
    assert!(
        body.is_grounded,
        "DynamicBody harus terdeteksi grounded pada lantai statis!"
    );
    assert!(
        (body.position.y - 0.5).abs() < 1e-3,
        "Posisi Y DynamicBody harus di-snap tepat ke 0.5m, terukur: {}",
        body.position.y
    );
    assert_eq!(body.velocity.y, 0.0);
}

#[test]
fn test_8c3_dynamic_body_horizontal_wall_collision_and_anti_tunneling() {
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));

    let mut chunk = Chunk::new(IVec3::ZERO);
    // Lantai di y = 0
    for vx in 0..32 {
        for vz in 0..16 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Dinding tipis 1-voxel di vx = 20 (x = 10.0..10.5m)
    for vy in 1..4 {
        for vz in 0..16 {
            chunk.set_voxel(20, vy, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    world.store.insert(chunk);

    // Badan dinamis (1-voxel) di x = 2.0m, bergerak cepat ke arah +X (vx = 50.0 m/s)
    let voxels = vec![(IVec3::new(4, 1, 4), VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(9, &voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);
    let body_mut = world.physics.get_body_mut(body_id).unwrap();
    body_mut.gravity_scale = 0.0;
    body_mut.velocity = Vec3::new(50.0, 0.0, 0.0); // 50 m/s ke arah dinding 10.0m

    // Dalam 1 tick (1/30 detik), delta_x teoritis = 50.0 / 30 = 1.67m
    // Jalankan 10 tick (cukup untuk menempuh 16.7m jika tidak terhalang)
    for _ in 0..10 {
        world.update(Vec3::ZERO, 1.0 / 30.0, None);
    }

    let body = world.physics.get_body(body_id).unwrap();
    // INVARIAN 8C.3: Badan dinamis tidak boleh menembus dinding di x = 10.0m!
    // Ujung kanan voxel badan (pos.x + 0.5) harus <= 10.001m
    let max_x = body.position.x + 0.5;
    assert!(
        max_x <= 10.001,
        "DynamicBody menembus dinding tipis statis! max_x: {}",
        max_x
    );
    assert_eq!(body.velocity.x, 0.0);
}

#[test]
fn test_8c3_dynamic_body_unloaded_chunk_boundary_blocks_motion() {
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));

    // Hanya muat chunk (0, 0, 0) [x = 0..16m]
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..32 {
        for vz in 0..10 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    world.store.insert(chunk);

    let initial_resident_chunks = world.store.resident_count();

    // Badan dinamis di x = 15.0m bergerak ke +X menuju chunk (1,0,0) yang belum dimuat
    let voxels = vec![(IVec3::new(30, 1, 4), VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(10, &voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);
    let body_mut = world.physics.get_body_mut(body_id).unwrap();
    body_mut.gravity_scale = 0.0;
    body_mut.velocity = Vec3::new(10.0, 0.0, 0.0);

    for _ in 0..5 {
        world.update(Vec3::ZERO, 1.0 / 30.0, None);
    }

    let body = world.physics.get_body(body_id).unwrap();
    // INVARIAN 8C.3 & UNLOADED BOUNDARY:
    // 1. Badan dinamis diblokir di perbatasan chunk yang belum dimuat (x <= 16.0m)
    let max_x = body.position.x + 0.5;
    assert!(
        max_x <= 16.001,
        "DynamicBody tidak boleh menembus ke chunk belum dimuat! max_x: {}",
        max_x
    );
    assert_eq!(body.velocity.x, 0.0);

    // 2. ChunkStore tidak termutasi
    assert_eq!(
        world.store.resident_count(),
        initial_resident_chunks,
        "Kueri tabrakan DynamicBody tidak boleh membuat chunk baru secara diam-diam!"
    );
}

#[test]
fn test_8c3_sleep_and_settled_state_transition() {
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));

    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..10 {
        for vz in 0..10 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    world.store.insert(chunk);

    // Badan dinamis tepat di atas tanah (y = 0.5m)
    let voxels = vec![(IVec3::new(2, 1, 2), VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(11, &voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);

    // Simulasikan melebihi sleep_ticks_required (15 tick) menggunakan world.physics.tick
    for _ in 0..25 {
        world.physics.tick(1.0 / 30.0, &world.store);
    }

    let body = world.physics.get_body(body_id).unwrap();
    // INVARIAN 8C.3: Badan dengan gravitasi di atas tanah solid harus berstatus Settled
    assert_eq!(
        body.state,
        omnisia::physics::DynamicBodyState::Settled,
        "Badan dinamis harus bertransisi ke Settled setelah diam di tanah solid!"
    );

    // Update dunia penuh sekarang akan memicu reintegrasi dua fase (8C.6)
    world.update(Vec3::ZERO, 1.0 / 30.0, None);
    assert!(world.physics.get_body(body_id).is_none());
    assert!(!world.store.get_voxel_world(IVec3::new(2, 1, 2)).is_air());
}

#[test]
fn test_8c3_sleeping_body_wakes_on_disturbance() {
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));
    world.store.insert(Chunk::new(IVec3::ZERO));

    let voxels = vec![(IVec3::new(2, 5, 2), VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(12, &voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);
    let body_mut = world.physics.get_body_mut(body_id).unwrap();
    body_mut.gravity_scale = 0.0;
    body_mut.state = omnisia::physics::DynamicBodyState::Sleeping;

    assert_eq!(
        world.physics.get_body(body_id).unwrap().state,
        omnisia::physics::DynamicBodyState::Sleeping
    );

    // Berikan impuls kecepatan yang melebihi threshold
    world.physics.get_body_mut(body_id).unwrap().velocity = Vec3::new(5.0, 0.0, 0.0);

    // Tick berikutnya harus membangunkannya ke Active
    world.update(Vec3::ZERO, 1.0 / 30.0, None);

    assert_eq!(
        world.physics.get_body(body_id).unwrap().state,
        omnisia::physics::DynamicBodyState::Active,
        "Badan sleeping harus bangun ke Active saat diberi impuls kecepatan!"
    );
}

// ============================================================================
// PHASE 8C.4: STRUCTURAL MUTATION DURING RUNTIME INTEGRATION TESTS
// ============================================================================

#[test]
fn test_8c4_player_breaks_support_block_spawns_dynamic_body_while_simulating() {
    let mut world = World::with_seed(WorldSeed(123));

    // Siapkan chunk (0,0,0) dengan lantai dan struktur overhang yang ditopang pilar
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..16 {
        for vz in 0..16 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Pilar di (5, 1..=3, 5)
    for vy in 1..=3 {
        chunk.set_voxel(5, vy, 5, VoxelBlock::new(MaterialId::STONE));
    }
    // Blok overhang di atas pilar (5, 4, 5) dan cabang di (6, 4, 5)
    chunk.set_voxel(5, 4, 5, VoxelBlock::new(MaterialId::STONE));
    chunk.set_voxel(6, 4, 5, VoxelBlock::new(MaterialId::STONE));
    world.store.insert(chunk);

    // Pemain aktif berjalan di tanah
    let mut player = PlayerController::new(Vec3::new(1.0, 0.5, 1.0));
    world.update_player(&mut player, 1.0 / 30.0, 0.0);

    let initial_bodies_count = world.physics.body_count();

    // Hancurkan pilar dasar di (5, 1, 5)
    let detached = world.set_voxel_world(IVec3::new(5, 1, 5), VoxelBlock::AIR);

    // INVARIAN 8C.4:
    // 1. Mutasi memicu pemisahan struktural
    assert!(
        !detached.is_empty(),
        "Pilar yang hancur harus memisahkan aggregate yang menggantung!"
    );

    // 2. Aggregate langsung terdaftar sebagai DynamicBody di PhysicsRuntime
    assert_eq!(
        world.physics.body_count(),
        initial_bodies_count + detached.len()
    );

    // 3. Simulasi runtime terus berjalan tanpa crash
    for _ in 0..10 {
        world.update(Vec3::ZERO, 1.0 / 30.0, None);
        world.update_player(&mut player, 1.0 / 30.0, 0.0);
    }
}

#[test]
fn test_8c4_player_standing_on_detached_aggregate_falls_with_it() {
    let mut world = World::with_seed(WorldSeed(123));

    let mut chunk = Chunk::new(IVec3::ZERO);
    // Lantai dasar di y = 0 (surface y = 0.5m)
    for vx in 0..16 {
        for vz in 0..16 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Pilar penopang di x = 4, y = 1..=10, z = 4
    for vy in 1..=10 {
        chunk.set_voxel(4, vy, 4, VoxelBlock::new(MaterialId::STONE));
    }
    // Platform balok di atas pilar di (5..=7, 10, 4)
    // Permukaan atas platform = (10 + 1) * 0.5 = 5.5m
    for vx in 5..=7 {
        chunk.set_voxel(vx, 10, 4, VoxelBlock::new(MaterialId::STONE));
    }
    world.store.insert(chunk);

    // Pemain berdiri di atas platform balok pada (6.0, 5.5, 2.0)
    let mut player = PlayerController::new(Vec3::new(3.0, 5.5, 2.0)); // vx=6 -> x=3.0m, vz=4 -> z=2.0m
    player.state.grounded = true;
    world.update_player(&mut player, 1.0 / 30.0, 0.0);
    assert!(player.state.grounded);

    // Hancurkan pilar penopang di (4, 10, 4)
    let detached = world.set_voxel_world(IVec3::new(4, 10, 4), VoxelBlock::AIR);
    assert!(
        !detached.is_empty(),
        "Platform harus terlepas saat penopangnya dihancurkan!"
    );

    // Simulasikan jatuhnya platform bersama pemain selama 30 tick
    for _ in 0..30 {
        world.update(Vec3::ZERO, 1.0 / 30.0, None);
        world.update_player(&mut player, 1.0 / 30.0, 0.0);
    }

    // INVARIAN 8C.4 & Section 20:
    // Pemain tidak jatuh ke void, posisi Y pemain menurun seiring jatuhnya platform,
    // dan pemain tetap berada di atas atau bertumpu pada platform yang jatuh
    assert!(
        player.state.position.y < 5.5,
        "Pemain harus jatuh mengikuti platform!"
    );
    assert!(
        player.state.position.y >= 0.5,
        "Pemain tidak boleh jatuh di bawah lantai statis! y = {}",
        player.state.position.y
    );
}

#[test]
fn test_8c4_breaking_terrain_beneath_sleeping_body_wakes_it() {
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));

    let mut chunk = Chunk::new(IVec3::ZERO);
    // Lantai dasar di y = 0
    for vx in 0..10 {
        for vz in 0..10 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Pilar tumpuan statis 1-voxel di (5, 1, 5) -> surface y = 1.0m
    chunk.set_voxel(5, 1, 5, VoxelBlock::new(MaterialId::STONE));
    world.store.insert(chunk);

    // Badan dinamis tepat di atas pilar (5, 2, 5) -> initial_pos y = 1.0m
    let voxels = vec![(IVec3::new(5, 2, 5), VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(13, &voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);

    // Biarkan badan mencapai kondisi diam (Sleeping/Settled) di atas pilar
    for _ in 0..20 {
        world.physics.tick(1.0 / 30.0, &world.store);
    }

    assert!(world.physics.get_body(body_id).unwrap().is_grounded);

    // Hancurkan pilar tumpuan statis di (5, 1, 5)
    world.set_voxel_world(IVec3::new(5, 1, 5), VoxelBlock::AIR);

    let body = world.physics.get_body(body_id).unwrap();
    // INVARIAN 8C.4 & Section 21:
    // Badan dinamis kehilangan tumpuan tanah dan dibangunkan ke status Active!
    assert_eq!(
        body.state,
        omnisia::physics::DynamicBodyState::Active,
        "Badan harus dibangunkan ke status Active saat tumpuannya dihancurkan!"
    );
    assert!(
        !body.is_grounded,
        "Badan harus kehilangan status is_grounded!"
    );

    // Pada tick fisika berikutnya, badan mulai jatuh
    world.physics.tick(1.0 / 30.0, &world.store);
    let body_after = world.physics.get_body(body_id).unwrap();
    assert!(
        body_after.position.y < 1.0,
        "Badan harus mulai jatuh ke arah lantai dasar! pos.y = {}",
        body_after.position.y
    );
}

// ============================================================================
// PHASE 8C.5: OWNERSHIP CONSISTENCY INTEGRATION TESTS
// ============================================================================

#[test]
fn test_8c5_ownership_conservation_across_full_lifecycle() {
    let mut world = World::with_seed(WorldSeed(123));

    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..16 {
        for vz in 0..16 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Pilar 1-voxel di (5, 1, 5)
    chunk.set_voxel(5, 1, 5, VoxelBlock::new(MaterialId::STONE));
    // Overhang 3-voxel di (5, 2, 5), (6, 2, 5), (7, 2, 5)
    chunk.set_voxel(5, 2, 5, VoxelBlock::new(MaterialId::STONE));
    chunk.set_voxel(6, 2, 5, VoxelBlock::new(MaterialId::STONE));
    chunk.set_voxel(7, 2, 5, VoxelBlock::new(MaterialId::STONE));
    world.store.insert(chunk);

    // 1. Audit kondisi awal
    let audit_0 = world.audit_world_ownership();
    assert_eq!(audit_0.duplicate_detections, 0);
    assert_eq!(audit_0.total_dynamic_voxels, 0);
    let initial_total = audit_0.total_world_voxels;

    // 2. Hancurkan pilar dasar di (5, 1, 5) -> menghilangkan tepat 1 voxel
    let detached = world.set_voxel_world(IVec3::new(5, 1, 5), VoxelBlock::AIR);
    assert!(!detached.is_empty());

    let audit_1 = world.audit_world_ownership();
    // INVARIAN 8C.5 & HUKUM KEKEKALAN MASSA:
    // Total voxel dunia harus berkurang tepat 1 voxel (yang dihancurkan)
    assert_eq!(
        audit_1.total_world_voxels,
        initial_total - 1,
        "Total voxel dunia harus terkonservasi sempurna!"
    );
    assert_eq!(
        audit_1.duplicate_detections, 0,
        "Tidak boleh ada duplikasi kepemilikan voxel!"
    );
    assert_eq!(
        audit_1.total_dynamic_voxels, 3,
        "Aggregate lepas harus memiliki tepat 3 voxel!"
    );
    assert_eq!(
        audit_1.total_static_voxels,
        initial_total - 4,
        "ChunkStore harus melepaskan kepemilikan 3 voxel overhang + 1 voxel yang dihancurkan!"
    );

    // 3. Simulasikan hingga mendarat, settled, dan reintegrasi otomatis
    for _ in 0..60 {
        world.physics.update(1.0 / 30.0, &world.store);
        let _ = world
            .physics
            .process_settled_reintegration(&mut world.store);
    }

    let audit_2 = world.audit_world_ownership();
    // Setelah reintegrasi:
    assert_eq!(
        audit_2.total_dynamic_voxels, 0,
        "Badan dinamis harus telah reintegrasi!"
    );
    assert_eq!(
        audit_2.total_world_voxels,
        initial_total - 1,
        "Total voxel setelah reintegrasi harus tetap kekal sempurna!"
    );
    assert_eq!(
        audit_2.duplicate_detections, 0,
        "Zero duplicate ownership setelah reintegrasi!"
    );
}

#[test]
fn test_8c5_negative_coordinate_ownership_consistency() {
    let mut world = World::with_seed(WorldSeed(123));

    // Chunk di koordinat negatif (-1, 0, -1)
    let mut neg_chunk = Chunk::new(IVec3::new(-1, 0, -1));
    for vx in 0..10 {
        for vz in 0..10 {
            neg_chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Pilar di koordinat negatif (-1 chunk, local 5) => world x = -32 + 5 = -27
    neg_chunk.set_voxel(5, 1, 5, VoxelBlock::new(MaterialId::STONE));
    neg_chunk.set_voxel(5, 2, 5, VoxelBlock::new(MaterialId::STONE));
    world.store.insert(neg_chunk);

    let audit_before = world.audit_world_ownership();
    assert_eq!(audit_before.duplicate_detections, 0);

    // Hancurkan pilar dasar
    let world_voxel = IVec3::new(-27, 1, -27);
    let detached = world.set_voxel_world(world_voxel, VoxelBlock::AIR);
    assert!(!detached.is_empty());

    let audit_after = world.audit_world_ownership();
    assert_eq!(
        audit_after.total_world_voxels,
        audit_before.total_world_voxels - 1
    );
    assert_eq!(audit_after.duplicate_detections, 0);
}

#[test]
fn test_8c5_material_and_resource_identity_preservation() {
    let mut world = World::with_seed(WorldSeed(123));

    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..16 {
        for vz in 0..16 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Struktur multi-material:
    // Penopang: STONE
    chunk.set_voxel(4, 1, 4, VoxelBlock::new(MaterialId::STONE));
    // Badan: OAK_WOOD, METAL_FRAME, GOLD_ACCENT
    chunk.set_voxel(4, 2, 4, VoxelBlock::new(MaterialId::OAK_WOOD));
    chunk.set_voxel(4, 3, 4, VoxelBlock::new(MaterialId::METAL_FRAME));
    chunk.set_voxel(4, 4, 4, VoxelBlock::new(MaterialId::GOLD_ACCENT));
    world.store.insert(chunk);

    // Hancurkan penopang
    let detached = world.set_voxel_world(IVec3::new(4, 1, 4), VoxelBlock::AIR);
    assert!(!detached.is_empty());

    // Periksa bahwa DynamicBody yang tercipta menyimpan identitas material sejati
    let body = world.physics.bodies.values().next().unwrap();
    let mat_ids: Vec<MaterialId> = body
        .aggregate
        .voxels
        .iter()
        .map(|v| v.block.material)
        .collect();
    assert!(mat_ids.contains(&MaterialId::OAK_WOOD));
    assert!(mat_ids.contains(&MaterialId::METAL_FRAME));
    assert!(mat_ids.contains(&MaterialId::GOLD_ACCENT));

    // Simulasikan hingga reintegrasi
    for _ in 0..60 {
        world.update(Vec3::ZERO, 1.0 / 30.0, None);
    }

    // Periksa bahwa voxel yang reintegrasi ke ChunkStore mempertahankan identitas material aslinya!
    let mut found_wood = false;
    let mut found_metal = false;
    let mut found_gold = false;

    for vy in 1..=4 {
        let block = world.store.get_voxel_world(IVec3::new(4, vy, 4));
        if block.material == MaterialId::OAK_WOOD {
            found_wood = true;
        }
        if block.material == MaterialId::METAL_FRAME {
            found_metal = true;
        }
        if block.material == MaterialId::GOLD_ACCENT {
            found_gold = true;
        }
    }

    assert!(
        found_wood,
        "OAK_WOOD harus tetap dipertahankan setelah reintegrasi!"
    );
    assert!(
        found_metal,
        "METAL_FRAME harus tetap dipertahankan setelah reintegrasi!"
    );
    assert!(
        found_gold,
        "GOLD_ACCENT harus tetap dipertahankan setelah reintegrasi!"
    );
}

#[test]
fn test_8c5_player_and_camera_contact_does_not_mutate_ownership() {
    let mut world = World::with_seed(WorldSeed(123));

    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..16 {
        for vz in 0..16 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    world.store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(2.0, 0.5, 2.0));
    player.state.grounded = true;

    let audit_before = world.audit_world_ownership();

    // Pemain berjalan, melompat, dan berjongkok
    player.set_input(PlayerInput::from_raw(
        true, false, false, false, true, false, true,
    ));
    for _ in 0..30 {
        world.update_player(&mut player, 1.0 / 30.0, 0.0);
    }

    let audit_after = world.audit_world_ownership();
    // INVARIAN 8C.5: Interaksi pemain tidak boleh memutasi kepemilikan voxel
    assert_eq!(audit_before, audit_after);
}

#[test]
fn test_8c5_streaming_unload_does_not_evict_or_corrupt_active_dynamic_body() {
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));

    let chunk = Chunk::new(IVec3::new(5, 5, 5));
    world.store.insert(chunk);

    let voxels = vec![(IVec3::new(10, 10, 10), VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(14, &voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);

    // Eviksi chunk yang jauh
    world.store.remove(&IVec3::new(5, 5, 5));

    // DynamicBody tetap utuh dengan voxel miliknya
    let body = world.physics.get_body(body_id).unwrap();
    assert_eq!(body.voxel_count(), 1);
    assert_eq!(body.state, omnisia::physics::DynamicBodyState::Active);
}

// ============================================================================
// PHASE 8C.6: PERSISTENCE / REINTEGRATION INTEGRATION TESTS
// ============================================================================

#[test]
fn test_8c6_reintegration_marks_affected_and_boundary_neighbor_chunks_dirty() {
    use omnisia::chunk::dirty_flags;
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));

    // Chunk (0,0,0) di mana voxel akan mendarat
    let mut chunk_main = Chunk::new(IVec3::ZERO);
    chunk_main.dirty_flags = 0;
    world.store.insert(chunk_main);

    // Chunk tetangga di (-1, 0, 0)
    let mut chunk_neighbor = Chunk::new(IVec3::new(-1, 0, 0));
    chunk_neighbor.dirty_flags = 0;
    world.store.insert(chunk_neighbor);

    // Dynamic body tepat di perbatasan x = 0 (local_x = 0)
    let voxels = vec![(IVec3::new(0, 5, 5), VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(15, &voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);
    let body_mut = world.physics.get_body_mut(body_id).unwrap();
    body_mut.state = omnisia::physics::DynamicBodyState::Settled;

    // Jalankan proses reintegrasi
    let reintegrated = world
        .physics
        .process_settled_reintegration(&mut world.store);
    assert_eq!(reintegrated.len(), 1);
    assert_eq!(reintegrated[0], body_id);

    // INVARIAN 8C.6 & Section 29:
    // 1. Chunk utama memiliki VOXEL_DIRTY, MESH_DIRTY, dan SAVE_DIRTY
    let main_c = world.store.get(&IVec3::ZERO).unwrap();
    assert!(main_c.dirty_flags & dirty_flags::VOXEL_DIRTY != 0);
    assert!(main_c.dirty_flags & dirty_flags::MESH_DIRTY != 0);
    assert!(main_c.dirty_flags & dirty_flags::SAVE_DIRTY != 0);

    // 2. Chunk tetangga pada perbatasan x = 0 dipropagasi dengan MESH_DIRTY
    let neighbor_c = world.store.get(&IVec3::new(-1, 0, 0)).unwrap();
    assert!(
        neighbor_c.dirty_flags & dirty_flags::MESH_DIRTY != 0,
        "Chunk tetangga harus ditandai MESH_DIRTY karena mutasi berada di batas x = 0!"
    );
}

#[test]
fn test_8c6_reintegration_failure_injection_unloaded_chunk() {
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));
    // Jangan muat chunk (2, 2, 2)

    let voxels = vec![(IVec3::new(64, 64, 64), VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(16, &voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);
    let body_mut = world.physics.get_body_mut(body_id).unwrap();
    body_mut.state = omnisia::physics::DynamicBodyState::Settled;

    let initial_voxels = world.physics.total_dynamic_voxels();

    // Reintegrasi harus ditolak
    let reintegrated = world
        .physics
        .process_settled_reintegration(&mut world.store);
    assert!(
        reintegrated.is_empty(),
        "Reintegrasi ke chunk belum termuat harus ditolak!"
    );

    // INVARIAN 8C.6 & Failure Injection C:
    // Badan dinamis tidak boleh dihapus, 0 voxel hilang
    assert!(world.physics.contains_body(body_id));
    assert_eq!(world.physics.total_dynamic_voxels(), initial_voxels);
}

#[test]
fn test_8c6_reintegration_failure_injection_occupied_destination() {
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));

    let mut chunk = Chunk::new(IVec3::ZERO);
    // Isi koordinat (5, 5, 5) dengan batu statis
    chunk.set_voxel(5, 5, 5, VoxelBlock::new(MaterialId::STONE));
    world.store.insert(chunk);

    // Badan dinamis yang mencoba reintegrasi ke lokasi yang sama (5, 5, 5)
    let voxels = vec![(IVec3::new(5, 5, 5), VoxelBlock::new(MaterialId::OAK_WOOD))];
    let agg = DetachedAggregate::from_world_voxels(17, &voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);
    let body_mut = world.physics.get_body_mut(body_id).unwrap();
    body_mut.state = omnisia::physics::DynamicBodyState::Settled;

    // Reintegrasi harus ditolak karena lokasi telah terisi
    let reintegrated = world
        .physics
        .process_settled_reintegration(&mut world.store);
    assert!(
        reintegrated.is_empty(),
        "Reintegrasi ke lokasi yang telah terisi harus ditolak!"
    );

    // INVARIAN 8C.6 & Failure Injection C:
    // 1. Badan dinamis tetap ada
    assert!(world.physics.contains_body(body_id));
    // 2. Voxel statis asli tidak tertimpa
    let existing = world.store.get_voxel_world(IVec3::new(5, 5, 5));
    assert_eq!(existing.material, MaterialId::STONE);
}

#[test]
fn test_8c6_save_load_roundtrip_preserves_reintegrated_aggregates_and_palette() {
    use omnisia::storage::{MemoryCompressedRegionStore, RegionStore};
    use omnisia::structure::aggregate::DetachedAggregate;

    let mut world = World::with_seed(WorldSeed(123));

    let chunk = Chunk::new(IVec3::ZERO);
    world.store.insert(chunk);

    // Buat badan dinamis dengan material berbeda
    let voxels = vec![
        (IVec3::new(2, 2, 2), VoxelBlock::new(MaterialId::OAK_WOOD)),
        (
            IVec3::new(2, 3, 2),
            VoxelBlock::new(MaterialId::METAL_FRAME),
        ),
    ];
    let agg = DetachedAggregate::from_world_voxels(18, &voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);
    let body_mut = world.physics.get_body_mut(body_id).unwrap();
    body_mut.state = omnisia::physics::DynamicBodyState::Settled;

    // Reintegrasikan ke ChunkStore
    let reintegrated = world
        .physics
        .process_settled_reintegration(&mut world.store);
    assert_eq!(reintegrated.len(), 1);

    // Simpan chunk ke storage dengan palette material
    let storage = MemoryCompressedRegionStore::new();
    let chunk_ref = world.store.get(&IVec3::ZERO).unwrap();
    storage.save_chunk(chunk_ref, &world.materials).unwrap();

    // Muat kembali chunk dari storage ke chunk baru dengan palette material
    let loaded_chunk = storage
        .load_chunk(IVec3::ZERO, &world.materials)
        .unwrap()
        .unwrap();

    // INVARIAN 8C.6 & Section 28:
    // Voxel yang direintegrasi tersimpan dan termuat kembali dengan integritas material 100%
    let v1 = loaded_chunk.get_voxel(2, 2, 2);
    let v2 = loaded_chunk.get_voxel(2, 3, 2);
    assert_eq!(v1.material, MaterialId::OAK_WOOD);
    assert_eq!(v2.material, MaterialId::METAL_FRAME);
}
