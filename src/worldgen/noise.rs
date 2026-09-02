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

#[inline(always)]
fn grad_dot(hash: u64, dx: f32, dz: f32) -> f32 {
    let (gx, gz) = GRADIENTS_2D[(hash & 7) as usize];
    gx * dx + gz * dz
}

#[inline(always)]
fn quintic_smooth(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline(always)]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}

/// Sample 2D Perlin Gradient Noise pada koordinat kontinu $(x, z)$ dalam rentang $[-1.0, 1.0]$
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

    let d00 = grad_dot(h00, dx0, dz0);
    let d10 = grad_dot(h10, dx1, dz0);
    let d01 = grad_dot(h01, dx0, dz1);
    let d11 = grad_dot(h11, dx1, dz1);

    let u = quintic_smooth(dx0);
    let v = quintic_smooth(dz0);

    let nx0 = lerp(d00, d10, u);
    let nx1 = lerp(d01, d11, u);

    lerp(nx0, nx1, v).clamp(-1.0, 1.0)
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
        // Transformasi ridge: 1.0 - |n|
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
