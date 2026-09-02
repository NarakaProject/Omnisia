use crate::chunk::Chunk;
use crate::coord::CHUNK_SIZE;
use crate::mesh::types::FaceDirection;

/// Menghitung nilai Ambient Occlusion (0..3) untuk satu sudut vertex.
///
/// Algoritma standard:
/// Jika kedua tetangga orthogonal (side1 & side2) terisi blok padat,
/// sudut tersebut otomatis teroklusi penuh (ao = 0).
/// Jika tidak, `ao = 3 - (side1 as u8 + side2 as u8 + corner as u8)`.
#[inline(always)]
pub fn vertex_ao(side1: bool, side2: bool, corner: bool) -> u8 {
    if side1 && side2 {
        0
    } else {
        3 - (side1 as u8 + side2 as u8 + corner as u8)
    }
}

/// Mengonversi nilai AO diskrit [0..3] ke float [0.25..1.0] untuk fragment shader
#[inline(always)]
pub fn ao_to_float(ao: u8) -> f32 {
    match ao {
        0 => 0.25,
        1 => 0.50,
        2 => 0.75,
        _ => 1.00,
    }
}

/// Helper untuk memeriksa apakah voxel pada koordinat (x, y, z) solid dalam chunk
#[inline(always)]
pub fn is_voxel_solid(chunk: &Chunk, x: i32, y: i32, z: i32) -> bool {
    if (0..CHUNK_SIZE).contains(&x) && (0..CHUNK_SIZE).contains(&y) && (0..CHUNK_SIZE).contains(&z)
    {
        !chunk.get_voxel(x as usize, y as usize, z as usize).is_air()
    } else {
        false // Di luar chunk diasumsikan terbuka (dapat disampling via neighbor chunks)
    }
}

/// Menghitung AO untuk ke-4 vertex dari satu face voxel
pub fn calculate_face_ao(
    chunk: &Chunk,
    x: i32,
    y: i32,
    z: i32,
    direction: FaceDirection,
) -> [f32; 4] {
    let (dx, dy, dz) = direction.offset();
    let (fx, fy, fz) = (x + dx, y + dy, z + dz);

    let (ao0, ao1, ao2, ao3) = match direction {
        FaceDirection::PosY => {
            // Sisi Atas (+Y)
            let s_w = is_voxel_solid(chunk, fx - 1, fy, fz);
            let s_e = is_voxel_solid(chunk, fx + 1, fy, fz);
            let s_s = is_voxel_solid(chunk, fx, fy, fz - 1);
            let s_n = is_voxel_solid(chunk, fx, fy, fz + 1);

            let c_sw = is_voxel_solid(chunk, fx - 1, fy, fz - 1);
            let c_se = is_voxel_solid(chunk, fx + 1, fy, fz - 1);
            let c_nw = is_voxel_solid(chunk, fx - 1, fy, fz + 1);
            let c_ne = is_voxel_solid(chunk, fx + 1, fy, fz + 1);

            (
                vertex_ao(s_w, s_s, c_sw),
                vertex_ao(s_e, s_s, c_se),
                vertex_ao(s_e, s_n, c_ne),
                vertex_ao(s_w, s_n, c_nw),
            )
        }
        FaceDirection::NegY => {
            // Sisi Bawah (-Y)
            let s_w = is_voxel_solid(chunk, fx - 1, fy, fz);
            let s_e = is_voxel_solid(chunk, fx + 1, fy, fz);
            let s_s = is_voxel_solid(chunk, fx, fy, fz - 1);
            let s_n = is_voxel_solid(chunk, fx, fy, fz + 1);

            let c_sw = is_voxel_solid(chunk, fx - 1, fy, fz - 1);
            let c_se = is_voxel_solid(chunk, fx + 1, fy, fz - 1);
            let c_nw = is_voxel_solid(chunk, fx - 1, fy, fz + 1);
            let c_ne = is_voxel_solid(chunk, fx + 1, fy, fz + 1);

            (
                vertex_ao(s_w, s_s, c_sw),
                vertex_ao(s_e, s_s, c_se),
                vertex_ao(s_e, s_n, c_ne),
                vertex_ao(s_w, s_n, c_nw),
            )
        }
        FaceDirection::PosZ => {
            // Sisi Depan (+Z)
            let s_l = is_voxel_solid(chunk, fx - 1, fy, fz);
            let s_r = is_voxel_solid(chunk, fx + 1, fy, fz);
            let s_b = is_voxel_solid(chunk, fx, fy - 1, fz);
            let s_t = is_voxel_solid(chunk, fx, fy + 1, fz);

            let c_bl = is_voxel_solid(chunk, fx - 1, fy - 1, fz);
            let c_br = is_voxel_solid(chunk, fx + 1, fy - 1, fz);
            let c_tl = is_voxel_solid(chunk, fx - 1, fy + 1, fz);
            let c_tr = is_voxel_solid(chunk, fx + 1, fy + 1, fz);

            (
                vertex_ao(s_l, s_b, c_bl),
                vertex_ao(s_r, s_b, c_br),
                vertex_ao(s_r, s_t, c_tr),
                vertex_ao(s_l, s_t, c_tl),
            )
        }
        FaceDirection::NegZ => {
            // Sisi Belakang (-Z)
            let s_l = is_voxel_solid(chunk, fx - 1, fy, fz);
            let s_r = is_voxel_solid(chunk, fx + 1, fy, fz);
            let s_b = is_voxel_solid(chunk, fx, fy - 1, fz);
            let s_t = is_voxel_solid(chunk, fx, fy + 1, fz);

            let c_bl = is_voxel_solid(chunk, fx - 1, fy - 1, fz);
            let c_br = is_voxel_solid(chunk, fx + 1, fy - 1, fz);
            let c_tl = is_voxel_solid(chunk, fx - 1, fy + 1, fz);
            let c_tr = is_voxel_solid(chunk, fx + 1, fy + 1, fz);

            (
                vertex_ao(s_l, s_b, c_bl),
                vertex_ao(s_r, s_b, c_br),
                vertex_ao(s_r, s_t, c_tr),
                vertex_ao(s_l, s_t, c_tl),
            )
        }
        FaceDirection::PosX => {
            // Sisi Kanan (+X)
            let s_b = is_voxel_solid(chunk, fx, fy - 1, fz);
            let s_t = is_voxel_solid(chunk, fx, fy + 1, fz);
            let s_f = is_voxel_solid(chunk, fx, fy, fz - 1);
            let s_k = is_voxel_solid(chunk, fx, fy, fz + 1);

            let c_bf = is_voxel_solid(chunk, fx, fy - 1, fz - 1);
            let c_bk = is_voxel_solid(chunk, fx, fy - 1, fz + 1);
            let c_tf = is_voxel_solid(chunk, fx, fy + 1, fz - 1);
            let c_tk = is_voxel_solid(chunk, fx, fy + 1, fz + 1);

            (
                vertex_ao(s_f, s_b, c_bf),
                vertex_ao(s_k, s_b, c_bk),
                vertex_ao(s_k, s_t, c_tk),
                vertex_ao(s_f, s_t, c_tf),
            )
        }
        FaceDirection::NegX => {
            // Sisi Kiri (-X)
            let s_b = is_voxel_solid(chunk, fx, fy - 1, fz);
            let s_t = is_voxel_solid(chunk, fx, fy + 1, fz);
            let s_f = is_voxel_solid(chunk, fx, fy, fz - 1);
            let s_k = is_voxel_solid(chunk, fx, fy, fz + 1);

            let c_bf = is_voxel_solid(chunk, fx, fy - 1, fz - 1);
            let c_bk = is_voxel_solid(chunk, fx, fy - 1, fz + 1);
            let c_tf = is_voxel_solid(chunk, fx, fy + 1, fz - 1);
            let c_tk = is_voxel_solid(chunk, fx, fy + 1, fz + 1);

            (
                vertex_ao(s_f, s_b, c_bf),
                vertex_ao(s_k, s_b, c_bk),
                vertex_ao(s_k, s_t, c_tk),
                vertex_ao(s_f, s_t, c_tf),
            )
        }
    };

    [
        ao_to_float(ao0),
        ao_to_float(ao1),
        ao_to_float(ao2),
        ao_to_float(ao3),
    ]
}
