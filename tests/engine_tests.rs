use glam::IVec3;
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
use omnisia::storage::{decompress_and_deserialize_chunk, serialize_and_compress_chunk};
use omnisia::voxel::VoxelBlock;

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
    let mut chunk = Chunk::new(IVec3::new(0, 0, 0));
    assert_eq!(chunk.non_air_count, 0);
    assert!(chunk.is_empty());

    // Set 1 solid block
    chunk.set_voxel(5, 5, 5, VoxelBlock::new(MaterialId::STONE));
    assert_eq!(chunk.non_air_count, 1);
    assert!(!chunk.is_empty());
    assert!(chunk.is_dirty(dirty_flags::VOXEL_DIRTY));
    assert!(chunk.is_dirty(dirty_flags::MESH_DIRTY));

    // Overwrite dengan block lain
    chunk.set_voxel(5, 5, 5, VoxelBlock::new(MaterialId::DIRT));
    assert_eq!(chunk.non_air_count, 1);

    // Set kembali ke Air
    chunk.set_voxel(5, 5, 5, VoxelBlock::AIR);
    assert_eq!(chunk.non_air_count, 0);
    assert!(chunk.is_empty());

    // Test fill
    chunk.fill_material(MaterialId::STONE);
    assert_eq!(chunk.non_air_count, 32768);
    assert!(chunk.is_full());
}

#[test]
fn test_material_registry() {
    let registry = MaterialRegistry::with_builtin_materials();
    assert!(registry.len() >= 10);

    let stone = registry.get(MaterialId::STONE).unwrap();
    assert_eq!(stone.name, "Stone");
    assert!(stone.is_solid);
    assert!(!stone.is_transparent);

    let air = registry.get(MaterialId::AIR).unwrap();
    assert!(!air.is_solid);
    assert!(air.is_transparent);
}

#[test]
fn test_ao_calculation() {
    assert_eq!(vertex_ao(true, true, true), 0); // Corner tertutup penuh
    assert_eq!(vertex_ao(true, true, false), 0);
    assert_eq!(vertex_ao(false, false, false), 3); // Terbuka penuh
    assert_eq!(vertex_ao(true, false, false), 2);
    assert_eq!(vertex_ao(false, false, true), 2);

    assert_eq!(ao_to_float(0), 0.25);
    assert_eq!(ao_to_float(3), 1.00);
}

#[test]
fn test_culled_mesher_single_voxel() {
    let mut chunk = Chunk::new(IVec3::ZERO);
    let registry = MaterialRegistry::with_builtin_materials();
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
    let registry = MaterialRegistry::with_builtin_materials();
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
fn test_zstd_chunk_persistence_roundtrip() {
    let mut chunk = Chunk::new(IVec3::new(3, -2, 7));
    chunk.set_voxel(0, 0, 0, VoxelBlock::new(MaterialId::STONE));
    chunk.set_voxel(10, 15, 20, VoxelBlock::new(MaterialId::GOLD_ACCENT));
    chunk.set_voxel(31, 31, 31, VoxelBlock::new(MaterialId::AG_CORE_CASING));

    let compressed = serialize_and_compress_chunk(&chunk).expect("Kompresi gagal");
    assert!(!compressed.is_empty());

    let loaded_chunk = decompress_and_deserialize_chunk(&compressed).expect("Dekompresi gagal");

    assert_eq!(loaded_chunk.position, chunk.position);
    assert_eq!(loaded_chunk.non_air_count, chunk.non_air_count);
    assert_eq!(
        loaded_chunk.get_voxel(0, 0, 0),
        chunk.get_voxel(0, 0, 0)
    );
    assert_eq!(
        loaded_chunk.get_voxel(10, 15, 20),
        chunk.get_voxel(10, 15, 20)
    );
    assert_eq!(
        loaded_chunk.get_voxel(31, 31, 31),
        chunk.get_voxel(31, 31, 31)
    );
}
