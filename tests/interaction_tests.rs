use glam::{IVec3, Vec3};
use omnisia::chunk::{dirty_flags, Chunk};
use omnisia::interaction::{
    raycast_player_interaction, raycast_player_interaction_with_reach, raycast_voxels,
    VoxelRaycastResult, DEFAULT_INTERACTION_REACH,
};
use omnisia::material::MaterialId;
use omnisia::mesh::types::FaceDirection;
use omnisia::player::PlayerController;
use omnisia::streaming::store::ChunkStore;
use omnisia::voxel::{VoxelBlock, VOXEL_SIZE};

const TEST_STONE: MaterialId = MaterialId(1);
const TEST_DIRT: MaterialId = MaterialId(2);

fn create_test_store() -> ChunkStore {
    ChunkStore::new()
}

fn add_empty_chunk(store: &mut ChunkStore, coord: IVec3) {
    let mut chunk = Chunk::new(coord);
    chunk.clear_dirty(dirty_flags::ALL);
    store.insert(chunk);
}

// ============================================================================
// 1. BASIC HIT & MISS
// ============================================================================

#[test]
fn test_basic_hit_voxel_coord_and_distance() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::ZERO);

    // Tempatkan voxel solid pada (4, 0, 0)
    // Dalam meter: X in [2.0, 2.5], Y in [0.0, 0.5], Z in [0.0, 0.5]
    let target_voxel = IVec3::new(4, 0, 0);
    store.set_voxel_world(target_voxel, VoxelBlock::new(TEST_STONE));

    // Ray dari origin (0.25, 0.25, 0.25) menghadap +X
    let origin = Vec3::new(0.25, 0.25, 0.25);
    let direction = Vec3::X;
    let max_reach = 5.0;

    let result = raycast_voxels(&store, origin, direction, max_reach);

    assert!(result.is_hit());
    assert!(!result.is_miss());
    assert!(!result.is_non_resident());

    let hit = result.hit().expect("Harus menghasilkan VoxelHit");
    assert_eq!(hit.voxel_coord, target_voxel);
    assert_eq!(hit.material, TEST_STONE);
    assert_eq!(hit.face, FaceDirection::NegX);
    assert_eq!(hit.normal, Vec3::new(-1.0, 0.0, 0.0));

    // Permukaan -X berada di X = 4 * 0.5 = 2.0 meter
    // Jarak = 2.0 - 0.25 = 1.75 meter
    let expected_distance = 1.75;
    assert!(
        (hit.distance - expected_distance).abs() < 1e-5,
        "Distance: actual {} vs expected {}",
        hit.distance,
        expected_distance
    );
    assert_eq!(hit.hit_point, Vec3::new(2.0, 0.25, 0.25));
}

#[test]
fn test_basic_miss_into_empty_space() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::ZERO);

    let origin = Vec3::new(0.25, 0.25, 0.25);
    let direction = Vec3::X;
    let max_reach = 5.0;

    let result = raycast_voxels(&store, origin, direction, max_reach);

    assert!(result.is_miss());
    assert!(!result.is_hit());
    assert!(!result.is_non_resident());
    assert!(result.hit().is_none());
    assert!(result.voxel_coord().is_none());
    assert!(result.hit_point().is_none());
}

// ============================================================================
// 2. REACH BOUNDARY BEHAVIOR
// ============================================================================

#[test]
fn test_reach_inside_at_boundary_and_beyond() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::ZERO);

    // Target pada (6, 0, 0), permukaan -X ada di X = 3.0m
    let target = IVec3::new(6, 0, 0);
    store.set_voxel_world(target, VoxelBlock::new(TEST_DIRT));

    let origin = Vec3::new(0.0, 0.25, 0.25);
    let direction = Vec3::X;
    // Jarak ke permukaan adalah persis 3.0m

    // 1. Inside reach (reach = 4.0m > 3.0m) -> HIT
    let res_inside = raycast_voxels(&store, origin, direction, 4.0);
    assert!(res_inside.is_hit());
    assert_eq!(res_inside.distance().unwrap(), 3.0);

    // 2. Exactly at reach boundary (reach = 3.0m == 3.0m) -> HIT (inklusif)
    let res_exact = raycast_voxels(&store, origin, direction, 3.0);
    assert!(
        res_exact.is_hit(),
        "Kontak tepat pada batas reach harus dihitung sebagai HIT"
    );
    assert_eq!(res_exact.distance().unwrap(), 3.0);

    // 3. Beyond reach (reach = 2.99m < 3.0m) -> MISS
    let res_beyond = raycast_voxels(&store, origin, direction, 2.99);
    assert!(
        res_beyond.is_miss(),
        "Kontak di luar batas reach harus dihitung sebagai MISS"
    );

    // 4. Zero & Negative reach -> MISS
    assert!(raycast_voxels(&store, origin, direction, 0.0).is_miss());
    assert!(raycast_voxels(&store, origin, direction, -1.0).is_miss());
}

// ============================================================================
// 3. SIX CANONICAL FACE DIRECTIONS
// ============================================================================

#[test]
fn test_six_canonical_face_directions() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::ZERO);

    // Kubus target di tengah (8, 8, 8)
    // Bounds: [4.0, 4.5] pada X, Y, Z
    let target = IVec3::new(8, 8, 8);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    let center_y = 4.25;
    let center_z = 4.25;
    let center_x = 4.25;

    // 1. Ray along +X hitting left face (-X)
    let res_pos_x = raycast_voxels(&store, Vec3::new(2.0, center_y, center_z), Vec3::X, 5.0);
    assert!(res_pos_x.is_hit());
    let hit_x = res_pos_x.hit().unwrap();
    assert_eq!(hit_x.face, FaceDirection::NegX);
    assert_eq!(hit_x.normal, Vec3::new(-1.0, 0.0, 0.0));
    assert_eq!(hit_x.hit_point, Vec3::new(4.0, center_y, center_z));

    // 2. Ray along -X hitting right face (+X)
    let res_neg_x = raycast_voxels(&store, Vec3::new(6.0, center_y, center_z), -Vec3::X, 5.0);
    assert!(res_neg_x.is_hit());
    let hit_neg_x = res_neg_x.hit().unwrap();
    assert_eq!(hit_neg_x.face, FaceDirection::PosX);
    assert_eq!(hit_neg_x.normal, Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(hit_neg_x.hit_point, Vec3::new(4.5, center_y, center_z));

    // 3. Ray along +Y hitting bottom face (-Y)
    let res_pos_y = raycast_voxels(&store, Vec3::new(center_x, 2.0, center_z), Vec3::Y, 5.0);
    assert!(res_pos_y.is_hit());
    let hit_y = res_pos_y.hit().unwrap();
    assert_eq!(hit_y.face, FaceDirection::NegY);
    assert_eq!(hit_y.normal, Vec3::new(0.0, -1.0, 0.0));
    assert_eq!(hit_y.hit_point, Vec3::new(center_x, 4.0, center_z));

    // 4. Ray along -Y hitting top face (+Y)
    let res_neg_y = raycast_voxels(&store, Vec3::new(center_x, 6.0, center_z), -Vec3::Y, 5.0);
    assert!(res_neg_y.is_hit());
    let hit_neg_y = res_neg_y.hit().unwrap();
    assert_eq!(hit_neg_y.face, FaceDirection::PosY);
    assert_eq!(hit_neg_y.normal, Vec3::new(0.0, 1.0, 0.0));
    assert_eq!(hit_neg_y.hit_point, Vec3::new(center_x, 4.5, center_z));

    // 5. Ray along +Z hitting back face (-Z)
    let res_pos_z = raycast_voxels(&store, Vec3::new(center_x, center_y, 2.0), Vec3::Z, 5.0);
    assert!(res_pos_z.is_hit());
    let hit_z = res_pos_z.hit().unwrap();
    assert_eq!(hit_z.face, FaceDirection::NegZ);
    assert_eq!(hit_z.normal, Vec3::new(0.0, 0.0, -1.0));
    assert_eq!(hit_z.hit_point, Vec3::new(center_x, center_y, 4.0));

    // 6. Ray along -Z hitting front face (+Z)
    let res_neg_z = raycast_voxels(&store, Vec3::new(center_x, center_y, 6.0), -Vec3::Z, 5.0);
    assert!(res_neg_z.is_hit());
    let hit_neg_z = res_neg_z.hit().unwrap();
    assert_eq!(hit_neg_z.face, FaceDirection::PosZ);
    assert_eq!(hit_neg_z.normal, Vec3::new(0.0, 0.0, 1.0));
    assert_eq!(hit_neg_z.hit_point, Vec3::new(center_x, center_y, 4.5));
}

// ============================================================================
// 4. COORDINATE CORRECTNESS & NEGATIVE COORDINATES
// ============================================================================

#[test]
fn test_negative_coordinates_traversal() {
    let mut store = create_test_store();
    // Chunk (-1, -1, -1) mencakup world voxels [-32..-1] pada X, Y, Z
    add_empty_chunk(&mut store, IVec3::new(-1, -1, -1));

    // Target pada (-4, -2, -6)
    // Bounds: X in [-2.0, -1.5], Y in [-1.0, -0.5], Z in [-3.0, -2.5]
    let target = IVec3::new(-4, -2, -6);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    // Ray dari origin (-0.5, -0.75, -2.75) bergerak ke arah -X
    let origin = Vec3::new(-0.5, -0.75, -2.75);
    let direction = -Vec3::X;
    let max_reach = 5.0;

    let result = raycast_voxels(&store, origin, direction, max_reach);

    assert!(result.is_hit());
    let hit = result.hit().unwrap();
    assert_eq!(hit.voxel_coord, target);
    assert_eq!(hit.face, FaceDirection::PosX); // Menabrak sisi +X dari blok negatif
    assert_eq!(hit.normal, Vec3::X);

    // Permukaan +X ada di X = (-4 + 1) * 0.5 = -1.5m
    // Jarak = -0.5 - (-1.5) = 1.0m
    let expected_distance = 1.0;
    assert!((hit.distance - expected_distance).abs() < 1e-5);
    assert_eq!(hit.hit_point, Vec3::new(-1.5, -0.75, -2.75));
}

#[test]
fn test_mixed_sign_coordinates_crossing_zero() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::new(-1, 0, 0));
    add_empty_chunk(&mut store, IVec3::new(0, 0, 0));

    // Target berada pada koordinat positif (2, 0, 0)
    let target = IVec3::new(2, 0, 0);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    // Ray dimulai dari wilayah koordinat negatif: X = -1.5m (voxel -3)
    let origin = Vec3::new(-1.5, 0.25, 0.25);
    let direction = Vec3::X;

    let result = raycast_voxels(&store, origin, direction, 10.0);

    assert!(result.is_hit());
    let hit = result.hit().unwrap();
    assert_eq!(hit.voxel_coord, target);
    assert_eq!(hit.face, FaceDirection::NegX);
    // Target face -X ada di X = 2 * 0.5 = 1.0m
    // Jarak = 1.0 - (-1.5) = 2.5m
    assert!((hit.distance - 2.5).abs() < 1e-5);
    assert_eq!(hit.hit_point, Vec3::new(1.0, 0.25, 0.25));
}

// ============================================================================
// 5. CHUNK BOUNDARIES
// ============================================================================

#[test]
fn test_chunk_boundary_crossing() {
    let mut store = create_test_store();
    // Chunk 0 mencakup X in [0..31] (0.0m - 16.0m)
    // Chunk 1 mencakup X in [32..63] (16.0m - 32.0m)
    add_empty_chunk(&mut store, IVec3::new(0, 0, 0));
    add_empty_chunk(&mut store, IVec3::new(1, 0, 0));

    // Target pada voxel 32 (voxel pertama di Chunk 1, tepat setelah boundary 16.0m)
    let target = IVec3::new(32, 0, 0);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    // Ray dimulai dari voxel 30 (X = 15.25m di Chunk 0)
    let origin = Vec3::new(15.25, 0.25, 0.25);
    let direction = Vec3::X;

    let result = raycast_voxels(&store, origin, direction, 5.0);

    assert!(result.is_hit());
    let hit = result.hit().unwrap();
    assert_eq!(hit.voxel_coord, target);
    assert_eq!(hit.face, FaceDirection::NegX);
    // Target face ada persis di boundary chunk X = 16.0m
    // Jarak = 16.0 - 15.25 = 0.75m
    assert!((hit.distance - 0.75).abs() < 1e-5);
    assert_eq!(hit.hit_point, Vec3::new(16.0, 0.25, 0.25));
}

// ============================================================================
// 6. RESIDENCY AWARENESS (UNLOADED CHUNK DETECTION)
// ============================================================================

#[test]
fn test_residency_awareness_unloaded_chunk_detection() {
    let mut store = create_test_store();
    // Hanya Chunk 0 yang dimuat
    add_empty_chunk(&mut store, IVec3::new(0, 0, 0));
    let initial_resident_count = store.resident_count();

    // Ray ditembakkan dari Chunk 0 (X = 15.0m) menghadap +X menuju Chunk 1 (belum dimuat)
    let origin = Vec3::new(15.0, 0.25, 0.25);
    let direction = Vec3::X;
    let max_reach = 5.0;

    let result = raycast_voxels(&store, origin, direction, max_reach);

    // Harus secara eksplisit melaporkan NonResident
    assert!(result.is_non_resident());
    assert!(!result.is_hit());
    assert!(!result.is_miss());

    match result {
        VoxelRaycastResult::NonResident {
            voxel_coord,
            distance,
            hit_point,
            face,
        } => {
            // Voxel non-resident pertama yang dimasuki adalah voxel 32
            assert_eq!(voxel_coord, IVec3::new(32, 0, 0));
            // Jarak ke boundary chunk 16.0m = 16.0 - 15.0 = 1.0m
            assert!((distance - 1.0).abs() < 1e-5);
            assert_eq!(hit_point, Vec3::new(16.0, 0.25, 0.25));
            assert_eq!(face, FaceDirection::NegX);
        }
        _ => panic!("Expected NonResident result"),
    }

    // INVARIANT: Query TIDAK BOLEH memuat atau menghasilkan chunk secara diam-diam!
    assert_eq!(
        store.resident_count(),
        initial_resident_count,
        "Query tidak boleh memutasi ChunkStore resident count"
    );
    assert!(!store.contains(&IVec3::new(1, 0, 0)));
}

#[test]
fn test_residency_origin_in_unloaded_space() {
    let store = create_test_store();
    // Tidak ada chunk sama sekali yang resident
    let origin = Vec3::new(10.0, 5.0, 10.0);
    let direction = Vec3::Y;

    let result = raycast_voxels(&store, origin, direction, 5.0);

    assert!(result.is_non_resident());
    match result {
        VoxelRaycastResult::NonResident {
            distance,
            hit_point,
            ..
        } => {
            assert_eq!(distance, 0.0);
            assert_eq!(hit_point, origin);
        }
        _ => panic!("Expected NonResident at distance 0"),
    }
}

// ============================================================================
// 7. DETERMINISM
// ============================================================================

#[test]
fn test_determinism_repeated_execution() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::ZERO);

    let target = IVec3::new(5, 2, 3);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    let origin = Vec3::new(0.5, 0.5, 0.5);
    let dir = (Vec3::new(5.0, 2.0, 3.0) * VOXEL_SIZE - origin).normalize();

    let baseline = raycast_voxels(&store, origin, dir, 10.0);
    assert!(baseline.is_hit());

    // Jalankan 1,000 repetisi dan verifikasi hasil identik bitwise
    for _ in 0..1000 {
        let repeat = raycast_voxels(&store, origin, dir, 10.0);
        assert_eq!(baseline, repeat);
    }
}

// ============================================================================
// 8. NUMERICAL ROBUSTNESS & EDGE CASES
// ============================================================================

#[test]
fn test_zero_and_nan_direction() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::ZERO);

    let origin = Vec3::new(1.0, 1.0, 1.0);

    // Arah nol -> Miss
    assert!(raycast_voxels(&store, origin, Vec3::ZERO, 5.0).is_miss());

    // Arah NaN -> Miss
    assert!(raycast_voxels(&store, origin, Vec3::NAN, 5.0).is_miss());

    // Origin NaN -> Miss
    assert!(raycast_voxels(&store, Vec3::NAN, Vec3::X, 5.0).is_miss());
}

#[test]
fn test_origin_inside_solid_voxel() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::ZERO);

    let voxel_pos = IVec3::new(1, 1, 1);
    store.set_voxel_world(voxel_pos, VoxelBlock::new(TEST_STONE));

    // Origin di dalam voxel (1, 1, 1) -> X in [0.5, 1.0]
    let origin = Vec3::new(0.75, 0.75, 0.75);
    let direction = Vec3::X;

    let result = raycast_voxels(&store, origin, direction, 5.0);

    assert!(result.is_hit());
    let hit = result.hit().unwrap();
    assert_eq!(hit.voxel_coord, voxel_pos);
    assert_eq!(hit.distance, 0.0);
    assert_eq!(hit.hit_point, origin);
}

#[test]
fn test_nearly_axis_aligned_and_diagonal_rays() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::ZERO);

    let target = IVec3::new(4, 0, 0);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    // Ray hampir axis-aligned: ada komponen Y dan Z sebesar 1e-7
    let origin = Vec3::new(0.25, 0.25, 0.25);
    let direction = Vec3::new(1.0, 1e-7, 1e-7);

    let result = raycast_voxels(&store, origin, direction, 5.0);
    assert!(result.is_hit());
    assert_eq!(result.hit().unwrap().voxel_coord, target);
}

// ============================================================================
// 9. PLAYER EYE ORIGIN INTEGRATION
// ============================================================================

#[test]
fn test_player_eye_origin_integration() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::ZERO);
    add_empty_chunk(&mut store, IVec3::new(0, -1, 0));

    // Lantai solid pada Y = -1 (world pos Y in [-0.5, 0.0])
    let floor_voxel = IVec3::new(0, -1, 0);
    store.set_voxel_world(floor_voxel, VoxelBlock::new(TEST_STONE));

    // Pemain berdiri pada posisi (0.25, 0.0, 0.25)
    let spawn_pos = Vec3::new(0.25, 0.0, 0.25);
    let player = PlayerController::new(spawn_pos);

    // Eye position pemain saat berdiri: 0.0 + 1.62 = 1.62m
    assert_eq!(player.eye_position(), Vec3::new(0.25, 1.62, 0.25));

    // Ray menghadap lurus ke bawah (-Vec3::Y)
    let result = raycast_player_interaction(&store, &player, -Vec3::Y);

    assert!(result.is_hit());
    let hit = result.hit().unwrap();
    assert_eq!(hit.voxel_coord, floor_voxel);
    assert_eq!(hit.face, FaceDirection::PosY); // Menabrak permukaan atas lantai
    assert_eq!(hit.normal, Vec3::Y);

    // Jarak dari eye (1.62m) ke permukaan atas lantai (0.0m) adalah 1.62m
    assert!((hit.distance - 1.62).abs() < 1e-5);
    assert_eq!(hit.hit_point, Vec3::new(0.25, 0.0, 0.25));

    // Default reach adalah 5.0m, jadi jarak 1.62m masih di dalam jangkauan
    assert_eq!(player.config.interaction_reach, DEFAULT_INTERACTION_REACH);

    // Jika custom reach < 1.62m (misal 1.0m), maka harus meleset (Miss)
    let res_short = raycast_player_interaction_with_reach(&store, &player, -Vec3::Y, 1.0);
    assert!(res_short.is_miss());
}
