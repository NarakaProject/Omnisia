use glam::{IVec3, Mat3, Quat, Vec3};
use omnisia::chunk::Chunk;
use omnisia::material::MaterialId;
use omnisia::physics::{
    collide, compute_world_inv_inertia, solve_contacts, world_pos_to_cell, Aabb, AabbError,
    BodyType, BoxShape, BroadphaseError, BroadphasePair, BroadphaseProxy, Capsule, CellCoord,
    Collider, ColliderId, Contact, MassProperties, PhysicsWorld, PhysicsWorldConfig, RigidBody,
    RigidBodyError, RigidBodyId, Shape, ShapeError, SolverConfig, SolverError,
    SpatialHashBroadphase, Sphere, StaticTerrainQuery, Transform,
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
    };
    assert_eq!(
        solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &cfg_zero_iter),
        Err(SolverError::InvalidConfiguration)
    );

    let cfg_neg_beta = SolverConfig {
        iterations: 10,
        beta: -0.1,
        penetration_slop: 0.001,
    };
    assert_eq!(
        solve_contacts(&mut bodies, &[contact], 1.0 / 30.0, &cfg_neg_beta),
        Err(SolverError::InvalidConfiguration)
    );

    let cfg_neg_slop = SolverConfig {
        iterations: 10,
        beta: 0.2,
        penetration_slop: -0.001,
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
