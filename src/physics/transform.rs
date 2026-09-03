use glam::{Quat, Vec3};

use super::shape::ShapeError;

/// Representasi transformasi spasial kaku 3D (posisi dan orientasi rotasi).
///
/// INVARIAN ARSITEKTURAL:
/// - Mengikuti kebijakan kanonikal Phase 9.2:
///   `rotation` wajib bernilai finite, memiliki panjang tak-nol (`length_squared() > 1e-8`),
///   dan selalu disimpan dalam bentuk ternormalisasi (`rotation.normalize()`).
/// - `position` wajib bernilai finite.
/// - Tidak ada komponen skala (scale-free physics transform).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub position: Vec3,
    pub rotation: Quat,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// Transformasi identitas (posisi (0,0,0) dan rotasi identitas).
    pub const IDENTITY: Self = Self {
        position: Vec3::ZERO,
        rotation: Quat::IDENTITY,
    };

    /// Membuat Transform baru dari posisi dan rotasi dengan validasi ketat.
    pub fn new(position: Vec3, rotation: Quat) -> Result<Self, ShapeError> {
        if !position.is_finite() {
            return Err(ShapeError::NonFiniteCoordinates);
        }
        if !rotation.is_finite() || rotation.length_squared() < 1e-8 {
            return Err(ShapeError::InvalidTransform);
        }
        Ok(Self {
            position,
            rotation: rotation.normalize(),
        })
    }

    /// Membuat Transform hanya dengan translasi (rotasi identitas).
    pub fn from_translation(position: Vec3) -> Result<Self, ShapeError> {
        if !position.is_finite() {
            return Err(ShapeError::NonFiniteCoordinates);
        }
        Ok(Self {
            position,
            rotation: Quat::IDENTITY,
        })
    }

    /// Membuat Transform hanya dengan rotasi (posisi origin).
    pub fn from_rotation(rotation: Quat) -> Result<Self, ShapeError> {
        if !rotation.is_finite() || rotation.length_squared() < 1e-8 {
            return Err(ShapeError::InvalidTransform);
        }
        Ok(Self {
            position: Vec3::ZERO,
            rotation: rotation.normalize(),
        })
    }

    /// Mentransformasikan sebuah titik dari ruang lokal ke ruang induk:
    /// $$p_{\text{world}} = p_{\text{parent}} + (R_{\text{parent}} \cdot p_{\text{local}})$$
    #[inline(always)]
    pub fn transform_point(&self, local_point: Vec3) -> Vec3 {
        self.position + (self.rotation * local_point)
    }

    /// Mentransformasikan sebuah vektor arah dari ruang lokal ke ruang induk:
    /// $$v_{\text{world}} = R_{\text{parent}} \cdot v_{\text{local}}$$
    #[inline(always)]
    pub fn transform_direction(&self, local_dir: Vec3) -> Vec3 {
        self.rotation * local_dir
    }

    /// Komposisi transformasi: $T_{\text{result}} = T_{\text{self}} \times T_{\text{local}}$
    ///
    /// Posisi gabungan memperhitungkan orientasi rotasi parent:
    /// $p_{\text{world}} = p_{\text{self}} + (R_{\text{self}} \cdot p_{\text{local}})$
    /// $R_{\text{world}} = (R_{\text{self}} \cdot R_{\text{local}}).normalize()$
    pub fn mul_transform(&self, local: &Transform) -> Transform {
        let combined_rotation = (self.rotation * local.rotation).normalize();
        let combined_position = self.position + (self.rotation * local.position);
        Transform {
            position: combined_position,
            rotation: combined_rotation,
        }
    }
}
