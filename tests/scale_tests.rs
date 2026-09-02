use glam::{IVec3, Vec3};

use omnisia::camera::{Camera, CameraSpeedPreset};
use omnisia::coord::{
    chunk_and_local_to_world_voxel, world_pos_to_world_voxel, world_voxel_to_chunk_and_local,
    world_voxel_to_world_pos, CHUNK_WORLD_SIZE,
};
use omnisia::scale::{
    HumanScaleReference, ScaleRuler, VegetationDimensionReport, METERS_PER_VOXEL,
    SCALE_RULER_INTERVALS_METERS, VOXELS_PER_METER,
};
use omnisia::voxel::VOXEL_SIZE;
use omnisia::world::World;
use omnisia::worldgen::seed::WorldSeed;

// ============================================================================
// 1. METRIC SCALE CONVERSIONS & EUCLIDEAN INVARIANTS
// ============================================================================

#[test]
fn test_metric_coordinate_conversions() {
    assert_eq!(VOXEL_SIZE, 0.5, "1 Voxel harus tepat 0.5 meter!");
    assert_eq!(METERS_PER_VOXEL, 0.5);
    assert_eq!(VOXELS_PER_METER, 2.0);
    assert_eq!(CHUNK_WORLD_SIZE, 16.0, "1 Chunk harus tepat 16.0 meter!");

    // Konversi voxel ke meter
    assert_eq!(ScaleRuler::voxels_to_meters(0.0), 0.0);
    assert_eq!(ScaleRuler::voxels_to_meters(1.0), 0.5);
    assert_eq!(ScaleRuler::voxels_to_meters(2.0), 1.0);
    assert_eq!(ScaleRuler::voxels_to_meters(32.0), 16.0);
    assert_eq!(ScaleRuler::voxels_to_meters(64.0), 32.0);

    // Konversi meter ke voxel
    assert_eq!(ScaleRuler::meters_to_voxels(1.0), 2.0);
    assert_eq!(ScaleRuler::meters_to_voxels(5.0), 10.0);
    assert_eq!(ScaleRuler::meters_to_voxels(16.0), 32.0);
    assert_eq!(ScaleRuler::meters_to_voxels(100.0), 200.0);

    // Konversi world_pos (meter) ke world_voxel
    assert_eq!(
        world_pos_to_world_voxel(Vec3::new(0.0, 0.0, 0.0)),
        IVec3::ZERO
    );
    assert_eq!(
        world_pos_to_world_voxel(Vec3::new(0.5, 1.0, 16.0)),
        IVec3::new(1, 2, 32)
    );
}

#[test]
fn test_negative_coordinates_metric_boundaries() {
    // Pengujian batas kritis koordinat negatif: x = -1, -32, -33
    // x = -1  => chunk -1, local 31
    let (chunk_m1, local_m1) = world_voxel_to_chunk_and_local(IVec3::new(-1, 0, 0));
    assert_eq!(chunk_m1.x, -1);
    assert_eq!(local_m1.x, 31);
    assert_eq!(
        world_voxel_to_world_pos(IVec3::new(-1, 0, 0)),
        Vec3::new(-0.5, 0.0, 0.0)
    );

    // x = -32 => chunk -1, local 0
    let (chunk_m32, local_m32) = world_voxel_to_chunk_and_local(IVec3::new(-32, 0, 0));
    assert_eq!(chunk_m32.x, -1);
    assert_eq!(local_m32.x, 0);
    assert_eq!(
        world_voxel_to_world_pos(IVec3::new(-32, 0, 0)),
        Vec3::new(-16.0, 0.0, 0.0)
    );

    // x = -33 => chunk -2, local 31
    let (chunk_m33, local_m33) = world_voxel_to_chunk_and_local(IVec3::new(-33, 0, 0));
    assert_eq!(chunk_m33.x, -2);
    assert_eq!(local_m33.x, 31);
    assert_eq!(
        world_voxel_to_world_pos(IVec3::new(-33, 0, 0)),
        Vec3::new(-16.5, 0.0, 0.0)
    );

    // Roundtrip rekonsiliasi
    let reconstructed_m33 = chunk_and_local_to_world_voxel(chunk_m33, local_m33);
    assert_eq!(reconstructed_m33, IVec3::new(-33, 0, 0));
}

// ============================================================================
// 2. CAMERA SPEED PRESETS & FRAME-RATE INVARIANCE (METER/SECOND)
// ============================================================================

#[test]
fn test_camera_speed_presets_meter_per_second() {
    let mut camera = Camera::new(Vec3::ZERO, 0.0, 0.0);

    // Default: Normal 20 m/s
    assert_eq!(camera.active_preset, CameraSpeedPreset::Normal);
    assert_eq!(camera.speed, 20.0);

    camera.set_speed_preset(CameraSpeedPreset::Slow);
    assert_eq!(camera.speed, 5.0);

    camera.set_speed_preset(CameraSpeedPreset::Fast);
    assert_eq!(camera.speed, 100.0);

    camera.set_speed_preset(CameraSpeedPreset::Extreme);
    assert_eq!(camera.speed, 500.0);
}

#[test]
fn test_camera_movement_frame_rate_invariance() {
    // Buktikan bahwa pergerakan 1 detik pada 60 FPS menghasilkan jarak yang SAMA PERSIS
    // dengan 1 detik pada 120 FPS atau 30 FPS (Guardrail 1: fisik m/s sesungguhnya)
    let speed = 20.0; // 20 m/s

    // Simulasi 60 FPS: 60 frame x (1/60)s
    let mut cam60 = Camera::new(Vec3::ZERO, 0.0, 0.0);
    cam60.speed = speed;
    let dt60 = 1.0 / 60.0;
    for _ in 0..60 {
        // Gerak maju (+X)
        cam60.position += cam60.forward() * (cam60.speed * dt60);
    }

    // Simulasi 120 FPS: 120 frame x (1/120)s
    let mut cam120 = Camera::new(Vec3::ZERO, 0.0, 0.0);
    cam120.speed = speed;
    let dt120 = 1.0 / 120.0;
    for _ in 0..120 {
        cam120.position += cam120.forward() * (cam120.speed * dt120);
    }

    let dist60 = cam60.position.length();
    let dist120 = cam120.position.length();

    assert!(
        (dist60 - 20.0).abs() < 1e-4,
        "Jarak 1 detik pada 60 FPS harus 20 meter! Hasil: {}",
        dist60
    );
    assert!(
        (dist120 - 20.0).abs() < 1e-4,
        "Jarak 1 detik pada 120 FPS harus 20 meter! Hasil: {}",
        dist120
    );
    assert!(
        (dist60 - dist120).abs() < 1e-5,
        "Kecepatan gerak harus invarian terhadap variasi FPS!"
    );
}

// ============================================================================
// 3. SCALE RULER & HUMAN SCALE REFERENCE
// ============================================================================

#[test]
fn test_scale_ruler_intervals_and_human_reference() {
    // Pastikan interval standar Scale Ruler: 1m, 2m, 5m, 10m, 25m, 50m, 100m
    let expected_intervals = [1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0];
    assert_eq!(SCALE_RULER_INTERVALS_METERS, expected_intervals);

    let expected_voxels = [2.0, 4.0, 10.0, 20.0, 50.0, 100.0, 200.0];
    for (i, &meters) in SCALE_RULER_INTERVALS_METERS.iter().enumerate() {
        assert_eq!(
            ScaleRuler::meters_to_voxels(meters),
            expected_voxels[i],
            "Interval {}m harus tepat {} voxel",
            meters,
            expected_voxels[i]
        );
    }

    // Referensi manusia ~1.8m
    let human = HumanScaleReference::default();
    assert_eq!(human.height_meters, 1.8);
    assert_eq!(human.width_meters, 0.6);
    assert_eq!(human.height_voxels, 3.6);
    assert_eq!(human.width_voxels, 1.2);
}

#[test]
fn test_vegetation_dimension_reporting() {
    // Validasi dimensi vegetasi aktual terhadap kisaran ekologis
    let oak = VegetationDimensionReport::measure_oak(5, 2);
    assert!(
        oak.is_ecologically_valid,
        "Pohon Oak dengan batang 2.5m dan kanopi 1.0m harus valid secara ekologis (total {:.1}m)!",
        oak.total_height_meters
    );
    assert_eq!(oak.trunk_height_meters, 2.5);

    let pine = VegetationDimensionReport::measure_pine(7, 2);
    assert!(
        pine.is_ecologically_valid,
        "Pohon Pine dengan batang 3.5m dan pucuk 2.0m harus valid secara ekologis (total {:.1}m)!",
        pine.total_height_meters
    );
    assert_eq!(pine.trunk_height_meters, 3.5);
}

// ============================================================================
// 4. STREAMING SEMANTICS & RESIDENCY AUDIT
// ============================================================================

#[test]
fn test_streaming_semantics_radius_consistency() {
    let world = World::with_seed(WorldSeed::default());

    // Invariant: simulation_radius < render_radius < retain_radius
    assert!(
        world.simulation_radius < world.render_radius,
        "Simulation radius ({}) harus lebih kecil dari render radius ({})!",
        world.simulation_radius,
        world.render_radius
    );
    assert!(
        world.render_radius < world.retain_radius,
        "Render radius ({}) harus lebih kecil dari retain radius ({})!",
        world.render_radius,
        world.retain_radius
    );

    // Verifikasi radius aktual:
    // simulation_radius = 3 (48m radius)
    // render_radius = 5 (80m radius)
    // retain_radius = 7 (112m buffer radius)
    assert_eq!(world.simulation_radius, 3);
    assert_eq!(world.render_radius, 5);
    assert_eq!(world.retain_radius, 7);
}
