use super::seed::splitmix64;

/// Hashing koordinat 2D integer dengan seed untuk gradien deterministik
#[inline(always)]
pub fn hash2d(x: i32, z: i32, seed: u64) -> u64 {
    let mut h = seed;
    h = h.wrapping_add((x as u64).wrapping_mul(0x9E3779B97F4A7C15));
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    h = h.wrapping_add((z as u64).wrapping_mul(0x94D049BB133111EB));
    splitmix64(h)
}

/// Hashing koordinat 3D integer dengan seed untuk gradien volumetrik deterministik
#[inline(always)]
pub fn hash3d(x: i32, y: i32, z: i32, seed: u64) -> u64 {
    let mut h = seed;
    h = h.wrapping_add((x as u64).wrapping_mul(0x9E3779B97F4A7C15));
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    h = h.wrapping_add((y as u64).wrapping_mul(0x94D049BB133111EB));
    h = (h ^ (h >> 27)).wrapping_mul(0x9E3779B97F4A7C15);
    h = h.wrapping_add((z as u64).wrapping_mul(0xC6BC279692B5C323));
    splitmix64(h)
}

/// Tabel gradien 2D normal (8 arah utama)
const GRADIENTS_2D: [(f32, f32); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (0.70710677, 0.70710677),
    (-0.70710677, 0.70710677),
    (0.70710677, -0.70710677),
    (-0.70710677, -0.70710677),
];

/// Tabel gradien 3D (12 vektor tepi kubus standar Perlin)
const GRADIENTS_3D: [(f32, f32, f32); 12] = [
    (1.0, 1.0, 0.0),
    (-1.0, 1.0, 0.0),
    (1.0, -1.0, 0.0),
    (-1.0, -1.0, 0.0),
    (1.0, 0.0, 1.0),
    (-1.0, 0.0, 1.0),
    (1.0, 0.0, -1.0),
    (-1.0, 0.0, -1.0),
    (0.0, 1.0, 1.0),
    (0.0, -1.0, 1.0),
    (0.0, 1.0, -1.0),
    (0.0, -1.0, -1.0),
];

#[inline(always)]
fn grad_dot_2d(hash: u64, dx: f32, dz: f32) -> f32 {
    let (gx, gz) = GRADIENTS_2D[(hash & 7) as usize];
    gx * dx + gz * dz
}

#[inline(always)]
fn grad_dot_3d(hash: u64, dx: f32, dy: f32, dz: f32) -> f32 {
    let (gx, gy, gz) = GRADIENTS_3D[(hash % 12) as usize];
    gx * dx + gy * dy + gz * dz
}

#[inline(always)]
fn quintic_smooth(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline(always)]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}

/// Sample 2D Gradient Noise pada koordinat kontinu $(x, z)$ dalam rentang $[-1.0, 1.0]$
pub fn sample_gradient_2d(x: f32, z: f32, seed: u64) -> f32 {
    let x0 = x.floor() as i32;
    let z0 = z.floor() as i32;
    let x1 = x0 + 1;
    let z1 = z0 + 1;

    let dx0 = x - x0 as f32;
    let dz0 = z - z0 as f32;
    let dx1 = dx0 - 1.0;
    let dz1 = dz0 - 1.0;

    let h00 = hash2d(x0, z0, seed);
    let h10 = hash2d(x1, z0, seed);
    let h01 = hash2d(x0, z1, seed);
    let h11 = hash2d(x1, z1, seed);

    let d00 = grad_dot_2d(h00, dx0, dz0);
    let d10 = grad_dot_2d(h10, dx1, dz0);
    let d01 = grad_dot_2d(h01, dx0, dz1);
    let d11 = grad_dot_2d(h11, dx1, dz1);

    let u = quintic_smooth(dx0);
    let v = quintic_smooth(dz0);

    let nx0 = lerp(d00, d10, u);
    let nx1 = lerp(d01, d11, u);

    lerp(nx0, nx1, v).clamp(-1.0, 1.0)
}

/// Sample 3D Gradient Noise kontinu pada $(x, y, z)$ dalam rentang $[-1.0, 1.0]$ bebas alokasi memori
pub fn sample_gradient_3d(x: f32, y: f32, z: f32, seed: u64) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let z0 = z.floor() as i32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let z1 = z0 + 1;

    let dx0 = x - x0 as f32;
    let dy0 = y - y0 as f32;
    let dz0 = z - z0 as f32;
    let dx1 = dx0 - 1.0;
    let dy1 = dy0 - 1.0;
    let dz1 = dz0 - 1.0;

    let h000 = hash3d(x0, y0, z0, seed);
    let h100 = hash3d(x1, y0, z0, seed);
    let h010 = hash3d(x0, y1, z0, seed);
    let h110 = hash3d(x1, y1, z0, seed);
    let h001 = hash3d(x0, y0, z1, seed);
    let h101 = hash3d(x1, y0, z1, seed);
    let h011 = hash3d(x0, y1, z1, seed);
    let h111 = hash3d(x1, y1, z1, seed);

    let d000 = grad_dot_3d(h000, dx0, dy0, dz0);
    let d100 = grad_dot_3d(h100, dx1, dy0, dz0);
    let d010 = grad_dot_3d(h010, dx0, dy1, dz0);
    let d110 = grad_dot_3d(h110, dx1, dy1, dz0);
    let d001 = grad_dot_3d(h001, dx0, dy0, dz1);
    let d101 = grad_dot_3d(h101, dx1, dy0, dz1);
    let d011 = grad_dot_3d(h011, dx0, dy1, dz1);
    let d111 = grad_dot_3d(h111, dx1, dy1, dz1);

    let u = quintic_smooth(dx0);
    let v = quintic_smooth(dy0);
    let w = quintic_smooth(dz0);

    let ix00 = lerp(d000, d100, u);
    let ix10 = lerp(d010, d110, u);
    let ix01 = lerp(d001, d101, u);
    let ix11 = lerp(d011, d111, u);

    let iy0 = lerp(ix00, ix10, v);
    let iy1 = lerp(ix01, ix11, v);

    lerp(iy0, iy1, w).clamp(-1.0, 1.0)
}

/// Sample 2D Fractal Brownian Motion (fBm) multi-octave
pub fn sample_fbm_2d(
    x: f32,
    z: f32,
    seed: u64,
    octaves: usize,
    persistence: f32,
    lacunarity: f32,
    scale: f32,
) -> f32 {
    let mut total = 0.0;
    let mut frequency = scale;
    let mut amplitude = 1.0;
    let mut max_amplitude = 0.0;

    for i in 0..octaves {
        let oct_seed = splitmix64(seed.wrapping_add((i as u64).wrapping_mul(7919)));
        let n = sample_gradient_2d(x * frequency, z * frequency, oct_seed);
        total += n * amplitude;
        max_amplitude += amplitude;
        amplitude *= persistence;
        frequency *= lacunarity;
    }

    if max_amplitude > 0.0 {
        (total / max_amplitude).clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

/// Sample 3D Fractal Brownian Motion (fBm) multi-octave
#[allow(clippy::too_many_arguments)]
pub fn sample_fbm_3d(
    x: f32,
    y: f32,
    z: f32,
    seed: u64,
    octaves: usize,
    persistence: f32,
    lacunarity: f32,
    scale: f32,
) -> f32 {
    let mut total = 0.0;
    let mut frequency = scale;
    let mut amplitude = 1.0;
    let mut max_amplitude = 0.0;

    for i in 0..octaves {
        let oct_seed = splitmix64(seed.wrapping_add((i as u64).wrapping_mul(7919)));
        let n = sample_gradient_3d(x * frequency, y * frequency, z * frequency, oct_seed);
        total += n * amplitude;
        max_amplitude += amplitude;
        amplitude *= persistence;
        frequency *= lacunarity;
    }

    if max_amplitude > 0.0 {
        (total / max_amplitude).clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

/// Sample 2D Ridged Multi-Fractal Noise untuk punggungan bukit/gunung terjal
pub fn sample_ridged_2d(
    x: f32,
    z: f32,
    seed: u64,
    octaves: usize,
    persistence: f32,
    lacunarity: f32,
    scale: f32,
) -> f32 {
    let mut total = 0.0;
    let mut frequency = scale;
    let mut amplitude = 1.0;
    let mut max_amplitude = 0.0;

    for i in 0..octaves {
        let oct_seed = splitmix64(seed.wrapping_add((i as u64).wrapping_mul(7919)));
        let n = sample_gradient_2d(x * frequency, z * frequency, oct_seed);
        let ridge = 1.0 - n.abs();
        total += ridge * ridge * amplitude;
        max_amplitude += amplitude;
        amplitude *= persistence;
        frequency *= lacunarity;
    }

    if max_amplitude > 0.0 {
        (total / max_amplitude).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Sample 3D Ridged Noise untuk rongga dan tabung volumetrik
#[allow(clippy::too_many_arguments)]
pub fn sample_ridged_3d(
    x: f32,
    y: f32,
    z: f32,
    seed: u64,
    octaves: usize,
    persistence: f32,
    lacunarity: f32,
    scale: f32,
) -> f32 {
    let mut total = 0.0;
    let mut frequency = scale;
    let mut amplitude = 1.0;
    let mut max_amplitude = 0.0;

    for i in 0..octaves {
        let oct_seed = splitmix64(seed.wrapping_add((i as u64).wrapping_mul(7919)));
        let n = sample_gradient_3d(x * frequency, y * frequency, z * frequency, oct_seed);
        let ridge = 1.0 - n.abs();
        total += ridge * ridge * amplitude;
        max_amplitude += amplitude;
        amplitude *= persistence;
        frequency *= lacunarity;
    }

    if max_amplitude > 0.0 {
        (total / max_amplitude).clamp(0.0, 1.0)
    } else {
        0.0
    }
}
