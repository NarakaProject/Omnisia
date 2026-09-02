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

    // Jalankan 15 tick (0.5 detik) -> jarak teoritis: 5.0 m/s * 0.5s = 2.5m
    for _ in 0..15 {
        world.update_player(&mut player, 1.0 / 30.0, 0.0);
    }

    // INVARIAN 8C.1: Pemain harus maju secara stabil tanpa jatuh atau keluar dari ground
    assert!(
        (player.state.position.x - 4.5).abs() < 0.05,
        "Pemain harus menempuh ~2.5m di sumbu X, terukur x = {}",
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
