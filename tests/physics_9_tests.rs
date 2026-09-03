use glam::{IVec3, Mat3, Quat, Vec3};
use omnisia::chunk::Chunk;
use omnisia::material::MaterialId;
use omnisia::physics::{
    world_pos_to_cell, Aabb, AabbError, BodyType, BroadphaseError, BroadphasePair, BroadphaseProxy,
    CellCoord, MassProperties, PhysicsWorld, PhysicsWorldConfig, RigidBody, RigidBodyError,
    RigidBodyId, SpatialHashBroadphase, StaticTerrainQuery,
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
