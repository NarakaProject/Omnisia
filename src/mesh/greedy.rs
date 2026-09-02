use glam::Vec3;

use crate::chunk::Chunk;
use crate::coord::{CHUNK_SIZE, CHUNK_SIZE_USIZE, CHUNK_WORLD_SIZE};
use crate::material::{MaterialId, MaterialRegistry};
use crate::mesh::types::{MeshData, VoxelVertex};
use crate::voxel::VOXEL_SIZE;

/// Elemen mask 2D untuk algoritma Greedy Meshing
#[derive(Clone, Copy, PartialEq, Eq, Default)]
struct MaskElement {
    material: MaterialId,
    /// Arah normal (+1 untuk positif, -1 untuk negatif, 0 untuk tidak ada face)
    normal_dir: i8,
}

/// Parameter pembuat quad hasil greedy meshing
pub struct GreedyQuadParams {
    pub origin: Vec3,
    pub pos: [i32; 3],
    pub du: [i32; 3],
    pub dv: [i32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
    pub is_positive_facing: bool,
}

/// Menghasilkan mesh optimal menggunakan algoritma Greedy Meshing.
///
/// Menggabungkan quad coplanar yang bersebelahan dengan material identik
/// menjadi poligon persegi panjang yang lebih besar, secara drastis
/// menurunkan jumlah vertex/index pada GPU (mereduksi draw call & bandwidth).
pub fn generate_greedy_mesh(
    chunk: &Chunk,
    materials: &MaterialRegistry,
    output: &mut MeshData,
) {
    output.clear();

    if chunk.is_empty() {
        return;
    }

    let chunk_origin = Vec3::new(
        chunk.position.x as f32 * CHUNK_WORLD_SIZE,
        chunk.position.y as f32 * CHUNK_WORLD_SIZE,
        chunk.position.z as f32 * CHUNK_WORLD_SIZE,
    );

    // Iterasi 3 sumbu utama: 0 = X, 1 = Y, 2 = Z
    for d in 0..3 {
        let u = (d + 1) % 3;
        let v = (d + 2) % 3;

        let mut x = [0i32; 3];
        let mut q = [0i32; 3];
        q[d] = 1;

        // Buffer mask 2D berukuran 32x32
        let mut mask = [MaskElement::default(); CHUNK_SIZE_USIZE * CHUNK_SIZE_USIZE];

        // Slicing melalui chunk sepanjang sumbu d
        x[d] = -1;
        while x[d] < CHUNK_SIZE {
            // 1. Hitung Mask untuk irisan saat ini
            let mut n = 0;
            x[v] = 0;
            while x[v] < CHUNK_SIZE {
                x[u] = 0;
                while x[u] < CHUNK_SIZE {
                    let block_a = if x[d] >= 0 {
                        chunk.get_voxel(x[0] as usize, x[1] as usize, x[2] as usize)
                    } else {
                        &crate::voxel::VoxelBlock::AIR
                    };

                    let block_b = if x[d] < CHUNK_SIZE - 1 {
                        chunk.get_voxel((x[0] + q[0]) as usize, (x[1] + q[1]) as usize, (x[2] + q[2]) as usize)
                    } else {
                        &crate::voxel::VoxelBlock::AIR
                    };

                    let a_solid = !block_a.is_air();
                    let b_solid = !block_b.is_air();

                    mask[n] = if a_solid == b_solid {
                        MaskElement::default()
                    } else if a_solid {
                        MaskElement {
                            material: block_a.material,
                            normal_dir: 1, // Menghadap +d
                        }
                    } else {
                        MaskElement {
                            material: block_b.material,
                            normal_dir: -1, // Menghadap -d
                        }
                    };

                    n += 1;
                    x[u] += 1;
                }
                x[v] += 1;
            }

            x[d] += 1;

            // 2. Greedy merge mask 2D menjadi quad besar
            n = 0;
            for j in 0..CHUNK_SIZE_USIZE {
                let mut i = 0;
                while i < CHUNK_SIZE_USIZE {
                    let current = mask[n];
                    if current.normal_dir != 0 {
                        // Cari lebar maksimum (w) dengan material dan arah normal yang sama
                        let mut w = 1;
                        while i + w < CHUNK_SIZE_USIZE && mask[n + w] == current {
                            w += 1;
                        }

                        // Cari tinggi maksimum (h) yang dapat diperluas bersama lebar w
                        let mut h = 1;
                        let mut can_extend = true;
                        while j + h < CHUNK_SIZE_USIZE && can_extend {
                            for k in 0..w {
                                if mask[n + k + h * CHUNK_SIZE_USIZE] != current {
                                    can_extend = false;
                                    break;
                                }
                            }
                            if can_extend {
                                h += 1;
                            }
                        }

                        // Buat quad gabungan
                        x[u] = i as i32;
                        x[v] = j as i32;

                        let mut du = [0i32; 3];
                        let mut dv = [0i32; 3];
                        du[u] = w as i32;
                        dv[v] = h as i32;

                        let normal = match (d, current.normal_dir) {
                            (0, 1) => [1.0, 0.0, 0.0],
                            (0, -1) => [-1.0, 0.0, 0.0],
                            (1, 1) => [0.0, 1.0, 0.0],
                            (1, -1) => [0.0, -1.0, 0.0],
                            (2, 1) => [0.0, 0.0, 1.0],
                            (2, -1) => [0.0, 0.0, -1.0],
                            _ => [0.0, 1.0, 0.0],
                        };

                        let color = materials.get_color(current.material);

                        emit_greedy_quad(
                            output,
                            GreedyQuadParams {
                                origin: chunk_origin,
                                pos: x,
                                du,
                                dv,
                                normal,
                                color,
                                is_positive_facing: current.normal_dir > 0,
                            },
                        );

                        // Kosongkan mask yang telah digabungkan
                        for l in 0..h {
                            for k in 0..w {
                                mask[n + k + l * CHUNK_SIZE_USIZE] = MaskElement::default();
                            }
                        }

                        i += w;
                        n += w;
                    } else {
                        i += 1;
                        n += 1;
                    }
                }
            }
        }
    }
}

fn emit_greedy_quad(mesh: &mut MeshData, params: GreedyQuadParams) {
    let s = VOXEL_SIZE;
    let base_idx = mesh.vertices.len() as u32;

    let p0 = [
        params.origin.x + (params.pos[0] as f32) * s,
        params.origin.y + (params.pos[1] as f32) * s,
        params.origin.z + (params.pos[2] as f32) * s,
    ];
    let p1 = [
        params.origin.x + ((params.pos[0] + params.du[0]) as f32) * s,
        params.origin.y + ((params.pos[1] + params.du[1]) as f32) * s,
        params.origin.z + ((params.pos[2] + params.du[2]) as f32) * s,
    ];
    let p2 = [
        params.origin.x + ((params.pos[0] + params.du[0] + params.dv[0]) as f32) * s,
        params.origin.y + ((params.pos[1] + params.du[1] + params.dv[1]) as f32) * s,
        params.origin.z + ((params.pos[2] + params.du[2] + params.dv[2]) as f32) * s,
    ];
    let p3 = [
        params.origin.x + ((params.pos[0] + params.dv[0]) as f32) * s,
        params.origin.y + ((params.pos[1] + params.dv[1]) as f32) * s,
        params.origin.z + ((params.pos[2] + params.dv[2]) as f32) * s,
    ];

    mesh.vertices.push(VoxelVertex::new(p0, params.normal, params.color, 1.0));
    mesh.vertices.push(VoxelVertex::new(p1, params.normal, params.color, 1.0));
    mesh.vertices.push(VoxelVertex::new(p2, params.normal, params.color, 1.0));
    mesh.vertices.push(VoxelVertex::new(p3, params.normal, params.color, 1.0));

    if params.is_positive_facing {
        mesh.indices.push(base_idx);
        mesh.indices.push(base_idx + 1);
        mesh.indices.push(base_idx + 2);

        mesh.indices.push(base_idx);
        mesh.indices.push(base_idx + 2);
        mesh.indices.push(base_idx + 3);
    } else {
        mesh.indices.push(base_idx);
        mesh.indices.push(base_idx + 2);
        mesh.indices.push(base_idx + 1);

        mesh.indices.push(base_idx);
        mesh.indices.push(base_idx + 3);
        mesh.indices.push(base_idx + 2);
    }
}
