use glam::{Mat3, Vec3};
use std::fmt;

use super::aabb::{Aabb, AabbError};
use super::transform::Transform;

/// Kesalahan pembuatan dan evaluasi geometri bentuk fisik.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeError {
    /// Koordinat memuat komponen non-finite (NaN atau Infinity)
    NonFiniteCoordinates,
    /// Radius bola atau kapsul <= 0.0 atau non-finite
    NonPositiveRadius,
    /// Half extents boks memuat komponen <= 0.0 atau non-finite
    InvalidHalfExtents,
    /// Dimensi kapsul tidak valid (radius <= 0 atau half_height < 0)
    InvalidCapsuleDimensions,
    /// Transformasi tidak valid (posisi non-finite atau rotasi non-finite / zero-length)
    InvalidTransform,
    /// Kesalahan konstruksi bounding box AABB
    InvalidAabb(AabbError),
}

impl fmt::Display for ShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteCoordinates => write!(f, "Koordinat memuat nilai non-finite"),
            Self::NonPositiveRadius => write!(f, "Radius harus lebih besar dari nol dan finite"),
            Self::InvalidHalfExtents => {
                write!(f, "Half extents boks harus bernilai positif dan finite")
            }
            Self::InvalidCapsuleDimensions => write!(f, "Dimensi kapsul tidak valid"),
            Self::InvalidTransform => {
                write!(f, "Transformasi tidak valid atau quaternion zero-length")
            }
            Self::InvalidAabb(err) => write!(f, "Kesalahan AABB: {:?}", err),
        }
    }
}

impl std::error::Error for ShapeError {}

impl From<AabbError> for ShapeError {
    fn from(err: AabbError) -> Self {
        Self::InvalidAabb(err)
    }
}

/// Bentuk geometris bola (Sphere).
///
/// Berpusat di origin koordinat lokal (0, 0, 0) dengan radius tertentu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sphere {
    radius: f32,
}

impl Sphere {
    /// Membuat bentuk bola baru.
    ///
    /// Validasi: `radius` harus finite dan bernilai > 0.0.
    pub fn new(radius: f32) -> Result<Self, ShapeError> {
        if !radius.is_finite() || radius <= 0.0 {
            return Err(ShapeError::NonPositiveRadius);
        }
        Ok(Self { radius })
    }

    /// Mengambil nilai radius bola.
    #[inline(always)]
    pub fn radius(&self) -> f32 {
        self.radius
    }

    /// Menghitung bounding box AABB dunia untuk bola.
    ///
    /// Rotasi tidak memengaruhi dimensi bola (invarian rotasi).
    pub fn compute_aabb(&self, transform: &Transform) -> Result<Aabb, ShapeError> {
        if !transform.position.is_finite() {
            return Err(ShapeError::NonFiniteCoordinates);
        }
        let r_vec = Vec3::splat(self.radius);
        let min = transform.position - r_vec;
        let max = transform.position + r_vec;
        Ok(Aabb::try_new(min, max)?)
    }
}

/// Bentuk geometris boks 3D (BoxShape).
///
/// Berpusat di origin koordinat lokal (0, 0, 0) dengan dimensi setengah bentang (`half_extents`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShape {
    half_extents: Vec3,
}

impl BoxShape {
    /// Membuat bentuk boks baru dengan setengah bentang (`half_extents`).
    ///
    /// Validasi: semua komponen x, y, z harus finite dan bernilai > 0.0.
    pub fn new(half_extents: Vec3) -> Result<Self, ShapeError> {
        if !half_extents.is_finite()
            || half_extents.x <= 0.0
            || half_extents.y <= 0.0
            || half_extents.z <= 0.0
        {
            return Err(ShapeError::InvalidHalfExtents);
        }
        Ok(Self { half_extents })
    }

    /// Mengambil setengah bentang boks.
    #[inline(always)]
    pub fn half_extents(&self) -> Vec3 {
        self.half_extents
    }

    /// Menghitung bounding box AABB dunia untuk boks berorientasi rotasi.
    ///
    /// Rumus analitis eksak:
    /// $R = \text{Mat3::from\_quat}(transform.rotation)$
    /// $|R|_{ij} = |R_{ij}|$
    /// $E_{\text{world}} = |R| \cdot E_{\text{local}}$
    pub fn compute_aabb(&self, transform: &Transform) -> Result<Aabb, ShapeError> {
        if !transform.position.is_finite() {
            return Err(ShapeError::NonFiniteCoordinates);
        }
        let rot_mat = Mat3::from_quat(transform.rotation);
        let abs_rot_mat = Mat3::from_cols(
            rot_mat.x_axis.abs(),
            rot_mat.y_axis.abs(),
            rot_mat.z_axis.abs(),
        );
        let world_half_extents = abs_rot_mat * self.half_extents;
        let min = transform.position - world_half_extents;
        let max = transform.position + world_half_extents;
        Ok(Aabb::try_new(min, max)?)
    }
}

/// Bentuk geometris kapsul 3D (Capsule).
///
/// KONVENSI MATEMATIS & SUMBU LOKAL:
/// - Sumbu lokal adalah **Lokal Y** ($+Y$ ke $-Y$).
/// - Segmen silinder tengah memanjang sepanjang lokal Y dari $-half\_height$ hingga $+half\_height$.
/// - Kedua ujung ditutup oleh hemisfer beradius `radius`.
/// - **Total tinggi ujung-ke-ujung (tip-to-tip height)** adalah:
///   $$\text{total\_height} = 2 \times half\_height + 2 \times radius$$
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Capsule {
    radius: f32,
    half_height: f32,
}

impl Capsule {
    /// Membuat bentuk kapsul baru berorientasi sumbu lokal Y.
    ///
    /// Validasi:
    /// - `radius` harus finite dan bernilai > 0.0.
    /// - `half_height` harus finite dan bernilai >= 0.0.
    pub fn new(radius: f32, half_height: f32) -> Result<Self, ShapeError> {
        if !radius.is_finite() || radius <= 0.0 {
            return Err(ShapeError::NonPositiveRadius);
        }
        if !half_height.is_finite() || half_height < 0.0 {
            return Err(ShapeError::InvalidCapsuleDimensions);
        }
        Ok(Self {
            radius,
            half_height,
        })
    }

    /// Mengambil radius kapsul.
    #[inline(always)]
    pub fn radius(&self) -> f32 {
        self.radius
    }

    /// Mengambil setengah tinggi segmen silinder (jarak dari tengah ke pusat tutup hemisfer).
    #[inline(always)]
    pub fn half_height(&self) -> f32 {
        self.half_height
    }

    /// Mengambil total tinggi tip-ke-tip kapsul: $2 \times half\_height + 2 \times radius$.
    #[inline(always)]
    pub fn total_height(&self) -> f32 {
        2.0 * self.half_height + 2.0 * self.radius
    }

    /// Menghitung bounding box AABB dunia analitis eksak untuk kapsul berotasi.
    ///
    /// Sumbu lokal Y dirotasikan ke ruang dunia:
    /// $v = transform.rotation \cdot (0, half\_height, 0)$
    /// Titik ujung segmen garis:
    /// $w_0 = transform.position - v$
    /// $w_1 = transform.position + v$
    /// AABB kapsul adalah Minkowski sum antara AABB segmen garis $[w_0, w_1]$ dan bola beradius $r$:
    /// $\min = \min(w_0, w_1) - r, \quad \max = \max(w_0, w_1) + r$
    pub fn compute_aabb(&self, transform: &Transform) -> Result<Aabb, ShapeError> {
        if !transform.position.is_finite() {
            return Err(ShapeError::NonFiniteCoordinates);
        }
        let local_axis_offset = Vec3::new(0.0, self.half_height, 0.0);
        let world_offset = transform.rotation * local_axis_offset;

        let w0 = transform.position - world_offset;
        let w1 = transform.position + world_offset;

        let seg_min = w0.min(w1);
        let seg_max = w0.max(w1);

        let r_vec = Vec3::splat(self.radius);
        let min = seg_min - r_vec;
        let max = seg_max + r_vec;

        Ok(Aabb::try_new(min, max)?)
    }
}

/// Enum penampung representasi bentuk geometris primitif untuk Phase 9.3.
///
/// INVARIAN:
/// - Shape hanya memuat informasi geometri murni.
/// - Tidak memuat RigidBodyId, ColliderId, transform dunia, kecepatan, massa, atau kontak.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Sphere(Sphere),
    Box(BoxShape),
    Capsule(Capsule),
}

impl Shape {
    /// Menghitung bounding box AABB dunia dari bentuk berdasarkan transformasi dunia yang diberikan.
    pub fn compute_aabb(&self, transform: &Transform) -> Result<Aabb, ShapeError> {
        match self {
            Self::Sphere(s) => s.compute_aabb(transform),
            Self::Box(b) => b.compute_aabb(transform),
            Self::Capsule(c) => c.compute_aabb(transform),
        }
    }
}
