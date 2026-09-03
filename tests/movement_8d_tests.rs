use glam::{IVec3, Vec3};
use omnisia::chunk::Chunk;
use omnisia::material::MaterialId;
use omnisia::physics::{DynamicBodyState, PhysicsRuntime};
use omnisia::player::collision::{check_ground_support, check_ground_support_with_physics};
use omnisia::player::{PlayerController, PlayerInput};
use omnisia::streaming::store::ChunkStore;
use omnisia::structure::aggregate::DetachedAggregate;
use omnisia::voxel::VoxelBlock;
use omnisia::world::World;
use omnisia::worldgen::seed::WorldSeed;

/// Menyiapkan lantai dasar solid di y = 0 (permukaan y = 0.5m)
fn setup_flat_ground(store: &mut ChunkStore, chunk_coord: IVec3) {
    let mut chunk = Chunk::new(chunk_coord);
    for vx in 0..32 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);
}

#[test]
fn test_8d1_flat_ground_movement_no_vertical_drift() {
    let mut store = ChunkStore::new();
    setup_flat_ground(&mut store, IVec3::ZERO);

    let mut player = PlayerController::new(Vec3::new(4.0, 0.5, 4.0));
    player.state.grounded = true;

    // Gerak maju lurus selama 30 tick (1.0s)
    let input = PlayerInput {
        move_forward: 1.0,
        ..Default::default()
    };
    player.set_input(input);

    for _ in 0..30 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }

    assert!(player.state.grounded);
    assert!(
        (player.state.position.y - 0.5).abs() < 1e-3,
        "Pemain di lantai datar tidak boleh mengalami pergeseran vertikal! Posisi Y: {}",
        player.state.position.y
    );
    assert!(player.state.position.x > 4.0);
}

#[test]
fn test_8d1_one_voxel_ledge_auto_step_success() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    // Lantai dasar di y = 0 (permukaan y = 0.5m)
    for vx in 0..32 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Undakan 1-voxel (0.5m) di x >= 8, y = 1 (permukaan atas y = 1.0m)
    for vx in 8..16 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);

    // Pemain spawn di x = 3.0m (sebelum undakan x = 4.0m)
    let mut player = PlayerController::new(Vec3::new(3.0, 0.5, 5.0));
    player.state.grounded = true;

    let input = PlayerInput {
        move_forward: 1.0, // maju ke arah +X (yaw = 0)
        ..Default::default()
    };
    player.set_input(input);

    // Berjalan menuju undakan selama 30 tick
    for _ in 0..30 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }

    assert!(player.state.grounded);
    assert!(
        player.state.position.x > 4.5,
        "Pemain harus berhasil menaiki undakan dan melintas ke x > 4.5! Posisi X: {}",
        player.state.position.x
    );
    assert!(
        (player.state.position.y - 1.0).abs() < 1e-2,
        "Pemain harus berada tepat di atas undakan (y = 1.0m)! Posisi Y: {}",
        player.state.position.y
    );
}

#[test]
fn test_8d1_two_voxel_wall_blocked() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    // Lantai dasar di y = 0
    for vx in 0..32 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Dinding 2-voxel (1.0m) di x = 8..16, y = 1..=2 (permukaan atas y = 1.5m, rise = 1.0m > 0.55m step_height)
    for vx in 8..16 {
        for vz in 0..32 {
            for vy in 1..=2 {
                chunk.set_voxel(vx, vy, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
    }
    store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(3.0, 0.5, 5.0));
    player.state.grounded = true;

    let input = PlayerInput {
        move_forward: 1.0,
        ..Default::default()
    };
    player.set_input(input);

    for _ in 0..30 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }

    // Dinding 1.0m harus memblokir pemain!
    // Dinding mulai di x = 4.0m. Dengan radius 0.3m, posisi X pemain tidak boleh menembus x = 4.0 - 0.3 = 3.7m
    assert!(
        player.state.position.x <= 3.71,
        "Pemain harus terblokir oleh dinding 1.0m! Posisi X: {}",
        player.state.position.x
    );
    assert!(
        (player.state.position.y - 0.5).abs() < 1e-2,
        "Pemain tidak boleh memanjat dinding 1.0m! Posisi Y: {}",
        player.state.position.y
    );
}

#[test]
fn test_8d1_full_height_wall_no_infinite_climbing() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    // Lantai di y = 0
    for vx in 0..32 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Dinding vertikal penuh 5 voxel (2.5m) di x = 10..12
    for vx in 10..12 {
        for vz in 0..32 {
            for vy in 1..=5 {
                chunk.set_voxel(vx, vy, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
    }
    store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(4.0, 0.5, 5.0));
    player.state.grounded = true;

    let input = PlayerInput {
        move_forward: 1.0,
        ..Default::default()
    };
    player.set_input(input);

    // Dorong pemain terus ke dinding selama 90 tick (3.0 detik)
    for _ in 0..90 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
        assert!(
            (player.state.position.y - 0.5).abs() < 1e-2,
            "AMENDMENT 3: Pemain dilarang memanjat dinding vertikal tak berujung! Y: {}",
            player.state.position.y
        );
    }

    assert!(player.state.position.x <= 4.71);
}

#[test]
fn test_8d1_sprint_auto_step_no_tunneling() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..32 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Undakan 1-voxel di x = 12..32, y = 1 (x >= 6.0m)
    for vx in 12..32 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(4.0, 0.5, 5.0));
    player.state.grounded = true;

    // Sprint maju (9.0 m/s)
    let input = PlayerInput {
        move_forward: 1.0,
        sprint: true,
        ..Default::default()
    };
    player.set_input(input);

    for _ in 0..20 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }

    assert!(player.state.grounded);
    assert!(
        player.state.position.x > 6.5,
        "Pemain sprint harus melintasi undakan! Posisi X: {}",
        player.state.position.x
    );
    assert!(
        (player.state.position.y - 1.0).abs() < 1e-2,
        "Pemain sprint harus berada di y = 1.0m! Posisi Y: {}",
        player.state.position.y
    );
}

#[test]
fn test_8d1_diagonal_edge_auto_step() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..32 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Undakan diagonal di vx + vz >= 16, y = 1
    for vx in 0..32 {
        for vz in 0..32 {
            if vx + vz >= 16 {
                chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
    }
    store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(3.0, 0.5, 3.0));
    player.state.grounded = true;

    // Gerak diagonal (W + D)
    let input = PlayerInput {
        move_forward: 1.0,
        move_right: 1.0,
        ..Default::default()
    };
    player.set_input(input);

    for _ in 0..40 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }

    assert!(player.state.grounded);
    assert!(
        (player.state.position.y - 1.0).abs() < 1e-2,
        "Pemain gerak diagonal harus menaiki undakan diagonal ke y = 1.0m! Posisi Y: {}",
        player.state.position.y
    );
}

#[test]
fn test_8d1_low_ceiling_step_prevented() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..32 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Undakan 1-voxel di x = 8..16, y = 1 (y = 0.5m..1.0m)
    for vx in 8..16 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Langit-langit rendah di y = 5 (y = 2.5m, headroom hanya 2.5m - 0.5m = 2.0m.
    // Tetapi di atas undakan y = 1.0m, tinggi sisa hanya 2.5m - 1.0m = 1.5m < standing_height 1.8m!)
    for vx in 8..16 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 5, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(3.0, 0.5, 5.0));
    player.state.grounded = true;

    let input = PlayerInput {
        move_forward: 1.0,
        ..Default::default()
    };
    player.set_input(input);

    for _ in 0..30 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }

    // Step harus DITOLAK karena di atas undakan clearance kapsul berdiri (1.8m) tidak cukup!
    assert!(
        player.state.position.x <= 3.71,
        "Pemain tidak boleh menaiki undakan dengan langit-langit rendah! Posisi X: {}",
        player.state.position.x
    );
    assert!((player.state.position.y - 0.5).abs() < 1e-2);
}

#[test]
fn test_8d1_unloaded_boundary_step_rejected() {
    let mut store = ChunkStore::new();
    // Hanya muat chunk (0, 0, 0). Chunk tetangga (+1, 0, 0) belum dimuat (Unknown)!
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..32 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Undakan di perbatasan timur (vx = 31, y = 1)
    for vz in 0..32 {
        chunk.set_voxel(31, 1, vz, VoxelBlock::new(MaterialId::STONE));
    }
    store.insert(chunk);

    // Pemain berada di dekat perbatasan x = 30 * 0.5 = 15.0m
    let mut player = PlayerController::new(Vec3::new(14.8, 0.5, 5.0));
    player.state.grounded = true;

    let input = PlayerInput {
        move_forward: 1.0,
        ..Default::default()
    };
    player.set_input(input);

    for _ in 0..20 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }

    // AMENDMENT 10: Step ke arah Unknown harus ditolak keras!
    assert!(
        player.state.position.x <= 15.71,
        "Pemain dilarang menaiki undakan melintasi perbatasan unknown chunk! Posisi X: {}",
        player.state.position.x
    );
}

#[test]
fn test_8d1_negative_coordinates_auto_step() {
    let mut store = ChunkStore::new();
    // Chunk negatif (-1, 0, -1)
    let mut chunk = Chunk::new(IVec3::new(-1, 0, -1));
    for vx in 0..32 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Undakan di vx = 16..24, y = 1
    // Koordinat dunia: x = (-32 + 16) * 0.5 = -16 * 0.5 = -8.0m
    for vx in 16..24 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(-9.0, 0.5, -5.0));
    player.state.grounded = true;

    let input = PlayerInput {
        move_forward: 1.0,
        ..Default::default()
    };
    player.set_input(input);

    for _ in 0..30 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }

    assert!(player.state.grounded);
    assert!(
        player.state.position.x > -7.5,
        "Pemain harus berhasil menaiki undakan di koordinat negatif! Posisi X: {}",
        player.state.position.x
    );
    assert!(
        (player.state.position.y - 1.0).abs() < 1e-2,
        "Pemain harus berada di y = 1.0m pada koordinat negatif! Posisi Y: {}",
        player.state.position.y
    );
}

#[test]
fn test_8d1_dynamic_body_auto_step_support() {
    let mut world = World::with_seed(WorldSeed(801));
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..32 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    world.store.insert(chunk);

    // Buat gugusan 1-voxel tebal sebagai DynamicBody di x = 8..24, y = 1 (world x = 4.0m..12.0m, y = 0.5m)
    let mut voxels = Vec::new();
    for vx in 8..24 {
        for vz in 4..8 {
            voxels.push((IVec3::new(vx, 1, vz), VoxelBlock::new(MaterialId::STONE)));
        }
    }
    let agg = DetachedAggregate::from_world_voxels(801, &voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);
    let body_mut = world.physics.get_body_mut(body_id).unwrap();
    body_mut.state = DynamicBodyState::Settled;
    body_mut.is_grounded = true;

    let mut player = PlayerController::new(Vec3::new(3.0, 0.5, 3.0));
    player.state.grounded = true;

    let input = PlayerInput {
        move_forward: 1.0,
        ..Default::default()
    };
    player.set_input(input);

    for _ in 0..30 {
        world.update_player(&mut player, 1.0 / 30.0, 0.0);
    }

    assert!(player.state.grounded);
    assert!(
        player.state.position.x > 4.5,
        "Pemain harus berhasil menaiki badan dinamis 0.5m! Posisi X: {}",
        player.state.position.x
    );
    assert!(
        (player.state.position.y - 1.0).abs() < 1e-2,
        "Pemain harus bertumpu di atas DynamicBody pada y = 1.0m! Posisi Y: {}",
        player.state.position.y
    );
}

#[test]
fn test_8d1_airborne_touch_wall_no_climbing_and_no_sticky_lock() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..32 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Dinding tinggi di x = 10..12, y = 1..=10 (world x = 5.0m..6.0m)
    for vx in 10..12 {
        for vz in 0..32 {
            for vy in 1..=10 {
                chunk.set_voxel(vx, vy, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
    }
    store.insert(chunk);

    // Pemain spawn di x = 4.0m, y = 0.5m. Lakukan lompatan menuju dinding (+X)
    let mut player = PlayerController::new(Vec3::new(4.0, 0.5, 5.0));
    player.state.grounded = true;

    let input = PlayerInput {
        move_forward: 1.0,
        jump: true,
        ..Default::default()
    };
    player.set_input(input);

    // Tick 0: eksekusi lompat
    player.step_simulation(1.0 / 30.0, &store, 0.0);
    assert!(!player.state.grounded);
    assert!(player.state.velocity.y > 0.0);

    // Lanjutkan simulasi saat pemain melayang di udara dan menyentuh dinding
    let mut reached_apex = false;
    let mut landed_back = false;

    for _ in 1..60 {
        // Tetap tekan W menuju dinding
        let in_flight = PlayerInput {
            move_forward: 1.0,
            ..Default::default()
        };
        player.set_input(in_flight);

        player.step_simulation(1.0 / 30.0, &store, 0.0);

        // AMENDMENT 3 & 7: Tidak boleh auto-step atau mendaki dinding saat airborne!
        assert!(
            player.state.position.x <= 4.71,
            "Pemain tidak boleh menembus dinding saat airborne! Posisi X: {}",
            player.state.position.x
        );

        if player.state.velocity.y < 0.0 {
            reached_apex = true;
        }

        if reached_apex && player.state.grounded {
            landed_back = true;
            break;
        }
    }

    // AMENDMENT 8: Pemain tidak boleh sticky / locked di udara, harus mencapai puncak parabola dan mendarat kembali
    assert!(
        reached_apex,
        "Pemain harus mengalami transisi gravitasi normal melewati titik puncak!"
    );
    assert!(landed_back, "Pemain tidak boleh tersangkut (sticky lock) di dinding udara; harus jatuh dan mendarat kembali!");
    assert!(
        (player.state.position.y - 0.5).abs() < 1e-2,
        "Pemain harus mendarat kembali di lantai dasar y = 0.5m! Posisi Y: {}",
        player.state.position.y
    );
}

#[test]
fn test_8d1_auto_step_disabled_config() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..32 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Undakan 1-voxel di x = 8..16, y = 1
    for vx in 8..16 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);

    let config = omnisia::player::PlayerConfig {
        auto_step_enabled: false,
        ..Default::default()
    };

    let mut player = PlayerController::with_config(Vec3::new(3.0, 0.5, 5.0), config);
    player.state.grounded = true;

    let input = PlayerInput {
        move_forward: 1.0,
        ..Default::default()
    };
    player.set_input(input);

    for _ in 0..30 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }

    // Karena auto_step_enabled = false, pemain terblokir di depan undakan 0.5m
    assert!(
        player.state.position.x <= 3.71,
        "Ketika auto_step_enabled = false, pemain harus terblokir di undakan! Posisi X: {}",
        player.state.position.x
    );
    assert!((player.state.position.y - 0.5).abs() < 1e-2);
}

// =========================================================================
// 8D.2: EXPLICIT BOUNDED GLIDE MECHANIC TESTS
// =========================================================================

#[test]
fn test_8d2_grounded_shift_is_sprint_not_glide() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..32 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(4.0, 0.5, 4.0));
    player.state.grounded = true;

    // Shift + W di darat
    let input = PlayerInput {
        move_forward: 1.0,
        sprint: true,
        ..Default::default()
    };
    player.set_input(input);
    player.step_simulation(1.0 / 30.0, &store, 0.0);

    // AMENDMENT 12 & 13: Di darat, Shift adalah SPRINT murni, BUKAN GLIDE!
    assert!(player.state.grounded);
    assert!(player.state.sprinting);
    assert!(!player.state.gliding);
    assert!((player.state.speed() - 9.0).abs() < 1e-2);
}

#[test]
fn test_8d2_glide_activation_airborne_only() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..32 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(4.0, 0.5, 4.0));
    player.state.grounded = true;

    // Lompat ke udara
    let jump_input = PlayerInput {
        jump: true,
        ..Default::default()
    };
    player.set_input(jump_input);
    player.step_simulation(1.0 / 30.0, &store, 0.0);
    assert!(!player.state.grounded);

    // Di udara tanpa Shift: bukan sprint dan bukan glide
    let air_input_no_shift = PlayerInput {
        move_forward: 1.0,
        ..Default::default()
    };
    player.set_input(air_input_no_shift);
    player.step_simulation(1.0 / 30.0, &store, 0.0);
    assert!(!player.state.sprinting);
    assert!(!player.state.gliding);

    // Di udara dengan Shift: Glide aktif, Sprint mati!
    let air_input_shift = PlayerInput {
        move_forward: 1.0,
        sprint: true,
        ..Default::default()
    };
    player.set_input(air_input_shift);
    player.step_simulation(1.0 / 30.0, &store, 0.0);
    assert!(
        !player.state.sprinting,
        "Airborne dilarang berstatus sprinting!"
    );
    assert!(
        player.state.gliding,
        "Airborne + Shift harus mengaktifkan gliding!"
    );
}

#[test]
fn test_8d2_glide_deactivation_on_release_shift() {
    let mut store = ChunkStore::new();
    for cy in 0..=2 {
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

    // Spawn tinggi di udara
    let mut player = PlayerController::new(Vec3::new(4.0, 20.0, 4.0));
    player.state.grounded = false;

    // Aktifkan glide
    let glide_input = PlayerInput {
        move_forward: 1.0,
        sprint: true,
        ..Default::default()
    };
    player.set_input(glide_input);
    player.step_simulation(1.0 / 30.0, &store, 0.0);
    assert!(player.state.gliding);

    // Lepas Shift
    let release_input = PlayerInput {
        move_forward: 1.0,
        sprint: false,
        ..Default::default()
    };
    player.set_input(release_input);
    player.step_simulation(1.0 / 30.0, &store, 0.0);

    // AMENDMENT 15: Melepas shift seketika mematikan glide
    assert!(!player.state.gliding);
    assert!(!player.state.sprinting);
}

#[test]
fn test_8d2_glide_deactivation_on_landing() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..32 {
        for vz in 0..32 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);

    // Spawn dekat dengan tanah (y = 0.55m, permukaan tanah = 0.5m)
    let mut player = PlayerController::new(Vec3::new(4.0, 0.55, 4.0));
    player.state.grounded = false;

    let glide_input = PlayerInput {
        move_forward: 1.0,
        sprint: true,
        ..Default::default()
    };
    player.set_input(glide_input);

    // Step hingga mendarat (dalam beberapa tick)
    for _ in 0..10 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
        if player.state.grounded {
            break;
        }
    }

    // AMENDMENT 15: Begitu mendarat, glide seketika mati
    assert!(player.state.grounded);
    assert!(
        !player.state.gliding,
        "Glide harus mati seketika saat menyentuh tanah!"
    );
    // Karena Shift + W masih ditekan dan sudah di tanah, sekarang menjadi Sprint
    assert!(player.state.sprinting);
}

#[test]
fn test_8d2_glide_fall_speed_bounded() {
    let mut store = ChunkStore::new();
    // Muat kolom chunk vertikal hingga cy = 6 (ketinggian hingga 112m)
    for cy in 0..=6 {
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

    // Spawn sangat tinggi di y = 100.0m
    let mut player_glide = PlayerController::new(Vec3::new(4.0, 100.0, 4.0));
    player_glide.state.grounded = false;

    let glide_input = PlayerInput {
        move_forward: 1.0,
        sprint: true,
        ..Default::default()
    };
    player_glide.set_input(glide_input);

    // Simulasi selama 60 tick (2.0 detik)
    for _ in 0..60 {
        player_glide.step_simulation(1.0 / 30.0, &store, 0.0);
        // AMENDMENT 14: Kecepatan jatuh ke bawah tidak boleh melebihi glide_max_downward_speed (2.5 m/s)
        assert!(
            player_glide.state.velocity.y >= -2.501,
            "Kecepatan jatuh glide terlampaui! Vy: {}",
            player_glide.state.velocity.y
        );
    }

    // Pemain tanpa glide pada ketinggian yang sama akan jatuh jauh lebih cepat
    let mut player_fall = PlayerController::new(Vec3::new(4.0, 100.0, 4.0));
    player_fall.state.grounded = false;
    for _ in 0..60 {
        player_fall.step_simulation(1.0 / 30.0, &store, 0.0);
    }
    assert!(
        player_fall.state.velocity.y < -15.0,
        "Pemain tanpa glide harus jatuh bebas lebih cepat daripada -15 m/s; didapat: {}",
        player_fall.state.velocity.y
    );
    assert!(player_glide.state.position.y > player_fall.state.position.y + 15.0);
}

#[test]
fn test_8d2_glide_no_upward_acceleration() {
    let mut store = ChunkStore::new();
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
    player.state.velocity = Vec3::new(0.0, -1.0, 0.0);

    let glide_input = PlayerInput {
        move_forward: 1.0,
        sprint: true,
        ..Default::default()
    };
    player.set_input(glide_input);

    let prev_y = player.state.position.y;
    player.step_simulation(1.0 / 30.0, &store, 0.0);

    // AMENDMENT 14: Glide tidak boleh menghasilkan akselerasi ke atas
    assert!(
        player.state.position.y < prev_y,
        "Glide tidak boleh mengangkat pemain ke atas!"
    );
}

#[test]
fn test_8d2_glide_disabled_config() {
    let mut store = ChunkStore::new();
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

    let config = omnisia::player::PlayerConfig {
        glide_enabled: false,
        ..Default::default()
    };

    let mut player = PlayerController::with_config(Vec3::new(4.0, 50.0, 4.0), config);
    player.state.grounded = false;

    let input = PlayerInput {
        move_forward: 1.0,
        sprint: true,
        ..Default::default()
    };
    player.set_input(input);
    player.step_simulation(1.0 / 30.0, &store, 0.0);

    // Dengan glide_enabled = false, glide tidak boleh aktif
    assert!(!player.state.gliding);
    // Dan akselerasi gravitasi penuh diterapkan
    let expected_vy = -9.81 * (1.0 / 30.0);
    assert!((player.state.velocity.y - expected_vy).abs() < 1e-3);
}

#[test]
fn test_8d2_glide_air_control_bounded() {
    let mut store = ChunkStore::new();
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

    let input = PlayerInput {
        move_forward: 1.0,
        sprint: true,
        ..Default::default()
    };
    player.set_input(input);
    player.step_simulation(1.0 / 30.0, &store, 0.0);

    // Target kecepatan horizontal glide = sprint_speed (9.0) * glide_air_control (0.85) = 7.65 m/s
    let horiz_speed = (player.state.velocity.x * player.state.velocity.x
        + player.state.velocity.z * player.state.velocity.z)
        .sqrt();
    let expected_glide_speed = 9.0 * 0.85;
    assert!(
        (horiz_speed - expected_glide_speed).abs() < 1e-2,
        "Kecepatan horizontal glide harus terikat air control! Didapat: {}, Ekspektasi: {}",
        horiz_speed,
        expected_glide_speed
    );
}

// ============================================================================
// PHASE 8D.4 — GROUNDING & TERRAIN CONTACT HARDENING TEST SUITE
// ============================================================================

// ----------------------------------------------------------------------------
// 1. FLAT GROUND CONTACT
// ----------------------------------------------------------------------------

#[test]
fn test_8d4_flat_ground_centered_support() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    // Voxel lantai di (4, 0, 4) -> permukaan atas y = 0.5m
    chunk.set_voxel(4, 0, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    let feet_pos = Vec3::new(2.25, 0.50, 2.25);
    let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
    assert!(res.grounded);
    assert_eq!(res.ground_y_surface, Some(0.5));
    assert_eq!(res.stable_feet_y, Some(0.5));
    assert_eq!(res.support_voxel, Some(IVec3::new(4, 0, 4)));
    assert!((res.ground_distance - 0.0).abs() < 1e-5);
}

#[test]
fn test_8d4_flat_ground_small_gap_tolerance() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    chunk.set_voxel(4, 0, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    // Kaki melayang 0.03m di atas tanah (0.03m <= epsilon 0.05m)
    let feet_pos = Vec3::new(2.25, 0.53, 2.25);
    let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
    assert!(res.grounded);
    assert!((res.ground_distance - 0.03).abs() < 1e-4);
    assert_eq!(res.stable_feet_y, Some(0.5));
}

#[test]
fn test_8d4_flat_ground_penetration_tolerance() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    chunk.set_voxel(4, 0, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    // Kaki sedikit penetrasi 0.02m ke bawah permukaan (0.02m <= penetration_tolerance 0.03m)
    let feet_pos = Vec3::new(2.25, 0.48, 2.25);
    let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
    assert!(res.grounded);
    assert_eq!(res.stable_feet_y, Some(0.5));
}

#[test]
fn test_8d4_flat_ground_beyond_tolerance_rejected() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    chunk.set_voxel(4, 0, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    // 1. Celah udara terlalu besar (0.07m > epsilon 0.05m)
    let feet_too_high = Vec3::new(2.25, 0.57, 2.25);
    assert!(!check_ground_support(feet_too_high, 0.30, 0.05, &store).grounded);

    // 2. Penetrasi terlalu dalam (0.07m > penetration_tolerance 0.05m)
    let feet_too_low = Vec3::new(2.25, 0.43, 2.25);
    assert!(!check_ground_support(feet_too_low, 0.30, 0.05, &store).grounded);
}

// ----------------------------------------------------------------------------
// 2. EDGE CONTACT & GEOMETRY ASSERTION
// ----------------------------------------------------------------------------

#[test]
fn test_8d4_edge_partial_overhang_grounded() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    // Voxel solid di vx = 4 (x: 2.0m .. 2.5m), permukaan atas 1.0m (vy = 1)
    chunk.set_voxel(4, 1, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    // Pusat kaki pemain berada di luar tepi voxel (x = 2.60m, offset d = 0.10m), z = 2.25m
    let feet_pos = Vec3::new(2.60, 0.98, 2.25);
    let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
    assert!(
        res.grounded,
        "Pemain yang sebagian menjorok keluar tepi harus tetap grounded!"
    );
    assert_eq!(res.support_voxel, Some(IVec3::new(4, 1, 4)));
    assert_eq!(res.ground_y_surface, Some(1.0));
}

#[test]
fn test_8d4_edge_lower_hemisphere_contact_geometry() {
    // MANDATORY GEOMETRY ASSERTION (Section 15):
    // support_surface_y = 1.0m, d > 0 => stable_feet_y < support_surface_y
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    chunk.set_voxel(4, 1, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    // Voxel x: [2.0, 2.5], permukaan 1.0m.
    // Posisi pemain di x = 2.65m (offset d = 0.15m dari tepi x = 2.50m)
    let feet_pos = Vec3::new(2.65, 0.96, 2.25);
    let res = check_ground_support(feet_pos, 0.30, 0.05, &store);

    assert!(res.grounded);
    let support_y = res.ground_y_surface.unwrap();
    let stable_feet = res.stable_feet_y.unwrap();

    assert_eq!(support_y, 1.0);
    // d = 0.15m, r = 0.30m => y_offset = sqrt(0.09 - 0.0225) = 0.2598m
    // stable_feet_y = 1.0 - (0.30 - 0.2598) = 0.9598m < 1.0m
    assert!(
        stable_feet < support_y,
        "HARD INVARIANT: Pada kontak tepi (d > 0), stable_feet_y ({}) harus < support_surface_y ({})!",
        stable_feet,
        support_y
    );
    assert!(
        (stable_feet - 0.9598).abs() < 1e-3,
        "stable_feet_y dihitung: {}, ekspektasi: 0.9598",
        stable_feet
    );
}

#[test]
fn test_8d4_edge_controller_snaps_to_stable_feet_no_upward_teleport() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    chunk.set_voxel(4, 1, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    // Spawn controller di tepi balok (x = 2.65m, d = 0.15m)
    let mut controller = PlayerController::new(Vec3::new(2.65, 0.96, 2.25));
    controller.step_simulation(1.0 / 30.0, &store, 0.0);

    assert!(controller.state.grounded);
    // HARD INVARIANT: Posisi kaki pengontrol harus sama persis dengan stable_feet_y (~0.96m),
    // BUKAN terangkat/teleportasi ke support_surface_y (1.00m)!
    assert!(
        (controller.state.position.y - 0.9598).abs() < 1e-3,
        "Pengontrol dilarang snap ke support_surface_y! Terukur: {}, Ekspektasi stable_feet_y: ~0.9598",
        controller.state.position.y
    );
}

#[test]
fn test_8d4_edge_deep_overhang_loss_of_support_becomes_airborne() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    chunk.set_voxel(4, 1, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    // Pemain menjorok terlalu jauh (x = 2.85m, d = 0.35m > radius 0.30m)
    let feet_overhang = Vec3::new(2.85, 0.96, 2.25);
    let res = check_ground_support(feet_overhang, 0.30, 0.05, &store);
    assert!(
        !res.grounded,
        "Pemain yang seluruh footprint-nya di luar tepi harus airborne!"
    );
}

// ----------------------------------------------------------------------------
// 3. UNEVEN TERRAIN, STAIRCASE, HILL & RIVER BANK
// ----------------------------------------------------------------------------

#[test]
fn test_8d4_hill_voxel_terrain_grounded() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    // Buat lereng bertingkat (hill):
    // vx = 4: vy = 0 (surface 0.5m)
    // vx = 5: vy = 1 (surface 1.0m)
    // vx = 6: vy = 2 (surface 1.5m)
    for vz in 0..10 {
        chunk.set_voxel(4, 0, vz, VoxelBlock::new(MaterialId::STONE));
        chunk.set_voxel(5, 1, vz, VoxelBlock::new(MaterialId::STONE));
        chunk.set_voxel(6, 2, vz, VoxelBlock::new(MaterialId::STONE));
    }
    store.insert(chunk);

    // Berdiri di lereng vx = 5, vy = 1 (surface 1.0m)
    let feet_slope = Vec3::new(2.75, 1.00, 2.50);
    let res = check_ground_support(feet_slope, 0.30, 0.05, &store);
    assert!(res.grounded);
    assert_eq!(res.ground_y_surface, Some(1.0));
}

#[test]
fn test_8d4_staircase_ascending_continuous_grounding() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    // Tangga 1-voxel: x = 4..8, masing-masing undakan naik 1 voxel (0.5m)
    for step in 0..4 {
        let vx = 4 + step;
        let vy = step;
        for vz in 0..10 {
            for fill_y in 0..=vy {
                chunk.set_voxel(vx, fill_y, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
    }
    store.insert(chunk);

    // Uji grounding di setiap pijakan tangga
    for step in 0..4 {
        let x = (4 + step) as f32 * 0.5 + 0.25;
        let y = (step + 1) as f32 * 0.5;
        let res = check_ground_support(Vec3::new(x, y, 2.5), 0.30, 0.05, &store);
        assert!(res.grounded, "Pijakan tangga step {} harus grounded!", step);
        assert_eq!(res.ground_y_surface, Some(y));
    }
}

#[test]
fn test_8d4_staircase_descending_continuous_grounding() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for step in 0..4 {
        let vx = 4 + step;
        let vy = 3 - step;
        for vz in 0..10 {
            for fill_y in 0..=vy {
                chunk.set_voxel(vx, fill_y, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
    }
    store.insert(chunk);

    for step in 0..4 {
        let x = (4 + step) as f32 * 0.5 + 0.25;
        let y = (3 - step + 1) as f32 * 0.5;
        let res = check_ground_support(Vec3::new(x, y, 2.5), 0.30, 0.05, &store);
        assert!(res.grounded, "Turunan tangga step {} harus grounded!", step);
        assert_eq!(res.ground_y_surface, Some(y));
    }
}

#[test]
fn test_8d4_river_bank_boundary_grounded() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    // Bantaran sungai: vx = 0..4 adalah tebing (y = 1.0m, vy = 1), vx >= 5 adalah dasar air (y = 0.5m, vy = 0)
    for vx in 0..=4 {
        for vz in 0..10 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
            chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    for vx in 5..10 {
        for vz in 0..10 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);

    // Pemain berdiri tepat di tepi bantaran tebing sungai (x = 2.40m, tepi di 2.50m)
    let feet_bank = Vec3::new(2.40, 1.00, 2.50);
    let res = check_ground_support(feet_bank, 0.30, 0.05, &store);
    assert!(
        res.grounded,
        "Pemain di tepi bantaran sungai harus grounded!"
    );
    assert_eq!(res.ground_y_surface, Some(1.0));
}

#[test]
fn test_8d4_uneven_multi_height_footprint_grounded() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    // Dua balok bersebelahan dengan ketinggian berbeda dalam satu footprint:
    // Balok A di vx = 4: vy = 0 (surface 0.5m)
    // Balok B di vx = 5: vy = 1 (surface 1.0m)
    chunk.set_voxel(4, 0, 4, VoxelBlock::new(MaterialId::STONE));
    chunk.set_voxel(5, 0, 4, VoxelBlock::new(MaterialId::STONE));
    chunk.set_voxel(5, 1, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    // Pemain bertumpu pada balok B yang lebih tinggi (x = 2.55m, y = 1.0m)
    let feet_pos = Vec3::new(2.55, 1.00, 2.25);
    let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
    assert!(res.grounded);
    assert_eq!(res.ground_y_surface, Some(1.0));
    assert_eq!(res.support_voxel, Some(IVec3::new(5, 1, 4)));
}

// ----------------------------------------------------------------------------
// 4. CANDIDATE SELECTION & DETERMINISM
// ----------------------------------------------------------------------------

#[test]
fn test_8d4_candidate_selection_deterministic_order_independence() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 4..=6 {
        for vz in 4..=6 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);

    let feet_pos = Vec3::new(2.75, 0.50, 2.75);
    let res1 = check_ground_support(feet_pos, 0.30, 0.05, &store);
    let res2 = check_ground_support(feet_pos, 0.30, 0.05, &store);

    assert!(res1.grounded);
    assert_eq!(res1.support_voxel, res2.support_voxel);
    assert_eq!(res1.ground_y_surface, res2.ground_y_surface);
    assert_eq!(res1.stable_feet_y, res2.stable_feet_y);
    assert_eq!(res1.ground_distance, res2.ground_distance);
}

#[test]
fn test_8d4_multiple_valid_surfaces_selects_closest() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 4..=5 {
        for vz in 4..=5 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);

    // Kaki lebih dekat ke voxel (5, 0, 5) daripada (4, 0, 4)
    let feet_pos = Vec3::new(2.70, 0.50, 2.70);
    let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
    assert!(res.grounded);
    assert_eq!(res.support_voxel, Some(IVec3::new(5, 0, 5)));
}

// ----------------------------------------------------------------------------
// 5. SIDE WALL REJECTION
// ----------------------------------------------------------------------------

#[test]
fn test_8d4_side_wall_0_5m_not_grounded() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    // Dinding di samping pemain (x = 2.5m .. 3.0m, vy = 1), tetapi pemain berada di udara bebas pada y = 0.2m
    chunk.set_voxel(5, 1, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    // Pemain di udara bebas pada y = 0.2m di samping dinding 1.0m (bukan lantai)
    let feet_side = Vec3::new(2.30, 0.20, 2.25);
    let res = check_ground_support(feet_side, 0.30, 0.05, &store);
    assert!(
        !res.grounded,
        "Dinding di samping tidak boleh membuat pemain grounded!"
    );
}

#[test]
fn test_8d4_side_wall_1_0m_not_grounded() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    chunk.set_voxel(5, 1, 4, VoxelBlock::new(MaterialId::STONE));
    chunk.set_voxel(5, 2, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    let feet_side = Vec3::new(2.30, 0.50, 2.25);
    let res = check_ground_support(feet_side, 0.30, 0.05, &store);
    assert!(
        !res.grounded,
        "Dinding 1.0m di samping tidak boleh membuat pemain grounded!"
    );
}

#[test]
fn test_8d4_side_wall_2_0m_not_grounded() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vy in 1..=4 {
        chunk.set_voxel(5, vy, 4, VoxelBlock::new(MaterialId::STONE));
    }
    store.insert(chunk);

    let feet_side = Vec3::new(2.30, 1.00, 2.25);
    let res = check_ground_support(feet_side, 0.30, 0.05, &store);
    assert!(
        !res.grounded,
        "Dinding tinggi 2.0m di samping tidak boleh membuat pemain grounded!"
    );
}

// ----------------------------------------------------------------------------
// 6. UNKNOWN != AIR HARD INVARIANTS
// ----------------------------------------------------------------------------

#[test]
fn test_8d4_unknown_below_player_never_grounded() {
    // ChunkStore kosong (Unknown)
    let store = ChunkStore::new();
    let feet_pos = Vec3::new(4.0, 1.0, 4.0);
    let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
    assert!(
        !res.grounded,
        "Unknown di bawah pemain dilarang menghasilkan grounded=true!"
    );
    assert_eq!(res.stable_feet_y, None);
}

#[test]
fn test_8d4_voxel_above_unknown_never_grounded() {
    // Balok tumpuan di chunk (0, 0, 0) y = 31 (vy = 31 -> surface 16.0m).
    // Chunk di atasnya (0, 1, 0) TIDAK DIMUAT (Unknown)!
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::new(0, 0, 0));
    chunk.set_voxel(4, 31, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    // Pemain berada di y = 16.0m tepat di atas voxel 31.
    // Karena voxel di atasnya adalah Unknown (bukan Known Air), kandidat HARUS DITOLAK!
    let feet_pos = Vec3::new(2.25, 16.00, 2.25);
    let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
    assert!(
        !res.grounded,
        "HARD INVARIANT: Jika voxel di atas adalah Unknown, tumpuan harus ditolak!"
    );
}

#[test]
fn test_8d4_mixed_known_unknown_footprint_safety() {
    // Chunk (0, 0, 0) dimuat, chunk (1, 0, 0) tidak dimuat (Unknown)
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vz in 0..10 {
        chunk.set_voxel(31, 0, vz, VoxelBlock::new(MaterialId::STONE));
    }
    store.insert(chunk);

    // Kaki di x = 15.95m (bersandar pada voxel 31 chunk 0, footprint menjangkau chunk 1 yang Unknown)
    let feet_pos = Vec3::new(15.95, 0.50, 2.25);
    let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
    // Harus bertumpu aman pada Known Solid chunk 0 tanpa crash atau error dari Unknown chunk 1
    assert!(res.grounded);
    assert_eq!(res.support_voxel, Some(IVec3::new(31, 0, 4)));
}

// ----------------------------------------------------------------------------
// 7. NEGATIVE COORDINATES & BOUNDARY FLOATS
// ----------------------------------------------------------------------------

#[test]
fn test_8d4_negative_x_grounding() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::new(-1, 0, 0));
    chunk.set_voxel(30, 0, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    // x di koordinat negatif: chunk -1, vx = 30 -> world x = -1 * 16 + 15 = -1.0m
    let feet_pos = Vec3::new(-0.75, 0.50, 2.25);
    let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
    assert!(res.grounded);
    assert_eq!(res.ground_y_surface, Some(0.5));
}

#[test]
fn test_8d4_negative_z_grounding() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::new(0, 0, -1));
    chunk.set_voxel(4, 0, 30, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    let feet_pos = Vec3::new(2.25, 0.50, -0.75);
    let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
    assert!(res.grounded);
    assert_eq!(res.ground_y_surface, Some(0.5));
}

#[test]
fn test_8d4_negative_chunk_boundary_grounding() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::new(-1, 0, -1));
    chunk.set_voxel(31, 2, 31, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    let feet_pos = Vec3::new(-0.25, 1.50, -0.25);
    let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
    assert!(res.grounded);
    assert_eq!(res.support_voxel, Some(IVec3::new(-1, 2, -1)));
    assert_eq!(res.ground_y_surface, Some(1.5));
    assert_eq!(res.stable_feet_y, Some(1.5));
}

#[test]
fn test_8d4_negative_y_surface_grounding() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::new(0, -1, 0));
    // Chunk -1: vy = 30 -> world vy = -1*32 + 30 = -2.
    // surface_y = (-2 + 1) * 0.5 = -0.5m
    chunk.set_voxel(4, 30, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    let feet_pos = Vec3::new(2.25, -0.50, 2.25);
    let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
    assert!(res.grounded);
    assert_eq!(res.ground_y_surface, Some(-0.5));
    assert_eq!(res.stable_feet_y, Some(-0.5));
}

#[test]
fn test_8d4_exact_voxel_boundaries_flooring_math() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    chunk.set_voxel(4, 1, 4, VoxelBlock::new(MaterialId::STONE)); // surface 1.0m (air above)
    chunk.set_voxel(8, 2, 8, VoxelBlock::new(MaterialId::STONE)); // surface 1.5m (air above)
    store.insert(chunk);

    // Uji nilai batas kritis sesuai Section 3 MEGAPROMPT
    // Kolom 1: surface 1.0m
    let test_heights_1m = [0.999999, 1.0, 1.000001];
    for &y in &test_heights_1m {
        let feet_pos = Vec3::new(2.25, y, 2.25);
        let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
        assert!(
            res.grounded,
            "Grounding harus stabil pada batas float y = {}",
            y
        );
    }

    // Kolom 2: surface 1.5m
    let test_heights_1_5m = [1.499999, 1.5, 1.500001];
    for &y in &test_heights_1_5m {
        let feet_pos = Vec3::new(4.25, y, 4.25);
        let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
        assert!(
            res.grounded,
            "Grounding harus stabil pada batas float y = {}",
            y
        );
    }
}

// ----------------------------------------------------------------------------
// 8. DYNAMIC BODY SUPPORT
// ----------------------------------------------------------------------------

#[test]
fn test_8d4_dynamic_body_flat_grounded() {
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
    let agg = DetachedAggregate::from_world_voxels(901, &voxels).unwrap();
    let body_id = physics.spawn_from_detached_aggregate(agg);
    let body_mut = physics.get_body_mut(body_id).unwrap();
    body_mut.state = DynamicBodyState::Settled;
    body_mut.is_grounded = true;

    // Pemain berdiri di atas DynamicBody pada y = 1.0m (center-supported)
    let feet_pos = Vec3::new(1.0, 1.0, 1.0);
    let res = check_ground_support_with_physics(feet_pos, 0.30, 0.05, &store, Some(&physics));
    assert!(res.grounded);
    assert_eq!(res.ground_y_surface, Some(1.0));
    assert_eq!(res.stable_feet_y, Some(1.0));
}

#[test]
fn test_8d4_dynamic_body_edge_grounded_stable_feet() {
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
    // Voxel x: [0, 1] -> world x: [0.0, 1.0], surface 1.0m
    for vx in 0..2 {
        for vz in 0..2 {
            voxels.push((IVec3::new(vx, 1, vz), VoxelBlock::new(MaterialId::STONE)));
        }
    }
    let agg = DetachedAggregate::from_world_voxels(902, &voxels).unwrap();
    let body_id = physics.spawn_from_detached_aggregate(agg);
    let body_mut = physics.get_body_mut(body_id).unwrap();
    body_mut.state = DynamicBodyState::Settled;
    body_mut.is_grounded = true;

    // Pemain di tepi DynamicBody pada x = 1.10m (offset d = 0.10m dari tepi 1.00m)
    let feet_pos = Vec3::new(1.10, 0.98, 0.50);
    let res = check_ground_support_with_physics(feet_pos, 0.30, 0.05, &store, Some(&physics));
    assert!(res.grounded);
    assert_eq!(res.ground_y_surface, Some(1.0));
    assert!(res.stable_feet_y.unwrap() < 1.0);
}

#[test]
fn test_8d4_dynamic_body_side_wall_not_grounded() {
    let mut store = ChunkStore::new();
    let chunk = Chunk::new(IVec3::ZERO);
    store.insert(chunk);

    let mut physics = PhysicsRuntime::default();
    let mut voxels = Vec::new();
    // DynamicBody di x = [4.0, 5.0], y = [1.0, 2.0]
    for vx in 8..10 {
        for vy in 2..4 {
            voxels.push((IVec3::new(vx, vy, 4), VoxelBlock::new(MaterialId::STONE)));
        }
    }
    let agg = DetachedAggregate::from_world_voxels(903, &voxels).unwrap();
    let body_id = physics.spawn_from_detached_aggregate(agg);
    let body_mut = physics.get_body_mut(body_id).unwrap();
    body_mut.state = DynamicBodyState::Settled;

    // Pemain di samping pada y = 0.5m (bukan di atasnya)
    let feet_pos = Vec3::new(3.80, 0.50, 2.25);
    let res = check_ground_support_with_physics(feet_pos, 0.30, 0.05, &store, Some(&physics));
    assert!(
        !res.grounded,
        "Kontak samping dengan DynamicBody tidak boleh grounded!"
    );
}

#[test]
fn test_8d4_dynamic_body_above_unknown_rejected() {
    // DynamicBody ada, tetapi chunk statis di atasnya belum dimuat (Unknown)
    let store = ChunkStore::new(); // empty store (Unknown)
    let mut physics = PhysicsRuntime::default();
    let voxels = vec![(IVec3::new(4, 1, 4), VoxelBlock::new(MaterialId::STONE))];
    let agg = DetachedAggregate::from_world_voxels(904, &voxels).unwrap();
    let _ = physics.spawn_from_detached_aggregate(agg);

    let feet_pos = Vec3::new(2.25, 1.00, 2.25);
    let res = check_ground_support_with_physics(feet_pos, 0.30, 0.05, &store, Some(&physics));
    assert!(
        !res.grounded,
        "DynamicBody dengan Unknown di atasnya harus ditolak!"
    );
}

// ----------------------------------------------------------------------------
// 9. ORIGINAL BUG REGRESSIONS & JUMP INTEGRATION
// ----------------------------------------------------------------------------

#[test]
fn test_8d4_regression_standing_on_slope_space_jumps() {
    // REGRESI BUG A: Pemain berdiri di lereng/undakan tidak bisa lompat karena false airborne
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..10 {
        for vz in 0..10 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
            if vx >= 5 {
                chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE)); // undakan 1.0m
            }
        }
    }
    store.insert(chunk);

    // Spawn di lereng undakan (x = 2.60m, y = 1.0m)
    let mut player = PlayerController::new(Vec3::new(2.60, 1.0, 2.5));
    // Jalankan beberapa tick untuk stabilisasi
    for _ in 0..5 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }
    assert!(player.state.grounded, "Pemain di lereng harus grounded!");

    // Tekan Space untuk melompat
    let jump_input = PlayerInput {
        jump: true,
        ..Default::default()
    };
    player.set_input(jump_input);
    player.step_simulation(1.0 / 30.0, &store, 0.0);

    // INVARIAN: Space harus berhasil memicu lompatan!
    assert!(
        !player.state.grounded,
        "Lompat harus membuat pemain lepas landas!"
    );
    assert!(
        player.state.velocity.y > 0.0,
        "Kecepatan vertikal harus positif setelah lompat! Terukur: {}",
        player.state.velocity.y
    );
    assert!(
        player.state.position.y > 1.0,
        "Posisi pemain harus terangkat ke atas! Terukur: {}",
        player.state.position.y
    );
}

#[test]
fn test_8d4_regression_standing_on_river_bank_space_jumps() {
    // REGRESI BUG A: Berdiri di tepi sungai bisa lompat
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..=5 {
        for vz in 0..10 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
            chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(2.85, 1.0, 2.5));
    for _ in 0..5 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }
    assert!(player.state.grounded);

    player.set_input(PlayerInput {
        jump: true,
        ..Default::default()
    });
    player.step_simulation(1.0 / 30.0, &store, 0.0);

    assert!(!player.state.grounded);
    assert!(player.state.velocity.y > 0.0);
}

#[test]
fn test_8d4_regression_standing_on_edge_space_jumps() {
    // REGRESI BUG A: Berdiri menjorok di tepi balok bisa lompat
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    chunk.set_voxel(4, 1, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(2.65, 0.96, 2.25));
    player.step_simulation(1.0 / 30.0, &store, 0.0);
    assert!(player.state.grounded);

    player.set_input(PlayerInput {
        jump: true,
        ..Default::default()
    });
    player.step_simulation(1.0 / 30.0, &store, 0.0);

    assert!(!player.state.grounded);
    assert!(player.state.velocity.y > 0.0);
}

#[test]
fn test_8d4_jump_from_staircase() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for step in 0..3 {
        for vz in 0..10 {
            for y in 0..=step {
                chunk.set_voxel(4 + step, y, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
    }
    store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(2.75, 1.0, 2.5));
    player.step_simulation(1.0 / 30.0, &store, 0.0);
    assert!(player.state.grounded);

    player.set_input(PlayerInput {
        jump: true,
        ..Default::default()
    });
    player.step_simulation(1.0 / 30.0, &store, 0.0);
    assert!(!player.state.grounded);
    assert!(player.state.velocity.y > 0.0);
}

#[test]
fn test_8d4_jump_while_sprinting_from_edge() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..10 {
        for vz in 0..10 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(2.5, 0.5, 2.5));
    player.step_simulation(1.0 / 30.0, &store, 0.0);
    assert!(player.state.grounded);

    // Sprint + Jump
    player.set_input(PlayerInput {
        move_forward: 1.0,
        sprint: true,
        jump: true,
        ..Default::default()
    });
    player.step_simulation(1.0 / 30.0, &store, 0.0);

    assert!(!player.state.grounded);
    assert!(player.state.velocity.y > 0.0);
}

#[test]
fn test_8d4_jump_immediately_after_landing_on_edge() {
    // REGRESI BUG C: Mendarat di tepi dan langsung melompat lagi
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    chunk.set_voxel(4, 0, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    // Pemain jatuh dari ketinggian y = 1.5m tepat menuju tepi (x = 2.60m)
    let mut player = PlayerController::new(Vec3::new(2.60, 1.5, 2.25));
    player.state.grounded = false;

    // Simulasikan hingga mendarat
    for _ in 0..30 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
        if player.state.grounded {
            break;
        }
    }
    assert!(
        player.state.grounded,
        "Pemain harus berhasil mendarat di tepi!"
    );

    // Langsung lompat
    player.set_input(PlayerInput {
        jump: true,
        ..Default::default()
    });
    player.step_simulation(1.0 / 30.0, &store, 0.0);
    assert!(!player.state.grounded);
    assert!(player.state.velocity.y > 0.0);
}

#[test]
fn test_8d4_jump_after_auto_step() {
    // REGRESI BUG B & C: Melangkah naik undakan lalu langsung melompat
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

    let mut player = PlayerController::new(Vec3::new(2.2, 0.5, 2.5));
    player.set_input(PlayerInput {
        move_forward: 1.0,
        ..Default::default()
    });

    // Jalankan langkah hingga menaiki undakan
    for _ in 0..10 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }
    assert!(player.state.position.x > 2.5);
    assert!(player.state.grounded);

    // Langsung tekan Jump
    player.set_input(PlayerInput {
        jump: true,
        ..Default::default()
    });
    player.step_simulation(1.0 / 30.0, &store, 0.0);
    assert!(!player.state.grounded);
    assert!(player.state.velocity.y > 0.0);
}

// ----------------------------------------------------------------------------
// 10. AUTO-STEP INTEGRATION
// ----------------------------------------------------------------------------

#[test]
fn test_8d4_auto_step_one_voxel_step_from_rest() {
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

    // Tepat di depan undakan 0.5m
    let mut player = PlayerController::new(Vec3::new(2.19, 0.5, 2.5));
    player.set_input(PlayerInput {
        move_forward: 1.0,
        ..Default::default()
    });

    for _ in 0..10 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }

    assert!(player.state.position.x > 2.5);
    assert!((player.state.position.y - 1.0).abs() < 1e-2);
}

#[test]
fn test_8d4_auto_step_walking_into_step() {
    let mut store = ChunkStore::new();
    setup_flat_ground(&mut store, IVec3::ZERO);
    if let Some(chunk) = store.get_mut(&IVec3::ZERO) {
        for vx in 6..32 {
            for vz in 0..32 {
                chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
    }

    let mut player = PlayerController::new(Vec3::new(1.5, 0.5, 2.5));
    player.set_input(PlayerInput {
        move_forward: 1.0,
        ..Default::default()
    });

    for _ in 0..20 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }

    assert!(player.state.position.x > 3.0);
    assert!((player.state.position.y - 1.0).abs() < 1e-2);
}

#[test]
fn test_8d4_auto_step_sprinting_into_step() {
    let mut store = ChunkStore::new();
    setup_flat_ground(&mut store, IVec3::ZERO);
    if let Some(chunk) = store.get_mut(&IVec3::ZERO) {
        for vx in 6..32 {
            for vz in 0..32 {
                chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
    }

    let mut player = PlayerController::new(Vec3::new(1.5, 0.5, 2.5));
    player.set_input(PlayerInput {
        move_forward: 1.0,
        sprint: true,
        ..Default::default()
    });

    for _ in 0..15 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }

    assert!(player.state.position.x > 3.0);
    assert!((player.state.position.y - 1.0).abs() < 1e-2);
}

#[test]
fn test_8d4_auto_step_diagonal_approach() {
    let mut store = ChunkStore::new();
    setup_flat_ground(&mut store, IVec3::ZERO);
    if let Some(chunk) = store.get_mut(&IVec3::ZERO) {
        for vx in 5..32 {
            for vz in 5..32 {
                chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
    }

    let mut player = PlayerController::new(Vec3::new(2.1, 0.5, 2.1));
    player.set_input(PlayerInput {
        move_forward: 1.0,
        move_right: 1.0,
        ..Default::default()
    });

    for _ in 0..20 {
        player.step_simulation(1.0 / 30.0, &store, 45.0);
    }

    assert!(player.state.position.x > 2.5 || player.state.position.z > 2.5);
}

#[test]
fn test_8d4_auto_step_repeated_stair_steps() {
    let mut store = ChunkStore::new();
    setup_flat_ground(&mut store, IVec3::ZERO);
    if let Some(chunk) = store.get_mut(&IVec3::ZERO) {
        for step in 1..3 {
            for vx in (4 + step * 2)..32 {
                for vz in 0..32 {
                    chunk.set_voxel(vx, step, vz, VoxelBlock::new(MaterialId::STONE));
                }
            }
        }
    }

    let mut player = PlayerController::new(Vec3::new(1.5, 0.5, 2.5));
    player.set_input(PlayerInput {
        move_forward: 1.0,
        ..Default::default()
    });

    for _ in 0..40 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }

    assert!(player.state.position.x > 4.0);
    assert!(player.state.position.y >= 1.0);
}

#[test]
fn test_8d4_auto_step_two_voxel_wall_blocked() {
    let mut store = ChunkStore::new();
    setup_flat_ground(&mut store, IVec3::ZERO);
    if let Some(chunk) = store.get_mut(&IVec3::ZERO) {
        for vx in 5..32 {
            for vz in 0..32 {
                chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
                chunk.set_voxel(vx, 2, vz, VoxelBlock::new(MaterialId::STONE)); // 1.0m wall
            }
        }
    }

    let mut player = PlayerController::new(Vec3::new(2.1, 0.5, 2.5));
    player.set_input(PlayerInput {
        move_forward: 1.0,
        ..Default::default()
    });

    for _ in 0..20 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }

    // Dinding 1.0m harus tetap terblokir!
    assert!(player.state.position.x < 2.25);
    assert_eq!(player.state.position.y, 0.5);
}

#[test]
fn test_8d4_step_plus_jump_no_permanent_sticking() {
    // REGRESI BUG B & C: Menekan lompat sambil mendekati step tidak membuat pemain stuck
    let mut store = ChunkStore::new();
    setup_flat_ground(&mut store, IVec3::ZERO);
    if let Some(chunk) = store.get_mut(&IVec3::ZERO) {
        for vx in 5..32 {
            for vz in 0..32 {
                chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
    }

    let mut player = PlayerController::new(Vec3::new(2.0, 0.5, 2.5));
    player.set_input(PlayerInput {
        move_forward: 1.0,
        jump: true,
        ..Default::default()
    });

    for _ in 0..35 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
    }

    // Pemain harus berhasil melompat melewati undakan dan mendarat mulus
    assert!(player.state.position.x > 2.5);
    assert!(player.state.grounded);
    assert!((player.state.position.y - 1.0).abs() < 1e-2);
}

// ----------------------------------------------------------------------------
// 11. SAFETY & STABILITY
// ----------------------------------------------------------------------------

#[test]
fn test_8d4_grounding_never_snaps_upward_by_large_amount() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    chunk.set_voxel(4, 2, 4, VoxelBlock::new(MaterialId::STONE)); // surface 1.5m
    store.insert(chunk);

    // Kaki pemain di y = 1.0m (0.5m di bawah permukaan voxel)
    let feet_pos = Vec3::new(2.25, 1.00, 2.25);
    let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
    // Dilarang snap ke atas tebing!
    assert!(!res.grounded);
    assert_eq!(res.stable_feet_y, None);
}

#[test]
fn test_8d4_grounding_never_bridges_large_gap() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    // Dua pilar terpisah jarak 0.6m (lebih besar dari diameter kapsul)
    chunk.set_voxel(2, 1, 4, VoxelBlock::new(MaterialId::STONE));
    chunk.set_voxel(6, 1, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    // Pemain berada di tengah celah jurang (x = 2.0m)
    let feet_pos = Vec3::new(2.0, 1.0, 2.25);
    let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
    assert!(
        !res.grounded,
        "Pemain di atas celah jurang tidak boleh grounded!"
    );
}

#[test]
fn test_8d4_repeated_ticks_on_stable_ground_remain_stable() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..10 {
        for vz in 0..10 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(2.5, 0.5, 2.5));
    for tick in 0..100 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
        assert!(player.state.grounded, "Tick {} harus tetap grounded!", tick);
        assert_eq!(player.state.position.y, 0.5);
    }
}
