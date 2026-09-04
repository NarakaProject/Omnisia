use glam::{IVec3, Vec3};

use crate::coord::{
    world_pos_to_world_voxel, world_voxel_to_chunk_and_local, world_voxel_to_world_pos,
};
use crate::impact::event::{ImpactError, ImpactEvent};
use crate::voxel::VOXEL_SIZE;

/// Representasi daerah spasial berbatas (*bounded affected volume*) yang dihasilkan oleh benturan.
///
/// Tanggung jawab struktur ini murni observasional spasial:
/// "Daerah koordinat dunia, voxel, dan chunk mana saja yang bersinggungan dengan benturan ini?"
///
/// INVARIAN SPASIAL:
/// - Menggunakan semantik pembagian Euclidean (`div_euclid`) dari `crate::coord` untuk menjamin
///   kebenaran mutlak pada koordinat negatif dan perbatasan chunk.
/// - Tidak pernah memindai seluruh dunia; batas chunk dihitung dalam O(1) dari radius benturan.
/// - Tidak memutasi ChunkStore, tidak melakukan CSG, dan tidak memicu pencarian BFS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AffectedVolume {
    /// Titik pusat benturan dalam koordinat dunia (meter)
    pub center: Vec3,
    /// Radius pengaruh benturan dalam meter
    pub radius: f32,
    /// Sudut minimum bounding box dunia dalam meter
    pub world_min: Vec3,
    /// Sudut maksimum bounding box dunia dalam meter
    pub world_max: Vec3,
    /// Koordinat voxel global minimum yang beririsan
    pub min_voxel: IVec3,
    /// Koordinat voxel global maksimum yang beririsan
    pub max_voxel: IVec3,
    /// Koordinat chunk minimum yang beririsan
    pub min_chunk: IVec3,
    /// Koordinat chunk maksimum yang beririsan
    pub max_chunk: IVec3,
}

impl AffectedVolume {
    /// Membuat query volume berbatas dari suatu ImpactEvent.
    pub fn from_event(event: &ImpactEvent) -> Self {
        Self::from_sphere(event.position, event.radius)
            .expect("ImpactEvent yang tervalidasi dijamin memiliki center dan radius terhingga")
    }

    /// Membuat query volume berbatas dari pusat dan radius sembarang.
    pub fn from_sphere(center: Vec3, radius: f32) -> Result<Self, ImpactError> {
        if !center.is_finite() {
            return Err(ImpactError::NonFinitePosition(center));
        }
        if !radius.is_finite() {
            return Err(ImpactError::NonFiniteRadius(radius));
        }
        if radius < 0.0 {
            return Err(ImpactError::NegativeRadius(radius));
        }

        let world_min = center - Vec3::splat(radius);
        let world_max = center + Vec3::splat(radius);

        let min_voxel = world_pos_to_world_voxel(world_min);
        let max_voxel = world_pos_to_world_voxel(world_max);

        let (min_chunk, _) = world_voxel_to_chunk_and_local(min_voxel);
        let (max_chunk, _) = world_voxel_to_chunk_and_local(max_voxel);

        Ok(Self {
            center,
            radius,
            world_min,
            world_max,
            min_voxel,
            max_voxel,
            min_chunk,
            max_chunk,
        })
    }

    /// Memeriksa apakah suatu titik dalam koordinat dunia berada di dalam radius pengaruh benturan.
    #[inline]
    pub fn contains_point(&self, point: Vec3) -> bool {
        self.center.distance_squared(point) <= self.radius * self.radius
    }

    /// Memeriksa apakah titik tengah sebuah voxel berada di dalam radius pengaruh benturan.
    #[inline]
    pub fn contains_voxel_center(&self, voxel: IVec3) -> bool {
        let voxel_center = world_voxel_to_world_pos(voxel) + Vec3::splat(VOXEL_SIZE * 0.5);
        self.contains_point(voxel_center)
    }

    /// Memeriksa apakah kubus voxel (AABB berukuran VOXEL_SIZE) berpotongan (*intersects*)
    /// dengan bola benturan.
    pub fn intersects_voxel(&self, voxel: IVec3) -> bool {
        let b_min = world_voxel_to_world_pos(voxel);
        let b_max = b_min + Vec3::splat(VOXEL_SIZE);

        // Titik terdekat pada AABB terhadap pusat bola benturan
        let closest = Vec3::new(
            self.center.x.clamp(b_min.x, b_max.x),
            self.center.y.clamp(b_min.y, b_max.y),
            self.center.z.clamp(b_min.z, b_max.z),
        );

        self.center.distance_squared(closest) <= self.radius * self.radius
    }

    /// Menghitung jumlah total chunk yang berada dalam rentang bounding box benturan.
    #[inline]
    pub fn chunk_count(&self) -> usize {
        let dx = (self.max_chunk.x - self.min_chunk.x + 1).max(0) as usize;
        let dy = (self.max_chunk.y - self.min_chunk.y + 1).max(0) as usize;
        let dz = (self.max_chunk.z - self.min_chunk.z + 1).max(0) as usize;
        dx * dy * dz
    }

    /// Menghasilkan iterator terurut deterministik (urutan kanonikal Y -> Z -> X)
    /// yang melintasi seluruh koordinat chunk yang berpotensi terdampak.
    pub fn iter_chunks(&self) -> impl Iterator<Item = IVec3> {
        let min_c = self.min_chunk;
        let max_c = self.max_chunk;

        (min_c.y..=max_c.y).flat_map(move |cy| {
            (min_c.z..=max_c.z)
                .flat_map(move |cz| (min_c.x..=max_c.x).map(move |cx| IVec3::new(cx, cy, cz)))
        })
    }

    /// Menghitung jumlah total voxel yang berada dalam rentang bounding box benturan.
    #[inline]
    pub fn voxel_count_bounded(&self) -> u64 {
        let dx = (self.max_voxel.x - self.min_voxel.x + 1).max(0) as u64;
        let dy = (self.max_voxel.y - self.min_voxel.y + 1).max(0) as u64;
        let dz = (self.max_voxel.z - self.min_voxel.z + 1).max(0) as u64;
        dx * dy * dz
    }

    /// Memeriksa apakah volume benturan ini berpotongan dengan volume benturan lain.
    #[inline]
    pub fn intersects_volume(&self, other: &Self) -> bool {
        let sum_r = self.radius + other.radius;
        self.center.distance_squared(other.center) <= sum_r * sum_r
    }
}

impl Eq for AffectedVolume {}

impl PartialOrd for AffectedVolume {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AffectedVolume {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.center
            .x
            .to_bits()
            .cmp(&other.center.x.to_bits())
            .then_with(|| self.center.y.to_bits().cmp(&other.center.y.to_bits()))
            .then_with(|| self.center.z.to_bits().cmp(&other.center.z.to_bits()))
            .then_with(|| self.radius.to_bits().cmp(&other.radius.to_bits()))
            .then_with(|| self.min_voxel.x.cmp(&other.min_voxel.x))
            .then_with(|| self.min_voxel.y.cmp(&other.min_voxel.y))
            .then_with(|| self.min_voxel.z.cmp(&other.min_voxel.z))
    }
}
