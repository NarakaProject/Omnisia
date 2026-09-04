use glam::{IVec3, Vec3};
use omnisia::chunk::Chunk;
use omnisia::impact::{
    AffectedVolume, DeterministicImpactPipeline, ImpactError, ImpactEvent, ImpactId, ImpactSource,
    ImpactSourceKind,
};
use omnisia::material::MaterialId;
use omnisia::streaming::store::ChunkStore;
use omnisia::voxel::VoxelBlock;

// ============================================================================
// A. EVENT CONSTRUCTION & VALIDATION
// ============================================================================

#[test]
fn test_10_1_01_valid_event_construction_with_builder() {
    let event = ImpactEvent::builder(ImpactId(1), Vec3::new(10.0, 20.0, 30.0), 5.0)
        .source(ImpactSource::projectile(42))
        .direction(Vec3::new(0.0, -10.0, 0.0))
        .surface_normal(Vec3::new(0.0, 1.0, 0.0))
        .energy(15_000.0)
        .impulse(300.0)
        .build()
        .expect("Valid event must construct successfully");

    assert_eq!(event.id, ImpactId(1));
    assert_eq!(event.source, ImpactSource::projectile(42));
    assert_eq!(event.position, Vec3::new(10.0, 20.0, 30.0));
    assert_eq!(event.radius, 5.0);

    // Direction must be normalized once during construction
    let dir = event.direction.unwrap();
    assert!((dir.length() - 1.0).abs() < 1e-6);
    assert_eq!(dir, Vec3::new(0.0, -1.0, 0.0));

    // Surface normal must be normalized
    let norm = event.surface_normal.unwrap();
    assert!((norm.length() - 1.0).abs() < 1e-6);
    assert_eq!(norm, Vec3::new(0.0, 1.0, 0.0));

    // Magnitude must preserve both energy and impulse
    assert_eq!(event.magnitude.energy(), Some(15_000.0));
    assert_eq!(event.magnitude.impulse(), Some(300.0));
}

#[test]
fn test_10_1_02_invalid_position_rejected() {
    let nan_pos = Vec3::new(f32::NAN, 1.0, 2.0);
    let inf_pos = Vec3::new(1.0, f32::INFINITY, 2.0);

    let res_nan = ImpactEvent::builder(ImpactId(1), nan_pos, 5.0)
        .energy(100.0)
        .build();
    assert!(matches!(res_nan, Err(ImpactError::NonFinitePosition(_))));

    let res_inf = ImpactEvent::builder(ImpactId(2), inf_pos, 5.0)
        .energy(100.0)
        .build();
    assert!(matches!(res_inf, Err(ImpactError::NonFinitePosition(_))));
}

#[test]
fn test_10_1_03_invalid_direction_and_normal_rejected() {
    // Zero-length direction
    let res_zero = ImpactEvent::builder(ImpactId(1), Vec3::ZERO, 5.0)
        .direction(Vec3::ZERO)
        .energy(100.0)
        .build();
    assert_eq!(res_zero, Err(ImpactError::ZeroLengthDirection));

    // Non-finite direction
    let res_nan_dir = ImpactEvent::builder(ImpactId(2), Vec3::ZERO, 5.0)
        .direction(Vec3::new(f32::NAN, 1.0, 0.0))
        .energy(100.0)
        .build();
    assert!(matches!(
        res_nan_dir,
        Err(ImpactError::NonFiniteDirection(_))
    ));

    // Zero-length surface normal
    let res_zero_norm = ImpactEvent::builder(ImpactId(3), Vec3::ZERO, 5.0)
        .surface_normal(Vec3::ZERO)
        .energy(100.0)
        .build();
    assert_eq!(res_zero_norm, Err(ImpactError::ZeroLengthNormal));

    // Non-finite surface normal
    let res_inf_norm = ImpactEvent::builder(ImpactId(4), Vec3::ZERO, 5.0)
        .surface_normal(Vec3::new(0.0, f32::NEG_INFINITY, 0.0))
        .energy(100.0)
        .build();
    assert!(matches!(res_inf_norm, Err(ImpactError::NonFiniteNormal(_))));
}

#[test]
fn test_10_1_04_invalid_magnitude_rejected() {
    // Negative energy
    let res_neg_e = ImpactEvent::builder(ImpactId(1), Vec3::ZERO, 5.0)
        .energy(-10.0)
        .build();
    assert_eq!(res_neg_e, Err(ImpactError::NegativeEnergy(-10.0)));

    // Non-finite energy
    let res_nan_e = ImpactEvent::builder(ImpactId(2), Vec3::ZERO, 5.0)
        .energy(f32::NAN)
        .build();
    assert!(matches!(res_nan_e, Err(ImpactError::NonFiniteEnergy(_))));

    // Negative impulse
    let res_neg_i = ImpactEvent::builder(ImpactId(3), Vec3::ZERO, 5.0)
        .impulse(-5.0)
        .build();
    assert_eq!(res_neg_i, Err(ImpactError::NegativeImpulse(-5.0)));

    // Missing both energy and impulse
    let res_missing = ImpactEvent::builder(ImpactId(4), Vec3::ZERO, 5.0).build();
    assert_eq!(res_missing, Err(ImpactError::MissingMagnitude));
}

#[test]
fn test_10_1_05_invalid_radius_rejected() {
    // Negative radius
    let res_neg_r = ImpactEvent::builder(ImpactId(1), Vec3::ZERO, -2.0)
        .energy(100.0)
        .build();
    assert_eq!(res_neg_r, Err(ImpactError::NegativeRadius(-2.0)));

    // Non-finite radius
    let res_inf_r = ImpactEvent::builder(ImpactId(2), Vec3::ZERO, f32::INFINITY)
        .energy(100.0)
        .build();
    assert!(matches!(res_inf_r, Err(ImpactError::NonFiniteRadius(_))));
}

// ============================================================================
// B. SOURCE REPRESENTATION
// ============================================================================

#[test]
fn test_10_1_06_generic_source_kinds_and_stability() {
    let s1 = ImpactSource::generic();
    let s2 = ImpactSource::projectile(101);
    let s3 = ImpactSource::creature(202);
    let s4 = ImpactSource::environment(303);
    let s5 = ImpactSource::ability(404);
    let s6 = ImpactSource::debris(505);

    assert_eq!(s1.kind, ImpactSourceKind::Generic);
    assert_eq!(s2.kind, ImpactSourceKind::Projectile);
    assert_eq!(s3.kind, ImpactSourceKind::Creature);
    assert_eq!(s4.kind, ImpactSourceKind::Environment);
    assert_eq!(s5.kind, ImpactSourceKind::Ability);
    assert_eq!(s6.kind, ImpactSourceKind::Debris);

    assert_eq!(s2.id, 101);
    assert_eq!(s3.id, 202);

    // Verify ordering between sources is deterministic
    assert!(s1 < s2);
}

// ============================================================================
// C. GEOMETRY & BOUNDED AFFECTED VOLUME
// ============================================================================

#[test]
fn test_10_1_07_zero_radius_and_single_voxel_intersection() {
    let center = Vec3::new(1.25, 2.25, 3.25);
    let volume = AffectedVolume::from_sphere(center, 0.0)
        .expect("Zero radius must be valid for point impact");

    assert_eq!(volume.radius, 0.0);
    assert_eq!(volume.world_min, center);
    assert_eq!(volume.world_max, center);

    // Voxel bounds must span exactly the single containing voxel (1.25 / 0.5 = 2.5 -> floor 2)
    assert_eq!(volume.min_voxel, IVec3::new(2, 4, 6));
    assert_eq!(volume.max_voxel, IVec3::new(2, 4, 6));
    assert_eq!(volume.voxel_count_bounded(), 1);
    assert_eq!(volume.chunk_count(), 1);

    assert!(volume.contains_point(center));
    assert!(!volume.contains_point(center + Vec3::new(0.01, 0.0, 0.0)));
}

#[test]
fn test_10_1_08_volume_spatial_intersection_queries() {
    let center = Vec3::new(0.0, 0.0, 0.0);
    let radius = 2.0;
    let volume = AffectedVolume::from_sphere(center, radius).unwrap();

    // Point queries
    assert!(volume.contains_point(Vec3::new(0.0, 1.5, 0.0)));
    assert!(volume.contains_point(Vec3::new(2.0, 0.0, 0.0)));
    assert!(!volume.contains_point(Vec3::new(2.01, 0.0, 0.0)));

    // Voxel center query: voxel (0, 0, 0) center is at (0.25, 0.25, 0.25)
    // distance is sqrt(3 * 0.25^2) = ~0.433 <= 2.0
    assert!(volume.contains_voxel_center(IVec3::new(0, 0, 0)));

    // Voxel (10, 10, 10) is far away
    assert!(!volume.contains_voxel_center(IVec3::new(10, 10, 10)));
    assert!(!volume.intersects_voxel(IVec3::new(10, 10, 10)));

    // Intersects volume query
    let other_touching = AffectedVolume::from_sphere(Vec3::new(3.0, 0.0, 0.0), 1.0).unwrap();
    assert!(volume.intersects_volume(&other_touching));

    let other_distant = AffectedVolume::from_sphere(Vec3::new(10.0, 0.0, 0.0), 1.0).unwrap();
    assert!(!volume.intersects_volume(&other_distant));
}

// ============================================================================
// D. COORDINATE CORRECTNESS (Positive, Zero, Negative, Chunk Boundaries)
// ============================================================================

#[test]
fn test_10_1_09_positive_chunk_bounds_calculation() {
    // Center at (8.0, 8.0, 8.0) with radius 4.0m -> world bounds [4.0, 12.0]
    // 1 chunk = 16.0m (voxels 0..31 -> world 0.0..16.0m)
    // All coordinates fit inside chunk (0, 0, 0)
    let volume = AffectedVolume::from_sphere(Vec3::splat(8.0), 4.0).unwrap();
    assert_eq!(volume.min_chunk, IVec3::ZERO);
    assert_eq!(volume.max_chunk, IVec3::ZERO);
    assert_eq!(volume.chunk_count(), 1);

    let chunks: Vec<IVec3> = volume.iter_chunks().collect();
    assert_eq!(chunks, vec![IVec3::ZERO]);
}

#[test]
fn test_10_1_10_chunk_boundary_spanning_positive() {
    // Chunk boundary is at x = 16.0m (voxel 31 is 15.5..16.0m, voxel 32 is in chunk 1)
    // Center at x = 16.0m, radius 2.0m -> x spans 14.0m to 18.0m
    let volume = AffectedVolume::from_sphere(Vec3::new(16.0, 8.0, 8.0), 2.0).unwrap();
    assert_eq!(volume.min_chunk, IVec3::new(0, 0, 0));
    assert_eq!(volume.max_chunk, IVec3::new(1, 0, 0));
    assert_eq!(volume.chunk_count(), 2);

    let chunks: Vec<IVec3> = volume.iter_chunks().collect();
    assert_eq!(chunks, vec![IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)]);
}

#[test]
fn test_10_1_11_negative_coordinates_euclidean_correctness() {
    // Negative world coordinates: center at (-10.0, 8.0, -10.0), radius 2.0m
    // x spans [-12.0, -8.0]. In 16.0m chunks:
    // -12.0 / 16.0 = -0.75 -> floor chunk -1
    // -8.0 / 16.0 = -0.5 -> floor chunk -1
    let volume = AffectedVolume::from_sphere(Vec3::new(-10.0, 8.0, -10.0), 2.0).unwrap();
    assert_eq!(volume.min_chunk, IVec3::new(-1, 0, -1));
    assert_eq!(volume.max_chunk, IVec3::new(-1, 0, -1));
    assert_eq!(volume.chunk_count(), 1);

    // Negative boundary crossing: center at (-16.0, 8.0, 8.0) (boundary between chunk -2 and -1)
    // Radius 2.0m -> x spans [-18.0, -14.0], y and z span [6.0, 10.0]
    // -18.0m in voxels is -36 -> chunk -2
    // -14.0m in voxels is -28 -> chunk -1
    let volume_neg_boundary = AffectedVolume::from_sphere(Vec3::new(-16.0, 8.0, 8.0), 2.0).unwrap();
    assert_eq!(volume_neg_boundary.min_chunk, IVec3::new(-2, 0, 0));
    assert_eq!(volume_neg_boundary.max_chunk, IVec3::new(-1, 0, 0));
    assert_eq!(volume_neg_boundary.chunk_count(), 2);

    let chunks: Vec<IVec3> = volume_neg_boundary.iter_chunks().collect();
    assert_eq!(chunks, vec![IVec3::new(-2, 0, 0), IVec3::new(-1, 0, 0)]);
}

#[test]
fn test_10_1_12_corner_crossing_eight_chunks() {
    // Origin crossing: center at (0.0, 0.0, 0.0) with radius 1.0m
    // Spans [-1.0, 1.0] across X, Y, Z.
    // -1.0m is in chunk -1, +1.0m is in chunk 0.
    // Must touch exactly 2x2x2 = 8 chunks:
    // x in {-1, 0}, y in {-1, 0}, z in {-1, 0}
    let volume = AffectedVolume::from_sphere(Vec3::ZERO, 1.0).unwrap();
    assert_eq!(volume.min_chunk, IVec3::new(-1, -1, -1));
    assert_eq!(volume.max_chunk, IVec3::new(0, 0, 0));
    assert_eq!(volume.chunk_count(), 8);

    let chunks: Vec<IVec3> = volume.iter_chunks().collect();
    assert_eq!(chunks.len(), 8);
    assert!(chunks.contains(&IVec3::new(-1, -1, -1)));
    assert!(chunks.contains(&IVec3::new(0, 0, 0)));
}

// ============================================================================
// E. DETERMINISTIC IMPACT PIPELINE & REPLAY
// ============================================================================

#[test]
fn test_10_1_13_deterministic_sorting_and_ordering() {
    let mut pipeline = DeterministicImpactPipeline::new();

    let e3 = ImpactEvent::builder(ImpactId(3), Vec3::new(0.0, 5.0, 0.0), 2.0)
        .energy(100.0)
        .build()
        .unwrap();
    let e1 = ImpactEvent::builder(ImpactId(1), Vec3::new(10.0, 0.0, 0.0), 1.0)
        .energy(200.0)
        .build()
        .unwrap();
    let e2 = ImpactEvent::builder(ImpactId(2), Vec3::new(-5.0, 0.0, 0.0), 3.0)
        .energy(300.0)
        .build()
        .unwrap();

    // Submit in out-of-order sequence: e3, e1, e2
    pipeline.submit(e3);
    pipeline.submit(e1);
    pipeline.submit(e2);

    let processed = pipeline.process();
    assert_eq!(processed.len(), 3);
    assert_eq!(processed[0].event.id, ImpactId(1));
    assert_eq!(processed[1].event.id, ImpactId(2));
    assert_eq!(processed[2].event.id, ImpactId(3));
}

#[test]
fn test_10_1_14_deterministic_replay_and_shuffled_invariance() {
    let events = vec![
        ImpactEvent::builder(ImpactId(10), Vec3::new(12.0, 4.0, -8.0), 2.5)
            .energy(500.0)
            .build()
            .unwrap(),
        ImpactEvent::builder(ImpactId(5), Vec3::new(-33.0, 10.0, 65.0), 4.0)
            .impulse(120.0)
            .build()
            .unwrap(),
        ImpactEvent::builder(ImpactId(20), Vec3::new(0.0, 0.0, 0.0), 1.0)
            .energy(50.0)
            .build()
            .unwrap(),
        ImpactEvent::builder(ImpactId(1), Vec3::new(100.0, 50.0, 200.0), 8.0)
            .energy(10_000.0)
            .build()
            .unwrap(),
    ];

    // Run 1: original order
    let mut pipe1 = DeterministicImpactPipeline::new();
    pipe1.submit_batch(events.clone());
    let res1 = pipe1.process();

    // Run 2: completely reversed order
    let mut pipe2 = DeterministicImpactPipeline::new();
    let mut reversed = events.clone();
    reversed.reverse();
    pipe2.submit_batch(reversed);
    let res2 = pipe2.process();

    // Run 3: custom arbitrary permutation
    let mut pipe3 = DeterministicImpactPipeline::new();
    pipe3.submit(events[2]);
    pipe3.submit(events[0]);
    pipe3.submit(events[3]);
    pipe3.submit(events[1]);
    let res3 = pipe3.process();

    // All results must be 100% bitwise identical!
    assert_eq!(res1, res2);
    assert_eq!(res2, res3);
}

#[test]
fn test_10_1_15_deduplication_by_id() {
    let mut pipeline = DeterministicImpactPipeline::new();

    let e1_a = ImpactEvent::builder(ImpactId(1), Vec3::new(1.0, 0.0, 0.0), 2.0)
        .energy(100.0)
        .build()
        .unwrap();
    let e1_b = ImpactEvent::builder(ImpactId(1), Vec3::new(2.0, 0.0, 0.0), 3.0)
        .energy(200.0)
        .build()
        .unwrap();
    let e2 = ImpactEvent::builder(ImpactId(2), Vec3::new(5.0, 0.0, 0.0), 1.0)
        .energy(50.0)
        .build()
        .unwrap();

    pipeline.submit(e1_a);
    pipeline.submit(e2);
    pipeline.submit(e1_b);

    assert_eq!(pipeline.len(), 3);
    let deduped = pipeline.process_and_deduplicate();
    assert_eq!(deduped.len(), 2);
    assert_eq!(deduped[0].event.id, ImpactId(1));
    assert_eq!(deduped[1].event.id, ImpactId(2));
}

#[test]
fn test_10_1_16_pipeline_query_affected_chunks() {
    let mut pipeline = DeterministicImpactPipeline::new();

    // Impact 1 inside chunk (0, 0, 0)
    let e1 = ImpactEvent::builder(ImpactId(1), Vec3::new(8.0, 8.0, 8.0), 2.0)
        .energy(100.0)
        .build()
        .unwrap();
    // Impact 2 inside chunk (2, 0, 2)
    let e2 = ImpactEvent::builder(ImpactId(2), Vec3::new(40.0, 8.0, 40.0), 2.0)
        .energy(100.0)
        .build()
        .unwrap();

    pipeline.submit(e1);
    pipeline.submit(e2);

    let chunks = pipeline.query_affected_chunks();
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks, vec![IVec3::new(0, 0, 0), IVec3::new(2, 0, 2)]);
}

// ============================================================================
// F. IMMUTABILITY & OBSERVATION BOUNDARY
// ============================================================================

#[test]
fn test_10_1_17_pipeline_is_pure_observation_and_does_not_mutate_world() {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    chunk.set_voxel(4, 4, 4, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    // Initial state check
    let block_before = store.get_voxel_world_checked(IVec3::new(4, 4, 4)).unwrap();
    assert_eq!(block_before.material, MaterialId::STONE);

    // Create and execute impact pipeline right at the voxel location
    let mut pipeline = DeterministicImpactPipeline::new();
    let impact = ImpactEvent::builder(ImpactId(1), Vec3::new(2.0, 2.0, 2.0), 1.0)
        .energy(1_000_000.0)
        .impulse(50_000.0)
        .build()
        .unwrap();

    pipeline.submit(impact);
    let processed = pipeline.process();
    let affected_chunks = pipeline.query_affected_chunks();

    assert_eq!(processed.len(), 1);
    assert_eq!(affected_chunks, vec![IVec3::ZERO]);

    // Authoritative voxel state must remain 100% UNTOUCHED!
    let block_after = store.get_voxel_world_checked(IVec3::new(4, 4, 4)).unwrap();
    assert_eq!(block_after.material, MaterialId::STONE);
    assert_eq!(block_after, block_before);
    assert_eq!(store.resident_count(), 1);
}
