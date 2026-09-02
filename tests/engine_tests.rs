use glam::{IVec3, Vec3};
use omnisia::camera::Camera;
use omnisia::chunk::{dirty_flags, Chunk};
use omnisia::coord::{
    canonical_coords_from_index, canonical_linear_index, world_voxel_to_chunk_and_local,
    CHUNK_SIZE, CHUNK_SIZE_USIZE, CHUNK_VOLUME,
};
use omnisia::material::{MaterialId, MaterialRegistry};
use omnisia::mesh::ao::{ao_to_float, vertex_ao};
use omnisia::mesh::culled::generate_culled_mesh;
use omnisia::mesh::greedy::generate_greedy_mesh;
use omnisia::mesh::types::MeshData;
use omnisia::modding::resource_id::ResourceId;
use omnisia::modding::runtime::ContentRuntime;
use omnisia::storage::{decompress_and_deserialize_chunk, serialize_and_compress_chunk};
use omnisia::voxel::VoxelBlock;

fn get_test_material_registry() -> MaterialRegistry {
    ContentRuntime::build_runtime("content/core", "mods")
        .expect("Core Content harus berhasil dimuat untuk test suite")
        .materials
}

#[test]
fn test_invariant_voxel_block_size() {
    assert_eq!(
        std::mem::size_of::<VoxelBlock>(),
        4,
        "INVARIANT 1 FAILED: VoxelBlock harus berukuran tepat 4 bytes"
    );
    assert_eq!(
        std::mem::align_of::<VoxelBlock>(),
        2,
        "VoxelBlock alignment harus 2 bytes"
    );
}

#[test]
fn test_invariant_chunk_size_and_volume() {
    assert_eq!(CHUNK_SIZE, 32);
    assert_eq!(CHUNK_VOLUME, 32768);

    let chunk = Chunk::new(IVec3::ZERO);
    assert_eq!(
        std::mem::size_of_val(&*chunk.voxels),
        131072, // 128 KiB
        "INVARIANT 3 FAILED: Chunk memory storage harus 128 KiB flat contiguous"
    );
}

#[test]
fn test_canonical_indexing_roundtrip() {
    for z in 0..CHUNK_SIZE_USIZE {
        for y in 0..CHUNK_SIZE_USIZE {
            for x in 0..CHUNK_SIZE_USIZE {
                let idx = canonical_linear_index(x, y, z);
                let (rx, ry, rz) = canonical_coords_from_index(idx);
                assert_eq!((x, y, z), (rx, ry, rz));
            }
        }
    }
}

#[test]
fn test_negative_coordinates_correctness() {
    let test_cases = [
        // (World coordinate, Expected Chunk coordinate, Expected Local coordinate)
        (0, 0, 0),
        (1, 0, 1),
        (31, 0, 31),
        (32, 1, 0),
        (33, 1, 1),
        (-1, -1, 31),
        (-2, -1, 30),
        (-31, -1, 1),
        (-32, -1, 0),
        (-33, -2, 31),
    ];

    for (world_val, exp_chunk, exp_local) in test_cases {
        // Test pada sumbu X
        let (cx, lx) = world_voxel_to_chunk_and_local(IVec3::new(world_val, 0, 0));
        assert_eq!(
            cx.x, exp_chunk,
            "Sumbu X: Chunk salah untuk world {}",
            world_val
        );
        assert_eq!(
            lx.x, exp_local,
            "Sumbu X: Local salah untuk world {}",
            world_val
        );

        // Test pada sumbu Y
        let (cy, ly) = world_voxel_to_chunk_and_local(IVec3::new(0, world_val, 0));
        assert_eq!(
            cy.y, exp_chunk,
            "Sumbu Y: Chunk salah untuk world {}",
            world_val
        );
        assert_eq!(
            ly.y, exp_local,
            "Sumbu Y: Local salah untuk world {}",
            world_val
        );

        // Test pada sumbu Z
        let (cz, lz) = world_voxel_to_chunk_and_local(IVec3::new(0, 0, world_val));
        assert_eq!(
            cz.z, exp_chunk,
            "Sumbu Z: Chunk salah untuk world {}",
            world_val
        );
        assert_eq!(
            lz.z, exp_local,
            "Sumbu Z: Local salah untuk world {}",
            world_val
        );
    }
}

#[test]
fn test_chunk_mutation_and_non_air_count() {
    let mut chunk = Chunk::new(IVec3::ZERO);
    assert_eq!(chunk.non_air_count, 0);

    // Tambah voxel solid
    chunk.set_voxel(0, 0, 0, VoxelBlock::new(MaterialId::STONE));
    assert_eq!(chunk.non_air_count, 1);
    assert!(chunk.is_dirty(dirty_flags::MESH_DIRTY));
    assert!(chunk.is_dirty(dirty_flags::SAVE_DIRTY));

    // Timpa dengan voxel solid lain
    chunk.set_voxel(0, 0, 0, VoxelBlock::new(MaterialId::DIRT));
    assert_eq!(chunk.non_air_count, 1);

    // Ganti kembali menjadi udara
    chunk.set_voxel(0, 0, 0, VoxelBlock::AIR);
    assert_eq!(chunk.non_air_count, 0);
}

#[test]
fn test_material_registry() {
    let registry = get_test_material_registry();
    let stone_res = ResourceId::core("stone").unwrap();
    let dirt_res = ResourceId::core("dirt").unwrap();

    let stone_id = registry
        .resolve_material_id(&stone_res)
        .expect("Material core:stone harus terdaftar");
    let dirt_id = registry
        .resolve_material_id(&dirt_res)
        .expect("Material core:dirt harus terdaftar");

    assert_ne!(stone_id, dirt_id);
    assert_eq!(registry.resolve_resource_id(stone_id), Some(&stone_res));

    let dirt_def = registry.get(dirt_id).unwrap();
    assert_eq!(dirt_def.name, "Dirt");
    assert!(dirt_def.is_solid);
}

#[test]
fn test_ao_calculation() {
    assert_eq!(vertex_ao(false, false, false), 3);
    assert_eq!(vertex_ao(true, false, false), 2);
    assert_eq!(vertex_ao(false, true, false), 2);
    assert_eq!(vertex_ao(true, true, false), 0);
    assert_eq!(vertex_ao(false, false, true), 2);
    assert_eq!(vertex_ao(true, true, true), 0);

    assert_eq!(ao_to_float(0), 0.25);
    assert_eq!(ao_to_float(1), 0.50);
    assert_eq!(ao_to_float(2), 0.75);
    assert_eq!(ao_to_float(3), 1.00);
}

#[test]
fn test_culled_mesher_single_voxel() {
    let mut chunk = Chunk::new(IVec3::ZERO);
    let registry = get_test_material_registry();
    let mut mesh = MeshData::new();

    // 1 Voxel di tengah chunk harus menghasilkan tepat 6 sisi (24 vertex, 36 index)
    chunk.set_voxel(16, 16, 16, VoxelBlock::new(MaterialId::STONE));
    generate_culled_mesh(&chunk, &registry, &mut mesh);

    assert_eq!(mesh.vertex_count(), 24);
    assert_eq!(mesh.index_count(), 36);
    assert_eq!(mesh.quad_count(), 6);
}

#[test]
fn test_greedy_mesher_optimization() {
    let mut chunk = Chunk::new(IVec3::ZERO);
    let registry = get_test_material_registry();
    let mut culled_mesh = MeshData::new();
    let mut greedy_mesh = MeshData::new();

    // Buat plat datar 10x10 voxel
    for z in 5..15 {
        for x in 5..15 {
            chunk.set_voxel(x, 10, z, VoxelBlock::new(MaterialId::STONE));
        }
    }

    generate_culled_mesh(&chunk, &registry, &mut culled_mesh);
    generate_greedy_mesh(&chunk, &registry, &mut greedy_mesh);

    // Greedy meshing harus menghasilkan jumlah vertex dan index yang jauh lebih sedikit dibanding culled meshing
    assert!(
        greedy_mesh.quad_count() < culled_mesh.quad_count(),
        "Greedy meshing harus mereduksi quad count: Greedy ({}) < Culled ({})",
        greedy_mesh.quad_count(),
        culled_mesh.quad_count()
    );
}

#[test]
fn test_greedy_mesher_chunk_boundary_continuity() {
    let registry = get_test_material_registry();

    // Buat dua chunk berdampingan: Chunk A di (0, 0, 0) dan Chunk B di (1, 0, 0)
    let mut chunk_a = Chunk::new(IVec3::new(0, 0, 0));
    let mut chunk_b = Chunk::new(IVec3::new(1, 0, 0));

    // Isi lapisan solid di sepanjang perbatasan (x=31 di A, x=0 di B)
    for z in 0..32 {
        for y in 0..10 {
            chunk_a.set_voxel(31, y, z, VoxelBlock::new(MaterialId::STONE));
            chunk_b.set_voxel(0, y, z, VoxelBlock::new(MaterialId::STONE));
        }
    }

    // Urutan A lalu B
    let mut mesh_a1 = MeshData::new();
    let mut mesh_b1 = MeshData::new();
    generate_greedy_mesh(&chunk_a, &registry, &mut mesh_a1);
    generate_greedy_mesh(&chunk_b, &registry, &mut mesh_b1);

    // Urutan B lalu A
    let mut mesh_b2 = MeshData::new();
    let mut mesh_a2 = MeshData::new();
    generate_greedy_mesh(&chunk_b, &registry, &mut mesh_b2);
    generate_greedy_mesh(&chunk_a, &registry, &mut mesh_a2);

    assert_eq!(
        mesh_a1.vertex_count(),
        mesh_a2.vertex_count(),
        "Generasi mesh Chunk A harus identik bebas urutan!"
    );
    assert_eq!(
        mesh_b1.vertex_count(),
        mesh_b2.vertex_count(),
        "Generasi mesh Chunk B harus identik bebas urutan!"
    );
    assert_eq!(mesh_a1.quad_count(), mesh_a2.quad_count());
    assert_eq!(mesh_b1.quad_count(), mesh_b2.quad_count());
}

#[test]
fn test_frustum_extraction_and_culling() {
    let camera = Camera::new(
        Vec3::new(0.0, 0.0, 0.0),
        -90.0, // Menghadap lurus ke arah -Z
        0.0,
    );

    let aspect = 16.0 / 9.0;
    let frustum = camera.extract_frustum(aspect);

    // 1. Chunk di depan kamera pada arah -Z (misal: (0, 0, -2) -> world Z: -32..-16) -> Harus Visible
    let chunk_in_front = IVec3::new(0, 0, -2);
    assert!(
        frustum.intersects_chunk(chunk_in_front),
        "Chunk di depan kamera harus visible!"
    );

    // 2. Chunk di belakang kamera pada arah +Z (misal: (0, 0, 2) -> world Z: 32..48) -> Harus Culled
    let chunk_behind = IVec3::new(0, 0, 2);
    assert!(
        !frustum.intersects_chunk(chunk_behind),
        "Chunk di belakang kamera harus ter-cull!"
    );

    // 3. Chunk jauh di samping kanan (+X = 100) -> Harus Culled
    let chunk_far_right = IVec3::new(100, 0, -2);
    assert!(
        !frustum.intersects_chunk(chunk_far_right),
        "Chunk jauh di samping harus ter-cull!"
    );
}

#[test]
fn test_frustum_negative_coordinates_and_zero_allocation() {
    let camera = Camera::new(
        Vec3::new(-50.0, 10.0, -50.0),
        -135.0, // Menghadap ke arah kuadran negatif (-X, -Z)
        -10.0,
    );

    let aspect = 16.0 / 9.0;
    let frustum = camera.extract_frustum(aspect);

    // Chunk di area negatif (-5, 0, -5) -> world bounds [-80..-64, 0..16, -80..-64]
    let chunk_neg_front = IVec3::new(-5, 0, -5);
    assert!(
        frustum.intersects_chunk(chunk_neg_front),
        "Chunk pada koordinat negatif di depan kamera harus visible!"
    );

    // Chunk di area positif (5, 0, 5) -> berada di belakang kamera
    let chunk_pos_behind = IVec3::new(5, 0, 5);
    assert!(
        !frustum.intersects_chunk(chunk_pos_behind),
        "Chunk di belakang kamera pada koordinat positif harus ter-cull!"
    );
}

#[test]
fn test_zstd_chunk_persistence_roundtrip() {
    let registry = get_test_material_registry();
    let mut chunk = Chunk::new(IVec3::new(3, -2, 7));
    chunk.set_voxel(0, 0, 0, VoxelBlock::new(MaterialId::STONE));
    chunk.set_voxel(10, 15, 20, VoxelBlock::new(MaterialId::GOLD_ACCENT));
    chunk.set_voxel(31, 31, 31, VoxelBlock::new(MaterialId::AG_CORE_CASING));

    let compressed = serialize_and_compress_chunk(&chunk, &registry).expect("Kompresi gagal");
    assert!(!compressed.is_empty());

    let loaded_chunk =
        decompress_and_deserialize_chunk(&compressed, &registry).expect("Dekompresi gagal");

    assert_eq!(loaded_chunk.position, chunk.position);
    assert_eq!(loaded_chunk.non_air_count, chunk.non_air_count);
    assert_eq!(loaded_chunk.get_voxel(0, 0, 0), chunk.get_voxel(0, 0, 0));
    assert_eq!(
        loaded_chunk.get_voxel(10, 15, 20),
        chunk.get_voxel(10, 15, 20)
    );
    assert_eq!(
        loaded_chunk.get_voxel(31, 31, 31),
        chunk.get_voxel(31, 31, 31)
    );
}
