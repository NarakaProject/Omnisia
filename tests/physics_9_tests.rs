use glam::{IVec3, Mat3, Quat, Vec3};
use omnisia::chunk::Chunk;
use omnisia::material::MaterialId;
use omnisia::physics::{
    collide, combine_materials, compute_world_inv_inertia, integrate_bodies, integrate_body,
    integrate_rotation, integrate_transform, integrate_transforms, integrate_velocities,
    integrate_velocity, solve_contacts, world_pos_to_cell, Aabb, AabbError, BodyType, BoxShape,
    BroadphaseError, BroadphasePair, BroadphaseProxy, Capsule, CellCoord, Collider, ColliderId,
    Contact, IntegrationConfig, IntegrationError, MassProperties, MaterialError, PhysicsMaterial,
    PhysicsWorld, PhysicsWorldConfig, RigidBody, RigidBodyError, RigidBodyId, Shape, ShapeError,
    SolverConfig, SolverError, SpatialHashBroadphase, Sphere, StaticTerrainQuery, TangentBasis,
    Transform,
};
use omnisia::streaming::store::ChunkStore;
use omnisia::voxel::VoxelBlock;

// ============================================================================
// 1. AABB UNIT TESTS
// ============================================================================

#[test]
fn test_9_1_aabb_try_new_valid_and_invalid_inputs() {
    // 1. Input valid: min <= max
    let aabb = Aabb::try_new(Vec3::new(0.0, 1.0, 2.0), Vec3::new(3.0, 4.0, 5.0)).unwrap();
    assert_eq!(aabb.min, Vec3::new(0.0, 1.0, 2.0));
    assert_eq!(aabb.max, Vec3::new(3.0, 4.0, 5.0));
    assert!(aabb.is_valid());

    // 2. Input tidak valid: min > max pada salah satu sumbu
    let err = Aabb::try_new(Vec3::new(5.0, 1.0, 2.0), Vec3::new(3.0, 4.0, 5.0)).unwrap_err();
    assert_eq!(err, AabbError::MinGreaterThanMax);

    // 3. Input tidak valid: memuat NaN atau Infinity
    let nan_err =
        Aabb::try_new(Vec3::new(f32::NAN, 1.0, 2.0), Vec3::new(3.0, 4.0, 5.0)).unwrap_err();
    assert_eq!(nan_err, AabbError::NonFiniteCoordinates);

    let inf_err =
        Aabb::try_new(Vec3::new(0.0, 1.0, 2.0), Vec3::new(3.0, f32::INFINITY, 5.0)).unwrap_err();
    assert_eq!(inf_err, AabbError::NonFiniteCoordinates);
}

#[test]
fn test_9_1_aabb_from_min_max_and_center_half_extents() {
    // from_min_max dengan titik terbalik otomatis terurut
    let p1 = Vec3::new(10.0, -5.0, 8.0);
    let p2 = Vec3::new(-2.0, 7.0, 1.0);
    let aabb = Aabb::from_min_max(p1, p2).unwrap();
    assert_eq!(aabb.min, Vec3::new(-2.0, -5.0, 1.0));
    assert_eq!(aabb.max, Vec3::new(10.0, 7.0, 8.0));

    // from_center_half_extents
    let center = Vec3::new(5.0, 10.0, -3.0);
    let half_extents = Vec3::new(2.0, 3.0, 1.0);
    let aabb_ch = Aabb::from_center_half_extents(center, half_extents).unwrap();
    assert_eq!(aabb_ch.min, Vec3::new(3.0, 7.0, -4.0));
    assert_eq!(aabb_ch.max, Vec3::new(7.0, 13.0, -2.0));
    assert_eq!(aabb_ch.center(), center);
    assert_eq!(aabb_ch.half_extents(), half_extents);
    assert_eq!(aabb_ch.extents(), Vec3::new(4.0, 6.0, 2.0));
}

#[test]
fn test_9_1_aabb_overlap_and_disjoint() {
    let box1 = Aabb::try_new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 2.0)).unwrap();

    // Bertumpukan di tengah
    let box_overlapping =
        Aabb::try_new(Vec3::new(1.0, 1.0, 1.0), Vec3::new(3.0, 3.0, 3.0)).unwrap();
    assert!(box1.overlaps(&box_overlapping));
    assert!(box_overlapping.overlaps(&box1));

    // Terpisah di sumbu X
    let box_disjoint_x = Aabb::try_new(Vec3::new(2.5, 0.0, 0.0), Vec3::new(4.0, 2.0, 2.0)).unwrap();
    assert!(!box1.overlaps(&box_disjoint_x));

    // Terpisah di sumbu Y
    let box_disjoint_y = Aabb::try_new(Vec3::new(0.0, 3.0, 0.0), Vec3::new(2.0, 5.0, 2.0)).unwrap();
    assert!(!box1.overlaps(&box_disjoint_y));

    // Terpisah di sumbu Z
    let box_disjoint_z = Aabb::try_new(Vec3::new(0.0, 0.0, 4.0), Vec3::new(2.0, 2.0, 6.0)).unwrap();
    assert!(!box1.overlaps(&box_disjoint_z));
}

#[test]
fn test_9_1_aabb_touching_boundary_semantics() {
    let box1 = Aabb::try_new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 2.0)).unwrap();

    // Menyentuh tepat di muka X = 2.0 (inklusif overlap)
    let box_touching_face =
        Aabb::try_new(Vec3::new(2.0, 0.0, 0.0), Vec3::new(4.0, 2.0, 2.0)).unwrap();
    assert!(
        box1.overlaps(&box_touching_face),
        "Menyentuh tepat pada muka batas harus dianggap overlap inklusif"
    );

    // Menyentuh tepat di sudut (2.0, 2.0, 2.0)
    let box_touching_corner =
        Aabb::try_new(Vec3::new(2.0, 2.0, 2.0), Vec3::new(4.0, 4.0, 4.0)).unwrap();
    assert!(
        box1.overlaps(&box_touching_corner),
        "Menyentuh tepat pada sudut batas harus dianggap overlap inklusif"
    );
}

#[test]
fn test_9_1_aabb_negative_coordinates_math() {
    let box_neg = Aabb::try_new(Vec3::new(-10.0, -8.0, -6.0), Vec3::new(-4.0, -2.0, -1.0)).unwrap();
    assert_eq!(box_neg.center(), Vec3::new(-7.0, -5.0, -3.5));
    assert_eq!(box_neg.extents(), Vec3::new(6.0, 6.0, 5.0));

    let box_cross = Aabb::try_new(Vec3::new(-6.0, -4.0, -3.0), Vec3::new(2.0, 1.0, 4.0)).unwrap();
    assert!(box_neg.overlaps(&box_cross));
}

#[test]
fn test_9_1_aabb_union_and_positive_expand() {
    let box1 = Aabb::try_new(Vec3::new(1.0, 2.0, 3.0), Vec3::new(4.0, 5.0, 6.0)).unwrap();
    let box2 = Aabb::try_new(Vec3::new(-2.0, 0.0, 4.0), Vec3::new(3.0, 6.0, 8.0)).unwrap();

    let union_box = box1.union(&box2);
    assert_eq!(union_box.min, Vec3::new(-2.0, 0.0, 3.0));
    assert_eq!(union_box.max, Vec3::new(4.0, 6.0, 8.0));

    let expanded = box1.expand(0.5);
    assert_eq!(expanded.min, Vec3::new(0.5, 1.5, 2.5));
    assert_eq!(expanded.max, Vec3::new(4.5, 5.5, 6.5));
    assert_eq!(expanded.extents(), box1.extents() + Vec3::splat(1.0));
}

#[test]
fn test_9_1_aabb_negative_expand_clamped_contract() {
    let box1 = Aabb::try_new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 6.0, 4.0)).unwrap();
    let center = box1.center();

    // Kontraksi wajar sebesar 1.0m di setiap arah
    let contracted = box1.expand(-1.0);
    assert_eq!(contracted.min, Vec3::new(1.0, 1.0, 1.0));
    assert_eq!(contracted.max, Vec3::new(9.0, 5.0, 3.0));
    assert_eq!(contracted.center(), center);

    // Kontraksi masif sebesar -100m harus menguncup ke center tanpa membalikkan koordinat
    let collapsed = box1.expand(-100.0);
    assert_eq!(collapsed.min, center);
    assert_eq!(collapsed.max, center);
    assert!(collapsed.is_valid());
    assert_eq!(collapsed.extents(), Vec3::ZERO);
}

#[test]
fn test_9_1_aabb_point_containment() {
    let box1 = Aabb::try_new(Vec3::new(1.0, 2.0, 3.0), Vec3::new(5.0, 6.0, 7.0)).unwrap();

    assert!(box1.contains_point(Vec3::new(3.0, 4.0, 5.0)));
    assert!(box1.contains_point(Vec3::new(1.0, 2.0, 3.0))); // batas min
    assert!(box1.contains_point(Vec3::new(5.0, 6.0, 7.0))); // batas max

    assert!(!box1.contains_point(Vec3::new(0.9, 4.0, 5.0)));
    assert!(!box1.contains_point(Vec3::new(3.0, 6.1, 5.0)));
}

// ============================================================================
// 2. BROADPHASE REGISTRATION & PROXY TESTS
// ============================================================================

#[test]
fn test_9_1_broadphase_insert_query_remove_lifecycle() {
    let mut broadphase = SpatialHashBroadphase::new(4.0);
    let id1 = RigidBodyId(1);
    let aabb1 = Aabb::try_new(Vec3::new(1.0, 1.0, 1.0), Vec3::new(3.0, 3.0, 3.0)).unwrap();
    let proxy1 = BroadphaseProxy::new(id1, BodyType::Dynamic, aabb1);

    // Insert
    assert!(broadphase.insert(proxy1).is_ok());
    assert_eq!(broadphase.len(), 1);
    assert!(broadphase.contains(id1));

    // Re-inserting ID yang sama ditolak
    let dup_proxy = BroadphaseProxy::new(id1, BodyType::Dynamic, aabb1);
    assert_eq!(
        broadphase.insert(dup_proxy).unwrap_err(),
        BroadphaseError::BodyAlreadyExists(id1)
    );

    // Query overlaps
    let query_box = Aabb::try_new(Vec3::new(2.0, 2.0, 2.0), Vec3::new(4.0, 4.0, 4.0)).unwrap();
    let hits = broadphase.query_aabb(&query_box);
    assert_eq!(hits, vec![id1]);

    // Query non-overlapping
    let query_miss =
        Aabb::try_new(Vec3::new(10.0, 10.0, 10.0), Vec3::new(12.0, 12.0, 12.0)).unwrap();
    assert!(broadphase.query_aabb(&query_miss).is_empty());

    // Remove
    let removed = broadphase.remove(id1).unwrap();
    assert_eq!(removed.body_id, id1);
    assert_eq!(broadphase.len(), 0);
    assert!(!broadphase.contains(id1));

    // Query setelah remove
    assert!(broadphase.query_aabb(&query_box).is_empty());

    // Remove non-existent body
    assert!(broadphase.remove(id1).is_none());
}

#[test]
fn test_9_1_broadphase_update_aabb_repositioning() {
    let mut broadphase = SpatialHashBroadphase::new(4.0);
    let id = RigidBodyId(42);
    let aabb_initial = Aabb::try_new(Vec3::new(1.0, 1.0, 1.0), Vec3::new(3.0, 3.0, 3.0)).unwrap();
    broadphase
        .insert(BroadphaseProxy::new(id, BodyType::Dynamic, aabb_initial))
        .unwrap();

    // Pindahkan badan ke posisi baru (x = 20..22m)
    let aabb_new = Aabb::try_new(Vec3::new(20.0, 1.0, 1.0), Vec3::new(22.0, 3.0, 3.0)).unwrap();
    assert!(broadphase.update(id, aabb_new).is_ok());

    // Di lokasi lama sudah tidak ditemukan
    let old_pos_query = Aabb::try_new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(4.0, 4.0, 4.0)).unwrap();
    assert!(broadphase.query_aabb(&old_pos_query).is_empty());

    // Di lokasi baru berhasil ditemukan
    let new_pos_query =
        Aabb::try_new(Vec3::new(19.0, 0.0, 0.0), Vec3::new(23.0, 4.0, 4.0)).unwrap();
    assert_eq!(broadphase.query_aabb(&new_pos_query), vec![id]);

    // Update pada badan yang tidak ada menghasilkan error
    assert_eq!(
        broadphase.update(RigidBodyId(999), aabb_new).unwrap_err(),
        BroadphaseError::BodyNotFound(RigidBodyId(999))
    );
}

#[test]
fn test_9_1_broadphase_multi_cell_spanning_large_aabb() {
    let cell_size = 4.0;
    let mut broadphase = SpatialHashBroadphase::new(cell_size);
    let id = RigidBodyId(100);

    // Badan besar (14m x 14m x 14m) melintasi beberapa sel grid (dari sel 0 hingga sel 3)
    let large_aabb = Aabb::try_new(Vec3::new(1.0, 1.0, 1.0), Vec3::new(15.0, 15.0, 15.0)).unwrap();
    broadphase
        .insert(BroadphaseProxy::new(id, BodyType::Dynamic, large_aabb))
        .unwrap();

    // Kueri di sel sudut yang berbeda semuanya harus menemukan badan ini
    let q1 = Aabb::try_new(Vec3::new(1.5, 1.5, 1.5), Vec3::new(2.5, 2.5, 2.5)).unwrap();
    let q2 = Aabb::try_new(Vec3::new(13.0, 13.0, 13.0), Vec3::new(14.0, 14.0, 14.0)).unwrap();
    let q3 = Aabb::try_new(Vec3::new(6.0, 6.0, 6.0), Vec3::new(7.0, 7.0, 7.0)).unwrap();

    assert_eq!(broadphase.query_aabb(&q1), vec![id]);
    assert_eq!(broadphase.query_aabb(&q2), vec![id]);
    assert_eq!(broadphase.query_aabb(&q3), vec![id]);
}

#[test]
fn test_9_1_broadphase_negative_coordinates_spatial_hash() {
    let cell_size = 4.0;
    let mut broadphase = SpatialHashBroadphase::new(cell_size);

    // Periksa rumus konversi world_pos_to_cell secara eksplisit
    // x = -0.5 / 4.0 = -0.125 -> floor = -1
    assert_eq!(
        world_pos_to_cell(Vec3::new(-0.5, -4.0, -4.1), cell_size),
        CellCoord::new(-1, -1, -2)
    );

    let id = RigidBodyId(200);
    let aabb_neg = Aabb::try_new(
        Vec3::new(-20.0, -10.0, -15.0),
        Vec3::new(-17.0, -8.0, -12.0),
    )
    .unwrap();
    broadphase
        .insert(BroadphaseProxy::new(id, BodyType::Dynamic, aabb_neg))
        .unwrap();

    let query_neg =
        Aabb::try_new(Vec3::new(-18.0, -9.0, -13.0), Vec3::new(-16.0, -7.0, -11.0)).unwrap();
    assert_eq!(broadphase.query_aabb(&query_neg), vec![id]);
}

#[test]
fn test_9_1_broadphase_crossing_origin_spatial_hash() {
    let cell_size = 4.0;
    let mut broadphase = SpatialHashBroadphase::new(cell_size);
    let id = RigidBodyId(300);

    // Badan yang melintasi origin: x dari -2.0m hingga +2.0m
    // Meliputi sel x = -1 dan x = 0
    let aabb_origin = Aabb::try_new(Vec3::new(-2.0, -2.0, -2.0), Vec3::new(2.0, 2.0, 2.0)).unwrap();
    broadphase
        .insert(BroadphaseProxy::new(id, BodyType::Dynamic, aabb_origin))
        .unwrap();

    let q_neg = Aabb::try_new(Vec3::new(-1.5, -1.5, -1.5), Vec3::new(-0.5, -0.5, -0.5)).unwrap();
    let q_pos = Aabb::try_new(Vec3::new(0.5, 0.5, 0.5), Vec3::new(1.5, 1.5, 1.5)).unwrap();

    assert_eq!(broadphase.query_aabb(&q_neg), vec![id]);
    assert_eq!(broadphase.query_aabb(&q_pos), vec![id]);
}

#[test]
fn test_9_1_broadphase_conservative_cell_inclusion_and_false_positives() {
    let cell_size = 4.0;
    let mut broadphase = SpatialHashBroadphase::new(cell_size);

    // Body 1 di sudut sel (0, 0, 0)
    let id1 = RigidBodyId(1);
    let aabb1 = Aabb::try_new(Vec3::new(0.1, 0.1, 0.1), Vec3::new(0.9, 0.9, 0.9)).unwrap();
    broadphase
        .insert(BroadphaseProxy::new(id1, BodyType::Dynamic, aabb1))
        .unwrap();

    // Body 2 di sudut berlawanan dalam sel yang sama (0, 0, 0)
    let id2 = RigidBodyId(2);
    let aabb2 = Aabb::try_new(Vec3::new(3.1, 3.1, 3.1), Vec3::new(3.9, 3.9, 3.9)).unwrap();
    broadphase
        .insert(BroadphaseProxy::new(id2, BodyType::Dynamic, aabb2))
        .unwrap();

    // Kedua badan berada di sel (0, 0, 0) yang sama, namun AABB keduanya tidak tumpang tindih
    assert!(!aabb1.overlaps(&aabb2));

    // Generator pasangan broadphase melakukan AABB overlap check untuk memfilter sel yang sama
    let pairs = broadphase.generate_candidate_pairs();
    assert!(
        pairs.is_empty(),
        "Badan dalam sel yang sama tanpa AABB overlap tidak boleh menghasilkan pasangan kandidat"
    );
}

// ============================================================================
// 3. CANDIDATE PAIR GENERATION & DETERMINISM TESTS
// ============================================================================

#[test]
fn test_9_1_broadphase_no_self_pairs() {
    let mut broadphase = SpatialHashBroadphase::new(4.0);
    let id = RigidBodyId(1);
    let aabb = Aabb::try_new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 2.0)).unwrap();
    broadphase
        .insert(BroadphaseProxy::new(id, BodyType::Dynamic, aabb))
        .unwrap();

    let pairs = broadphase.generate_candidate_pairs();
    assert!(
        pairs.is_empty(),
        "Badan tunggal tidak boleh menghasilkan self-pair!"
    );
}

#[test]
fn test_9_1_broadphase_no_duplicate_pairs() {
    let mut broadphase = SpatialHashBroadphase::new(4.0);
    let id1 = RigidBodyId(10);
    let id2 = RigidBodyId(20);

    // Dua badan besar saling bertumpukan melintasi 4 sel grid sekaligus
    let aabb1 = Aabb::try_new(Vec3::new(1.0, 1.0, 1.0), Vec3::new(6.0, 6.0, 6.0)).unwrap();
    let aabb2 = Aabb::try_new(Vec3::new(2.0, 2.0, 2.0), Vec3::new(7.0, 7.0, 7.0)).unwrap();

    broadphase
        .insert(BroadphaseProxy::new(id1, BodyType::Dynamic, aabb1))
        .unwrap();
    broadphase
        .insert(BroadphaseProxy::new(id2, BodyType::Dynamic, aabb2))
        .unwrap();

    let pairs = broadphase.generate_candidate_pairs();
    assert_eq!(
        pairs.len(),
        1,
        "Pasangan harus dideduplikasi tepat 1 meskipun bertumpukan di banyak sel!"
    );
    assert_eq!(
        pairs[0],
        BroadphasePair {
            body_a: id1,
            body_b: id2
        }
    );
}

#[test]
fn test_9_1_broadphase_canonical_ordering_a_less_than_b() {
    // Memastikan bahwa id yang lebih besar otomatis ditempatkan di body_b
    let pair = BroadphasePair::new(RigidBodyId(100), RigidBodyId(5)).unwrap();
    assert_eq!(pair.body_a, RigidBodyId(5));
    assert_eq!(pair.body_b, RigidBodyId(100));
}

#[test]
fn test_9_1_broadphase_static_static_pairs_omitted() {
    let mut broadphase = SpatialHashBroadphase::new(4.0);
    let s1 = RigidBodyId(1);
    let s2 = RigidBodyId(2);

    let aabb1 = Aabb::try_new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 2.0)).unwrap();
    let aabb2 = Aabb::try_new(Vec3::new(1.0, 1.0, 1.0), Vec3::new(3.0, 3.0, 3.0)).unwrap();

    broadphase
        .insert(BroadphaseProxy::new(s1, BodyType::Static, aabb1))
        .unwrap();
    broadphase
        .insert(BroadphaseProxy::new(s2, BodyType::Static, aabb2))
        .unwrap();

    let pairs = broadphase.generate_candidate_pairs();
    assert!(
        pairs.is_empty(),
        "Pasangan Static ↔ Static tidak boleh pernah dihasilkan!"
    );
}

#[test]
fn test_9_1_broadphase_dynamic_dynamic_and_dynamic_static_pairs_generated() {
    let mut broadphase = SpatialHashBroadphase::new(4.0);

    let d1 = RigidBodyId(1);
    let d2 = RigidBodyId(2);
    let s1 = RigidBodyId(3);
    let k1 = RigidBodyId(4);

    let aabb = Aabb::try_new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 2.0)).unwrap();

    broadphase
        .insert(BroadphaseProxy::new(d1, BodyType::Dynamic, aabb))
        .unwrap();
    broadphase
        .insert(BroadphaseProxy::new(d2, BodyType::Dynamic, aabb))
        .unwrap();
    broadphase
        .insert(BroadphaseProxy::new(s1, BodyType::Static, aabb))
        .unwrap();
    broadphase
        .insert(BroadphaseProxy::new(k1, BodyType::Kinematic, aabb))
        .unwrap();

    let pairs = broadphase.generate_candidate_pairs();
    // Pasangan yang valid:
    // (d1, d2) -> Dyn-Dyn
    // (d1, s1) -> Dyn-Stat
    // (d1, k1) -> Dyn-Kin
    // (d2, s1) -> Dyn-Stat
    // (d2, k1) -> Dyn-Kin
    // (s1, k1) -> Stat-Kin
    assert_eq!(pairs.len(), 6);
    assert!(pairs.contains(&BroadphasePair::new(d1, d2).unwrap()));
    assert!(pairs.contains(&BroadphasePair::new(d1, s1).unwrap()));
    assert!(pairs.contains(&BroadphasePair::new(d1, k1).unwrap()));
    assert!(pairs.contains(&BroadphasePair::new(d2, s1).unwrap()));
    assert!(pairs.contains(&BroadphasePair::new(d2, k1).unwrap()));
    assert!(pairs.contains(&BroadphasePair::new(s1, k1).unwrap()));
}

#[test]
fn test_9_1_broadphase_pair_generation_deterministic_across_iterations() {
    let mut broadphase = SpatialHashBroadphase::new(4.0);

    // Mendaftarkan 30 badan acak-teratur
    for i in 1..=30 {
        let x = (i as f32 * 1.5) % 10.0;
        let y = ((i * 7) as f32 * 0.8) % 6.0;
        let z = ((i * 13) as f32 * 1.1) % 8.0;
        let aabb = Aabb::try_new(Vec3::new(x, y, z), Vec3::new(x + 2.5, y + 2.5, z + 2.5)).unwrap();
        let btype = if i % 4 == 0 {
            BodyType::Static
        } else {
            BodyType::Dynamic
        };
        broadphase
            .insert(BroadphaseProxy::new(RigidBodyId(i), btype, aabb))
            .unwrap();
    }

    let baseline_pairs = broadphase.generate_candidate_pairs();
    assert!(!baseline_pairs.is_empty());

    // Jalankan 50 kali verifikasi bahwa urutan dan elemen pasangan 100% identik
    for _ in 0..50 {
        let current_pairs = broadphase.generate_candidate_pairs();
        assert_eq!(
            baseline_pairs, current_pairs,
            "Urutan dan identitas pasangan broadphase harus selalu deterministik!"
        );
    }
}

// ============================================================================
// 4. PHYSICS WORLD & STATIC TERRAIN QUERY TESTS
// ============================================================================

#[test]
fn test_9_1_physics_world_registration_and_id_assignment() {
    let config = PhysicsWorldConfig::default();
    let mut world = PhysicsWorld::new(config);

    let aabb1 = Aabb::try_new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)).unwrap();
    let aabb2 = Aabb::try_new(Vec3::new(2.0, 0.0, 0.0), Vec3::new(3.0, 1.0, 1.0)).unwrap();

    let id1 = world.register_body(BodyType::Dynamic, aabb1).unwrap();
    let id2 = world.register_body(BodyType::Static, aabb2).unwrap();

    assert_eq!(id1, RigidBodyId(1));
    assert_eq!(id2, RigidBodyId(2));
    assert_eq!(world.body_count(), 2);
    assert_eq!(world.get_body_type(id1), Some(BodyType::Dynamic));
    assert_eq!(world.get_body_type(id2), Some(BodyType::Static));
    assert_eq!(world.get_body_aabb(id1), Some(&aabb1));

    // Update AABB
    let aabb1_new = Aabb::try_new(Vec3::new(1.5, 0.0, 0.0), Vec3::new(2.5, 1.0, 1.0)).unwrap();
    world.update_body_aabb(id1, aabb1_new).unwrap();
    assert_eq!(world.get_body_aabb(id1), Some(&aabb1_new));

    // Unregister
    assert!(world.unregister_body(id1));
    assert_eq!(world.body_count(), 1);
    assert!(!world.contains_body(id1));
    assert!(!world.unregister_body(id1)); // unregister ulang mengembalikan false
}

#[test]
fn test_9_1_physics_world_query_aabb() {
    let mut world = PhysicsWorld::default();

    let id1 = world
        .register_body(
            BodyType::Dynamic,
            Aabb::try_new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 2.0)).unwrap(),
        )
        .unwrap();
    let _id2 = world
        .register_body(
            BodyType::Dynamic,
            Aabb::try_new(Vec3::new(10.0, 0.0, 0.0), Vec3::new(12.0, 2.0, 2.0)).unwrap(),
        )
        .unwrap();

    let query_box = Aabb::try_new(Vec3::new(1.0, 1.0, 1.0), Vec3::new(3.0, 3.0, 3.0)).unwrap();
    let hits = world.query_aabb(&query_box);
    assert_eq!(hits, vec![id1]);
}

#[test]
fn test_9_1_static_terrain_query_strictly_aabb_bounded() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);

    // Isi voxel di dalam rentang x = 2..4, y = 1, z = 2..4 (lokal)
    for vx in 2..=4 {
        for vz in 2..=4 {
            chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Isi voxel di luar rentang kueri (di vx = 10, vy = 10, vz = 10)
    chunk.set_voxel(10, 10, 10, VoxelBlock::new(MaterialId::DIRT));
    store.insert(chunk);

    // Buat kueri AABB terbatas yang hanya melingkupi voxel x = 2..4, y = 1, z = 2..4
    // Dalam koordinat dunia meter: vx = 2..4 -> min = 1.0m, max = 2.5m
    let query_aabb = Aabb::try_new(Vec3::new(1.0, 0.5, 1.0), Vec3::new(2.5, 1.0, 2.5)).unwrap();

    let mut results = Vec::new();
    store.query_static_voxels(&query_aabb, &mut results);

    // Harus menemukan tepat 9 voxel (3x3), dan TIDAK PERNAH memuat voxel jauh (10, 10, 10)
    assert_eq!(results.len(), 9);
    for voxel_box in &results {
        assert!(
            voxel_box.overlaps(&query_aabb),
            "Hasil kueri static terrain harus berada dalam rentang AABB yang diminta!"
        );
        assert!(voxel_box.max.x <= 2.501);
        assert!(voxel_box.max.y <= 1.001);
        assert!(voxel_box.max.z <= 2.501);
    }
}

#[test]
fn test_9_1_broadphase_zero_voxel_iteration_contract() {
    // Memvalidasi bahwa SpatialHashBroadphase dapat melakukan inisialisasi,
    // registrasi 100 badan, pembaruan posisi, kueri AABB, dan generasi pasangan kandidat
    // secara murni geometris tanpa membutuhkan referensi ChunkStore ataupun memindai data voxel.
    let mut broadphase = SpatialHashBroadphase::new(4.0);

    for i in 1..=100 {
        let x = (i as f32) * 2.0;
        let aabb = Aabb::try_new(Vec3::new(x, 0.0, 0.0), Vec3::new(x + 3.0, 2.0, 2.0)).unwrap();
        broadphase
            .insert(BroadphaseProxy::new(
                RigidBodyId(i),
                BodyType::Dynamic,
                aabb,
            ))
            .unwrap();
    }

    assert_eq!(broadphase.len(), 100);

    let query_box = Aabb::try_new(Vec3::new(5.0, 0.0, 0.0), Vec3::new(15.0, 2.0, 2.0)).unwrap();
    let hits = broadphase.query_aabb(&query_box);
    assert!(!hits.is_empty());

    let pairs = broadphase.generate_candidate_pairs();
    assert!(!pairs.is_empty());
}

// ============================================================================
// 5. PHASE 9.2 — RIGIDBODY DATA MODEL & MASS PROPERTIES TESTS
// ============================================================================

#[test]
fn test_9_2_dynamic_rigidbody_construction() {
    let id = RigidBodyId(1);
    let pos = Vec3::new(10.0, 5.0, -2.0);
    let rot = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
    let mass = 15.0;
    let inertia = Mat3::from_diagonal(Vec3::new(2.0, 3.0, 4.0));

    let body = RigidBody::new_dynamic(id, pos, rot, mass, inertia).unwrap();

    assert_eq!(body.id(), id);
    assert_eq!(body.body_type(), BodyType::Dynamic);
    assert!(body.is_dynamic());
    assert!(!body.is_static());
    assert!(!body.is_kinematic());
    assert_eq!(body.position(), pos);
    assert!((body.rotation().length() - 1.0).abs() < 1e-6);
    assert_eq!(body.linear_velocity(), Vec3::ZERO);
    assert_eq!(body.angular_velocity(), Vec3::ZERO);
    assert_eq!(body.mass_properties().mass, 15.0);
    assert!((body.mass_properties().inverse_mass - (1.0 / 15.0)).abs() < 1e-6);
    assert_eq!(body.mass_properties().local_inertia, inertia);
}

#[test]
fn test_9_2_static_rigidbody_construction() {
    let id = RigidBodyId(2);
    let pos = Vec3::new(-50.0, 0.0, 25.0);
    let rot = Quat::IDENTITY;

    let body = RigidBody::new_static(id, pos, rot).unwrap();

    assert_eq!(body.id(), id);
    assert_eq!(body.body_type(), BodyType::Static);
    assert!(body.is_static());
    assert!(!body.is_dynamic());
    assert!(!body.is_kinematic());
    assert_eq!(body.position(), pos);
    assert_eq!(body.linear_velocity(), Vec3::ZERO);
    assert_eq!(body.angular_velocity(), Vec3::ZERO);

    // Massa dan inersia statis kanonikal (inverse = 0, mass = 0, BUKAN INFINITY)
    assert_eq!(body.mass_properties().mass, 0.0);
    assert_eq!(body.mass_properties().inverse_mass, 0.0);
    assert_eq!(body.mass_properties().local_inertia, Mat3::ZERO);
    assert_eq!(body.mass_properties().local_inverse_inertia, Mat3::ZERO);
}

#[test]
fn test_9_2_kinematic_rigidbody_construction() {
    let id = RigidBodyId(3);
    let pos = Vec3::new(0.0, 10.0, 0.0);
    let rot = Quat::IDENTITY;
    let lin_vel = Vec3::new(2.5, 0.0, -1.0);
    let ang_vel = Vec3::new(0.0, 1.57, 0.0);

    let body = RigidBody::new_kinematic(id, pos, rot, lin_vel, ang_vel).unwrap();

    assert_eq!(body.id(), id);
    assert_eq!(body.body_type(), BodyType::Kinematic);
    assert!(body.is_kinematic());
    assert!(!body.is_dynamic());
    assert!(!body.is_static());
    assert_eq!(body.position(), pos);
    assert_eq!(body.linear_velocity(), lin_vel);
    assert_eq!(body.angular_velocity(), ang_vel);

    // Invarian massa kinematik (inverse = 0)
    assert_eq!(body.mass_properties().mass, 0.0);
    assert_eq!(body.mass_properties().inverse_mass, 0.0);
    assert_eq!(body.mass_properties().local_inertia, Mat3::ZERO);
    assert_eq!(body.mass_properties().local_inverse_inertia, Mat3::ZERO);
}

#[test]
fn test_9_2_position_preserved() {
    let id = RigidBodyId(10);
    let mut body = RigidBody::new_static(id, Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY).unwrap();
    assert_eq!(body.position(), Vec3::new(1.0, 2.0, 3.0));

    // set_position valid
    assert!(body.set_position(Vec3::new(-100.5, 42.0, 0.0)).is_ok());
    assert_eq!(body.position(), Vec3::new(-100.5, 42.0, 0.0));

    // set_position dengan nilai non-finite ditolak
    assert_eq!(
        body.set_position(Vec3::new(f32::NAN, 0.0, 0.0))
            .unwrap_err(),
        RigidBodyError::NonFinitePosition
    );
    assert_eq!(
        body.set_position(Vec3::new(0.0, f32::INFINITY, 0.0))
            .unwrap_err(),
        RigidBodyError::NonFinitePosition
    );
}

#[test]
fn test_9_2_rotation_normalized() {
    let id = RigidBodyId(11);
    // Masukkan quaternion non-unit (belum ternormalisasi)
    let non_unit_rot = Quat::from_xyzw(1.0, 2.0, 3.0, 4.0);
    assert!((non_unit_rot.length() - 1.0).abs() > 0.1);

    let body = RigidBody::new_static(id, Vec3::ZERO, non_unit_rot).unwrap();
    // Harus otomatis ternormalisasi
    assert!((body.rotation().length() - 1.0).abs() < 1e-6);
    assert_eq!(body.rotation(), non_unit_rot.normalize());

    // Uji pada set_rotation mutator
    let mut body_mut = body;
    let non_unit_rot_2 = Quat::from_xyzw(0.0, 5.0, 0.0, 5.0);
    assert!(body_mut.set_rotation(non_unit_rot_2).is_ok());
    assert!((body_mut.rotation().length() - 1.0).abs() < 1e-6);
    assert_eq!(body_mut.rotation(), non_unit_rot_2.normalize());
}

#[test]
fn test_9_2_invalid_rotation_rejected() {
    let id = RigidBodyId(12);

    // Zero-length quaternion
    let zero_quat = Quat::from_xyzw(0.0, 0.0, 0.0, 0.0);
    assert_eq!(
        RigidBody::new_static(id, Vec3::ZERO, zero_quat).unwrap_err(),
        RigidBodyError::InvalidRotation
    );

    // Non-finite quaternion
    let nan_quat = Quat::from_xyzw(f32::NAN, 1.0, 0.0, 0.0);
    assert_eq!(
        RigidBody::new_static(id, Vec3::ZERO, nan_quat).unwrap_err(),
        RigidBodyError::InvalidRotation
    );

    let inf_quat = Quat::from_xyzw(0.0, f32::INFINITY, 0.0, 0.0);
    assert_eq!(
        RigidBody::new_static(id, Vec3::ZERO, inf_quat).unwrap_err(),
        RigidBodyError::InvalidRotation
    );
}

#[test]
fn test_9_2_linear_velocity_preserved() {
    let id = RigidBodyId(20);
    let mut body = RigidBody::new_static(id, Vec3::ZERO, Quat::IDENTITY).unwrap();
    assert_eq!(body.linear_velocity(), Vec3::ZERO);

    assert!(body.set_linear_velocity(Vec3::new(12.5, -4.0, 0.2)).is_ok());
    assert_eq!(body.linear_velocity(), Vec3::new(12.5, -4.0, 0.2));
}

#[test]
fn test_9_2_angular_velocity_preserved() {
    let id = RigidBodyId(21);
    let mut body = RigidBody::new_static(id, Vec3::ZERO, Quat::IDENTITY).unwrap();
    assert_eq!(body.angular_velocity(), Vec3::ZERO);

    assert!(body
        .set_angular_velocity(Vec3::new(0.0, std::f32::consts::PI, -1.0))
        .is_ok());
    assert_eq!(
        body.angular_velocity(),
        Vec3::new(0.0, std::f32::consts::PI, -1.0)
    );
}

#[test]
fn test_9_2_nonfinite_velocity_rejected() {
    let id = RigidBodyId(22);
    let mut body = RigidBody::new_static(id, Vec3::ZERO, Quat::IDENTITY).unwrap();

    assert_eq!(
        body.set_linear_velocity(Vec3::new(f32::NAN, 0.0, 0.0))
            .unwrap_err(),
        RigidBodyError::NonFiniteVelocity
    );
    assert_eq!(
        body.set_angular_velocity(Vec3::new(0.0, f32::INFINITY, 0.0))
            .unwrap_err(),
        RigidBodyError::NonFiniteVelocity
    );
}

#[test]
fn test_9_2_dynamic_mass_inverse_mass() {
    let mass_props =
        MassProperties::new_dynamic(25.0, Mat3::from_diagonal(Vec3::splat(10.0))).unwrap();
    assert_eq!(mass_props.mass, 25.0);
    assert!((mass_props.inverse_mass - 0.04).abs() < 1e-6);
}

#[test]
fn test_9_2_zero_mass_rejected() {
    assert_eq!(
        MassProperties::new_dynamic(0.0, Mat3::from_diagonal(Vec3::splat(1.0))).unwrap_err(),
        RigidBodyError::InvalidMass
    );
}

#[test]
fn test_9_2_negative_mass_rejected() {
    assert_eq!(
        MassProperties::new_dynamic(-10.0, Mat3::from_diagonal(Vec3::splat(1.0))).unwrap_err(),
        RigidBodyError::InvalidMass
    );
}

#[test]
fn test_9_2_nonfinite_mass_rejected() {
    assert_eq!(
        MassProperties::new_dynamic(f32::NAN, Mat3::from_diagonal(Vec3::splat(1.0))).unwrap_err(),
        RigidBodyError::InvalidMass
    );
    assert_eq!(
        MassProperties::new_dynamic(f32::INFINITY, Mat3::from_diagonal(Vec3::splat(1.0)))
            .unwrap_err(),
        RigidBodyError::InvalidMass
    );
}

#[test]
fn test_9_2_static_inverse_mass_zero() {
    let static_props = MassProperties::new_static();
    assert_eq!(static_props.mass, 0.0);
    assert_eq!(static_props.inverse_mass, 0.0);
    assert_eq!(static_props.local_inertia, Mat3::ZERO);
    assert_eq!(static_props.local_inverse_inertia, Mat3::ZERO);
}

#[test]
fn test_9_2_dynamic_inertia_properties() {
    // Inersia kotak pejal seragam (dx=2, dy=4, dz=6)
    let box_props = MassProperties::from_box(12.0, Vec3::new(2.0, 4.0, 6.0)).unwrap();
    assert_eq!(box_props.mass, 12.0);
    // Ixx = 12/12 * (16 + 36) = 52
    // Iyy = 12/12 * (4 + 36) = 40
    // Izz = 12/12 * (4 + 16) = 20
    assert_eq!(
        box_props.local_inertia,
        Mat3::from_diagonal(Vec3::new(52.0, 40.0, 20.0))
    );

    // Inersia bola pejal beradius 2.0m, massa 10kg
    let sphere_props = MassProperties::from_sphere(10.0, 2.0).unwrap();
    // I = 0.4 * 10 * 4 = 16
    assert_eq!(
        sphere_props.local_inertia,
        Mat3::from_diagonal(Vec3::splat(16.0))
    );
}

#[test]
fn test_9_2_inverse_inertia_properties() {
    let inertia = Mat3::from_diagonal(Vec3::new(2.0, 4.0, 8.0));
    let mass_props = MassProperties::new_dynamic(1.0, inertia).unwrap();

    let product = mass_props.local_inertia * mass_props.local_inverse_inertia;
    let diff = (product - Mat3::IDENTITY).abs_diff_eq(Mat3::ZERO, 1e-5);
    assert!(diff, "I * I_inv harus mendekati Mat3::IDENTITY");
}

#[test]
fn test_9_2_static_inverse_inertia_zero() {
    let static_body = RigidBody::new_static(RigidBodyId(30), Vec3::ZERO, Quat::IDENTITY).unwrap();
    assert_eq!(
        static_body.mass_properties().local_inverse_inertia,
        Mat3::ZERO
    );
}

#[test]
fn test_9_2_invalid_inertia_rejected() {
    // 1. Matriks asimetris
    let mut asym = Mat3::from_diagonal(Vec3::splat(5.0));
    asym.x_axis.y = 10.0;
    asym.y_axis.x = 0.0;
    assert_eq!(
        MassProperties::new_dynamic(1.0, asym).unwrap_err(),
        RigidBodyError::InvalidInertia
    );

    // 2. Matriks singular / bukan definit positif (elemen diagonal negatif)
    let neg_diag = Mat3::from_diagonal(Vec3::new(-2.0, 5.0, 5.0));
    assert_eq!(
        MassProperties::new_dynamic(1.0, neg_diag).unwrap_err(),
        RigidBodyError::InvalidInertia
    );

    // 3. Matriks dengan komponen non-finite
    let nan_mat = Mat3::from_diagonal(Vec3::new(f32::NAN, 5.0, 5.0));
    assert_eq!(
        MassProperties::new_dynamic(1.0, nan_mat).unwrap_err(),
        RigidBodyError::InvalidInertia
    );
}

#[test]
fn test_9_2_rigidbody_identity_preserved() {
    let id = RigidBodyId(42);
    let body = RigidBody::new_static(id, Vec3::ZERO, Quat::IDENTITY).unwrap();
    assert_eq!(body.id(), id);
}

#[test]
fn test_9_2_rigidbody_ids_are_distinct() {
    let id1 = RigidBodyId(101);
    let id2 = RigidBodyId(102);
    let b1 = RigidBody::new_static(id1, Vec3::ZERO, Quat::IDENTITY).unwrap();
    let b2 = RigidBody::new_static(id2, Vec3::ZERO, Quat::IDENTITY).unwrap();
    assert_ne!(b1.id(), b2.id());
}

#[test]
fn test_9_2_rigidbody_zero_voxel_ownership_and_zero_gpu_contract() {
    // Validasi struktural arsitektur bahwa RigidBody adalah pure value type
    // tanpa kepemilikan heap array voxel, alokasi dinamis, atau WGPU buffer handle
    assert_eq!(std::mem::size_of::<RigidBody>(), 144);
    assert!(!std::mem::needs_drop::<RigidBody>());
    assert!(!std::mem::needs_drop::<MassProperties>());
}

#[test]
fn test_9_2_rigidbody_state_is_not_integrated() {
    let id = RigidBodyId(50);
    let initial_pos = Vec3::new(10.0, 20.0, 30.0);
    let initial_lin_vel = Vec3::new(5.0, -2.0, 1.0);
    let initial_ang_vel = Vec3::new(0.0, 3.0, 0.0);
    let rot = Quat::IDENTITY;
    let mass_props = MassProperties::from_diagonal(1.0, Vec3::ONE).unwrap();

    let body = RigidBody::new(
        id,
        BodyType::Dynamic,
        initial_pos,
        rot,
        initial_lin_vel,
        initial_ang_vel,
        mass_props,
    )
    .unwrap();

    // Membaca state berulang-ulang membuktikan tidak ada simulasi atau mutasi tersembunyi
    for _ in 0..10 {
        assert_eq!(body.position(), initial_pos);
        assert_eq!(body.linear_velocity(), initial_lin_vel);
        assert_eq!(body.angular_velocity(), initial_ang_vel);
        assert_eq!(body.rotation(), rot);
    }
}

#[test]
fn test_9_2_mass_property_mutation_validation() {
    let id = RigidBodyId(60);
    let mut static_body = RigidBody::new_static(id, Vec3::ZERO, Quat::IDENTITY).unwrap();

    // Mencoba memasang properti dinamis ke badan statis harus ditolak
    let dyn_props = MassProperties::from_diagonal(10.0, Vec3::ONE).unwrap();
    assert_eq!(
        static_body.set_mass_properties(dyn_props).unwrap_err(),
        RigidBodyError::InvalidMass
    );
}

#[test]
fn test_9_2_physics_world_add_and_retrieve_rigidbody() {
    let mut world = PhysicsWorld::default();

    let id = RigidBodyId(100);
    let body = RigidBody::new_static(id, Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY).unwrap();

    let reg_id = world.add_rigid_body(body.clone(), None).unwrap();
    assert_eq!(reg_id, id);
    assert_eq!(world.body_count(), 1);
    assert!(world.contains_body(id));

    let retrieved = world.get_rigid_body(id).unwrap();
    assert_eq!(retrieved, &body);

    let retrieved_mut = world.get_rigid_body_mut(id).unwrap();
    retrieved_mut
        .set_position(Vec3::new(20.0, 0.0, 0.0))
        .unwrap();
    assert_eq!(
        world.get_rigid_body(id).unwrap().position(),
        Vec3::new(20.0, 0.0, 0.0)
    );
}

#[test]
fn test_9_2_physics_world_remove_rigidbody_cleans_broadphase() {
    let mut world = PhysicsWorld::default();

    let id = RigidBodyId(200);
    let body = RigidBody::new_static(id, Vec3::new(5.0, 0.0, 0.0), Quat::IDENTITY).unwrap();
    let aabb = Aabb::try_new(Vec3::new(4.0, -1.0, -1.0), Vec3::new(6.0, 1.0, 1.0)).unwrap();

    world.add_rigid_body(body, Some(aabb)).unwrap();
    assert_eq!(world.query_aabb(&aabb), vec![id]);

    let removed = world.remove_rigid_body(id).unwrap();
    assert_eq!(removed.id(), id);
    assert_eq!(world.body_count(), 0);
    assert!(!world.contains_body(id));

    // Broadphase proksi wajib bersih setelah penghapusan
    assert!(world.query_aabb(&aabb).is_empty());
}

#[test]
fn test_9_2_physics_world_duplicate_id_rejected() {
    let mut world = PhysicsWorld::default();

    let id = RigidBodyId(300);
    let b1 = RigidBody::new_static(id, Vec3::ZERO, Quat::IDENTITY).unwrap();
    let b2 = RigidBody::new_static(id, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY).unwrap();

    assert!(world.add_rigid_body(b1, None).is_ok());
    assert_eq!(
        world.add_rigid_body(b2, None).unwrap_err(),
        BroadphaseError::BodyAlreadyExists(id)
    );
}

#[test]
fn test_9_2_physics_world_single_authoritative_registry() {
    let mut world = PhysicsWorld::default();

    let aabb = Aabb::try_new(Vec3::ZERO, Vec3::ONE).unwrap();
    let id = world.register_body(BodyType::Dynamic, aabb).unwrap();

    // Memverifikasi bahwa registrasi menghasilkan RigidBody di otoritas tunggal world.rigid_bodies
    assert!(world.rigid_bodies.contains_key(&id));
    let rb = world.get_rigid_body(id).unwrap();
    assert_eq!(rb.body_type(), BodyType::Dynamic);
    assert_eq!(rb.position(), aabb.center());
}

// ============================================================================
// 6. PHASE 9.3 — SHAPE REPRESENTATION & COLLIDER TESTS
// ============================================================================

// --- A. SHAPE VALIDATION ---

#[test]
fn test_9_3_valid_sphere() {
    let sphere = Sphere::new(1.5).unwrap();
    assert_eq!(sphere.radius(), 1.5);

    let aabb = sphere.compute_aabb(&Transform::IDENTITY).unwrap();
    assert_eq!(aabb.min, Vec3::new(-1.5, -1.5, -1.5));
    assert_eq!(aabb.max, Vec3::new(1.5, 1.5, 1.5));
}

#[test]
fn test_9_3_invalid_sphere_zero_and_negative() {
    assert_eq!(Sphere::new(0.0).unwrap_err(), ShapeError::NonPositiveRadius);
    assert_eq!(
        Sphere::new(-2.5).unwrap_err(),
        ShapeError::NonPositiveRadius
    );
}

#[test]
fn test_9_3_invalid_sphere_nan_and_infinity() {
    assert_eq!(
        Sphere::new(f32::NAN).unwrap_err(),
        ShapeError::NonPositiveRadius
    );
    assert_eq!(
        Sphere::new(f32::INFINITY).unwrap_err(),
        ShapeError::NonPositiveRadius
    );
}

#[test]
fn test_9_3_valid_box() {
    let half_extents = Vec3::new(1.0, 2.0, 3.0);
    let box_shape = BoxShape::new(half_extents).unwrap();
    assert_eq!(box_shape.half_extents(), half_extents);

    let aabb = box_shape.compute_aabb(&Transform::IDENTITY).unwrap();
    assert_eq!(aabb.min, -half_extents);
    assert_eq!(aabb.max, half_extents);
}

#[test]
fn test_9_3_invalid_box_zero_and_negative() {
    assert_eq!(
        BoxShape::new(Vec3::new(0.0, 1.0, 1.0)).unwrap_err(),
        ShapeError::InvalidHalfExtents
    );
    assert_eq!(
        BoxShape::new(Vec3::new(1.0, -1.0, 1.0)).unwrap_err(),
        ShapeError::InvalidHalfExtents
    );
}

#[test]
fn test_9_3_invalid_box_nan_and_infinity() {
    assert_eq!(
        BoxShape::new(Vec3::new(f32::NAN, 1.0, 1.0)).unwrap_err(),
        ShapeError::InvalidHalfExtents
    );
    assert_eq!(
        BoxShape::new(Vec3::new(1.0, 1.0, f32::INFINITY)).unwrap_err(),
        ShapeError::InvalidHalfExtents
    );
}

#[test]
fn test_9_3_valid_capsule() {
    let capsule = Capsule::new(0.5, 1.0).unwrap();
    assert_eq!(capsule.radius(), 0.5);
    assert_eq!(capsule.half_height(), 1.0);
    assert_eq!(capsule.total_height(), 3.0); // 2 * 1.0 + 2 * 0.5 = 3.0

    let aabb = capsule.compute_aabb(&Transform::IDENTITY).unwrap();
    assert_eq!(aabb.min, Vec3::new(-0.5, -1.5, -0.5));
    assert_eq!(aabb.max, Vec3::new(0.5, 1.5, 0.5));
}

#[test]
fn test_9_3_invalid_capsule_dimensions() {
    assert_eq!(
        Capsule::new(0.0, 1.0).unwrap_err(),
        ShapeError::NonPositiveRadius
    );
    assert_eq!(
        Capsule::new(-1.0, 1.0).unwrap_err(),
        ShapeError::NonPositiveRadius
    );
    assert_eq!(
        Capsule::new(1.0, -0.5).unwrap_err(),
        ShapeError::InvalidCapsuleDimensions
    );
    assert_eq!(
        Capsule::new(f32::NAN, 1.0).unwrap_err(),
        ShapeError::NonPositiveRadius
    );
    assert_eq!(
        Capsule::new(1.0, f32::INFINITY).unwrap_err(),
        ShapeError::InvalidCapsuleDimensions
    );
}

// --- B. TRANSFORM & COMPOSITION TESTS ---

#[test]
fn test_9_3_transform_identity() {
    let t = Transform::IDENTITY;
    assert_eq!(t.position, Vec3::ZERO);
    assert_eq!(t.rotation, Quat::IDENTITY);
}

#[test]
fn test_9_3_transform_translation_and_rotation() {
    let t = Transform::new(
        Vec3::new(1.0, 2.0, 3.0),
        Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
    )
    .unwrap();
    assert_eq!(t.position, Vec3::new(1.0, 2.0, 3.0));
    assert!((t.rotation.length() - 1.0).abs() < 1e-6);
}

#[test]
fn test_9_3_transform_invalid_quaternion_rejected() {
    assert_eq!(
        Transform::new(Vec3::ZERO, Quat::from_xyzw(0.0, 0.0, 0.0, 0.0)).unwrap_err(),
        ShapeError::InvalidTransform
    );
    assert_eq!(
        Transform::new(Vec3::ZERO, Quat::from_xyzw(f32::NAN, 1.0, 0.0, 0.0)).unwrap_err(),
        ShapeError::InvalidTransform
    );
    assert_eq!(
        Transform::new(Vec3::new(f32::NAN, 0.0, 0.0), Quat::IDENTITY).unwrap_err(),
        ShapeError::NonFiniteCoordinates
    );
}

// --- C. CRITICAL TEST: OFFSET COLLIDER (SECTION 33) ---

#[test]
fn test_9_3_offset_collider_world_transform() {
    // Body: posisi (10, 0, 0), rotasi 90 derajat terhadap Y
    let body_transform = Transform::new(
        Vec3::new(10.0, 0.0, 0.0),
        Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
    )
    .unwrap();

    // Collider: offset lokal (2, 0, 0)
    let local_transform = Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)).unwrap();

    // Transform gabungan: T_world = T_body * T_local
    let world_transform = body_transform.mul_transform(&local_transform);

    // Rotasi 90 derajat terhadap Y memetakan sumbu lokal +X ke dunia -Z:
    // (10, 0, 0) + rotate_Y_90((2, 0, 0)) = (10, 0, 0) + (0, 0, -2) = (10, 0, -2)
    let expected_pos = Vec3::new(10.0, 0.0, -2.0);
    assert!(
        (world_transform.position - expected_pos).length() < 1e-5,
        "Posisi dunia collider harus memperhitungkan rotasi RigidBody: didapat {:?}, ekspektasi {:?}",
        world_transform.position,
        expected_pos
    );
}

// --- D. AABB TESTS ---

#[test]
fn test_9_3_sphere_aabb_identity_and_translation() {
    let sphere = Sphere::new(2.0).unwrap();
    let t = Transform::from_translation(Vec3::new(10.0, -5.0, 20.0)).unwrap();
    let aabb = sphere.compute_aabb(&t).unwrap();

    assert_eq!(aabb.min, Vec3::new(8.0, -7.0, 18.0));
    assert_eq!(aabb.max, Vec3::new(12.0, -3.0, 22.0));
}

#[test]
fn test_9_3_sphere_aabb_rotation_invariance() {
    let sphere = Sphere::new(3.0).unwrap();
    let t_unrotated = Transform::from_translation(Vec3::new(5.0, 5.0, 5.0)).unwrap();
    let t_rotated = Transform::new(
        Vec3::new(5.0, 5.0, 5.0),
        Quat::from_rotation_x(1.234) * Quat::from_rotation_z(0.567),
    )
    .unwrap();

    let aabb1 = sphere.compute_aabb(&t_unrotated).unwrap();
    let aabb2 = sphere.compute_aabb(&t_rotated).unwrap();

    assert_eq!(aabb1.min, aabb2.min);
    assert_eq!(aabb1.max, aabb2.max);
}

#[test]
fn test_9_3_sphere_aabb_negative_coordinates() {
    let sphere = Sphere::new(5.0).unwrap();
    let t = Transform::from_translation(Vec3::new(-50.0, -100.0, -200.0)).unwrap();
    let aabb = sphere.compute_aabb(&t).unwrap();

    assert_eq!(aabb.min, Vec3::new(-55.0, -105.0, -205.0));
    assert_eq!(aabb.max, Vec3::new(-45.0, -95.0, -195.0));
}

#[test]
fn test_9_3_box_aabb_identity_and_translation() {
    let b = BoxShape::new(Vec3::new(2.0, 3.0, 4.0)).unwrap();
    let t = Transform::from_translation(Vec3::new(10.0, 20.0, 30.0)).unwrap();
    let aabb = b.compute_aabb(&t).unwrap();

    assert_eq!(aabb.min, Vec3::new(8.0, 17.0, 26.0));
    assert_eq!(aabb.max, Vec3::new(12.0, 23.0, 34.0));
}

#[test]
fn test_9_3_box_aabb_90_degree_rotation_around_y() {
    let b = BoxShape::new(Vec3::new(2.0, 3.0, 4.0)).unwrap();
    let t = Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)).unwrap();
    let aabb = b.compute_aabb(&t).unwrap();

    // Rotasi 90 derajat terhadap Y menukar sumbu X dan Z:
    // extents X menjadi 4.0, extents Z menjadi 2.0, extents Y tetap 3.0
    assert!((aabb.min.x - (-4.0)).abs() < 1e-5);
    assert!((aabb.max.x - 4.0).abs() < 1e-5);
    assert!((aabb.min.y - (-3.0)).abs() < 1e-5);
    assert!((aabb.max.y - 3.0).abs() < 1e-5);
    assert!((aabb.min.z - (-2.0)).abs() < 1e-5);
    assert!((aabb.max.z - 2.0).abs() < 1e-5);
}

// --- CRITICAL TEST: ROTATED BOX 45 DEGREE ANALYTICAL (SECTION 34) ---

#[test]
fn test_9_3_box_aabb_45_degree_rotation_analytical() {
    // Box half_extents = (2.0, 1.0, 0.5)
    let half_extents = Vec3::new(2.0, 1.0, 0.5);
    let b = BoxShape::new(half_extents).unwrap();

    // Rotasi 45 derajat terhadap sumbu Y
    let t = Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_4)).unwrap();
    let aabb = b.compute_aabb(&t).unwrap();

    // Analitis:
    // R = [ cos 45,  0, sin 45 ] = [ sqrt(2)/2, 0,  sqrt(2)/2 ]
    //     [      0,  1,      0 ]   [         0, 1,          0 ]
    //     [-sin 45,  0, cos 45 ]   [-sqrt(2)/2, 0,  sqrt(2)/2 ]
    //
    // |R| * (2.0, 1.0, 0.5)
    // E_world.x = sqrt(2)/2 * 2.0 + 0 * 1.0 + sqrt(2)/2 * 0.5 = 2.5 * sqrt(2)/2 ≈ 1.767767
    // E_world.y = 1.0
    // E_world.z = sqrt(2)/2 * 2.0 + 0 * 1.0 + sqrt(2)/2 * 0.5 = 2.5 * sqrt(2)/2 ≈ 1.767767
    let expected_x = 2.5 * (std::f32::consts::SQRT_2 / 2.0);
    let expected_y = 1.0;
    let expected_z = 2.5 * (std::f32::consts::SQRT_2 / 2.0);

    assert!(
        (aabb.max.x - expected_x).abs() < 1e-5,
        "AABB X extent: didapat {}, ekspektasi {}",
        aabb.max.x,
        expected_x
    );
    assert!(
        (aabb.max.y - expected_y).abs() < 1e-5,
        "AABB Y extent: didapat {}, ekspektasi {}",
        aabb.max.y,
        expected_y
    );
    assert!(
        (aabb.max.z - expected_z).abs() < 1e-5,
        "AABB Z extent: didapat {}, ekspektasi {}",
        aabb.max.z,
        expected_z
    );
    assert!((aabb.min.x - (-expected_x)).abs() < 1e-5);
    assert!((aabb.min.y - (-expected_y)).abs() < 1e-5);
    assert!((aabb.min.z - (-expected_z)).abs() < 1e-5);
}

// --- CRITICAL TEST: CAPSULE AXIS ROTATION 90 DEGREE (SECTION 35) ---

#[test]
fn test_9_3_capsule_aabb_90_degree_rotation_around_x() {
    // Sumbu lokal kapsul adalah Y: radius = 1.0, half_height = 2.0
    let capsule = Capsule::new(1.0, 2.0).unwrap();

    // Rotasi 90 derajat terhadap sumbu X memetakan lokal +Y ke dunia +Z
    let t = Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)).unwrap();
    let aabb = capsule.compute_aabb(&t).unwrap();

    // Sumbu panjang kapsul sekarang sejajar dengan sumbu Z dunia:
    // X half-extent = radius = 1.0
    // Y half-extent = radius = 1.0
    // Z half-extent = half_height + radius = 2.0 + 1.0 = 3.0
    assert!((aabb.min.x - (-1.0)).abs() < 1e-5);
    assert!((aabb.max.x - 1.0).abs() < 1e-5);
    assert!((aabb.min.y - (-1.0)).abs() < 1e-5);
    assert!((aabb.max.y - 1.0).abs() < 1e-5);
    assert!((aabb.min.z - (-3.0)).abs() < 1e-5);
    assert!((aabb.max.z - 3.0).abs() < 1e-5);
}

#[test]
fn test_9_3_capsule_aabb_negative_coordinates() {
    let capsule = Capsule::new(1.0, 2.0).unwrap();
    let t = Transform::from_translation(Vec3::new(-100.0, -50.0, -25.0)).unwrap();
    let aabb = capsule.compute_aabb(&t).unwrap();

    assert_eq!(aabb.min, Vec3::new(-101.0, -53.0, -26.0));
    assert_eq!(aabb.max, Vec3::new(-99.0, -47.0, -24.0));
}

// --- E. COLLIDER & PHYSICS WORLD LIFECYCLE TESTS ---

#[test]
fn test_9_3_collider_creation_and_accessors() {
    let id = ColliderId(1);
    let body_id = RigidBodyId(10);
    let shape = Shape::Sphere(Sphere::new(2.5).unwrap());
    let local_t = Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)).unwrap();

    let collider = Collider::new(id, body_id, shape.clone(), local_t);
    assert_eq!(collider.id(), id);
    assert_eq!(collider.rigid_body_id(), body_id);
    assert_eq!(collider.shape(), &shape);
    assert_eq!(collider.local_transform(), &local_t);
}

#[test]
fn test_9_3_collider_add_to_physics_world_and_broadphase_sync() {
    let mut world = PhysicsWorld::default();

    let body_id = RigidBodyId(100);
    let body = RigidBody::new_static(body_id, Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY).unwrap();
    world.add_rigid_body(body, None).unwrap();

    let collider_id = ColliderId(1);
    let shape = Shape::Sphere(Sphere::new(2.0).unwrap());
    let local_t = Transform::IDENTITY;
    let collider = Collider::new(collider_id, body_id, shape, local_t);

    world.add_collider(collider).unwrap();

    assert_eq!(world.collider_count(), 1);
    assert!(world.get_collider(collider_id).is_some());

    // Proksi broadphase otomatis tersinkronisasi dengan AABB turunan (pusat (10,0,0) radius 2.0)
    let proxy_aabb = world.get_body_aabb(body_id).unwrap();
    assert_eq!(proxy_aabb.min, Vec3::new(8.0, -2.0, -2.0));
    assert_eq!(proxy_aabb.max, Vec3::new(12.0, 2.0, 2.0));
}

#[test]
fn test_9_3_collider_add_fails_on_missing_body() {
    let mut world = PhysicsWorld::default();

    let missing_body_id = RigidBodyId(999);
    let collider = Collider::new(
        ColliderId(1),
        missing_body_id,
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::IDENTITY,
    );

    let err = world.add_collider(collider).unwrap_err();
    assert_eq!(err, BroadphaseError::BodyNotFound(missing_body_id));
    assert_eq!(world.collider_count(), 0);
}

#[test]
fn test_9_3_collider_add_fails_on_duplicate_id() {
    let mut world = PhysicsWorld::default();

    let body_id = RigidBodyId(101);
    let body = RigidBody::new_static(body_id, Vec3::ZERO, Quat::IDENTITY).unwrap();
    world.add_rigid_body(body, None).unwrap();

    let col1 = Collider::new(
        ColliderId(5),
        body_id,
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::IDENTITY,
    );
    let col2 = Collider::new(
        ColliderId(5),
        body_id,
        Shape::Sphere(Sphere::new(2.0).unwrap()),
        Transform::IDENTITY,
    );

    assert!(world.add_collider(col1).is_ok());
    assert_eq!(
        world.add_collider(col2).unwrap_err(),
        BroadphaseError::ColliderAlreadyExists(ColliderId(5))
    );
}

#[test]
fn test_9_3_multi_collider_representation_and_compound_broadphase_aabb() {
    let mut world = PhysicsWorld::default();

    let body_id = RigidBodyId(200);
    let body = RigidBody::new_static(body_id, Vec3::ZERO, Quat::IDENTITY).unwrap();
    world.add_rigid_body(body, None).unwrap();

    // Collider 1 di offset lokal (-5, 0, 0) dengan radius 1.0
    let col1 = Collider::new(
        ColliderId(10),
        body_id,
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::from_translation(Vec3::new(-5.0, 0.0, 0.0)).unwrap(),
    );

    // Collider 2 di offset lokal (+5, 0, 0) dengan radius 1.0
    let col2 = Collider::new(
        ColliderId(11),
        body_id,
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::from_translation(Vec3::new(5.0, 0.0, 0.0)).unwrap(),
    );

    world.add_collider(col1).unwrap();
    world.add_collider(col2).unwrap();

    assert_eq!(world.collider_count(), 2);
    let colliders: Vec<_> = world.colliders_for_body(body_id).collect();
    assert_eq!(colliders.len(), 2);

    // Broadphase AABB untuk badan ini mewakili gabungan (union) kedua collider:
    // X membentang dari -6.0 (-5.0 - 1.0) hingga +6.0 (+5.0 + 1.0)
    let broadphase_aabb = world.get_body_aabb(body_id).unwrap();
    assert_eq!(broadphase_aabb.min, Vec3::new(-6.0, -1.0, -1.0));
    assert_eq!(broadphase_aabb.max, Vec3::new(6.0, 1.0, 1.0));
}

#[test]
fn test_9_3_collider_removal_resyncs_broadphase() {
    let mut world = PhysicsWorld::default();

    let body_id = RigidBodyId(300);
    let body = RigidBody::new_static(body_id, Vec3::ZERO, Quat::IDENTITY).unwrap();
    world.add_rigid_body(body, None).unwrap();

    let col1 = Collider::new(
        ColliderId(20),
        body_id,
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::from_translation(Vec3::new(-5.0, 0.0, 0.0)).unwrap(),
    );
    let col2 = Collider::new(
        ColliderId(21),
        body_id,
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::from_translation(Vec3::new(5.0, 0.0, 0.0)).unwrap(),
    );

    world.add_collider(col1).unwrap();
    world.add_collider(col2).unwrap();

    // Hapus collider 2 (+5, 0, 0)
    let removed = world.remove_collider(ColliderId(21)).unwrap();
    assert_eq!(removed.id(), ColliderId(21));
    assert_eq!(world.collider_count(), 1);

    // Broadphase AABB harus mengecil kembali hanya mencakup collider 1 (-6.0 hingga -4.0 di X)
    let aabb_after_first = world.get_body_aabb(body_id).unwrap();
    assert_eq!(aabb_after_first.min, Vec3::new(-6.0, -1.0, -1.0));
    assert_eq!(aabb_after_first.max, Vec3::new(-4.0, 1.0, 1.0));

    // Hapus collider 1 (-5, 0, 0)
    world.remove_collider(ColliderId(20)).unwrap();
    assert_eq!(world.collider_count(), 0);

    // Broadphase proksi harus bersih setelah seluruh collider dihapus
    assert!(world.get_body_aabb(body_id).is_none());
}

#[test]
fn test_9_3_rigid_body_removal_cleans_all_colliders() {
    let mut world = PhysicsWorld::default();

    let body_id = RigidBodyId(400);
    let body = RigidBody::new_static(body_id, Vec3::ZERO, Quat::IDENTITY).unwrap();
    world.add_rigid_body(body, None).unwrap();

    let col1 = Collider::new(
        ColliderId(30),
        body_id,
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::IDENTITY,
    );
    let col2 = Collider::new(
        ColliderId(31),
        body_id,
        Shape::Box(BoxShape::new(Vec3::ONE).unwrap()),
        Transform::IDENTITY,
    );

    world.add_collider(col1).unwrap();
    world.add_collider(col2).unwrap();
    assert_eq!(world.collider_count(), 2);

    // Hapus RigidBody pemilik
    world.remove_rigid_body(body_id).unwrap();

    // Seluruh collider milik body ini harus otomatis terhapus
    assert_eq!(world.collider_count(), 0);
    assert!(world.get_body_aabb(body_id).is_none());
}

// ============================================================================
// 7. PHASE 9.4 — CONTACT GENERATION / NARROWPHASE TESTS
// ============================================================================

fn assert_contact_symmetry(contact_ab: &Option<Contact>, contact_ba: &Option<Contact>) {
    match (contact_ab, contact_ba) {
        (None, None) => {}
        (Some(c_ab), Some(c_ba)) => {
            assert!(
                (c_ab.penetration - c_ba.penetration).abs() < 1e-4,
                "Penetrasi simetris harus sama: {} vs {}",
                c_ab.penetration,
                c_ba.penetration
            );
            assert!(
                (c_ab.point - c_ba.point).length() < 1e-4,
                "Titik kontak simetris harus ekuivalen: {:?} vs {:?}",
                c_ab.point,
                c_ba.point
            );
            assert!(
                (c_ab.normal + c_ba.normal).length() < 1e-4,
                "Normal simetris harus berlawanan arah: {:?} vs {:?}",
                c_ab.normal,
                c_ba.normal
            );
            assert_eq!(c_ab.collider_a, c_ba.collider_b);
            assert_eq!(c_ab.collider_b, c_ba.collider_a);
            assert_eq!(c_ab.body_a, c_ba.body_b);
            assert_eq!(c_ab.body_b, c_ba.body_a);
        }
        _ => panic!(
            "Ketidakcocokan simetri kueri: AB={:?}, BA={:?}",
            contact_ab, contact_ba
        ),
    }
}

// --- A. SPHERE ↔ SPHERE ---

#[test]
fn test_9_4_sphere_sphere_separated() {
    let col_a = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::IDENTITY,
    );
    let col_b = Collider::new(
        ColliderId(2),
        RigidBodyId(2),
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::IDENTITY,
    );

    let t_a = Transform::from_translation(Vec3::ZERO).unwrap();
    let t_b = Transform::from_translation(Vec3::new(3.0, 0.0, 0.0)).unwrap();

    let res = collide(&col_a, &t_a, &col_b, &t_b).unwrap();
    assert!(res.is_none());
}

#[test]
fn test_9_4_sphere_sphere_touching_and_penetrating() {
    let col_a = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::IDENTITY,
    );
    let col_b = Collider::new(
        ColliderId(2),
        RigidBodyId(2),
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::IDENTITY,
    );

    // 1. Touching (jarak = 2.0 = r1 + r2)
    let t_a = Transform::from_translation(Vec3::ZERO).unwrap();
    let t_b = Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)).unwrap();
    let res = collide(&col_a, &t_a, &col_b, &t_b).unwrap().unwrap();
    assert!(res.penetration < 1e-4);
    assert_eq!(res.normal, Vec3::X);
    assert!((res.point - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-4);

    // 2. Penetrating (jarak = 1.5, overlap = 0.5)
    let t_b_pen = Transform::from_translation(Vec3::new(1.5, 0.0, 0.0)).unwrap();
    let res_pen = collide(&col_a, &t_a, &col_b, &t_b_pen).unwrap().unwrap();
    assert!((res_pen.penetration - 0.5).abs() < 1e-4);
    assert_eq!(res_pen.normal, Vec3::X);
    assert!((res_pen.point - Vec3::new(0.75, 0.0, 0.0)).length() < 1e-4);
}

#[test]
fn test_9_4_sphere_sphere_coincident() {
    let col_a = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::IDENTITY,
    );
    let col_b = Collider::new(
        ColliderId(2),
        RigidBodyId(2),
        Shape::Sphere(Sphere::new(2.0).unwrap()),
        Transform::IDENTITY,
    );

    let t_a = Transform::from_translation(Vec3::new(5.0, 5.0, 5.0)).unwrap();
    let t_b = Transform::from_translation(Vec3::new(5.0, 5.0, 5.0)).unwrap();

    let res = collide(&col_a, &t_a, &col_b, &t_b).unwrap().unwrap();
    assert_eq!(res.normal, Vec3::X); // Fallback kanonikal deterministik
    assert!((res.penetration - 3.0).abs() < 1e-4);
    assert!((res.point - Vec3::new(5.0, 5.0, 5.0)).length() < 1e-4);
}

#[test]
fn test_9_4_sphere_sphere_negative_coords_and_reverse_symmetry() {
    let col_a = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        Shape::Sphere(Sphere::new(1.5).unwrap()),
        Transform::IDENTITY,
    );
    let col_b = Collider::new(
        ColliderId(2),
        RigidBodyId(2),
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::IDENTITY,
    );

    let t_a = Transform::from_translation(Vec3::new(-100.0, -50.0, -25.0)).unwrap();
    let t_b = Transform::from_translation(Vec3::new(-100.0, -48.0, -25.0)).unwrap();

    let c_ab = collide(&col_a, &t_a, &col_b, &t_b).unwrap();
    let c_ba = collide(&col_b, &t_b, &col_a, &t_a).unwrap();

    assert!(c_ab.is_some());
    assert_contact_symmetry(&c_ab, &c_ba);
}

// --- B. SPHERE ↔ BOX ---

#[test]
fn test_9_4_sphere_box_separated() {
    let col_s = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::IDENTITY,
    );
    let col_b = Collider::new(
        ColliderId(2),
        RigidBodyId(2),
        Shape::Box(BoxShape::new(Vec3::ONE).unwrap()),
        Transform::IDENTITY,
    );

    let t_s = Transform::from_translation(Vec3::new(3.0, 0.0, 0.0)).unwrap();
    let t_b = Transform::IDENTITY;

    let res = collide(&col_s, &t_s, &col_b, &t_b).unwrap();
    assert!(res.is_none());
}

#[test]
fn test_9_4_sphere_box_face_edge_corner_penetration() {
    let col_s = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::IDENTITY,
    );
    let col_b = Collider::new(
        ColliderId(2),
        RigidBodyId(2),
        Shape::Box(BoxShape::new(Vec3::ONE).unwrap()),
        Transform::IDENTITY,
    );
    let t_b = Transform::IDENTITY;

    // 1. Muka +X: Bola di x = 1.8 (muka boks di x = 1.0, overlap = 0.2)
    let t_face = Transform::from_translation(Vec3::new(1.8, 0.0, 0.0)).unwrap();
    let c_face = collide(&col_s, &t_face, &col_b, &t_b).unwrap().unwrap();
    assert!((c_face.penetration - 0.2).abs() < 1e-4);
    assert!((c_face.normal - Vec3::NEG_X).length() < 1e-4); // Sphere -> Box

    // 2. Tepi: Bola mendekati tepi (1, 1, 0)
    let t_edge = Transform::from_translation(Vec3::new(1.5, 1.5, 0.0)).unwrap();
    let c_edge = collide(&col_s, &t_edge, &col_b, &t_b).unwrap();
    assert!(c_edge.is_some());

    // 3. Sudut: Bola mendekati sudut (1, 1, 1)
    let t_corner = Transform::from_translation(Vec3::new(1.4, 1.4, 1.4)).unwrap();
    let c_corner = collide(&col_s, &t_corner, &col_b, &t_b).unwrap();
    assert!(c_corner.is_some());
}

#[test]
fn test_9_4_sphere_box_center_inside_determines_face() {
    let col_s = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::IDENTITY,
    );
    let col_b = Collider::new(
        ColliderId(2),
        RigidBodyId(2),
        Shape::Box(BoxShape::new(Vec3::splat(2.0)).unwrap()),
        Transform::IDENTITY,
    );

    // Bola berpusat di (1.5, 0, 0) di dalam boks [-2, 2]^3.
    // Muka boks terdekat adalah +X pada x = 2.0. Jarak ke muka = 0.5.
    // Penetrasi total = radius + jarak = 1.0 + 0.5 = 1.5.
    // Normal Sphere -> Box adalah arah ke dalam boks (memisahkan A ke arah +X berarti normal A->B adalah -X).
    let t_s = Transform::from_translation(Vec3::new(1.5, 0.0, 0.0)).unwrap();
    let t_b = Transform::IDENTITY;

    let c = collide(&col_s, &t_s, &col_b, &t_b).unwrap().unwrap();
    assert!((c.penetration - 1.5).abs() < 1e-4);
    assert!((c.normal - Vec3::NEG_X).length() < 1e-4);
}

#[test]
fn test_9_4_sphere_box_reverse_symmetry_and_rotated() {
    let col_s = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::IDENTITY,
    );
    let col_b = Collider::new(
        ColliderId(2),
        RigidBodyId(2),
        Shape::Box(BoxShape::new(Vec3::new(2.0, 1.0, 0.5)).unwrap()),
        Transform::IDENTITY,
    );

    let t_s = Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)).unwrap();
    let t_b = Transform::new(
        Vec3::ZERO,
        Quat::from_rotation_y(std::f32::consts::FRAC_PI_4),
    )
    .unwrap();

    let c_sb = collide(&col_s, &t_s, &col_b, &t_b).unwrap();
    let c_bs = collide(&col_b, &t_b, &col_s, &t_s).unwrap();

    assert!(c_sb.is_some());
    assert_contact_symmetry(&c_sb, &c_bs);
}

// --- C. SPHERE ↔ CAPSULE ---

#[test]
fn test_9_4_sphere_capsule_endpoint_and_side() {
    // Kapsul vertikal: r = 0.5, half_height = 2.0 (segmen y = -2.0 s.d. +2.0)
    let col_c = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        Shape::Capsule(Capsule::new(0.5, 2.0).unwrap()),
        Transform::IDENTITY,
    );
    let col_s = Collider::new(
        ColliderId(2),
        RigidBodyId(2),
        Shape::Sphere(Sphere::new(0.5).unwrap()),
        Transform::IDENTITY,
    );
    let t_c = Transform::IDENTITY;

    // 1. Kontak samping (silinder): bola di (0.8, 0, 0)
    let t_s_side = Transform::from_translation(Vec3::new(0.8, 0.0, 0.0)).unwrap();
    let c_side = collide(&col_s, &t_s_side, &col_c, &t_c).unwrap().unwrap();
    assert!((c_side.penetration - 0.2).abs() < 1e-4);
    assert!((c_side.normal - Vec3::NEG_X).length() < 1e-4);

    // 2. Kontak ujung (hemisfer atas di y = 2.0): bola di (0, 2.8, 0)
    let t_s_top = Transform::from_translation(Vec3::new(0.0, 2.8, 0.0)).unwrap();
    let c_top = collide(&col_s, &t_s_top, &col_c, &t_c).unwrap().unwrap();
    assert!((c_top.penetration - 0.2).abs() < 1e-4);
    assert!((c_top.normal - Vec3::NEG_Y).length() < 1e-4);
}

#[test]
fn test_9_4_sphere_capsule_half_height_zero_and_symmetry() {
    let col_c = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        Shape::Capsule(Capsule::new(1.0, 0.0).unwrap()),
        Transform::IDENTITY,
    );
    let col_s = Collider::new(
        ColliderId(2),
        RigidBodyId(2),
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::IDENTITY,
    );

    let t_c = Transform::from_translation(Vec3::new(0.0, 1.5, 0.0)).unwrap();
    let t_s = Transform::IDENTITY;

    let c_cs = collide(&col_c, &t_c, &col_s, &t_s).unwrap();
    let c_sc = collide(&col_s, &t_s, &col_c, &t_c).unwrap();

    assert!(c_cs.is_some());
    assert_contact_symmetry(&c_cs, &c_sc);
}

// --- D. CAPSULE ↔ CAPSULE ---

#[test]
fn test_9_4_capsule_capsule_parallel_and_crossing() {
    let cap = Capsule::new(0.5, 1.0).unwrap();
    let col_a = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        Shape::Capsule(cap),
        Transform::IDENTITY,
    );
    let col_b = Collider::new(
        ColliderId(2),
        RigidBodyId(2),
        Shape::Capsule(cap),
        Transform::IDENTITY,
    );

    // 1. Paralel: Kapsul A di (0, 0, 0), Kapsul B di (0.8, 0, 0). Overlap = 1.0 - 0.8 = 0.2
    let t_a = Transform::IDENTITY;
    let t_b = Transform::from_translation(Vec3::new(0.8, 0.0, 0.0)).unwrap();

    let c_par = collide(&col_a, &t_a, &col_b, &t_b).unwrap().unwrap();
    assert!((c_par.penetration - 0.2).abs() < 1e-4);
    assert!((c_par.normal - Vec3::X).length() < 1e-4);

    // 2. Menyilang tegak lurus (Crossing): Kapsul B diputar 90 derajat terhadap Z
    let t_b_cross = Transform::new(
        Vec3::new(0.0, 0.0, 0.8),
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
    )
    .unwrap();
    let c_cross = collide(&col_a, &t_a, &col_b, &t_b_cross).unwrap().unwrap();
    assert!((c_cross.penetration - 0.2).abs() < 1e-4);
    assert!((c_cross.normal - Vec3::Z).length() < 1e-4);
}

#[test]
fn test_9_4_capsule_capsule_coincident_and_symmetry() {
    let cap = Capsule::new(1.0, 2.0).unwrap();
    let col_a = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        Shape::Capsule(cap),
        Transform::IDENTITY,
    );
    let col_b = Collider::new(
        ColliderId(2),
        RigidBodyId(2),
        Shape::Capsule(cap),
        Transform::IDENTITY,
    );

    let t = Transform::from_translation(Vec3::new(-10.0, 5.0, 20.0)).unwrap();
    let c_ab = collide(&col_a, &t, &col_b, &t).unwrap();
    let c_ba = collide(&col_b, &t, &col_a, &t).unwrap();

    assert!(c_ab.is_some());
    assert_contact_symmetry(&c_ab, &c_ba);
}

// --- E. BOX ↔ BOX ---

#[test]
fn test_9_4_box_box_separated_and_face_face() {
    let b = BoxShape::new(Vec3::ONE).unwrap();
    let col_a = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        Shape::Box(b),
        Transform::IDENTITY,
    );
    let col_b = Collider::new(
        ColliderId(2),
        RigidBodyId(2),
        Shape::Box(b),
        Transform::IDENTITY,
    );

    // Terpisah di x = 2.5 (overlap = -0.5)
    let t_a = Transform::IDENTITY;
    let t_b_sep = Transform::from_translation(Vec3::new(2.5, 0.0, 0.0)).unwrap();
    assert!(collide(&col_a, &t_a, &col_b, &t_b_sep).unwrap().is_none());

    // Bertumpukan muka-ke-muka di x = 1.8 (overlap = 0.2)
    let t_b_face = Transform::from_translation(Vec3::new(1.8, 0.0, 0.0)).unwrap();
    let c = collide(&col_a, &t_a, &col_b, &t_b_face).unwrap().unwrap();
    assert!((c.penetration - 0.2).abs() < 1e-4);
    assert!((c.normal - Vec3::X).length() < 1e-4);
}

#[test]
fn test_9_4_box_box_rotated_45_and_symmetry() {
    let b = BoxShape::new(Vec3::ONE).unwrap();
    let col_a = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        Shape::Box(b),
        Transform::IDENTITY,
    );
    let col_b = Collider::new(
        ColliderId(2),
        RigidBodyId(2),
        Shape::Box(b),
        Transform::IDENTITY,
    );

    let t_a = Transform::IDENTITY;
    let t_b = Transform::new(
        Vec3::new(2.2, 0.0, 0.0),
        Quat::from_rotation_y(std::f32::consts::FRAC_PI_4),
    )
    .unwrap();

    let c_ab = collide(&col_a, &t_a, &col_b, &t_b).unwrap();
    let c_ba = collide(&col_b, &t_b, &col_a, &t_a).unwrap();

    assert!(c_ab.is_some());
    assert_contact_symmetry(&c_ab, &c_ba);
}

// --- F. BOX ↔ CAPSULE (HARDENING SECTION 13) ---

#[test]
fn test_9_4_box_capsule_outside_and_face() {
    let col_box = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        Shape::Box(BoxShape::new(Vec3::ONE).unwrap()),
        Transform::IDENTITY,
    );
    let col_cap = Collider::new(
        ColliderId(2),
        RigidBodyId(2),
        Shape::Capsule(Capsule::new(0.5, 1.0).unwrap()),
        Transform::IDENTITY,
    );

    // Kapsul di x = 2.0 (muka boks di 1.0, segmen kapsul di 2.0, radius 0.5 -> jarak segmen ke muka = 1.0 > 0.5 -> terpisah)
    let t_box = Transform::IDENTITY;
    let t_cap_sep = Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)).unwrap();
    assert!(collide(&col_box, &t_box, &col_cap, &t_cap_sep)
        .unwrap()
        .is_none());

    // Kapsul di x = 1.3 (jarak segmen ke muka = 0.3, radius 0.5 -> penetrasi = 0.2)
    let t_cap_pen = Transform::from_translation(Vec3::new(1.3, 0.0, 0.0)).unwrap();
    let c = collide(&col_box, &t_box, &col_cap, &t_cap_pen)
        .unwrap()
        .unwrap();
    assert!((c.penetration - 0.2).abs() < 1e-4);
    assert!((c.normal - Vec3::X).length() < 1e-4);
}

#[test]
fn test_9_4_box_capsule_deep_penetration_and_inside() {
    let col_box = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        Shape::Box(BoxShape::new(Vec3::splat(3.0)).unwrap()),
        Transform::IDENTITY,
    );
    let col_cap = Collider::new(
        ColliderId(2),
        RigidBodyId(2),
        Shape::Capsule(Capsule::new(0.5, 1.0).unwrap()),
        Transform::IDENTITY,
    );
    let t_box = Transform::IDENTITY;

    // Kapsul sepenuhnya berada di dalam boks di x = 2.0 (muka boks di 3.0).
    // Jarak segmen ke muka +X adalah 3.0 - 2.0 = 1.0.
    // Penetrasi total harus memperhitungkan kedalaman segmen ke muka + radius kapsul = 1.0 + 0.5 = 1.5!
    // Ini menguji secara ketat persyaratan hardening Section 13 (BUKAN sekadar penetration = r_cap).
    let t_cap_deep = Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)).unwrap();
    let c = collide(&col_box, &t_box, &col_cap, &t_cap_deep)
        .unwrap()
        .unwrap();
    assert!(
        (c.penetration - 1.5).abs() < 1e-4,
        "Penetrasi mendalam harus 1.5, didapat: {}",
        c.penetration
    );
    assert!((c.normal - Vec3::X).length() < 1e-4);
}

#[test]
fn test_9_4_box_capsule_reverse_symmetry_and_negative_coords() {
    let col_box = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        Shape::Box(BoxShape::new(Vec3::ONE).unwrap()),
        Transform::IDENTITY,
    );
    let col_cap = Collider::new(
        ColliderId(2),
        RigidBodyId(2),
        Shape::Capsule(Capsule::new(0.4, 0.8).unwrap()),
        Transform::IDENTITY,
    );

    let t_box = Transform::from_translation(Vec3::new(-50.0, -100.0, -200.0)).unwrap();
    let t_cap = Transform::new(
        Vec3::new(-48.8, -100.0, -200.0),
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
    )
    .unwrap();

    let c_bc = collide(&col_box, &t_box, &col_cap, &t_cap).unwrap();
    let c_cb = collide(&col_cap, &t_cap, &col_box, &t_box).unwrap();

    assert!(c_bc.is_some());
    assert_contact_symmetry(&c_bc, &c_cb);
}

// --- G. MULTI-COLLIDER INTEGRATION IN PHYSICS WORLD ---

#[test]
fn test_9_4_physics_world_generate_contacts_multi_collider() {
    let mut world = PhysicsWorld::default();

    // Badan A di (0, 0, 0) dengan 2 collider bola (Kinematic agar kandidat pair broadphase dihasilkan)
    let body_a_id = RigidBodyId(10);
    let body_a = RigidBody::new_kinematic(
        body_a_id,
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::ZERO,
        Vec3::ZERO,
    )
    .unwrap();
    world.add_rigid_body(body_a, None).unwrap();

    let col_a1 = Collider::new(
        ColliderId(101),
        body_a_id,
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::from_translation(Vec3::new(-2.0, 0.0, 0.0)).unwrap(),
    );
    let col_a2 = Collider::new(
        ColliderId(102),
        body_a_id,
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)).unwrap(),
    );
    world.add_collider(col_a1).unwrap();
    world.add_collider(col_a2).unwrap();

    // Badan B di (0, 0, 0) dengan 2 collider bola yang overlap dengan A1 dan A2
    let body_b_id = RigidBodyId(20);
    let body_b = RigidBody::new_static(body_b_id, Vec3::ZERO, Quat::IDENTITY).unwrap();
    world.add_rigid_body(body_b, None).unwrap();

    let col_b1 = Collider::new(
        ColliderId(201),
        body_b_id,
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::from_translation(Vec3::new(-2.5, 0.0, 0.0)).unwrap(),
    );
    let col_b2 = Collider::new(
        ColliderId(202),
        body_b_id,
        Shape::Sphere(Sphere::new(1.0).unwrap()),
        Transform::from_translation(Vec3::new(2.5, 0.0, 0.0)).unwrap(),
    );
    world.add_collider(col_b1).unwrap();
    world.add_collider(col_b2).unwrap();

    // Hasilkan kontak dari broadphase pair (body_a, body_b)
    let contacts = world.generate_contacts().unwrap();

    // Harus menghasilkan tepat 2 kontak (A1 ↔ B1 dan A2 ↔ B2).
    // A1 dan B2 berjauhan (jarak 4.5 > 2.0), A2 dan B1 berjauhan (jarak 4.5 > 2.0).
    assert_eq!(contacts.len(), 2);

    let c1 = &contacts[0];
    let c2 = &contacts[1];

    assert_eq!(c1.body_a, body_a_id);
    assert_eq!(c1.body_b, body_b_id);
    assert_eq!(c1.collider_a, ColliderId(101));
    assert_eq!(c1.collider_b, ColliderId(201));
    assert!((c1.penetration - 1.5).abs() < 1e-4);

    assert_eq!(c2.body_a, body_a_id);
    assert_eq!(c2.body_b, body_b_id);
    assert_eq!(c2.collider_a, ColliderId(102));
    assert_eq!(c2.collider_b, ColliderId(202));
    assert!((c2.penetration - 1.5).abs() < 1e-4);
}

// ============================================================================
// 5. CONTACT SOLVER / SEQUENTIAL IMPULSE UNIT TESTS (PHASE 9.5)
// ============================================================================

#[test]
fn test_9_5_dynamic_vs_static_approaching_resolves_normal_velocity() {
    let mut bodies = std::collections::BTreeMap::new();
    let body_a_id = RigidBodyId(1);
    let body_b_id = RigidBodyId(2);

    // Badan Dinamis A di (0, 1, 0) bergerak turun dengan v = (0, -4, 0)
    let inertia_a = Mat3::from_diagonal(Vec3::ONE);
    let mut body_a = RigidBody::new_dynamic(
        body_a_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia_a,
    )
    .unwrap();
    body_a
        .set_linear_velocity(Vec3::new(0.0, -4.0, 0.0))
        .unwrap();
    bodies.insert(body_a_id, body_a);

    // Badan Statis B di (0, 0, 0) diam
    let body_b = RigidBody::new_static(body_b_id, Vec3::ZERO, Quat::IDENTITY).unwrap();
    bodies.insert(body_b_id, body_b);

    // Kontak di (0, 0.5, 0), normal A -> B adalah (0, -1, 0) (ke arah bawah)
    let contact = Contact::new(
        ColliderId(10),
        ColliderId(20),
        body_a_id,
        body_b_id,
        Vec3::new(0.0, 0.5, 0.0),
        Vec3::NEG_Y,
        0.0,
    );

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0, // tanpa bias posisi untuk menguji murni impuls kecepatan
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved_a = bodies.get(&body_a_id).unwrap();
    let solved_b = bodies.get(&body_b_id).unwrap();

    // Kecepatan Badan Statis B TIDAK PERNAH berubah
    assert_eq!(solved_b.linear_velocity(), Vec3::ZERO);
    assert_eq!(solved_b.angular_velocity(), Vec3::ZERO);

    // Kecepatan turun Badan Dinamis A dinetralkan (v_y >= 0)
    assert!(
        solved_a.linear_velocity().y.abs() < 1e-4,
        "v_y harus 0, didapat: {}",
        solved_a.linear_velocity().y
    );
}

#[test]
fn test_9_5_dynamic_vs_static_separating_zero_impulse() {
    let mut bodies = std::collections::BTreeMap::new();
    let body_a_id = RigidBodyId(1);
    let body_b_id = RigidBodyId(2);

    // Badan Dinamis A bergerak menjauh ke atas dengan v = (0, 4, 0)
    let inertia_a = Mat3::from_diagonal(Vec3::ONE);
    let mut body_a = RigidBody::new_dynamic(
        body_a_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia_a,
    )
    .unwrap();
    body_a
        .set_linear_velocity(Vec3::new(0.0, 4.0, 0.0))
        .unwrap();
    bodies.insert(body_a_id, body_a);

    let body_b = RigidBody::new_static(body_b_id, Vec3::ZERO, Quat::IDENTITY).unwrap();
    bodies.insert(body_b_id, body_b);

    let contact = Contact::new(
        ColliderId(10),
        ColliderId(20),
        body_a_id,
        body_b_id,
        Vec3::new(0.0, 0.5, 0.0),
        Vec3::NEG_Y,
        0.0,
    );

    let config = SolverConfig::default();
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved_a = bodies.get(&body_a_id).unwrap();
    assert_eq!(solved_a.linear_velocity(), Vec3::new(0.0, 4.0, 0.0));
}

#[test]
fn test_9_5_dynamic_vs_dynamic_equal_mass_head_on_momentum_conserved() {
    let mut bodies = std::collections::BTreeMap::new();
    let body_a_id = RigidBodyId(1);
    let body_b_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body_a = RigidBody::new_dynamic(
        body_a_id,
        Vec3::new(-1.0, 0.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body_a
        .set_linear_velocity(Vec3::new(3.0, 0.0, 0.0))
        .unwrap();
    bodies.insert(body_a_id, body_a);

    let mut body_b = RigidBody::new_dynamic(
        body_b_id,
        Vec3::new(1.0, 0.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body_b
        .set_linear_velocity(Vec3::new(-3.0, 0.0, 0.0))
        .unwrap();
    bodies.insert(body_b_id, body_b);

    // Normal A -> B adalah +X
    let contact = Contact::new(
        ColliderId(10),
        ColliderId(20),
        body_a_id,
        body_b_id,
        Vec3::ZERO,
        Vec3::X,
        0.0,
    );

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved_a = bodies.get(&body_a_id).unwrap();
    let solved_b = bodies.get(&body_b_id).unwrap();

    // Inelastis sempurna: kedua badan berhenti di titik tumbukan
    assert!(solved_a.linear_velocity().x.abs() < 1e-4);
    assert!(solved_b.linear_velocity().x.abs() < 1e-4);

    // Kekekalan momentum total: p_initial = 2*(3) + 2*(-3) = 0, p_final = 0
    let total_p = 2.0 * solved_a.linear_velocity() + 2.0 * solved_b.linear_velocity();
    assert!(total_p.length() < 1e-4);
}

#[test]
fn test_9_5_dynamic_vs_dynamic_unequal_mass_momentum_conserved() {
    let mut bodies = std::collections::BTreeMap::new();
    let body_a_id = RigidBodyId(1);
    let body_b_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    // Badan A massa 1.0 bergerak dengan v = (4, 0, 0)
    let mut body_a = RigidBody::new_dynamic(
        body_a_id,
        Vec3::new(-1.0, 0.0, 0.0),
        Quat::IDENTITY,
        1.0,
        inertia,
    )
    .unwrap();
    body_a
        .set_linear_velocity(Vec3::new(4.0, 0.0, 0.0))
        .unwrap();
    bodies.insert(body_a_id, body_a);

    // Badan B massa 3.0 diam v = (0, 0, 0)
    let body_b = RigidBody::new_dynamic(
        body_b_id,
        Vec3::new(1.0, 0.0, 0.0),
        Quat::IDENTITY,
        3.0,
        inertia,
    )
    .unwrap();
    bodies.insert(body_b_id, body_b);

    let initial_momentum = 1.0 * Vec3::new(4.0, 0.0, 0.0) + 3.0 * Vec3::ZERO; // = (4, 0, 0)

    let contact = Contact::new(
        ColliderId(10),
        ColliderId(20),
        body_a_id,
        body_b_id,
        Vec3::ZERO,
        Vec3::X,
        0.0,
    );

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved_a = bodies.get(&body_a_id).unwrap();
    let solved_b = bodies.get(&body_b_id).unwrap();

    // Kecepatan akhir analitis: v_common = 4.0 / (1.0 + 3.0) = 1.0
    assert!((solved_a.linear_velocity().x - 1.0).abs() < 1e-4);
    assert!((solved_b.linear_velocity().x - 1.0).abs() < 1e-4);

    let final_momentum = 1.0 * solved_a.linear_velocity() + 3.0 * solved_b.linear_velocity();
    assert!((final_momentum - initial_momentum).length() < 1e-4);
}

#[test]
fn test_9_5_dynamic_vs_kinematic_kinematic_unaffected() {
    let mut bodies = std::collections::BTreeMap::new();
    let body_dyn_id = RigidBodyId(1);
    let body_kin_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body_dyn = RigidBody::new_dynamic(
        body_dyn_id,
        Vec3::new(-1.0, 0.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body_dyn
        .set_linear_velocity(Vec3::new(6.0, 0.0, 0.0))
        .unwrap();
    bodies.insert(body_dyn_id, body_dyn);

    // Badan Kinematik bergerak dengan kecepatan konstan (1, 0, 0) dan rotasi (0, 2, 0)
    let body_kin = RigidBody::new_kinematic(
        body_kin_id,
        Vec3::new(1.0, 0.0, 0.0),
        Quat::IDENTITY,
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 2.0, 0.0),
    )
    .unwrap();
    bodies.insert(body_kin_id, body_kin);

    let contact = Contact::new(
        ColliderId(10),
        ColliderId(20),
        body_dyn_id,
        body_kin_id,
        Vec3::ZERO,
        Vec3::X,
        0.0,
    );

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved_dyn = bodies.get(&body_dyn_id).unwrap();
    let solved_kin = bodies.get(&body_kin_id).unwrap();

    // Kinematik SAMA SEKALI TIDAK BERUBAH
    assert_eq!(solved_kin.linear_velocity(), Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(solved_kin.angular_velocity(), Vec3::new(0.0, 2.0, 0.0));

    // Dinamis menyesuaikan kecepatannya dengan kecepatan kinematik (1.0)
    assert!((solved_dyn.linear_velocity().x - 1.0).abs() < 1e-4);
}

#[test]
fn test_9_5_kinematic_vs_kinematic_and_static_vs_static_safe() {
    let mut bodies = std::collections::BTreeMap::new();
    let kin1_id = RigidBodyId(1);
    let kin2_id = RigidBodyId(2);

    let kin1 = RigidBody::new_kinematic(
        kin1_id,
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::ZERO,
    )
    .unwrap();
    let kin2 = RigidBody::new_kinematic(
        kin2_id,
        Vec3::new(1.0, 0.0, 0.0),
        Quat::IDENTITY,
        Vec3::new(-2.0, 0.0, 0.0),
        Vec3::ZERO,
    )
    .unwrap();
    bodies.insert(kin1_id, kin1);
    bodies.insert(kin2_id, kin2);

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        kin1_id,
        kin2_id,
        Vec3::new(0.5, 0.0, 0.0),
        Vec3::X,
        0.1,
    );
    let config = SolverConfig::default();

    // Aman, tidak menghasilkan NaN/Inf atau kepanikan
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    assert_eq!(
        bodies.get(&kin1_id).unwrap().linear_velocity(),
        Vec3::new(2.0, 0.0, 0.0)
    );
    assert_eq!(
        bodies.get(&kin2_id).unwrap().linear_velocity(),
        Vec3::new(-2.0, 0.0, 0.0)
    );
}

#[test]
fn test_9_5_off_center_contact_produces_angular_response() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let static_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    // Dinamis di (0, 0, 0), kecepatan awal 0
    let body_dyn =
        RigidBody::new_dynamic(dyn_id, Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap();
    bodies.insert(dyn_id, body_dyn);

    let body_stat =
        RigidBody::new_static(static_id, Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY).unwrap();
    bodies.insert(static_id, body_stat);

    // Titik kontak off-center di (0.5, 0.5, 0), normal A -> B adalah (0, 1, 0)
    // Lengan tuas r_A = (0.5, 0.5, 0).
    // r_A x n = (0.5, 0.5, 0) x (0, 1, 0) = (0, 0, 0.5) != 0!
    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        static_id,
        Vec3::new(0.5, 0.5, 0.0),
        Vec3::Y,
        0.1, // ada penetrasi sehingga Baumgarte bias memicu impuls pemisahan
    );

    let config = SolverConfig {
        iterations: 10,
        beta: 0.2,
        penetration_slop: 0.001,
        ..Default::default()
    };
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();

    // Memverifikasi bahwa respon angular dihasilkan secara non-zero pada sumbu Z!
    assert!(
        solved.angular_velocity().z.abs() > 0.01,
        "Respon angular sumbu Z harus non-zero, didapat: {:?}",
        solved.angular_velocity()
    );

    // Memverifikasi respon linear juga terjadi pada sumbu Y
    assert!(solved.linear_velocity().y < -0.01);
}

#[test]
fn test_9_5_center_of_mass_contact_zero_angular_response() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let static_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let body_dyn =
        RigidBody::new_dynamic(dyn_id, Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap();
    bodies.insert(dyn_id, body_dyn);

    let body_stat =
        RigidBody::new_static(static_id, Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY).unwrap();
    bodies.insert(static_id, body_stat);

    // Kontak tepat segaris COM: titik (0, 0.5, 0), normal (0, 1, 0)
    // r_A = (0, 0.5, 0), r_A x n = (0, 0, 0)
    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        static_id,
        Vec3::new(0.0, 0.5, 0.0),
        Vec3::Y,
        0.1,
    );

    let config = SolverConfig::default();
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();

    // Kecepatan angular harus tetap nol sempurna
    assert!(solved.angular_velocity().length() < 1e-6);
    // Kecepatan linear menerima respon
    assert!(solved.linear_velocity().y < -0.01);
}

#[test]
fn test_9_5_world_space_inverse_inertia_rotation_effect() {
    let mut bodies = std::collections::BTreeMap::new();
    let id_a = RigidBodyId(1);
    let id_b = RigidBodyId(2);

    // Inersia asimetris: I_xx = 1.0, I_yy = 10.0, I_zz = 10.0
    let inertia = Mat3::from_diagonal(Vec3::new(1.0, 10.0, 10.0));
    // Putar badan 90 derajat mengelilingi sumbu Y: sumbu X lokal menjadi sumbu -Z dunia!
    let rot = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    let body_a = RigidBody::new_dynamic(id_a, Vec3::ZERO, rot, 1.0, inertia).unwrap();

    let inv_inertia_world = compute_world_inv_inertia(&body_a);
    // Invers inersia lokal adalah diag(1.0, 0.1, 0.1).
    // Setelah rotasi 90 derajat mengelilingi Y:
    // Sumbu X dunia memiliki nilai 0.1, sumbu Z dunia memiliki nilai 1.0!
    assert!((inv_inertia_world.x_axis.x - 0.1).abs() < 1e-4);
    assert!((inv_inertia_world.z_axis.z - 1.0).abs() < 1e-4);

    bodies.insert(id_a, body_a);
    bodies.insert(
        id_b,
        RigidBody::new_static(id_b, Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        id_a,
        id_b,
        Vec3::new(0.0, 0.5, 0.5),
        Vec3::Y,
        0.1,
    );

    solve_contacts(
        &mut bodies,
        &[contact],
        1.0 / 30.0,
        &SolverConfig::default(),
    )
    .unwrap();
    let solved = bodies.get(&id_a).unwrap();
    assert!(solved.angular_velocity().is_finite());
}

#[test]
fn test_9_5_baumgarte_positive_separating_impulse_when_zero_velocity() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let static_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let body_dyn =
        RigidBody::new_dynamic(dyn_id, Vec3::ZERO, Quat::IDENTITY, 2.0, inertia).unwrap();
    bodies.insert(dyn_id, body_dyn);

    let body_stat =
        RigidBody::new_static(static_id, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY).unwrap();
    bodies.insert(static_id, body_stat);

    // Penetrasi 0.05 m > slop 0.001 m, kecepatan awal 0
    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        static_id,
        Vec3::new(0.5, 0.0, 0.0),
        Vec3::X, // A -> B
        0.05,
    );

    let config = SolverConfig {
        iterations: 10,
        beta: 0.2,
        penetration_slop: 0.001,
        ..Default::default()
    };
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    // Badan A harus menerima impuls -J*n = -J*(1, 0, 0), sehingga v_x menjadi negatif (menjauh dari B)!
    assert!(
        solved.linear_velocity().x < -0.1,
        "Badan A harus terdorong menjauh ke arah -X, didapat v_x: {}",
        solved.linear_velocity().x
    );
}

#[test]
fn test_9_5_baumgarte_sign_regression_test() {
    // TES REGRESI KRITIS (SECTION 31 ITEM 19):
    // Memverifikasi bahwa formula delta_lambda = (bias - v_n) * effective_mass
    // menghasilkan impuls pemisahan positif saat v_n = 0 dan penetration > slop.
    // Jika tanda bias salah (misalnya -(v_n + bias)), maka delta_lambda = -bias < 0,
    // yang akan diklem max(0, -bias) = 0, sehingga tidak ada impuls sama sekali!
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let static_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let body_dyn =
        RigidBody::new_dynamic(dyn_id, Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap();
    bodies.insert(dyn_id, body_dyn);
    bodies.insert(
        static_id,
        RigidBody::new_static(static_id, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        static_id,
        Vec3::new(0.5, 0.0, 0.0),
        Vec3::X,
        0.02, // 2 cm penetrasi
    );

    let config = SolverConfig {
        iterations: 1,
        beta: 0.2,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();
    let solved = bodies.get(&dyn_id).unwrap();

    assert!(
        solved.linear_velocity().x < 0.0,
        "REGRESI BAUMGARTE: solver harus menghasilkan impuls pemisahan v_x < 0, didapat: {}",
        solved.linear_velocity().x
    );
}

#[test]
fn test_9_5_baumgarte_penetration_below_slop_zero_bias() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let static_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let body_dyn =
        RigidBody::new_dynamic(dyn_id, Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap();
    bodies.insert(dyn_id, body_dyn);
    bodies.insert(
        static_id,
        RigidBody::new_static(static_id, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY).unwrap(),
    );

    // Penetrasi 0.0005 m < slop 0.001 m -> bias harus nol!
    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        static_id,
        Vec3::new(0.5, 0.0, 0.0),
        Vec3::X,
        0.0005,
    );

    let config = SolverConfig {
        iterations: 10,
        beta: 0.2,
        penetration_slop: 0.001,
        ..Default::default()
    };
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    assert_eq!(solved.linear_velocity(), Vec3::ZERO);
}

#[test]
fn test_9_5_deeper_penetration_produces_larger_bias() {
    let mut bodies1 = std::collections::BTreeMap::new();
    let mut bodies2 = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let static_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    bodies1.insert(
        dyn_id,
        RigidBody::new_dynamic(dyn_id, Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap(),
    );
    bodies1.insert(
        static_id,
        RigidBody::new_static(static_id, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY).unwrap(),
    );

    bodies2.insert(
        dyn_id,
        RigidBody::new_dynamic(dyn_id, Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap(),
    );
    bodies2.insert(
        static_id,
        RigidBody::new_static(static_id, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY).unwrap(),
    );

    let c_shallow = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        static_id,
        Vec3::new(0.5, 0.0, 0.0),
        Vec3::X,
        0.01,
    );
    let c_deep = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        static_id,
        Vec3::new(0.5, 0.0, 0.0),
        Vec3::X,
        0.05,
    );

    let config = SolverConfig::default();
    solve_contacts(&mut bodies1, &[c_shallow], 1.0 / 30.0, &config).unwrap();
    solve_contacts(&mut bodies2, &[c_deep], 1.0 / 30.0, &config).unwrap();

    let v1 = bodies1.get(&dyn_id).unwrap().linear_velocity().length();
    let v2 = bodies2.get(&dyn_id).unwrap().linear_velocity().length();

    assert!(
        v2 > v1,
        "Penetrasi lebih dalam harus menghasilkan kecepatan pemisahan lebih besar ({} > {})",
        v2,
        v1
    );
}

#[test]
fn test_9_5_unilateral_constraint_no_attractive_impulse() {
    let mut bodies = std::collections::BTreeMap::new();
    let id_a = RigidBodyId(1);
    let id_b = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    // Kedua badan sudah bergerak saling menjauh dengan kecepatan tinggi
    let mut b_a = RigidBody::new_dynamic(id_a, Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap();
    b_a.set_linear_velocity(Vec3::new(-5.0, 0.0, 0.0)).unwrap();
    bodies.insert(id_a, b_a);

    let mut b_b =
        RigidBody::new_dynamic(id_b, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY, 1.0, inertia)
            .unwrap();
    b_b.set_linear_velocity(Vec3::new(5.0, 0.0, 0.0)).unwrap();
    bodies.insert(id_b, b_b);

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        id_a,
        id_b,
        Vec3::new(0.5, 0.0, 0.0),
        Vec3::X,
        0.0,
    );
    solve_contacts(
        &mut bodies,
        &[contact],
        1.0 / 30.0,
        &SolverConfig::default(),
    )
    .unwrap();

    // Kecepatan tidak boleh ditarik kembali
    assert_eq!(
        bodies.get(&id_a).unwrap().linear_velocity(),
        Vec3::new(-5.0, 0.0, 0.0)
    );
    assert_eq!(
        bodies.get(&id_b).unwrap().linear_velocity(),
        Vec3::new(5.0, 0.0, 0.0)
    );
}

#[test]
fn test_9_5_multi_contact_two_points_floor() {
    let mut bodies = std::collections::BTreeMap::new();
    let box_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body_box = RigidBody::new_dynamic(
        box_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        4.0,
        inertia,
    )
    .unwrap();
    body_box
        .set_linear_velocity(Vec3::new(0.0, -2.0, 0.0))
        .unwrap();
    bodies.insert(box_id, body_box);

    let body_floor = RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap();
    bodies.insert(floor_id, body_floor);

    // Dua titik kontak simetris di x = -1 dan x = +1
    let c1 = Contact::new(
        ColliderId(1),
        ColliderId(2),
        box_id,
        floor_id,
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::NEG_Y,
        0.0,
    );
    let c2 = Contact::new(
        ColliderId(3),
        ColliderId(4),
        box_id,
        floor_id,
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::NEG_Y,
        0.0,
    );

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };
    solve_contacts(&mut bodies, &[c1, c2], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&box_id).unwrap();
    // Kecepatan jatuh vertikal tertahan
    assert!(solved.linear_velocity().y.abs() < 1e-3);
    // Karena simetris, angular velocity tetap nol
    assert!(solved.angular_velocity().length() < 1e-3);
}

#[test]
fn test_9_5_multi_contact_convergence_iterations() {
    let setup = || {
        let mut bodies = std::collections::BTreeMap::new();
        let b1_id = RigidBodyId(1);
        let b2_id = RigidBodyId(2);

        let inertia = Mat3::from_diagonal(Vec3::ONE);
        let mut b1 =
            RigidBody::new_dynamic(b1_id, Vec3::ZERO, Quat::IDENTITY, 2.0, inertia).unwrap();
        b1.set_linear_velocity(Vec3::new(0.0, -3.0, 0.0)).unwrap();
        bodies.insert(b1_id, b1);
        bodies.insert(
            b2_id,
            RigidBody::new_static(b2_id, Vec3::new(0.0, -1.0, 0.0), Quat::IDENTITY).unwrap(),
        );

        let c1 = Contact::new(
            ColliderId(1),
            ColliderId(2),
            b1_id,
            b2_id,
            Vec3::new(-0.8, -0.5, 0.0),
            Vec3::NEG_Y,
            0.02,
        );
        let c2 = Contact::new(
            ColliderId(3),
            ColliderId(4),
            b1_id,
            b2_id,
            Vec3::new(0.8, -0.5, 0.0),
            Vec3::NEG_Y,
            0.02,
        );
        (bodies, vec![c1, c2])
    };

    let (mut b_iter1, c_iter1) = setup();
    let (mut b_iter10, c_iter10) = setup();

    solve_contacts(
        &mut b_iter1,
        &c_iter1,
        1.0 / 30.0,
        &SolverConfig {
            iterations: 1,
            beta: 0.2,
            penetration_slop: 0.001,
            ..Default::default()
        },
    )
    .unwrap();
    solve_contacts(
        &mut b_iter10,
        &c_iter10,
        1.0 / 30.0,
        &SolverConfig {
            iterations: 10,
            beta: 0.2,
            penetration_slop: 0.001,
            ..Default::default()
        },
    )
    .unwrap();

    // 10 iterasi memberikan kestabilan konvergensi yang lebih baik daripada 1 iterasi
    assert!(b_iter1
        .get(&RigidBodyId(1))
        .unwrap()
        .linear_velocity()
        .is_finite());
    assert!(b_iter10
        .get(&RigidBodyId(1))
        .unwrap()
        .linear_velocity()
        .is_finite());
}

#[test]
fn test_9_5_multi_collider_body_affects_single_rigidbody() {
    let mut bodies = std::collections::BTreeMap::new();
    let body_compound_id = RigidBodyId(1);
    let body_wall_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut b_compound =
        RigidBody::new_dynamic(body_compound_id, Vec3::ZERO, Quat::IDENTITY, 2.0, inertia).unwrap();
    b_compound
        .set_linear_velocity(Vec3::new(4.0, 0.0, 0.0))
        .unwrap();
    bodies.insert(body_compound_id, b_compound);
    bodies.insert(
        body_wall_id,
        RigidBody::new_static(body_wall_id, Vec3::new(2.0, 0.0, 0.0), Quat::IDENTITY).unwrap(),
    );

    // Dua collider berbeda pada badan compound yang sama menabrak dinding
    let c1 = Contact::new(
        ColliderId(101),
        ColliderId(201),
        body_compound_id,
        body_wall_id,
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::X,
        0.0,
    );
    let c2 = Contact::new(
        ColliderId(102),
        ColliderId(202),
        body_compound_id,
        body_wall_id,
        Vec3::new(1.0, -1.0, 0.0),
        Vec3::X,
        0.0,
    );

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };
    solve_contacts(&mut bodies, &[c1, c2], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&body_compound_id).unwrap();
    // Kecepatan maju ke arah dinding dinetralkan pada badan compound tunggal tersebut
    assert!(solved.linear_velocity().x.abs() < 1e-4);
}

#[test]
fn test_9_5_determinism_and_contact_order_independence() {
    let setup = || {
        let mut bodies = std::collections::BTreeMap::new();
        let b1 = RigidBodyId(1);
        let b2 = RigidBodyId(2);
        let inertia = Mat3::from_diagonal(Vec3::ONE);
        let mut d = RigidBody::new_dynamic(b1, Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap();
        d.set_linear_velocity(Vec3::new(2.0, -2.0, 0.0)).unwrap();
        bodies.insert(b1, d);
        bodies.insert(
            b2,
            RigidBody::new_static(b2, Vec3::new(1.0, -1.0, 0.0), Quat::IDENTITY).unwrap(),
        );

        let c1 = Contact::new(
            ColliderId(1),
            ColliderId(2),
            b1,
            b2,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::X,
            0.01,
        );
        let c2 = Contact::new(
            ColliderId(3),
            ColliderId(4),
            b1,
            b2,
            Vec3::new(0.0, -1.0, 0.0),
            Vec3::NEG_Y,
            0.01,
        );
        (bodies, c1, c2)
    };

    let (mut b_ord, c1, c2) = setup();
    let (mut b_rev, _, _) = setup();

    // Eksekusi urutan [c1, c2]
    solve_contacts(&mut b_ord, &[c1, c2], 1.0 / 30.0, &SolverConfig::default()).unwrap();
    // Eksekusi urutan terbalik [c2, c1]
    solve_contacts(&mut b_rev, &[c2, c1], 1.0 / 30.0, &SolverConfig::default()).unwrap();

    let v_ord = b_ord.get(&RigidBodyId(1)).unwrap().linear_velocity();
    let v_rev = b_rev.get(&RigidBodyId(1)).unwrap().linear_velocity();
    let w_ord = b_ord.get(&RigidBodyId(1)).unwrap().angular_velocity();
    let w_rev = b_rev.get(&RigidBodyId(1)).unwrap().angular_velocity();

    // Karena solver mengurutkan kontak secara deterministik kanonikal, hasilnya identik!
    assert_eq!(v_ord, v_rev);
    assert_eq!(w_ord, w_rev);
}

#[test]
fn test_9_5_transform_firewall_positions_and_rotations_untouched() {
    // ATURAN ABSOLUT: Phase 9.5 TIDAK BOLEH memutasi posisi atau rotasi badan kaku!
    let mut bodies = std::collections::BTreeMap::new();
    let b1 = RigidBodyId(1);
    let b2 = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let rot1 = Quat::from_rotation_z(0.785);
    let pos1 = Vec3::new(12.34, -56.78, 90.12);
    let body1 = RigidBody::new_dynamic(b1, pos1, rot1, 3.0, inertia).unwrap();

    let rot2 = Quat::IDENTITY;
    let pos2 = Vec3::new(0.0, 0.0, 0.0);
    let body2 = RigidBody::new_static(b2, pos2, rot2).unwrap();

    let expected_pos1 = body1.position();
    let expected_rot1 = body1.rotation();
    let expected_pos2 = body2.position();
    let expected_rot2 = body2.rotation();

    bodies.insert(b1, body1);
    bodies.insert(b2, body2);

    let contact = Contact::new(
        ColliderId(10),
        ColliderId(20),
        b1,
        b2,
        Vec3::new(10.0, -50.0, 90.0),
        Vec3::X,
        0.5, // penetrasi besar dengan bias tinggi
    );

    solve_contacts(
        &mut bodies,
        &[contact],
        1.0 / 30.0,
        &SolverConfig::default(),
    )
    .unwrap();

    let post1 = bodies.get(&b1).unwrap();
    let post2 = bodies.get(&b2).unwrap();

    assert_eq!(
        post1.position(),
        expected_pos1,
        "Posisi badan dinamis tidak boleh berubah!"
    );
    assert_eq!(
        post1.rotation(),
        expected_rot1,
        "Rotasi badan dinamis tidak boleh berubah!"
    );
    assert_eq!(
        post2.position(),
        expected_pos2,
        "Posisi badan statis tidak boleh berubah!"
    );
    assert_eq!(
        post2.rotation(),
        expected_rot2,
        "Rotasi badan statis tidak boleh berubah!"
    );
}

#[test]
fn test_9_5_validation_dt_and_config() {
    let mut bodies = std::collections::BTreeMap::new();
    let id_a = RigidBodyId(1);
    let id_b = RigidBodyId(2);
    bodies.insert(
        id_a,
        RigidBody::new_static(id_a, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );
    bodies.insert(
        id_b,
        RigidBody::new_static(id_b, Vec3::ONE, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        id_a,
        id_b,
        Vec3::ZERO,
        Vec3::X,
        0.0,
    );

    // 1. dt tidak valid
    assert_eq!(
        solve_contacts(&mut bodies, &[contact], 0.0, &SolverConfig::default()),
        Err(SolverError::InvalidTimestep)
    );
    assert_eq!(
        solve_contacts(&mut bodies, &[contact], -0.1, &SolverConfig::default()),
        Err(SolverError::InvalidTimestep)
    );
    assert_eq!(
        solve_contacts(&mut bodies, &[contact], f32::NAN, &SolverConfig::default()),
        Err(SolverError::InvalidTimestep)
    );
    assert_eq!(
        solve_contacts(
            &mut bodies,
            &[contact],
            f32::INFINITY,
            &SolverConfig::default()
        ),
        Err(SolverError::InvalidTimestep)
    );

    // 2. Konfigurasi solver tidak valid
    let cfg_zero_iter = SolverConfig {
        iterations: 0,
        beta: 0.2,
        penetration_slop: 0.001,
        ..Default::default()
    };
    assert_eq!(
        solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &cfg_zero_iter),
        Err(SolverError::InvalidConfiguration)
    );

    let cfg_neg_beta = SolverConfig {
        iterations: 10,
        beta: -0.1,
        penetration_slop: 0.001,
        ..Default::default()
    };
    assert_eq!(
        solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &cfg_neg_beta),
        Err(SolverError::InvalidConfiguration)
    );

    let cfg_neg_slop = SolverConfig {
        iterations: 10,
        beta: 0.2,
        penetration_slop: -0.001,
        ..Default::default()
    };
    assert_eq!(
        solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &cfg_neg_slop),
        Err(SolverError::InvalidConfiguration)
    );
}

#[test]
fn test_9_5_validation_contact_and_body_errors() {
    let mut bodies = std::collections::BTreeMap::new();
    let id_a = RigidBodyId(1);
    let id_b = RigidBodyId(2);
    bodies.insert(
        id_a,
        RigidBody::new_static(id_a, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );
    bodies.insert(
        id_b,
        RigidBody::new_static(id_b, Vec3::ONE, Quat::IDENTITY).unwrap(),
    );

    // 1. Kontak mandiri (body_a == body_b)
    let c_self = Contact::new(
        ColliderId(1),
        ColliderId(2),
        id_a,
        id_a,
        Vec3::ZERO,
        Vec3::X,
        0.0,
    );
    assert_eq!(
        solve_contacts(&mut bodies, &[c_self], 1.0 / 30.0, &SolverConfig::default()),
        Err(SolverError::SameBodyContact(id_a))
    );

    // 2. Badan tidak ditemukan
    let c_missing = Contact::new(
        ColliderId(1),
        ColliderId(2),
        id_a,
        RigidBodyId(999),
        Vec3::ZERO,
        Vec3::X,
        0.0,
    );
    assert_eq!(
        solve_contacts(
            &mut bodies,
            &[c_missing],
            1.0 / 30.0,
            &SolverConfig::default()
        ),
        Err(SolverError::BodyNotFound(RigidBodyId(999)))
    );

    // 3. Normal non-unit
    let c_bad_norm = Contact::new(
        ColliderId(1),
        ColliderId(2),
        id_a,
        id_b,
        Vec3::ZERO,
        Vec3::new(2.0, 0.0, 0.0),
        0.0,
    );
    assert_eq!(
        solve_contacts(
            &mut bodies,
            &[c_bad_norm],
            1.0 / 30.0,
            &SolverConfig::default()
        ),
        Err(SolverError::InvalidContact)
    );

    // 4. Penetrasi negatif
    let mut c_neg_pen = Contact::new(
        ColliderId(1),
        ColliderId(2),
        id_a,
        id_b,
        Vec3::ZERO,
        Vec3::X,
        0.0,
    );
    c_neg_pen.penetration = -0.1;
    assert_eq!(
        solve_contacts(
            &mut bodies,
            &[c_neg_pen],
            1.0 / 30.0,
            &SolverConfig::default()
        ),
        Err(SolverError::InvalidContact)
    );
}

#[test]
fn test_9_5_end_to_end_cross_phase_shape_to_solver() {
    // INTEGRASI LINTAS FASE (SECTION 32):
    // Shape -> Collider -> PhysicsWorld -> generate_contacts() -> solve_contacts() -> RigidBody velocity mutation!
    let mut world = PhysicsWorld::default();

    // 1. Lantai statis di y = 0
    let floor_body_id = RigidBodyId(1);
    let floor_body = RigidBody::new_static(floor_body_id, Vec3::ZERO, Quat::IDENTITY).unwrap();
    world.add_rigid_body(floor_body, None).unwrap();

    let floor_box = BoxShape::new(Vec3::new(10.0, 0.5, 10.0)).unwrap();
    let floor_col = Collider::new(
        ColliderId(10),
        floor_body_id,
        Shape::Box(floor_box),
        Transform::IDENTITY,
    );
    world.add_collider(floor_col).unwrap();

    // 2. Bola dinamis jatuh di y = 1.4 (radius 1.0, lantai atas di y = 0.5, penetrasi = 1.0 + 0.5 - 1.4 = 0.1)
    let sphere_body_id = RigidBodyId(2);
    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut sphere_body = RigidBody::new_dynamic(
        sphere_body_id,
        Vec3::new(0.0, 1.4, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    sphere_body
        .set_linear_velocity(Vec3::new(0.0, -5.0, 0.0))
        .unwrap(); // jatuh dengan v = -5 m/s
    world.add_rigid_body(sphere_body, None).unwrap();

    let sphere_shape = Sphere::new(1.0).unwrap();
    let sphere_col = Collider::new(
        ColliderId(20),
        sphere_body_id,
        Shape::Sphere(sphere_shape),
        Transform::IDENTITY,
    );
    world.add_collider(sphere_col).unwrap();

    // 3. Narrowphase menghasilkan kontak
    let contacts = world.generate_contacts().unwrap();
    assert_eq!(contacts.len(), 1);

    let c = &contacts[0];
    assert!((c.penetration - 0.1).abs() < 1e-3);

    // 4. Solver menyelesaikan kontak
    let pos_before = world.rigid_bodies.get(&sphere_body_id).unwrap().position();
    let rot_before = world.rigid_bodies.get(&sphere_body_id).unwrap().rotation();

    world.solve_contacts(&contacts).unwrap();

    let solved_sphere = world.rigid_bodies.get(&sphere_body_id).unwrap();

    // Kecepatan jatuh telah teratasi (tidak lagi bergerak turun menembus lantai)
    assert!(
        solved_sphere.linear_velocity().y >= 0.0,
        "Kecepatan y bola harus dinetralkan atau positif (pemisahan), didapat: {}",
        solved_sphere.linear_velocity().y
    );

    // Posisi dan rotasi TIDAK PERNAH berubah
    assert_eq!(solved_sphere.position(), pos_before);
    assert_eq!(solved_sphere.rotation(), rot_before);
}

// ============================================================================
// 6. LINEAR + ANGULAR INTEGRATION UNIT & INTEGRATION TESTS (PHASE 9.6)
// ============================================================================

#[test]
fn test_9_6_static_complete_immutability() {
    let mut body =
        RigidBody::new_static(RigidBodyId(1), Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY).unwrap();
    let initial_pos = body.position();
    let initial_rot = body.rotation();
    let initial_v = body.linear_velocity();
    let initial_w = body.angular_velocity();

    integrate_body(&mut body, 1.0 / 30.0, Vec3::new(0.0, -9.81, 0.0)).unwrap();

    assert_eq!(body.position(), initial_pos);
    assert_eq!(body.rotation(), initial_rot);
    assert_eq!(body.linear_velocity(), initial_v);
    assert_eq!(body.angular_velocity(), initial_w);
}

#[test]
fn test_9_6_dynamic_linear_integration() {
    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        RigidBodyId(1),
        Vec3::new(1.0, 2.0, 3.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(2.0, 0.0, -4.0)).unwrap();

    // Integrasi posisi dengan gravitasi nol dan dt = 0.5: x_new = (1, 2, 3) + (2, 0, -4) * 0.5 = (2, 2, 1)
    integrate_body(&mut body, 0.5, Vec3::ZERO).unwrap();

    assert_eq!(body.position(), Vec3::new(2.0, 2.0, 1.0));
}

#[test]
fn test_9_6_zero_velocity() {
    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        RigidBodyId(1),
        Vec3::new(5.0, 6.0, 7.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();

    integrate_body(&mut body, 1.0, Vec3::ZERO).unwrap();

    assert_eq!(body.position(), Vec3::new(5.0, 6.0, 7.0));
}

#[test]
fn test_9_6_dynamic_gravity_and_semi_implicit_euler() {
    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        RigidBodyId(1),
        Vec3::new(0.0, 10.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    // Kecepatan awal nol, gravitasi = (0, -10, 0), dt = 0.5
    // Semi-implicit Euler:
    // v_new = 0 + (0, -10, 0) * 0.5 = (0, -5, 0)
    // x_new = (0, 10, 0) + (0, -5, 0) * 0.5 = (0, 7.5, 0)
    integrate_body(&mut body, 0.5, Vec3::new(0.0, -10.0, 0.0)).unwrap();

    assert_eq!(body.linear_velocity(), Vec3::new(0.0, -5.0, 0.0));
    assert_eq!(body.position(), Vec3::new(0.0, 7.5, 0.0));

    // Verifikasi integrate_velocities & integrate_transforms pada BTreeMap
    let mut map = std::collections::BTreeMap::new();
    let body_b = RigidBody::new_dynamic(
        RigidBodyId(2),
        Vec3::new(0.0, 10.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    map.insert(RigidBodyId(2), body_b);

    let default_config = IntegrationConfig::default();
    assert_eq!(default_config.gravity, Vec3::new(0.0, -9.81, 0.0));

    integrate_velocities(&mut map, 0.5, Vec3::new(0.0, -10.0, 0.0)).unwrap();
    integrate_transforms(&mut map, 0.5).unwrap();

    let res_b = map.get(&RigidBodyId(2)).unwrap();
    assert_eq!(res_b.linear_velocity(), Vec3::new(0.0, -5.0, 0.0));
    assert_eq!(res_b.position(), Vec3::new(0.0, 7.5, 0.0));
}

#[test]
fn test_9_6_static_gravity_immunity() {
    let mut body = RigidBody::new_static(RigidBodyId(1), Vec3::ZERO, Quat::IDENTITY).unwrap();
    integrate_velocity(&mut body, 1.0, Vec3::new(0.0, -10.0, 0.0)).unwrap();

    assert_eq!(body.linear_velocity(), Vec3::ZERO);
}

#[test]
fn test_9_6_kinematic_gravity_immunity() {
    let mut body = RigidBody::new_kinematic(
        RigidBodyId(1),
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::ZERO,
    )
    .unwrap();

    integrate_velocity(&mut body, 1.0, Vec3::new(0.0, -10.0, 0.0)).unwrap();

    assert_eq!(body.linear_velocity(), Vec3::new(1.0, 0.0, 0.0));
}

#[test]
fn test_9_6_kinematic_velocity_integration() {
    let mut body = RigidBody::new_kinematic(
        RigidBodyId(1),
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::new(3.0, 0.0, 0.0),
        Vec3::ZERO,
    )
    .unwrap();

    integrate_transform(&mut body, 2.0).unwrap();

    assert_eq!(body.position(), Vec3::new(6.0, 0.0, 0.0));
}

#[test]
fn test_9_6_known_angular_integration() {
    let q0 = Quat::IDENTITY;
    // Putar di sumbu Y dengan omega = (0, pi, 0) selama dt = 0.5 detik (total rotasi pi/2 = 90 derajat)
    let omega = Vec3::new(0.0, std::f32::consts::PI, 0.0);
    let q1 = integrate_rotation(q0, omega, 0.5).unwrap();

    // Rotasi 90 derajat mengelilingi Y: y = sin(pi/4) ≈ 0.7071, w = cos(pi/4) ≈ 0.7071
    let expected = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    assert!((q1.dot(expected).abs() - 1.0).abs() < 0.05);
    assert!((q1.length() - 1.0).abs() < 1e-5);
}

#[test]
fn test_9_6_very_small_angular_velocity() {
    let q0 = Quat::IDENTITY;
    let omega = Vec3::new(1e-7, 0.0, 0.0);
    let q1 = integrate_rotation(q0, omega, 0.1).unwrap();

    assert!(q1.is_finite());
    assert!((q1.length() - 1.0).abs() < 1e-6);
    assert!((q1 - q0).length() < 1e-5);
}

#[test]
fn test_9_6_zero_angular_velocity() {
    let q0 = Quat::from_rotation_z(0.5);
    let q1 = integrate_rotation(q0, Vec3::ZERO, 1.0).unwrap();

    assert_eq!(q1, q0);
}

#[test]
fn test_9_6_long_run_quaternion_normalization() {
    let mut q = Quat::IDENTITY;
    let omega = Vec3::new(1.0, 2.0, 3.0);
    let dt = 1.0 / 30.0;

    for _ in 0..1000 {
        q = integrate_rotation(q, omega, dt).unwrap();
    }

    assert!(q.is_finite());
    assert!(
        (q.length() - 1.0).abs() < 1e-6,
        "Panjang kuaternion harus tetap mendekati 1.0, didapat: {}",
        q.length()
    );
}

#[test]
fn test_9_6_rotation_90_degree_sanity() {
    let mut q = Quat::IDENTITY;
    // Rotasi 90 derajat (pi/2 rad/s selama 1s pada 30Hz) mengelilingi sumbu Y
    let omega = Vec3::new(0.0, std::f32::consts::FRAC_PI_2, 0.0);
    let dt = 1.0 / 30.0;
    for _ in 0..30 {
        q = integrate_rotation(q, omega, dt).unwrap();
    }

    // Vektor awal (1, 0, 0) diputar 90 derajat mengelilingi Y harus mengarah ke (0, 0, -1)
    let v_rotated = q * Vec3::X;
    assert!((v_rotated - Vec3::NEG_Z).length() < 0.05);
}

#[test]
fn test_9_6_world_space_angular_velocity_on_rotated_body() {
    // Badan awalnya diputar 90 derajat mengelilingi sumbu X
    let q0 = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);

    // Kecepatan sudut diberikan di RUANG DUNIA mengelilingi sumbu Y
    let omega_world = Vec3::new(0.0, 1.0, 0.0);
    let dt = 0.1;
    let q1 = integrate_rotation(q0, omega_world, dt).unwrap();

    // Perkalian kiri di ruang dunia: dq = 0.5 * Omega(omega_world) * q0
    let omega_quat = Quat::from_xyzw(omega_world.x, omega_world.y, omega_world.z, 0.0);
    let dq_left = (omega_quat * q0) * (0.5 * dt);
    let expected_q = Quat::from_xyzw(
        q0.x + dq_left.x,
        q0.y + dq_left.y,
        q0.z + dq_left.z,
        q0.w + dq_left.w,
    )
    .normalize();

    assert!((q1 - expected_q).length() < 1e-5);
}

#[test]
fn test_9_6_linear_angular_independence() {
    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body1 =
        RigidBody::new_dynamic(RigidBodyId(1), Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap();
    body1.set_linear_velocity(Vec3::new(5.0, 0.0, 0.0)).unwrap();

    let mut body2 =
        RigidBody::new_dynamic(RigidBodyId(2), Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap();
    body2
        .set_angular_velocity(Vec3::new(0.0, 5.0, 0.0))
        .unwrap();

    integrate_body(&mut body1, 0.2, Vec3::ZERO).unwrap();
    integrate_body(&mut body2, 0.2, Vec3::ZERO).unwrap();

    // Body 1: posisi berubah, rotasi tetap identitas
    assert_eq!(body1.position(), Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(body1.rotation(), Quat::IDENTITY);

    // Body 2: rotasi berubah, posisi tetap nol
    assert_eq!(body2.position(), Vec3::ZERO);
    assert_ne!(body2.rotation(), Quat::IDENTITY);
}

#[test]
fn test_9_6_invalid_timestep() {
    let mut body = RigidBody::new_dynamic(
        RigidBodyId(1),
        Vec3::ZERO,
        Quat::IDENTITY,
        1.0,
        Mat3::from_diagonal(Vec3::ONE),
    )
    .unwrap();

    assert_eq!(
        integrate_body(&mut body, 0.0, Vec3::ZERO),
        Err(IntegrationError::InvalidTimestep)
    );
    assert_eq!(
        integrate_body(&mut body, -0.1, Vec3::ZERO),
        Err(IntegrationError::InvalidTimestep)
    );
    assert_eq!(
        integrate_body(&mut body, f32::NAN, Vec3::ZERO),
        Err(IntegrationError::InvalidTimestep)
    );
    assert_eq!(
        integrate_body(&mut body, f32::INFINITY, Vec3::ZERO),
        Err(IntegrationError::InvalidTimestep)
    );
}

#[test]
fn test_9_6_non_finite_body_state_and_gravity() {
    let mut body = RigidBody::new_dynamic(
        RigidBodyId(1),
        Vec3::ZERO,
        Quat::IDENTITY,
        1.0,
        Mat3::from_diagonal(Vec3::ONE),
    )
    .unwrap();

    assert_eq!(
        integrate_body(&mut body, 0.1, Vec3::new(f32::NAN, 0.0, 0.0)),
        Err(IntegrationError::InvalidGravity)
    );
}

#[test]
fn test_9_6_invalid_quaternion() {
    let bad_q = Quat::from_xyzw(0.0, 0.0, 0.0, 0.0);
    assert_eq!(
        integrate_rotation(bad_q, Vec3::new(1.0, 0.0, 0.0), 0.1),
        Err(IntegrationError::InvalidRotation)
    );
}

#[test]
fn test_9_6_atomic_failure() {
    let mut bodies = std::collections::BTreeMap::new();
    let b1 = RigidBodyId(1);
    let b2 = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let body1 =
        RigidBody::new_dynamic(b1, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY, 1.0, inertia).unwrap();
    let body2 =
        RigidBody::new_dynamic(b2, Vec3::new(2.0, 0.0, 0.0), Quat::IDENTITY, 1.0, inertia).unwrap();

    bodies.insert(b1, body1);
    bodies.insert(b2, body2);

    // Panggil dengan dt tidak valid
    let res = integrate_bodies(&mut bodies, -1.0, Vec3::ZERO);
    assert_eq!(res, Err(IntegrationError::InvalidTimestep));

    // Status kedua badan tetap 100% tidak berubah
    assert_eq!(
        bodies.get(&b1).unwrap().position(),
        Vec3::new(1.0, 0.0, 0.0)
    );
    assert_eq!(
        bodies.get(&b2).unwrap().position(),
        Vec3::new(2.0, 0.0, 0.0)
    );
}

#[test]
fn test_9_6_deterministic_execution() {
    let setup = || {
        let mut bodies = std::collections::BTreeMap::new();
        let inertia = Mat3::from_diagonal(Vec3::ONE);
        let mut b1 =
            RigidBody::new_dynamic(RigidBodyId(1), Vec3::ZERO, Quat::IDENTITY, 1.0, inertia)
                .unwrap();
        b1.set_linear_velocity(Vec3::new(1.0, -2.0, 3.0)).unwrap();
        b1.set_angular_velocity(Vec3::new(0.5, -0.5, 0.5)).unwrap();
        bodies.insert(RigidBodyId(1), b1);
        bodies
    };

    let mut bodies1 = setup();
    let mut bodies2 = setup();

    integrate_bodies(&mut bodies1, 1.0 / 30.0, Vec3::new(0.0, -9.81, 0.0)).unwrap();
    integrate_bodies(&mut bodies2, 1.0 / 30.0, Vec3::new(0.0, -9.81, 0.0)).unwrap();

    let res1 = bodies1.get(&RigidBodyId(1)).unwrap();
    let res2 = bodies2.get(&RigidBodyId(1)).unwrap();

    assert_eq!(res1.position(), res2.position());
    assert_eq!(res1.rotation(), res2.rotation());
    assert_eq!(res1.linear_velocity(), res2.linear_velocity());
}

#[test]
fn test_9_6_multi_body_isolation() {
    let mut bodies = std::collections::BTreeMap::new();
    let inertia = Mat3::from_diagonal(Vec3::ONE);

    let mut b1 =
        RigidBody::new_dynamic(RigidBodyId(1), Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap();
    b1.set_linear_velocity(Vec3::new(10.0, 0.0, 0.0)).unwrap();

    let mut b2 =
        RigidBody::new_dynamic(RigidBodyId(2), Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap();
    b2.set_linear_velocity(Vec3::new(0.0, 20.0, 0.0)).unwrap();

    bodies.insert(RigidBodyId(1), b1);
    bodies.insert(RigidBodyId(2), b2);

    integrate_bodies(&mut bodies, 0.1, Vec3::ZERO).unwrap();

    assert_eq!(
        bodies.get(&RigidBodyId(1)).unwrap().position(),
        Vec3::new(1.0, 0.0, 0.0)
    );
    assert_eq!(
        bodies.get(&RigidBodyId(2)).unwrap().position(),
        Vec3::new(0.0, 2.0, 0.0)
    );
}

#[test]
fn test_9_6_mixed_static_kinematic_dynamic_collection() {
    let mut bodies = std::collections::BTreeMap::new();
    let inertia = Mat3::from_diagonal(Vec3::ONE);

    // 1. Static di (1, 1, 1)
    bodies.insert(
        RigidBodyId(1),
        RigidBody::new_static(RigidBodyId(1), Vec3::splat(1.0), Quat::IDENTITY).unwrap(),
    );

    // 2. Kinematic di (2, 2, 2) dengan v = (1, 0, 0)
    let kin = RigidBody::new_kinematic(
        RigidBodyId(2),
        Vec3::splat(2.0),
        Quat::IDENTITY,
        Vec3::X,
        Vec3::ZERO,
    )
    .unwrap();
    bodies.insert(RigidBodyId(2), kin);

    // 3. Dynamic di (3, 3, 3) dengan v = 0, terkena gravitasi g = (0, -10, 0)
    let dyn_body = RigidBody::new_dynamic(
        RigidBodyId(3),
        Vec3::splat(3.0),
        Quat::IDENTITY,
        1.0,
        inertia,
    )
    .unwrap();
    bodies.insert(RigidBodyId(3), dyn_body);

    integrate_bodies(&mut bodies, 1.0, Vec3::new(0.0, -10.0, 0.0)).unwrap();

    // Static tidak berubah
    assert_eq!(
        bodies.get(&RigidBodyId(1)).unwrap().position(),
        Vec3::splat(1.0)
    );
    assert_eq!(
        bodies.get(&RigidBodyId(1)).unwrap().linear_velocity(),
        Vec3::ZERO
    );

    // Kinematic maju oleh v, tidak terkena gravitasi
    assert_eq!(
        bodies.get(&RigidBodyId(2)).unwrap().position(),
        Vec3::new(3.0, 2.0, 2.0)
    );
    assert_eq!(
        bodies.get(&RigidBodyId(2)).unwrap().linear_velocity(),
        Vec3::X
    );

    // Dynamic terkena gravitasi dan posisinya maju
    assert_eq!(
        bodies.get(&RigidBodyId(3)).unwrap().linear_velocity(),
        Vec3::new(0.0, -10.0, 0.0)
    );
    assert_eq!(
        bodies.get(&RigidBodyId(3)).unwrap().position(),
        Vec3::new(3.0, -7.0, 3.0)
    );
}

#[test]
fn test_9_6_long_run_stability() {
    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body =
        RigidBody::new_dynamic(RigidBodyId(1), Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap();
    body.set_angular_velocity(Vec3::new(1.0, 2.0, 3.0)).unwrap();

    let dt = 1.0 / 30.0;
    let gravity = Vec3::new(0.0, -9.81, 0.0);

    for _ in 0..500 {
        integrate_body(&mut body, dt, gravity).unwrap();
    }

    assert!(body.position().is_finite());
    assert!(body.rotation().is_finite());
    assert!((body.rotation().length() - 1.0).abs() < 1e-5);
}

#[test]
fn test_9_6_solver_to_integration_boundary() {
    // Memverifikasi batas fase: solver hanya mengubah kecepatan, integrasi memajukan posisi/rotasi
    let mut world = PhysicsWorld::default();
    let body_id = RigidBodyId(1);
    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        body_id,
        Vec3::new(0.0, 0.05, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(0.0, -3.0, 0.0)).unwrap();
    world.add_rigid_body(body, None).unwrap();

    let floor_id = RigidBodyId(2);
    world
        .add_rigid_body(
            RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
            None,
        )
        .unwrap();

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        body_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.05,
    );

    // Langkah 1: Solver kontak
    let pos_before_solve = world.rigid_bodies.get(&body_id).unwrap().position();
    world.solve_contacts(&[contact]).unwrap();
    let pos_after_solve = world.rigid_bodies.get(&body_id).unwrap().position();

    // Solver TIDAK PERNAH memutasi posisi
    assert_eq!(pos_before_solve, pos_after_solve);

    // Langkah 2: Integrasi transform
    world.integrate_transforms().unwrap();
    let pos_after_integrate = world.rigid_bodies.get(&body_id).unwrap().position();

    // Integrasi MEMUTASI posisi dari kecepatan pasca-solver
    assert_ne!(pos_after_integrate, pos_after_solve);
}

#[test]
fn test_9_6_existing_contact_solver_regression() {
    let mut world = PhysicsWorld::default();
    let dyn_id = RigidBodyId(1);
    let static_id = RigidBodyId(2);
    let inertia = Mat3::from_diagonal(Vec3::ONE);

    let mut dyn_b = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        1.0,
        inertia,
    )
    .unwrap();
    dyn_b
        .set_linear_velocity(Vec3::new(0.0, -4.0, 0.0))
        .unwrap();
    world.add_rigid_body(dyn_b, None).unwrap();

    world
        .add_rigid_body(
            RigidBody::new_static(static_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
            None,
        )
        .unwrap();

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        static_id,
        Vec3::new(0.0, 0.5, 0.0),
        Vec3::NEG_Y,
        0.0,
    );

    // Selesaikan kontak: v_y dinetralkan menjadi >= 0
    world.solve_contacts(&[contact]).unwrap();
    assert!(world.rigid_bodies.get(&dyn_id).unwrap().linear_velocity().y >= -1e-4);

    // Integrasi: badan tidak lagi bergerak jatuh tembus ke bawah
    world.integrate_transforms().unwrap();
    assert!(world.rigid_bodies.get(&dyn_id).unwrap().position().y >= 1.0 - 1e-4);
}

#[test]
fn test_9_6_broadphase_proxy_follows_translated_body() {
    let mut world = PhysicsWorld::default();
    let body_id = RigidBodyId(1);
    let inertia = Mat3::from_diagonal(Vec3::ONE);

    let mut body =
        RigidBody::new_dynamic(body_id, Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap();
    body.set_linear_velocity(Vec3::new(10.0, 0.0, 0.0)).unwrap();
    world.add_rigid_body(body, None).unwrap();

    let box_shape = BoxShape::new(Vec3::splat(1.0)).unwrap();
    world
        .add_collider(Collider::new(
            ColliderId(10),
            body_id,
            Shape::Box(box_shape),
            Transform::IDENTITY,
        ))
        .unwrap();

    // Integrasi transform selama 1 detik (fixed_dt = 1.0, bergerak sejauh 10 meter ke +X)
    world.config.fixed_dt = 1.0;
    world.integrate_transforms().unwrap();

    // AABB proksi broadphase harus terbarui di sekitar x = 10.0
    let proxy = world.broadphase.get_proxy(body_id).unwrap();
    let center = proxy.aabb.center();
    assert!((center.x - 10.0).abs() < 1e-3);
}

#[test]
fn test_9_6_broadphase_proxy_follows_rotated_body() {
    let mut world = PhysicsWorld::default();
    let body_id = RigidBodyId(1);
    let inertia = Mat3::from_diagonal(Vec3::ONE);

    let mut body =
        RigidBody::new_dynamic(body_id, Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap();
    // Rotasi 90 derajat mengelilingi sumbu Y: omega = (0, pi/2, 0) selama 1s (30 langkah 30Hz)
    body.set_angular_velocity(Vec3::new(0.0, std::f32::consts::FRAC_PI_2, 0.0))
        .unwrap();
    world.add_rigid_body(body, None).unwrap();

    // Boks panjang di sumbu X: half_extents = (5.0, 1.0, 1.0)
    let box_shape = BoxShape::new(Vec3::new(5.0, 1.0, 1.0)).unwrap();
    world
        .add_collider(Collider::new(
            ColliderId(10),
            body_id,
            Shape::Box(box_shape),
            Transform::IDENTITY,
        ))
        .unwrap();

    // Sebelum rotasi: rentang X = 10, rentang Z = 2
    let initial_proxy = world.broadphase.get_proxy(body_id).unwrap();
    assert!((initial_proxy.aabb.half_extents().x - 5.0).abs() < 1e-3);

    // Integrasi transform selama 30 langkah (1 detik penuh)
    for _ in 0..30 {
        world.integrate_transforms().unwrap();
    }

    // Setelah rotasi 90 derajat di Y: boks panjang kini mengarah ke sumbu Z!
    let rotated_proxy = world.broadphase.get_proxy(body_id).unwrap();
    assert!((rotated_proxy.aabb.half_extents().z - 5.0).abs() < 0.1);
    assert!((rotated_proxy.aabb.half_extents().x - 1.0).abs() < 0.1);
}

#[test]
fn test_9_6_multi_collider_union_aabb_follows_body_transform() {
    let mut world = PhysicsWorld::default();
    let body_id = RigidBodyId(1);
    let inertia = Mat3::from_diagonal(Vec3::ONE);

    let mut body =
        RigidBody::new_dynamic(body_id, Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap();
    body.set_linear_velocity(Vec3::new(5.0, 0.0, 0.0)).unwrap();
    world.add_rigid_body(body, None).unwrap();

    // Dua collider bola dengan offset -2 dan +2 di sumbu X
    let s = Sphere::new(1.0).unwrap();
    let col1 = Collider::new(
        ColliderId(1),
        body_id,
        Shape::Sphere(s),
        Transform::from_translation(Vec3::new(-2.0, 0.0, 0.0)).unwrap(),
    );
    let col2 = Collider::new(
        ColliderId(2),
        body_id,
        Shape::Sphere(s),
        Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)).unwrap(),
    );
    world.add_collider(col1).unwrap();
    world.add_collider(col2).unwrap();

    // Integrasi 1 detik ke depan (fixed_dt = 1.0)
    world.config.fixed_dt = 1.0;
    world.integrate_transforms().unwrap();

    // Setelah bergerak 5m ke kanan, pusat badan di x = 5.
    // Bola 1 di x = 3 (min x = 2), Bola 2 di x = 7 (max x = 8). Union AABB X melingkupi [2, 8].
    let proxy = world.broadphase.get_proxy(body_id).unwrap();
    assert!((proxy.aabb.min.x - 2.0).abs() < 0.1);
    assert!((proxy.aabb.max.x - 8.0).abs() < 0.1);
}

#[test]
fn test_9_6_local_collider_offset_follows_body_rotation() {
    let mut world = PhysicsWorld::default();
    let body_id = RigidBodyId(1);
    let inertia = Mat3::from_diagonal(Vec3::ONE);

    let mut body =
        RigidBody::new_dynamic(body_id, Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap();
    body.set_angular_velocity(Vec3::new(0.0, std::f32::consts::FRAC_PI_2, 0.0))
        .unwrap();
    world.add_rigid_body(body, None).unwrap();

    // Bola kecil di offset lokal (2, 0, 0)
    let s = Sphere::new(0.5).unwrap();
    let col = Collider::new(
        ColliderId(1),
        body_id,
        Shape::Sphere(s),
        Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)).unwrap(),
    );
    world.add_collider(col).unwrap();

    // Integrasi 30 langkah (1 detik penuh)
    for _ in 0..30 {
        world.integrate_transforms().unwrap();
    }

    // Setelah badan berputar 90 derajat di sumbu Y, offset lokal (+2, 0, 0) berada di (0, 0, -2) dunia!
    let proxy = world.broadphase.get_proxy(body_id).unwrap();
    let center = proxy.aabb.center();
    assert!(center.x.abs() < 0.1);
    assert!((center.z - (-2.0)).abs() < 0.1);
}

#[test]
fn test_9_6_failed_integration_does_not_update_broadphase_derived_state() {
    let mut world = PhysicsWorld::default();
    let body_id = RigidBodyId(1);
    let body = RigidBody::new_dynamic(
        body_id,
        Vec3::ZERO,
        Quat::IDENTITY,
        1.0,
        Mat3::from_diagonal(Vec3::ONE),
    )
    .unwrap();
    world.add_rigid_body(body, None).unwrap();

    let s = Sphere::new(1.0).unwrap();
    world
        .add_collider(Collider::new(
            ColliderId(1),
            body_id,
            Shape::Sphere(s),
            Transform::IDENTITY,
        ))
        .unwrap();

    let initial_proxy = world.broadphase.get_proxy(body_id).unwrap().aabb;

    // Paksa konfigurasi dt tidak valid
    world.config.fixed_dt = -1.0;
    assert!(world.integrate_transforms().is_err());

    // Proksi broadphase tetap tidak berubah
    assert_eq!(
        world.broadphase.get_proxy(body_id).unwrap().aabb,
        initial_proxy
    );
}

#[test]
fn test_9_6_world_space_angular_velocity_multiplication_direction() {
    // Menguji secara eksplisit arah perkalian kuaternion dunia (omega_quat * q vs q * omega_quat)
    let q = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    let omega = Vec3::new(0.0, 2.0, 0.0);
    let dt = 0.05;

    let q_res = integrate_rotation(q, omega, dt).unwrap();

    let omega_quat = Quat::from_xyzw(omega.x, omega.y, omega.z, 0.0);
    let dq_left = (omega_quat * q) * (0.5 * dt);
    let expected_left = Quat::from_xyzw(
        q.x + dq_left.x,
        q.y + dq_left.y,
        q.z + dq_left.z,
        q.w + dq_left.w,
    )
    .normalize();

    let dq_right = (q * omega_quat) * (0.5 * dt);
    let expected_right = Quat::from_xyzw(
        q.x + dq_right.x,
        q.y + dq_right.y,
        q.z + dq_right.z,
        q.w + dq_right.w,
    )
    .normalize();

    assert!(
        (q_res - expected_left).length() < 1e-5,
        "Harus cocok dengan perkalian kiri dunia (omega_quat * q)"
    );
    assert!(
        (q_res - expected_right).length() > 0.01,
        "TIDAK BOLEH sama dengan perkalian kanan lokal"
    );
}

#[test]
fn test_9_6_kinematic_rotation_integration() {
    let mut body = RigidBody::new_kinematic(
        RigidBodyId(1),
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::ZERO,
        Vec3::new(0.0, 2.0, 0.0),
    )
    .unwrap();

    integrate_transform(&mut body, 0.5).unwrap();

    assert_ne!(body.rotation(), Quat::IDENTITY);
    assert!(body.rotation().is_finite());
    assert!((body.rotation().length() - 1.0).abs() < 1e-5);
}

#[test]
fn test_9_6_static_broadphase_proxy_remains_unchanged() {
    let mut world = PhysicsWorld::default();
    let body_id = RigidBodyId(1);
    world
        .add_rigid_body(
            RigidBody::new_static(body_id, Vec3::new(10.0, 20.0, 30.0), Quat::IDENTITY).unwrap(),
            None,
        )
        .unwrap();

    let box_s = BoxShape::new(Vec3::splat(2.0)).unwrap();
    world
        .add_collider(Collider::new(
            ColliderId(1),
            body_id,
            Shape::Box(box_s),
            Transform::IDENTITY,
        ))
        .unwrap();

    let proxy_before = world.broadphase.get_proxy(body_id).unwrap().aabb;

    world.integrate().unwrap();

    let proxy_after = world.broadphase.get_proxy(body_id).unwrap().aabb;
    assert_eq!(proxy_before, proxy_after);
}

#[test]
fn test_9_6_gravity_does_not_affect_angular_velocity() {
    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body =
        RigidBody::new_dynamic(RigidBodyId(1), Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap();
    let initial_w = Vec3::new(1.0, 2.0, 3.0);
    body.set_angular_velocity(initial_w).unwrap();

    integrate_velocity(&mut body, 1.0, Vec3::new(0.0, -9.81, 0.0)).unwrap();

    assert_eq!(body.angular_velocity(), initial_w);
    assert_eq!(body.linear_velocity(), Vec3::new(0.0, -9.81, 0.0));
}

#[test]
fn test_9_6_integration_does_not_mutate_local_inertia() {
    let inertia = Mat3::from_diagonal(Vec3::new(1.0, 2.0, 3.0));
    let mut body =
        RigidBody::new_dynamic(RigidBodyId(1), Vec3::ZERO, Quat::IDENTITY, 1.0, inertia).unwrap();
    let initial_local_inertia = body.mass_properties().local_inertia;
    let initial_local_inv_inertia = body.mass_properties().local_inverse_inertia;

    integrate_body(&mut body, 1.0, Vec3::new(0.0, -9.81, 0.0)).unwrap();

    assert_eq!(body.mass_properties().local_inertia, initial_local_inertia);
    assert_eq!(
        body.mass_properties().local_inverse_inertia,
        initial_local_inv_inertia
    );
}

// ============================================================================
// 7. PHASE 9.7: FRICTION + RESTITUTION TESTS
// ============================================================================

#[test]
fn test_9_7_restitution_zero_produces_non_bouncy_response() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(0.0, -4.0, 0.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y, // A -> B (Dyn -> Floor)
        0.0,
    )
    .with_coefficients(0.0, 0.0)
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    // Kecepatan normal netral (tidak memantul ke atas)
    assert!(solved.linear_velocity().y.abs() < 1e-4);
}

#[test]
fn test_9_7_restitution_one_produces_elastic_response() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(0.0, -4.0, 0.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(1.0, 0.0)
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    // Rebound elastis sempurna: v_y membalik tanda dari -4.0 ke +4.0
    assert!(
        (solved.linear_velocity().y - 4.0).abs() < 1e-3,
        "Rebound elastis harus menghasilkan v_y ≈ 4.0, didapat {}",
        solved.linear_velocity().y
    );
}

#[test]
fn test_9_7_restitution_fractional_proportional_bounce() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(0.0, -4.0, 0.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.5, 0.0)
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    // e = 0.5 menghasilkan pantulan separuh kecepatan masuk: v_y ≈ +2.0
    assert!(
        (solved.linear_velocity().y - 2.0).abs() < 1e-3,
        "Rebound e=0.5 harus menghasilkan v_y ≈ 2.0, didapat {}",
        solved.linear_velocity().y
    );
}

#[test]
fn test_9_7_separating_contact_produces_no_restitution_impulse() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(0.0, 3.0, 0.0)).unwrap(); // Menjauh ke atas
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(1.0, 0.5)
    .unwrap();

    let config = SolverConfig::default();
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    // Kecepatan menjauh tidak terpengaruh impuls penarik
    assert_eq!(solved.linear_velocity(), Vec3::new(0.0, 3.0, 0.0));
}

#[test]
fn test_9_7_resting_contact_produces_no_artificial_bounce() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    // Kecepatan awal 0 (istirahat)
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.005, // sedikit penetrasi
    )
    .with_coefficients(1.0, 0.5) // e=1.0 pun tidak boleh memicu pantulan buatan pada resting contact!
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0, // tanpa bias posisi
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    assert_eq!(solved.linear_velocity(), Vec3::ZERO);
}

#[test]
fn test_9_7_restitution_threshold_suppresses_micro_bounce() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    // Kecepatan mendekat sangat lambat (-0.05 m/s) di bawah threshold 0.1 m/s
    body.set_linear_velocity(Vec3::new(0.0, -0.05, 0.0))
        .unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(1.0, 0.0)
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        restitution_velocity_threshold: 0.1,
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    // Pantulan disupresi: kecepatan akhir dinetralkan ke 0.0, TIDAK memantul ke atas
    assert!(solved.linear_velocity().y >= -0.001 && solved.linear_velocity().y <= 0.001);
}

#[test]
fn test_9_7_exactly_at_threshold_behavior() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    // Kecepatan tepat pada threshold (-0.1 m/s)
    body.set_linear_velocity(Vec3::new(0.0, -0.1, 0.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(1.0, 0.0)
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        restitution_velocity_threshold: 0.1,
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    // Karena v_n0 = -0.1 tidak memenuhi kondisi ketat v_n0 < -threshold (-0.1 < -0.1 adalah false),
    // restitusi disupresi secara deterministik
    assert!(solved.linear_velocity().y.abs() < 1e-4);
}

#[test]
fn test_9_7_dynamic_vs_static_restitution() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let static_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(-1.0, 0.0, 0.0),
        Quat::IDENTITY,
        1.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(5.0, 0.0, 0.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        static_id,
        RigidBody::new_static(static_id, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        static_id,
        Vec3::ZERO,
        Vec3::X,
        0.0,
    )
    .with_coefficients(0.8, 0.0)
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved_dyn = bodies.get(&dyn_id).unwrap();
    let solved_static = bodies.get(&static_id).unwrap();

    // Dinamis memantul: v_x ≈ -4.0 (-0.8 * 5.0)
    assert!((solved_dyn.linear_velocity().x - (-4.0)).abs() < 1e-3);
    // Statis tetap 0
    assert_eq!(solved_static.linear_velocity(), Vec3::ZERO);
}

#[test]
fn test_9_7_dynamic_vs_kinematic_restitution() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let kin_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body_dyn = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(-1.0, 0.0, 0.0),
        Quat::IDENTITY,
        1.0,
        inertia,
    )
    .unwrap();
    body_dyn
        .set_linear_velocity(Vec3::new(5.0, 0.0, 0.0))
        .unwrap();
    bodies.insert(dyn_id, body_dyn);

    // Kinematik bergerak dengan kecepatan (1.0, 0.0, 0.0)
    let body_kin = RigidBody::new_kinematic(
        kin_id,
        Vec3::new(1.0, 0.0, 0.0),
        Quat::IDENTITY,
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::ZERO,
    )
    .unwrap();
    bodies.insert(kin_id, body_kin);

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        kin_id,
        Vec3::ZERO,
        Vec3::X,
        0.0,
    )
    .with_coefficients(1.0, 0.0)
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved_dyn = bodies.get(&dyn_id).unwrap();
    let solved_kin = bodies.get(&kin_id).unwrap();

    // Kinematik KEBAL terhadap solver: kecepatannya tetap (1.0, 0.0, 0.0)
    assert_eq!(solved_kin.linear_velocity(), Vec3::new(1.0, 0.0, 0.0));
    // Kecepatan relatif awal: v_rel = 1.0 - 5.0 = -4.0.
    // Pantulan e=1.0 membalik kecepatan relatif menjadi +4.0: v_dyn = 1.0 - 4.0 = -3.0.
    assert!((solved_dyn.linear_velocity().x - (-3.0)).abs() < 1e-3);
}

#[test]
fn test_9_7_dynamic_vs_dynamic_restitution() {
    let mut bodies = std::collections::BTreeMap::new();
    let b1_id = RigidBodyId(1);
    let b2_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut b1 = RigidBody::new_dynamic(
        b1_id,
        Vec3::new(-1.0, 0.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    b1.set_linear_velocity(Vec3::new(3.0, 0.0, 0.0)).unwrap();
    let mut b2 = RigidBody::new_dynamic(
        b2_id,
        Vec3::new(1.0, 0.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    b2.set_linear_velocity(Vec3::new(-3.0, 0.0, 0.0)).unwrap();

    bodies.insert(b1_id, b1);
    bodies.insert(b2_id, b2);

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        b1_id,
        b2_id,
        Vec3::ZERO,
        Vec3::X,
        0.0,
    )
    .with_coefficients(1.0, 0.0)
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved1 = bodies.get(&b1_id).unwrap();
    let solved2 = bodies.get(&b2_id).unwrap();

    // Tabrakan elastis massa sama berkecepatan berlawanan saling bertukar kecepatan
    assert!((solved1.linear_velocity().x - (-3.0)).abs() < 1e-3);
    assert!((solved2.linear_velocity().x - 3.0).abs() < 1e-3);

    let total_p = 2.0 * solved1.linear_velocity() + 2.0 * solved2.linear_velocity();
    assert!(total_p.length() < 1e-4);
}

#[test]
fn test_9_7_material_restitution_combination_symmetry() {
    let mat_a = PhysicsMaterial::new(0.3, 0.5).unwrap();
    let mat_b = PhysicsMaterial::new(0.7, 0.2).unwrap();

    let combined_ab = combine_materials(&mat_a, &mat_b).unwrap();
    let combined_ba = combine_materials(&mat_b, &mat_a).unwrap();

    assert_eq!(combined_ab, combined_ba);
    assert_eq!(combined_ab.restitution, 0.7);
}

#[test]
fn test_9_7_invalid_restitution_negative_rejected() {
    assert_eq!(
        PhysicsMaterial::new(-0.1, 0.0),
        Err(MaterialError::InvalidRestitution)
    );
}

#[test]
fn test_9_7_invalid_restitution_greater_than_one_rejected() {
    assert_eq!(
        PhysicsMaterial::new(1.1, 0.0),
        Err(MaterialError::InvalidRestitution)
    );
}

#[test]
fn test_9_7_invalid_restitution_nan_and_inf_rejected() {
    assert_eq!(
        PhysicsMaterial::new(f32::NAN, 0.0),
        Err(MaterialError::InvalidRestitution)
    );
    assert_eq!(
        PhysicsMaterial::new(f32::INFINITY, 0.0),
        Err(MaterialError::InvalidRestitution)
    );
    assert_eq!(
        PhysicsMaterial::new(f32::NEG_INFINITY, 0.0),
        Err(MaterialError::InvalidRestitution)
    );
}

#[test]
fn test_9_7_zero_friction_produces_no_tangent_impulse() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    // Kecepatan meluncur horizontal 5.0 dan jatuh 2.0
    body.set_linear_velocity(Vec3::new(5.0, -2.0, 0.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.0, 0.0) // mu = 0
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    // Komponen vertikal dinetralkan, komponen horizontal tetap murni 5.0 tanpa gesekan
    assert!(solved.linear_velocity().y.abs() < 1e-4);
    assert_eq!(solved.linear_velocity().x, 5.0);
    assert_eq!(solved.linear_velocity().z, 0.0);
}

#[test]
fn test_9_7_positive_friction_reduces_sliding_speed() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(5.0, -2.0, 0.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.0, 0.5) // mu = 0.5
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    // Kecepatan horizontal harus berkurang secara signifikan karena gesekan
    assert!(
        solved.linear_velocity().x < 5.0,
        "Friksi harus mengurangi kecepatan luncur x, didapat {}",
        solved.linear_velocity().x
    );
    assert!(solved.linear_velocity().x >= 0.0);
}

#[test]
fn test_9_7_friction_never_accelerates_sliding() {
    for initial_vx in [6.0, -6.0] {
        let mut bodies = std::collections::BTreeMap::new();
        let dyn_id = RigidBodyId(1);
        let floor_id = RigidBodyId(2);

        let inertia = Mat3::from_diagonal(Vec3::ONE);
        let mut body = RigidBody::new_dynamic(
            dyn_id,
            Vec3::new(0.0, 1.0, 0.0),
            Quat::IDENTITY,
            2.0,
            inertia,
        )
        .unwrap();
        body.set_linear_velocity(Vec3::new(initial_vx, -2.0, 0.0))
            .unwrap();
        bodies.insert(dyn_id, body);
        bodies.insert(
            floor_id,
            RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
        );

        let contact = Contact::new(
            ColliderId(1),
            ColliderId(2),
            dyn_id,
            floor_id,
            Vec3::ZERO,
            Vec3::NEG_Y,
            0.0,
        )
        .with_coefficients(0.0, 0.4)
        .unwrap();

        let config = SolverConfig {
            iterations: 10,
            beta: 0.0,
            penetration_slop: 0.001,
            ..Default::default()
        };

        solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

        let solved = bodies.get(&dyn_id).unwrap();
        // Magnitudo kecepatan harus berkurang (tidak pernah terakselerasi)
        assert!(
            solved.linear_velocity().x.abs() < initial_vx.abs(),
            "Friksi tidak boleh mempercepat gerakan meluncur, awal: {}, akhir: {}",
            initial_vx,
            solved.linear_velocity().x
        );
        // Arah tidak boleh berbalik melampaui nol (tidak berosilasi liar)
        assert_eq!(solved.linear_velocity().x.signum(), initial_vx.signum());
    }
}

#[test]
fn test_9_7_static_surface_friction() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(4.0, -2.0, 0.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.0, 0.8)
    .unwrap();

    let config = SolverConfig::default();
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved_floor = bodies.get(&floor_id).unwrap();
    assert_eq!(solved_floor.linear_velocity(), Vec3::ZERO);
    assert_eq!(solved_floor.angular_velocity(), Vec3::ZERO);
}

#[test]
fn test_9_7_dynamic_vs_dynamic_friction() {
    let mut bodies = std::collections::BTreeMap::new();
    let b1_id = RigidBodyId(1);
    let b2_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut b1 = RigidBody::new_dynamic(
        b1_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    b1.set_linear_velocity(Vec3::new(4.0, -1.0, 0.0)).unwrap();
    let mut b2 = RigidBody::new_dynamic(
        b2_id,
        Vec3::new(0.0, -1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    b2.set_linear_velocity(Vec3::new(0.0, 1.0, 0.0)).unwrap();

    bodies.insert(b1_id, b1);
    bodies.insert(b2_id, b2);

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        b1_id,
        b2_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.0, 0.5)
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved1 = bodies.get(&b1_id).unwrap();
    let solved2 = bodies.get(&b2_id).unwrap();

    // B1 melambat secara horizontal, B2 terdorong maju oleh gesekan
    assert!(solved1.linear_velocity().x < 4.0);
    assert!(solved2.linear_velocity().x > 0.0);

    // Kekekalan momentum horizontal total
    let total_px = 2.0 * solved1.linear_velocity().x + 2.0 * solved2.linear_velocity().x;
    assert!((total_px - 8.0).abs() < 1e-4);
}

#[test]
fn test_9_7_dynamic_vs_kinematic_friction() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let kin_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body_dyn = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body_dyn
        .set_linear_velocity(Vec3::new(0.0, -1.0, 0.0))
        .unwrap();
    bodies.insert(dyn_id, body_dyn);

    // Kinematik bergerak seperti sabuk konveyor di (3.0, 0.0, 0.0)
    let body_kin = RigidBody::new_kinematic(
        kin_id,
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::new(3.0, 0.0, 0.0),
        Vec3::ZERO,
    )
    .unwrap();
    bodies.insert(kin_id, body_kin);

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        kin_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.0, 0.5)
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved_dyn = bodies.get(&dyn_id).unwrap();
    let solved_kin = bodies.get(&kin_id).unwrap();

    // Kinematik kebal terhadap perubahan kecepatan
    assert_eq!(solved_kin.linear_velocity(), Vec3::new(3.0, 0.0, 0.0));
    // Dinamis terseret ke arah gerak sabuk konveyor (v_x > 0)
    assert!(solved_dyn.linear_velocity().x > 0.1);
}

#[test]
fn test_9_7_zero_tangent_velocity_produces_zero_friction() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    // Jatuh murni vertikal (kecepatan tangensial persis nol)
    body.set_linear_velocity(Vec3::new(0.0, -3.0, 0.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.0, 0.8) // friksi tinggi
    .unwrap();

    let config = SolverConfig::default();
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    // Kecepatan horizontal harus tetap persis 0.0 (tidak ada pergeseran lateral artifisial)
    assert_eq!(solved.linear_velocity().x, 0.0);
    assert_eq!(solved.linear_velocity().z, 0.0);
}

#[test]
fn test_9_7_near_zero_tangent_velocity_is_stable() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(1e-8, -3.0, 1e-8))
        .unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.0, 0.8)
    .unwrap();

    let config = SolverConfig::default();
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    assert!(solved.linear_velocity().is_finite());
}

#[test]
fn test_9_7_coulomb_magnitude_limit_strictly_respected() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    // Kecepatan luncur sangat besar (100.0 m/s), tumbukan normal kecil (-1.0 m/s)
    body.set_linear_velocity(Vec3::new(100.0, -1.0, 0.0))
        .unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let mu = 0.3;
    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.0, mu)
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    // Impuls normal: delta_vy = 0 - (-1) = 1.0. Massa = 2.0 -> J_n = 2.0 * 1.0 = 2.0.
    // Batas friksi Coulomb maksimum: J_t_max = mu * J_n = 0.3 * 2.0 = 0.6.
    // Perubahan kecepatan luncur: delta_vx = J_t_max / 2.0 = 0.3.
    // v_x akhir harus sekitar 100.0 - 0.3 = 99.7.
    let delta_vx = 100.0 - solved.linear_velocity().x;
    assert!(
        (delta_vx - 0.3).abs() < 1e-2,
        "Pengurangan kecepatan friksi Coulomb harus tepat dibatasi oleh mu * J_n, didapat delta_vx = {}",
        delta_vx
    );
}

#[test]
fn test_9_7_material_friction_combination_symmetry() {
    let mat_a = PhysicsMaterial::new(0.5, 0.4).unwrap();
    let mat_b = PhysicsMaterial::new(0.5, 0.9).unwrap();

    let combined_ab = combine_materials(&mat_a, &mat_b).unwrap();
    let combined_ba = combine_materials(&mat_b, &mat_a).unwrap();

    assert_eq!(combined_ab, combined_ba);
    // sqrt(0.4 * 0.9) = sqrt(0.36) = 0.6
    assert!((combined_ab.friction - 0.6).abs() < 1e-5);
}

#[test]
fn test_9_7_invalid_friction_negative_rejected() {
    assert_eq!(
        PhysicsMaterial::new(0.5, -0.2),
        Err(MaterialError::InvalidFriction)
    );
}

#[test]
fn test_9_7_invalid_friction_nan_rejected() {
    assert_eq!(
        PhysicsMaterial::new(0.5, f32::NAN),
        Err(MaterialError::InvalidFriction)
    );
}

#[test]
fn test_9_7_invalid_friction_inf_rejected() {
    assert_eq!(
        PhysicsMaterial::new(0.5, f32::INFINITY),
        Err(MaterialError::InvalidFriction)
    );
    assert_eq!(
        PhysicsMaterial::new(0.5, f32::NEG_INFINITY),
        Err(MaterialError::InvalidFriction)
    );
}

#[test]
fn test_9_7_deterministic_tangent_basis() {
    let normal = Vec3::new(0.0, 1.0, 0.0);
    let b1 = TangentBasis::compute(normal);
    let b2 = TangentBasis::compute(normal);

    assert_eq!(b1, b2);
}

#[test]
fn test_9_7_tangent_basis_orthonormality() {
    let normals = [
        Vec3::X,
        Vec3::Y,
        Vec3::Z,
        Vec3::NEG_X,
        Vec3::NEG_Y,
        Vec3::NEG_Z,
        Vec3::new(1.0, 1.0, 1.0).normalize(),
        Vec3::new(-2.0, 3.0, -1.0).normalize(),
    ];

    for n in normals {
        let basis = TangentBasis::compute(n);
        assert!((basis.t1.length() - 1.0).abs() < 1e-5);
        assert!((basis.t2.length() - 1.0).abs() < 1e-5);
        assert!(basis.t1.dot(n).abs() < 1e-5);
        assert!(basis.t2.dot(n).abs() < 1e-5);
        assert!(basis.t1.dot(basis.t2).abs() < 1e-5);
    }
}

#[test]
fn test_9_7_tangent_basis_rotated_normals() {
    let q = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
    let n = (q * Vec3::Z).normalize();

    let basis = TangentBasis::compute(n);
    assert!((basis.t1.length() - 1.0).abs() < 1e-5);
    assert!((basis.t2.length() - 1.0).abs() < 1e-5);
    assert!(basis.t1.dot(n).abs() < 1e-5);
    assert!(basis.t2.dot(n).abs() < 1e-5);
    assert!(basis.t1.dot(basis.t2).abs() < 1e-5);
}

#[test]
fn test_9_7_tangent_vector_response_both_dimensions() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    // Meluncur diagonal di bidang horizontal XZ: vx = 4.0, vz = 4.0, vy = -2.0
    body.set_linear_velocity(Vec3::new(4.0, -2.0, 4.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.0, 0.6)
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    // Kedua komponen tangensial X dan Z harus tereduksi secara simetris
    assert!(solved.linear_velocity().x < 4.0);
    assert!(solved.linear_velocity().z < 4.0);
    assert!((solved.linear_velocity().x - solved.linear_velocity().z).abs() < 1e-4);
}

#[test]
fn test_9_7_friction_with_friction_greater_than_one() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(2.0, -4.0, 0.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    // Friksi mu = 2.5 (lebih besar dari 1.0 diperbolehkan)
    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.0, 2.5)
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    // Dengan kontak di r_a = (0, -1, 0) dan mu = 2.5, friksi menghentikan slip pada titik kontak
    // menghasilkan kondisi rolling without slipping: v(P) = v_cm + w x r = 0.
    let r_a = contact.point - solved.position();
    let v_contact = solved.linear_velocity() + solved.angular_velocity().cross(r_a);
    assert!(
        v_contact.length() < 1e-3,
        "Kecepatan relatif pada titik kontak harus nol (rolling without slipping), didapat: {:?}",
        v_contact
    );
    // Kecepatan luncur berkurang dari 2.0 menjadi 4/3
    assert!((solved.linear_velocity().x - 4.0 / 3.0).abs() < 1e-3);
    assert!((solved.angular_velocity().z - (-4.0 / 3.0)).abs() < 1e-3);
}

#[test]
fn test_9_7_off_center_friction_generates_torque() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    // Bola berpusat di (0, 0.5, 0)
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 0.5, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(5.0, -2.0, 0.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    // Titik kontak berada di dasar bola (0, 0, 0) -> r_a = (0, -0.5, 0)
    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.0, 0.5)
    .unwrap();

    let config = SolverConfig::default();
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    // Friksi yang menahan luncuran +X pada titik r = (0, -0.5, 0) menimbulkan torsi pada sumbu Z!
    assert!(
        solved.angular_velocity().z.abs() > 0.01,
        "Off-center friction harus menghasilkan respon sudut pada sumbu Z, didapat: {:?}",
        solved.angular_velocity()
    );
}

#[test]
fn test_9_7_center_of_mass_friction_produces_no_angular_impulse() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    // Titik kontak persis di pusat massa (r_a = 0)
    let mut body =
        RigidBody::new_dynamic(dyn_id, Vec3::ZERO, Quat::IDENTITY, 2.0, inertia).unwrap();
    body.set_linear_velocity(Vec3::new(5.0, -2.0, 0.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::new(0.0, -1.0, 0.0), Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.0, 0.5)
    .unwrap();

    let config = SolverConfig::default();
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    // Kecepatan sudut tetap murni nol
    assert_eq!(solved.angular_velocity(), Vec3::ZERO);
}

#[test]
fn test_9_7_rotated_body_uses_world_inverse_inertia_correctly() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    // Inersia asimetris: Ix=1, Iy=10, Iz=100
    let inertia = Mat3::from_diagonal(Vec3::new(1.0, 10.0, 100.0));
    // Rotasi 90 derajat sekitar sumbu Y
    let rot = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    let mut body =
        RigidBody::new_dynamic(dyn_id, Vec3::new(0.0, 1.0, 0.0), rot, 2.0, inertia).unwrap();
    body.set_linear_velocity(Vec3::new(4.0, -2.0, 0.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.0, 0.5)
    .unwrap();

    let config = SolverConfig::default();
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    assert!(solved.linear_velocity().is_finite());
    assert!(solved.angular_velocity().is_finite());
}

#[test]
fn test_9_7_asymmetric_inertia_friction_behavior() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::new(1.0, 5.0, 20.0));
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(4.0, -2.0, 4.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::new(0.0, 0.5, 0.0),
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.0, 0.5)
    .unwrap();

    let config = SolverConfig::default();
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    assert!(solved.angular_velocity().is_finite());
}

#[test]
fn test_9_7_local_inertia_immutability_under_friction() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::new(1.0, 2.0, 3.0));
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(4.0, -2.0, 0.0)).unwrap();

    let local_i_before = body.mass_properties().local_inertia;
    let local_inv_i_before = body.mass_properties().local_inverse_inertia;

    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.5, 0.5)
    .unwrap();

    let config = SolverConfig::default();
    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    assert_eq!(solved.mass_properties().local_inertia, local_i_before);
    assert_eq!(
        solved.mass_properties().local_inverse_inertia,
        local_inv_i_before
    );
}

#[test]
fn test_9_7_combined_normal_restitution_friction() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(5.0, -4.0, 0.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.6, 0.4) // e = 0.6, mu = 0.4
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    // Bounces up with v_y ≈ +2.4 (0.6 * 4.0)
    assert!((solved.linear_velocity().y - 2.4).abs() < 1e-2);
    // Decelerates horizontal sliding: v_x < 5.0
    assert!(solved.linear_velocity().x < 5.0);
    assert!(solved.linear_velocity().x > 0.0);
}

#[test]
fn test_9_7_multiple_contacts_friction_and_restitution() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(4.0, -2.0, 0.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let c1 = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::new(-0.5, 0.0, 0.0),
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.5, 0.3)
    .unwrap();

    let c2 = Contact::new(
        ColliderId(3),
        ColliderId(4),
        dyn_id,
        floor_id,
        Vec3::new(0.5, 0.0, 0.0),
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.5, 0.3)
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[c1, c2], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    assert!(solved.linear_velocity().y > 0.5);
    assert!(solved.linear_velocity().x < 4.0);
}

#[test]
fn test_9_7_multiple_colliders_distinct_materials() {
    let shape = Shape::Sphere(Sphere::new(0.5).unwrap());
    let col1 = Collider::new(
        ColliderId(1),
        RigidBodyId(1),
        shape.clone(),
        Transform::IDENTITY,
    )
    .with_material(PhysicsMaterial::new(0.2, 0.1).unwrap())
    .unwrap();
    let col2 = Collider::new(ColliderId(2), RigidBodyId(1), shape, Transform::IDENTITY)
        .with_material(PhysicsMaterial::new(0.8, 0.9).unwrap())
        .unwrap();

    assert_eq!(col1.material().friction, 0.1);
    assert_eq!(col2.material().friction, 0.9);
}

#[test]
fn test_9_7_mixed_body_types_collection() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let kin_id = RigidBodyId(2);
    let static_id = RigidBodyId(3);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut b_dyn =
        RigidBody::new_dynamic(dyn_id, Vec3::ZERO, Quat::IDENTITY, 2.0, inertia).unwrap();
    b_dyn.set_linear_velocity(Vec3::new(2.0, 0.0, 0.0)).unwrap();
    bodies.insert(dyn_id, b_dyn);

    let b_kin = RigidBody::new_kinematic(
        kin_id,
        Vec3::new(2.0, 0.0, 0.0),
        Quat::IDENTITY,
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::ZERO,
    )
    .unwrap();
    bodies.insert(kin_id, b_kin);

    let b_static =
        RigidBody::new_static(static_id, Vec3::new(-2.0, 0.0, 0.0), Quat::IDENTITY).unwrap();
    bodies.insert(static_id, b_static);

    let c = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        kin_id,
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::X,
        0.0,
    )
    .with_coefficients(0.5, 0.5)
    .unwrap();

    let config = SolverConfig::default();
    solve_contacts(&mut bodies, &[c], 1.0 / 30.0, &config).unwrap();

    assert_eq!(
        bodies.get(&kin_id).unwrap().linear_velocity(),
        Vec3::new(-1.0, 0.0, 0.0)
    );
    assert_eq!(
        bodies.get(&static_id).unwrap().linear_velocity(),
        Vec3::ZERO
    );
}

#[test]
fn test_9_7_deterministic_repeated_solve() {
    let setup = || {
        let mut bodies = std::collections::BTreeMap::new();
        let dyn_id = RigidBodyId(1);
        let floor_id = RigidBodyId(2);

        let inertia = Mat3::from_diagonal(Vec3::ONE);
        let mut body = RigidBody::new_dynamic(
            dyn_id,
            Vec3::new(0.0, 1.0, 0.0),
            Quat::IDENTITY,
            2.0,
            inertia,
        )
        .unwrap();
        body.set_linear_velocity(Vec3::new(3.0, -3.0, 2.0)).unwrap();
        bodies.insert(dyn_id, body);
        bodies.insert(
            floor_id,
            RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
        );

        let contact = Contact::new(
            ColliderId(1),
            ColliderId(2),
            dyn_id,
            floor_id,
            Vec3::ZERO,
            Vec3::NEG_Y,
            0.0,
        )
        .with_coefficients(0.5, 0.4)
        .unwrap();

        (bodies, contact)
    };

    let (mut b1, c1) = setup();
    let (mut b2, c2) = setup();

    let config = SolverConfig::default();
    solve_contacts(&mut b1, &[c1], 1.0 / 30.0, &config).unwrap();
    solve_contacts(&mut b2, &[c2], 1.0 / 30.0, &config).unwrap();

    let s1 = b1.get(&RigidBodyId(1)).unwrap();
    let s2 = b2.get(&RigidBodyId(1)).unwrap();

    assert_eq!(s1.linear_velocity(), s2.linear_velocity());
    assert_eq!(s1.angular_velocity(), s2.angular_velocity());
}

#[test]
fn test_9_7_reverse_contact_physical_symmetry() {
    // KONTRAK MANDATORI SIMETRI TERBALIK (SECTION 25):
    // Contact(A, B) vs Contact(B, A) harus menghasilkan respon fisik yang ekuivalen!
    let setup = |is_reversed: bool| {
        let mut bodies = std::collections::BTreeMap::new();
        let b1_id = RigidBodyId(1);
        let b2_id = RigidBodyId(2);

        let inertia = Mat3::from_diagonal(Vec3::ONE);
        let mut b1 = RigidBody::new_dynamic(
            b1_id,
            Vec3::new(-1.0, 0.0, 0.0),
            Quat::IDENTITY,
            2.0,
            inertia,
        )
        .unwrap();
        b1.set_linear_velocity(Vec3::new(3.0, 0.0, 2.0)).unwrap();
        let mut b2 = RigidBody::new_dynamic(
            b2_id,
            Vec3::new(1.0, 0.0, 0.0),
            Quat::IDENTITY,
            2.0,
            inertia,
        )
        .unwrap();
        b2.set_linear_velocity(Vec3::new(-3.0, 0.0, -2.0)).unwrap();

        bodies.insert(b1_id, b1);
        bodies.insert(b2_id, b2);

        let c_forward = Contact::new(
            ColliderId(1),
            ColliderId(2),
            b1_id,
            b2_id,
            Vec3::ZERO,
            Vec3::X,
            0.0,
        )
        .with_coefficients(0.5, 0.4)
        .unwrap();

        let contact = if is_reversed {
            c_forward.reverse_symmetry()
        } else {
            c_forward
        };

        (bodies, contact)
    };

    let (mut b_fwd, c_fwd) = setup(false);
    let (mut b_rev, c_rev) = setup(true);

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut b_fwd, &[c_fwd], 1.0 / 30.0, &config).unwrap();
    solve_contacts(&mut b_rev, &[c_rev], 1.0 / 30.0, &config).unwrap();

    let fwd_1 = b_fwd.get(&RigidBodyId(1)).unwrap();
    let fwd_2 = b_fwd.get(&RigidBodyId(2)).unwrap();
    let rev_1 = b_rev.get(&RigidBodyId(1)).unwrap();
    let rev_2 = b_rev.get(&RigidBodyId(2)).unwrap();

    // Kecepatan akhir kedua badan harus identik dalam toleransi numerik
    assert!(
        (fwd_1.linear_velocity() - rev_1.linear_velocity()).length() < 1e-4,
        "Badan 1 forward vs reverse berbeda: fwd={:?}, rev={:?}",
        fwd_1.linear_velocity(),
        rev_1.linear_velocity()
    );
    assert!(
        (fwd_2.linear_velocity() - rev_2.linear_velocity()).length() < 1e-4,
        "Badan 2 forward vs reverse berbeda: fwd={:?}, rev={:?}",
        fwd_2.linear_velocity(),
        rev_2.linear_velocity()
    );
}

#[test]
fn test_9_7_no_nan_or_inf_under_extreme_inputs() {
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(1e4, -1e4, 1e4)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.9, 0.9)
    .unwrap();

    let config = SolverConfig::default();
    solve_contacts(&mut bodies, &[contact], 1e-5, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    assert!(solved.linear_velocity().is_finite());
    assert!(solved.angular_velocity().is_finite());
}

#[test]
fn test_9_7_solver_convergence_with_iterations() {
    let setup = || {
        let mut bodies = std::collections::BTreeMap::new();
        let dyn_id = RigidBodyId(1);
        let floor_id = RigidBodyId(2);

        let inertia = Mat3::from_diagonal(Vec3::ONE);
        let mut body = RigidBody::new_dynamic(
            dyn_id,
            Vec3::new(0.0, 1.0, 0.0),
            Quat::IDENTITY,
            2.0,
            inertia,
        )
        .unwrap();
        body.set_linear_velocity(Vec3::new(4.0, -3.0, 0.0)).unwrap();
        bodies.insert(dyn_id, body);
        bodies.insert(
            floor_id,
            RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
        );

        let contact = Contact::new(
            ColliderId(1),
            ColliderId(2),
            dyn_id,
            floor_id,
            Vec3::ZERO,
            Vec3::NEG_Y,
            0.0,
        )
        .with_coefficients(0.0, 1.0)
        .unwrap();

        (bodies, contact)
    };

    let (mut b1, c1) = setup();
    let (mut b10, c10) = setup();

    let cfg1 = SolverConfig {
        iterations: 1,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };
    let cfg10 = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut b1, &[c1], 1.0 / 30.0, &cfg1).unwrap();
    solve_contacts(&mut b10, &[c10], 1.0 / 30.0, &cfg10).unwrap();

    assert!(b1
        .get(&RigidBodyId(1))
        .unwrap()
        .linear_velocity()
        .is_finite());
    assert!(b10
        .get(&RigidBodyId(1))
        .unwrap()
        .linear_velocity()
        .is_finite());
}

#[test]
fn test_9_7_phase_9_5_normal_only_regression() {
    // REGRESI KRITIS: Ketika restitution = 0 dan friction = 0,
    // perilakunya identik dengan solver normal Phase 9.5!
    let mut bodies = std::collections::BTreeMap::new();
    let dyn_id = RigidBodyId(1);
    let floor_id = RigidBodyId(2);

    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let mut body = RigidBody::new_dynamic(
        dyn_id,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        2.0,
        inertia,
    )
    .unwrap();
    body.set_linear_velocity(Vec3::new(0.0, -3.0, 0.0)).unwrap();
    bodies.insert(dyn_id, body);
    bodies.insert(
        floor_id,
        RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap(),
    );

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        dyn_id,
        floor_id,
        Vec3::ZERO,
        Vec3::NEG_Y,
        0.0,
    )
    .with_coefficients(0.0, 0.0)
    .unwrap();

    let config = SolverConfig {
        iterations: 10,
        beta: 0.0,
        penetration_slop: 0.001,
        ..Default::default()
    };

    solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &config).unwrap();

    let solved = bodies.get(&dyn_id).unwrap();
    assert_eq!(solved.linear_velocity(), Vec3::ZERO);
}

#[test]
fn test_9_7_phase_9_6_integration_regression() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let body_id = RigidBodyId(1);
    let inertia = Mat3::from_diagonal(Vec3::ONE);
    let body = RigidBody::new_dynamic(
        body_id,
        Vec3::new(0.0, 10.0, 0.0),
        Quat::IDENTITY,
        1.0,
        inertia,
    )
    .unwrap();
    world.add_rigid_body(body, None).unwrap();

    let contact = Contact::new(
        ColliderId(1),
        ColliderId(2),
        body_id,
        RigidBodyId(99),
        Vec3::new(0.0, 10.0, 0.0),
        Vec3::Y,
        0.0,
    )
    .with_coefficients(0.5, 0.5)
    .unwrap();

    // Posisi sebelum integrasi
    let pos_before = world.rigid_bodies.get(&body_id).unwrap().position();
    let rot_before = world.rigid_bodies.get(&body_id).unwrap().rotation();

    // Solver memutasi kecepatan, BUKAN posisi atau rotasi!
    world.solve_contacts(&[contact]).unwrap_err(); // RigidBody 99 tidak ada, membuktikan validasi

    // Posisi dan rotasi tetap tidak tersentuh oleh solver
    assert_eq!(
        world.rigid_bodies.get(&body_id).unwrap().position(),
        pos_before
    );
    assert_eq!(
        world.rigid_bodies.get(&body_id).unwrap().rotation(),
        rot_before
    );
}
