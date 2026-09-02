use glam::Vec3;
use std::time::Instant;

use omnisia::camera::Camera;
use omnisia::coord::{world_pos_to_world_voxel, world_voxel_to_chunk_and_local};
use omnisia::scale::ScaleRuler;
use omnisia::world::World;
use omnisia::worldgen::seed::WorldSeed;

fn main() {
    println!("============================================================");
    println!("     OMNISIA PHASE 7 REAL-WORLD TRAVERSAL VALIDATION        ");
    println!("============================================================");

    let seed = WorldSeed::default();
    let mut world = World::with_seed(seed);
    let mut camera = Camera::new(Vec3::new(0.0, 35.0, 0.0), -90.0, 0.0);

    println!("[INIT] World initialized with seed: {:?}", seed);
    println!(
        "[INIT] Camera spawned at: ({:.1}m, {:.1}m, {:.1}m)",
        camera.position.x, camera.position.y, camera.position.z
    );
    println!(
        "[INIT] Active Speed Preset: {} ({:.1} m/s)",
        camera.active_preset.name(),
        camera.speed
    );
    println!("[INIT] {}\n", ScaleRuler::ruler_summary());

    // 1. Initial warm-up at spawn (50 frames to populate spawn area)
    println!("--- STAGE 0: Spawn Warm-up (Origin 0.0m) ---");
    let dt = 1.0 / 60.0;
    for _ in 0..60 {
        world.update(camera.position, dt, None);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    report_stage_telemetry("Spawn Area (0m)", &camera, &world);

    // Traversal targets in meters
    let checkpoints = [
        (
            "Traverse +100m (Forest/Plains)",
            Vec3::new(100.0, 35.0, 0.0),
        ),
        (
            "Traverse +250m (Hillside/River)",
            Vec3::new(250.0, 38.0, 50.0),
        ),
        (
            "Traverse +500m (Mountain/Valley)",
            Vec3::new(500.0, 45.0, 100.0),
        ),
        (
            "Traverse +1,000m (1km Deep Traversal)",
            Vec3::new(1000.0, 50.0, 200.0),
        ),
        (
            "Traverse Negative Transition (-100m, crossing x=-1, -32, -33)",
            Vec3::new(-100.0, 35.0, -50.0),
        ),
        (
            "Traverse -250m (Negative Basin)",
            Vec3::new(-250.0, 32.0, -100.0),
        ),
        (
            "Traverse -500m (Negative Mountain)",
            Vec3::new(-500.0, 48.0, -200.0),
        ),
        (
            "Traverse -1,000m (-1km Negative Outpost)",
            Vec3::new(-1000.0, 52.0, -300.0),
        ),
        ("Return Toward Origin (0.0m)", Vec3::new(0.0, 35.0, 0.0)),
    ];

    for (stage_name, target_pos) in &checkpoints {
        println!(
            "\n--- STAGE: {} -> Target: ({:.1}m, {:.1}m, {:.1}m) ---",
            stage_name, target_pos.x, target_pos.y, target_pos.z
        );
        let start_pos = camera.position;
        let total_dist = start_pos.distance(*target_pos);
        let dir = (*target_pos - start_pos).normalize_or_zero();

        // Switch to appropriate speed for realistic developer traversal
        if total_dist > 400.0 {
            camera.set_speed_preset(omnisia::camera::CameraSpeedPreset::Extreme);
        // 500 m/s
        } else if total_dist > 150.0 {
            camera.set_speed_preset(omnisia::camera::CameraSpeedPreset::Fast); // 100 m/s
        } else {
            camera.set_speed_preset(omnisia::camera::CameraSpeedPreset::Normal);
            // 20 m/s
        }

        let stage_start = Instant::now();
        let mut frames = 0;

        while camera.position.distance(*target_pos) > 2.0 && frames < 500 {
            let step = camera.speed * dt;
            let to_target = *target_pos - camera.position;
            if to_target.length() <= step {
                camera.position = *target_pos;
            } else {
                camera.position += dir * step;
            }

            world.update(camera.position, dt, None);
            std::thread::sleep(std::time::Duration::from_millis(4));
            frames += 1;
        }

        // Stabilization frames at arrival
        for _ in 0..20 {
            world.update(camera.position, dt, None);
            std::thread::sleep(std::time::Duration::from_millis(4));
        }

        let elapsed = stage_start.elapsed();
        let fps_est = frames as f64 / elapsed.as_secs_f64();
        println!(
            "  Traversed {:.1}m in {:.2}s ({} frames, ~{:.1} FPS)",
            total_dist,
            elapsed.as_secs_f64(),
            frames,
            fps_est
        );
        report_stage_telemetry(stage_name, &camera, &world);
    }

    println!("\n============================================================");
    println!("      TRAVERSAL VALIDATION & RESIDENCY AUDIT SUCCESS        ");
    println!("============================================================");
}

fn report_stage_telemetry(label: &str, camera: &Camera, world: &World) {
    let cam_vox = world_pos_to_world_voxel(camera.position);
    let (chunk_coord, local_vox) = world_voxel_to_chunk_and_local(cam_vox);
    let mem = world.store.memory_usage(0);

    println!("  [TELEMETRY: {}]", label);
    println!(
        "    Pos (Meters):      ({:.2}m, {:.2}m, {:.2}m)",
        camera.position.x, camera.position.y, camera.position.z
    );
    println!(
        "    Chunk Coord:       ({}, {}, {})",
        chunk_coord.x, chunk_coord.y, chunk_coord.z
    );
    println!(
        "    Local Voxel:       ({}, {}, {})",
        local_vox.x, local_vox.y, local_vox.z
    );
    println!(
        "    Speed Preset:      {} ({:.1} m/s)",
        camera.active_preset.name(),
        camera.speed
    );
    println!(
        "    CPU Resident:      {} chunks",
        world.store.resident_count()
    );
    println!("    Pending Jobs:      {}", world.pending_jobs_count());
    println!("    Upload Backlog:    {}", world.upload_backlog());
    println!("    Memory Usage:      {:.2} MB", mem.total_megabytes());
    println!(
        "    Structural Events: {}",
        world.structure.total_events_processed
    );
    println!(
        "    Pending Checks:    {}",
        world.structure.pending_checks.len()
    );
    println!(
        "    Aggregates Extr:   {}",
        world.structure.total_detached_extracted
    );

    // Assert residency invariants
    assert!(
        world.store.resident_count() <= 850,
        "Resident chunks must stay bounded by retain_radius (max ~800 chunks)! Actual: {}",
        world.store.resident_count()
    );
    assert!(
        world.upload_backlog() <= 100,
        "Upload backlog must not grow unbounded!"
    );
}
