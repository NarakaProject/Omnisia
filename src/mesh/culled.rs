use glam::Vec3;

use crate::chunk::Chunk;
use crate::coord::{CHUNK_SIZE_USIZE, CHUNK_WORLD_SIZE};
use crate::material::MaterialRegistry;
use crate::mesh::ao::{calculate_face_ao, is_voxel_solid};
use crate::mesh::types::{FaceDirection, MeshData, VoxelVertex};
use crate::voxel::VOXEL_SIZE;

/// Menghasilkan mesh 3D menggunakan algoritma Culled Face Meshing dengan kalkulasi AO.
pub fn generate_culled_mesh(chunk: &Chunk, materials: &MaterialRegistry, output: &mut MeshData) {
    output.clear();

    if chunk.is_empty() {
        return;
    }

    let chunk_origin = Vec3::new(
        chunk.position.x as f32 * CHUNK_WORLD_SIZE,
        chunk.position.y as f32 * CHUNK_WORLD_SIZE,
        chunk.position.z as f32 * CHUNK_WORLD_SIZE,
    );

    for z in 0..CHUNK_SIZE_USIZE {
        for y in 0..CHUNK_SIZE_USIZE {
            for x in 0..CHUNK_SIZE_USIZE {
                let block = *chunk.get_voxel(x, y, z);
                if block.is_air() {
                    continue;
                }

                let color = materials.get_color(block.material);
                let voxel_pos = chunk_origin
                    + Vec3::new(
                        x as f32 * VOXEL_SIZE,
                        y as f32 * VOXEL_SIZE,
                        z as f32 * VOXEL_SIZE,
                    );

                for &dir in &FaceDirection::ALL {
                    let (dx, dy, dz) = dir.offset();
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    let nz = z as i32 + dz;

                    // Face hanya dirender jika tetangga adalah udara / transparan
                    let is_neighbor_opaque = is_voxel_solid(chunk, nx, ny, nz);
                    if is_neighbor_opaque {
                        continue;
                    }

                    let normal = dir.normal();
                    let ao = calculate_face_ao(chunk, x as i32, y as i32, z as i32, dir);
                    emit_quad(output, voxel_pos, dir, normal, color, ao);
                }
            }
        }
    }
}

/// Menambahkan 4 vertex dan 2 segitiga untuk satu quad muka voxel
fn emit_quad(
    mesh: &mut MeshData,
    origin: Vec3,
    direction: FaceDirection,
    normal: [f32; 3],
    color: [f32; 3],
    ao: [f32; 4],
) {
    let s = VOXEL_SIZE;

    let corners: [[f32; 3]; 4] = match direction {
        FaceDirection::PosY => [[0.0, s, s], [s, s, s], [s, s, 0.0], [0.0, s, 0.0]],
        FaceDirection::NegY => [[0.0, 0.0, 0.0], [s, 0.0, 0.0], [s, 0.0, s], [0.0, 0.0, s]],
        FaceDirection::PosZ => [[0.0, 0.0, s], [s, 0.0, s], [s, s, s], [0.0, s, s]],
        FaceDirection::NegZ => [[s, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, s, 0.0], [s, s, 0.0]],
        FaceDirection::PosX => [[s, 0.0, s], [s, 0.0, 0.0], [s, s, 0.0], [s, s, s]],
        FaceDirection::NegX => [[0.0, 0.0, 0.0], [0.0, 0.0, s], [0.0, s, s], [0.0, s, 0.0]],
    };

    let base_idx = mesh.vertices.len() as u32;

    for i in 0..4 {
        let pos = [
            origin.x + corners[i][0],
            origin.y + corners[i][1],
            origin.z + corners[i][2],
        ];
        mesh.vertices
            .push(VoxelVertex::new(pos, normal, color, ao[i]));
    }

    if ao[0] + ao[2] > ao[1] + ao[3] {
        mesh.indices.push(base_idx);
        mesh.indices.push(base_idx + 1);
        mesh.indices.push(base_idx + 2);

        mesh.indices.push(base_idx);
        mesh.indices.push(base_idx + 2);
        mesh.indices.push(base_idx + 3);
    } else {
        mesh.indices.push(base_idx + 1);
        mesh.indices.push(base_idx + 2);
        mesh.indices.push(base_idx + 3);

        mesh.indices.push(base_idx + 1);
        mesh.indices.push(base_idx + 3);
        mesh.indices.push(base_idx);
    }
}
