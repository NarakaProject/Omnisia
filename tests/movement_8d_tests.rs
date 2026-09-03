use glam::{IVec3, Vec3};
use omnisia::chunk::Chunk;
use omnisia::material::MaterialId;
use omnisia::physics::DynamicBodyState;
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
