use glam::{IVec3, Vec3};
use std::time::Instant;

use omnisia::chunk::Chunk;
use omnisia::modding::resource_id::ResourceId;
use omnisia::physics::{DynamicBodyState, PhysicsRuntime};
use omnisia::streaming::store::ChunkStore;
use omnisia::structure::aggregate::DetachedAggregate;
use omnisia::voxel::VoxelBlock;
use omnisia::world::World;
use omnisia::worldgen::seed::WorldSeed;

fn main() {
    println!("============================================================");
    println!("    OMNISIA PHASE 8A — DYNAMIC AGGREGATE RUNTIME VALIDATION  ");
    println!("============================================================");

    let start_time = Instant::now();

    // ========================================================================
    // STAGE 1: INITIATION & ATOMIC TRANSFER
    // ========================================================================
    println!("\n[STAGE 1] Inisialisasi Dunia & Transfer Kepemilikan Atomik...");
    let mut world = World::with_seed(WorldSeed(8888));
    world.store.insert(Chunk::new(IVec3::ZERO));

    let stone_id = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .expect("stone material");
    let wood_id = world
        .materials
        .resolve_material_id(&ResourceId::core("wood_oak").unwrap())
        .expect("wood_oak material");

    // Fondasi batu di (15, 0..=3, 15)
    for y in 0..=3 {
        world
            .store
            .set_voxel_world(IVec3::new(15, y, 15), VoxelBlock::new(stone_id));
    }
    // Tiang batu yang akan diputus di (15, 4, 15)
    world
        .store
        .set_voxel_world(IVec3::new(15, 4, 15), VoxelBlock::new(stone_id));

    // Gugusan kayu 2-voxel di atas tiang (15, 5..=6, 15)
    world
        .store
        .set_voxel_world(IVec3::new(15, 5, 15), VoxelBlock::new(wood_id));
    world
        .store
        .set_voxel_world(IVec3::new(15, 6, 15), VoxelBlock::new(wood_id));

    assert_eq!(world.physics.body_count(), 0);
    println!("  -> Fondasi statis terpasang (5 batu, 2 kayu).");

    // Putuskan tiang batu di y=4
    println!("  -> Memutus tiang di y=4...");
    let detached = world.set_voxel_world(IVec3::new(15, 4, 15), VoxelBlock::AIR);
    assert_eq!(detached.len(), 1, "Harus menghasilkan 1 detached aggregate");
    assert_eq!(detached[0].voxel_count(), 2);

    // Verifikasi invarian kepemilikan tunggal (Amendment 1)
    assert!(
        world.store.get_voxel_world(IVec3::new(15, 5, 15)).is_air(),
        "ChunkStore y=5 harus sudah AIR"
    );
    assert!(
        world.store.get_voxel_world(IVec3::new(15, 6, 15)).is_air(),
        "ChunkStore y=6 harus sudah AIR"
    );
    assert_eq!(world.physics.body_count(), 1);
    assert_eq!(world.physics.total_dynamic_voxels(), 2);
    println!("  [PASS] Transfer kepemilikan atomik terbukti: ChunkStore kosong, DynamicBody memegang 100%!");

    // ========================================================================
    // STAGE 2: 30 HZ FIXED TIMESTEP FALLING & SWEPT VERTICAL COLLISION
    // ========================================================================
    println!("\n[STAGE 2] Simulasi Jatuh 30 Hz & Swept Vertical Collision...");
    let body_id = *world.physics.bodies.keys().next().unwrap();
    let initial_pos = world.physics.get_body(body_id).unwrap().position;
    println!("  -> Posisi awal meter: {:?}", initial_pos);

    // Jalankan 10 frame render (setara ~0.33 detik)
    for _ in 0..10 {
        world.update(Vec3::ZERO, 1.0 / 30.0, None);
    }

    let mid_body = world.physics.get_body(body_id).unwrap();
    println!(
        "  -> Posisi setelah kontak: {:?}, kecepatan: {:?}",
        mid_body.position, mid_body.velocity
    );
    assert_eq!(
        mid_body.position.y, 2.0,
        "Bagian bawah badan harus tertahan tepat di y=4 (2.0m) di atas lantai y=3"
    );
    assert_eq!(mid_body.velocity.y, 0.0);
    assert!(mid_body.is_grounded);
    println!("  [PASS] Swept vertical collision berhasil menahan dan menyelaraskan ke kisi integer voxel!");

    // ========================================================================
    // STAGE 3: SETTLED TRANSITION & TWO-PHASE REINTEGRATION
    // ========================================================================
    println!("\n[STAGE 3] Deteksi Settled & Reintegrasi Statis Dua Fase...");
    // Jalankan tambahan 20 tick agar memenuhi syarat sleep_ticks_required (15 ticks)
    for _ in 0..20 {
        world.update(Vec3::ZERO, 1.0 / 30.0, None);
    }

    assert_eq!(
        world.physics.body_count(),
        0,
        "Badan dinamis harus telah otomatis direintegrasi ke ChunkStore!"
    );
    assert_eq!(world.physics.total_reintegrated, 1);

    // Verifikasi bahwa voxel kayu telah tertulis kembali ke ChunkStore pada posisi barunya
    let rest_bottom = world.store.get_voxel_world(IVec3::new(15, 4, 15));
    let rest_top = world.store.get_voxel_world(IVec3::new(15, 5, 15));
    assert_eq!(rest_bottom.material(), wood_id);
    assert_eq!(rest_top.material(), wood_id);
    println!("  -> Voxel kayu berhasil kembali ke ChunkStore pada y=4 dan y=5!");
    println!("  [PASS] Siklus penuh Static -> Dynamic -> Static terbukti 100% konsisten!");

    // ========================================================================
    // STAGE 4: ANTIGRAVITY FLOATING & CONSERVATIVE SLEEP
    // ========================================================================
    println!("\n[STAGE 4] Validasi Gugusan AntiGravity (gravity_scale = 0.0)...");
    let mut ag_runtime = PhysicsRuntime::default();
    let ag_store = ChunkStore::new();

    let ag_voxels = vec![
        (IVec3::new(0, 10, 0), VoxelBlock::new(stone_id)),
        (IVec3::new(1, 10, 0), VoxelBlock::new(stone_id)),
    ];
    let ag_agg = DetachedAggregate::from_world_voxels(555, &ag_voxels).unwrap();
    let ag_body_id = ag_runtime.spawn_from_detached_aggregate(ag_agg);
    ag_runtime.get_body_mut(ag_body_id).unwrap().gravity_scale = 0.0;

    let ag_initial_pos = ag_runtime.get_body(ag_body_id).unwrap().position;

    // Jalankan 30 ticks
    for _ in 0..30 {
        ag_runtime.tick(1.0 / 30.0, &ag_store);
    }

    let ag_body = ag_runtime.get_body(ag_body_id).unwrap();
    assert_eq!(
        ag_body.position, ag_initial_pos,
        "Posisi tidak boleh berubah!"
    );
    assert_eq!(ag_body.velocity, Vec3::ZERO, "Kecepatan harus nol mutlak!");
    assert_eq!(
        ag_body.state,
        DynamicBodyState::Sleeping,
        "Badan AntiGravity harus Sleeping, BUKAN Settled!"
    );
    assert_eq!(ag_runtime.settled_body_count(), 0);
    println!("  [PASS] AntiGravity terbukti mengapung stabil, Sleeping, dan TIDAK PERNAH Settled!");

    // ========================================================================
    // STAGE 5: UNLOADED CHUNK COLLISION BARRIER
    // ========================================================================
    println!("\n[STAGE 5] Validasi Proteksi Batas Chunk Belum Dimuat (Unknown != Air)...");
    let mut unl_store = ChunkStore::new();
    unl_store.insert(Chunk::new(IVec3::ZERO)); // Hanya chunk Y=0 resident

    let mut unl_runtime = PhysicsRuntime::default();
    let unl_voxels = vec![(IVec3::new(0, 0, 0), VoxelBlock::new(stone_id))];
    let unl_agg = DetachedAggregate::from_world_voxels(777, &unl_voxels).unwrap();
    let unl_body_id = unl_runtime.spawn_from_detached_aggregate(unl_agg);

    // Langkah ke bawah sejauh 2.0 meter
    unl_runtime.tick(1.0 / 30.0, &unl_store);

    let unl_body = unl_runtime.get_body(unl_body_id).unwrap();
    assert_eq!(
        unl_body.position.y, 0.0,
        "Badan harus tertahan di batas chunk (y=0.0m) dan tidak boleh jatuh ke void!"
    );
    println!("  [PASS] Chunk yang belum dimuat terbukti menahan badan dinamis!");

    println!("\n============================================================");
    println!(
        "   ALL 5 VALIDATION STAGES PASSED IN {:.3} ms!             ",
        start_time.elapsed().as_secs_f64() * 1000.0
    );
    println!("============================================================");
}
