use glam::{IVec3, Vec3};

use crate::voxel::VOXEL_SIZE;

/// Konversi dasar skala metrik dunia Omnisia:
/// - 1 voxel = 0.5 meter
/// - 1 chunk = 32 voxel = 16.0 meter
pub const METERS_PER_VOXEL: f32 = VOXEL_SIZE; // 0.5m
pub const VOXELS_PER_METER: f32 = 1.0 / METERS_PER_VOXEL; // 2.0 voxel/m

/// Interval standar penggaris skala (Scale Ruler) dalam meter
pub const SCALE_RULER_INTERVALS_METERS: [f32; 7] = [1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0];

/// Representasi referensi skala manusia (~1.8m)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HumanScaleReference {
    /// Tinggi manusia dalam meter (~1.8m)
    pub height_meters: f32,
    /// Lebar bahu manusia dalam meter (~0.6m)
    pub width_meters: f32,
    /// Tinggi dalam jumlah voxel
    pub height_voxels: f32,
    /// Lebar bahu dalam jumlah voxel
    pub width_voxels: f32,
}

impl Default for HumanScaleReference {
    fn default() -> Self {
        Self {
            height_meters: 1.8,
            width_meters: 0.6,
            height_voxels: 1.8 / METERS_PER_VOXEL, // 3.6 voxel
            width_voxels: 0.6 / METERS_PER_VOXEL,  // 1.2 voxel
        }
    }
}

/// Penggaris skala dan pengukur proporsi fisik dunia
pub struct ScaleRuler;

impl ScaleRuler {
    /// Mengonversi jarak dalam voxel ke meter
    #[inline(always)]
    pub fn voxels_to_meters(voxels: f32) -> f32 {
        voxels * METERS_PER_VOXEL
    }

    /// Mengonversi jarak dalam meter ke voxel
    #[inline(always)]
    pub fn meters_to_voxels(meters: f32) -> f32 {
        meters * VOXELS_PER_METER
    }

    /// Menghitung jarak Euclidean dalam meter antara dua posisi dunia
    #[inline(always)]
    pub fn distance_meters(pos_a: Vec3, pos_b: Vec3) -> f32 {
        pos_a.distance(pos_b)
    }

    /// Menghitung jarak Euclidean dalam meter antara dua koordinat voxel dunia
    #[inline(always)]
    pub fn voxel_distance_meters(vox_a: IVec3, vox_b: IVec3) -> f32 {
        let diff = (vox_a - vox_b).as_vec3();
        diff.length() * METERS_PER_VOXEL
    }

    /// Format ringkasan penggaris skala untuk telemetry/debug display
    pub fn ruler_summary() -> &'static str {
        "Scale Ruler: [1m=2vx, 2m=4vx, 5m=10vx, 10m=20vx, 25m=50vx, 50m=100vx, 100m=200vx] | Human Ref: ~1.8m (3.6vx)"
    }
}

/// Struktur data untuk mendokumentasikan dimensi vegetasi aktual dalam meter
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VegetationDimensionReport {
    pub name: &'static str,
    pub trunk_height_meters: f32,
    pub canopy_radius_meters: f32,
    pub total_height_meters: f32,
    pub expected_range_meters: &'static str,
    pub is_ecologically_valid: bool,
}

impl VegetationDimensionReport {
    /// Mengukur dimensi spesifikasi pohon Oak
    pub fn measure_oak(trunk_voxels: i32, canopy_radius_voxels: i32) -> Self {
        let trunk_m = trunk_voxels as f32 * METERS_PER_VOXEL;
        let canopy_m = canopy_radius_voxels as f32 * METERS_PER_VOXEL;
        let total_m = trunk_m + (canopy_radius_voxels as f32 * METERS_PER_VOXEL * 1.5);
        Self {
            name: "Oak Tree",
            trunk_height_meters: trunk_m,
            canopy_radius_meters: canopy_m,
            total_height_meters: total_m,
            expected_range_meters: "3.5m - 6.0m",
            is_ecologically_valid: (3.0..=7.0).contains(&total_m),
        }
    }

    /// Mengukur dimensi spesifikasi pohon Pine
    pub fn measure_pine(trunk_voxels: i32, canopy_radius_voxels: i32) -> Self {
        let trunk_m = trunk_voxels as f32 * METERS_PER_VOXEL;
        let canopy_m = canopy_radius_voxels as f32 * METERS_PER_VOXEL;
        let total_m = trunk_m + (canopy_radius_voxels as f32 * METERS_PER_VOXEL * 2.0);
        Self {
            name: "Pine Tree",
            trunk_height_meters: trunk_m,
            canopy_radius_meters: canopy_m,
            total_height_meters: total_m,
            expected_range_meters: "5.0m - 9.0m",
            is_ecologically_valid: (4.5..=10.0).contains(&total_m),
        }
    }
}
