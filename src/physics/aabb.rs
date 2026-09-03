use glam::Vec3;
use std::fmt;

/// Kesalahan saat konstruksi AABB yang tidak valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AabbError {
    /// Koordinat minimum lebih besar daripada koordinat maksimum di salah satu sumbu
    MinGreaterThanMax,
    /// Salah satu komponen bernilai non-finite (NaN atau Infinity)
    NonFiniteCoordinates,
}

impl fmt::Display for AabbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MinGreaterThanMax => write!(f, "AABB min lebih besar dari max"),
            Self::NonFiniteCoordinates => write!(f, "AABB memuat komponen non-finite (NaN/Inf)"),
        }
    }
}

impl std::error::Error for AabbError {}

/// Axis-Aligned Bounding Box (AABB) dalam ruang dunia 3D (satuan meter).
///
/// INVARIAN GEOMETRIS:
/// - `min <= max` pada seluruh sumbu (X, Y, Z).
/// - Seluruh komponen harus bernilai terhingga (finite).
/// - Operasi overlap bersifat inklusif pada batas permukaan (touching boundary overlaps).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    /// Memvalidasi koordinat dan membuat AABB baru tanpa panic.
    pub fn try_new(min: Vec3, max: Vec3) -> Result<Self, AabbError> {
        if !min.is_finite() || !max.is_finite() {
            return Err(AabbError::NonFiniteCoordinates);
        }
        if min.x > max.x || min.y > max.y || min.z > max.z {
            return Err(AabbError::MinGreaterThanMax);
        }
        Ok(Self { min, max })
    }

    /// Konstruktor aman dari dua titik sembarang, otomatis menghitung komponen minimum dan maksimum.
    pub fn from_min_max(p1: Vec3, p2: Vec3) -> Result<Self, AabbError> {
        if !p1.is_finite() || !p2.is_finite() {
            return Err(AabbError::NonFiniteCoordinates);
        }
        Ok(Self {
            min: p1.min(p2),
            max: p1.max(p2),
        })
    }

    /// Konstruktor aman dari titik pusat (*center*) dan setengah ukuran (*half_extents*).
    pub fn from_center_half_extents(center: Vec3, half_extents: Vec3) -> Result<Self, AabbError> {
        if !center.is_finite() || !half_extents.is_finite() {
            return Err(AabbError::NonFiniteCoordinates);
        }
        let half = half_extents.abs();
        Ok(Self {
            min: center - half,
            max: center + half,
        })
    }

    /// Titik pusat dari AABB.
    #[inline(always)]
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Setengah ukuran (*half-extents*) di setiap sumbu (dx/2, dy/2, dz/2).
    #[inline(always)]
    pub fn half_extents(&self) -> Vec3 {
        (self.max - self.min) * 0.5
    }

    /// Ukuran dimensi penuh (*extents*) di setiap sumbu (dx, dy, dz).
    #[inline(always)]
    pub fn extents(&self) -> Vec3 {
        self.max - self.min
    }

    /// Memeriksa apakah suatu titik berada di dalam atau tepat pada batas AABB (inklusif).
    #[inline]
    pub fn contains_point(&self, point: Vec3) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }

    /// Memeriksa apakah AABB ini bertumpukan (*overlap*) dengan AABB lain.
    /// Menggunakan semantik batas inklusif: jika kedua AABB bersentuhan tepat di tepi/muka,
    /// irisan interval dianggap valid bertumpukan.
    #[inline]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
    }

    /// Menghasilkan bounding box union yang melingkupi kedua AABB (self dan other).
    #[inline]
    pub fn union(&self, other: &Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    /// Memperluas atau menyusutkan AABB sebesar `amount` di seluruh arah.
    ///
    /// PERILAKU EKSPANSI & KONTRAKSI:
    /// - Jika `amount >= 0.0`: AABB diperluas ke luar sebesar `amount`.
    /// - Jika `amount < 0.0`: AABB menyusut ke dalam (*contract*).
    ///   Penyusutan dibatasi (*clamped*) pada setengah ukuran dimensi (`half_extents`)
    ///   sehingga AABB menguncup paling banyak ke titik pusatnya dan TIDAK PERNAH membalikkan
    ///   koordinat (`min <= max` selalu terjamin).
    pub fn expand(&self, amount: f32) -> Self {
        if amount >= 0.0 {
            Self {
                min: self.min - Vec3::splat(amount),
                max: self.max + Vec3::splat(amount),
            }
        } else {
            let shrink = -amount;
            let half = self.half_extents();
            let clamped_shrink =
                Vec3::new(shrink.min(half.x), shrink.min(half.y), shrink.min(half.z));
            let center = self.center();
            Self {
                min: center - (half - clamped_shrink),
                max: center + (half - clamped_shrink),
            }
        }
    }

    /// Memvalidasi apakah AABB mematuhi seluruh invarian geometris:
    /// komponen terhingga dan `min <= max`.
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.min.is_finite()
            && self.max.is_finite()
            && self.min.x <= self.max.x
            && self.min.y <= self.max.y
            && self.min.z <= self.max.z
    }
}
