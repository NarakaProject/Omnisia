use super::aabb::Aabb;
use super::broadphase::RigidBodyId;
use super::shape::{Shape, ShapeError};
use super::transform::Transform;

/// Identifier unik runtime untuk sebuah instance Collider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColliderId(pub u64);

/// Kesalahan validasi material fisik permukaan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialError {
    /// Restitusi non-finite atau di luar rentang [0.0, 1.0]
    InvalidRestitution,
    /// Friksi non-finite atau bernilai negatif (< 0.0)
    InvalidFriction,
}

impl std::fmt::Display for MaterialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRestitution => {
                write!(
                    f,
                    "Koefisien restitusi harus finite dan bernilai 0.0 <= e <= 1.0"
                )
            }
            Self::InvalidFriction => {
                write!(f, "Koefisien friksi harus finite dan bernilai >= 0.0")
            }
        }
    }
}

impl std::error::Error for MaterialError {}

/// Koefisien material fisik permukaan kontak (Phase 9.7).
///
/// INVARIAN ARSITEKTURAL:
/// - Dimiliki secara otoritatif oleh `Collider` (setiap collider dapat memiliki material independen).
/// - Restitusi $e \in [0.0, 1.0]$.
/// - Friksi Coulomb $\mu \ge 0.0$ (tidak dibatasi secara artifisial pada 1.0).
/// - Kombinasi material bersifat simetris: $\text{combine}(A, B) == \text{combine}(B, A)$.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicsMaterial {
    /// Koefisien restitusi pemantulan tabrakan (0.0 <= e <= 1.0)
    pub restitution: f32,
    /// Koefisien friksi Coulomb gesek permukaan (mu >= 0.0)
    pub friction: f32,
}

impl Default for PhysicsMaterial {
    fn default() -> Self {
        Self {
            restitution: 0.0,
            friction: 0.0,
        }
    }
}

impl PhysicsMaterial {
    /// Membuat PhysicsMaterial baru dengan validasi nilai.
    pub fn new(restitution: f32, friction: f32) -> Result<Self, MaterialError> {
        let mat = Self {
            restitution,
            friction,
        };
        mat.validate()?;
        Ok(mat)
    }

    /// Memvalidasi koefisien material fisik.
    pub fn validate(&self) -> Result<(), MaterialError> {
        if !self.restitution.is_finite() || self.restitution < 0.0 || self.restitution > 1.0 {
            return Err(MaterialError::InvalidRestitution);
        }
        if !self.friction.is_finite() || self.friction < 0.0 {
            return Err(MaterialError::InvalidFriction);
        }
        Ok(())
    }

    /// Mengombinasikan dua material fisik secara deterministik dan simetris (Phase 9.7):
    /// - $e = \max(e_A, e_B)$
    /// - $\mu = \sqrt{\mu_A \cdot \mu_B}$
    ///
    /// Menjamin: $\text{combine}(A, B) == \text{combine}(B, A)$.
    pub fn combine(&self, other: &Self) -> Result<Self, MaterialError> {
        self.validate()?;
        other.validate()?;
        Ok(Self {
            restitution: self.restitution.max(other.restitution),
            friction: (self.friction * other.friction).sqrt(),
        })
    }
}

/// Fungsi pembantu pengombinasian material simetris: $\text{combine}(A, B) == \text{combine}(B, A)$.
#[inline]
pub fn combine_materials(
    a: &PhysicsMaterial,
    b: &PhysicsMaterial,
) -> Result<PhysicsMaterial, MaterialError> {
    a.combine(b)
}

/// Abstraksi instance bentuk tabrakan (Collider) yang terpasang pada sebuah badan kaku (RigidBody).
///
/// PEMISAHAN PERAN ARSITEKTURAL:
/// - `Shape`: Definisi geometri bentuk murni (Sphere, Box, Capsule).
/// - `Collider`: Instance bentuk geometris yang terikat pada `RigidBodyId` tertentu dengan transformasi lokal dan material fisik.
/// - `RigidBody`: State fisik murni (massa, kecepatan, transform dunia).
///
/// DOKUMENTASI DUKUNGAN MULTI-COLLIDER:
/// Model data `Collider` mendukung banyak collider yang merujuk pada satu `RigidBodyId` yang sama
/// (`RigidBody -> [ColliderId]`).
/// Namun, broadphase Phase 9.1 saat ini diindeks berdasarkan `RigidBodyId`. Oleh karena itu,
/// pada Phase 9.3, representasi broadphase adalah bounding box gabungan (AABB union) dari
/// collider-collider milik badan tersebut sebagai representasi transisional yang stabil,
/// dan BUKAN mengklaim bahwa broadphase Phase 9.1 telah memiliki proksi terpisah per-collider.
#[derive(Debug, Clone, PartialEq)]
pub struct Collider {
    id: ColliderId,
    rigid_body_id: RigidBodyId,
    shape: Shape,
    local_transform: Transform,
    material: PhysicsMaterial,
}

impl Collider {
    /// Membuat Collider baru dengan default material (restitusi 0.0, friksi 0.0).
    pub fn new(
        id: ColliderId,
        rigid_body_id: RigidBodyId,
        shape: Shape,
        local_transform: Transform,
    ) -> Self {
        Self {
            id,
            rigid_body_id,
            shape,
            local_transform,
            material: PhysicsMaterial::default(),
        }
    }

    /// Mengambil identifier unik collider.
    #[inline(always)]
    pub fn id(&self) -> ColliderId {
        self.id
    }

    /// Mengambil ID badan kaku pemilik collider ini.
    #[inline(always)]
    pub fn rigid_body_id(&self) -> RigidBodyId {
        self.rigid_body_id
    }

    /// Mengambil referensi ke bentuk geometri tabrakan.
    #[inline(always)]
    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    /// Mengubah bentuk geometri tabrakan.
    #[inline(always)]
    pub fn set_shape(&mut self, shape: Shape) {
        self.shape = shape;
    }

    /// Mengambil referensi transformasi lokal collider terhadap badan kaku pemilik.
    #[inline(always)]
    pub fn local_transform(&self) -> &Transform {
        &self.local_transform
    }

    /// Mengubah transformasi lokal collider.
    #[inline(always)]
    pub fn set_local_transform(&mut self, transform: Transform) {
        self.local_transform = transform;
    }

    /// Mengambil referensi material fisik permukaan collider.
    #[inline(always)]
    pub fn material(&self) -> PhysicsMaterial {
        self.material
    }

    /// Mengubah material fisik permukaan collider.
    pub fn set_material(&mut self, material: PhysicsMaterial) -> Result<(), MaterialError> {
        material.validate()?;
        self.material = material;
        Ok(())
    }

    /// Builder pattern untuk menentukan material fisik permukaan collider.
    pub fn with_material(mut self, material: PhysicsMaterial) -> Result<Self, MaterialError> {
        material.validate()?;
        self.material = material;
        Ok(self)
    }

    /// Menghitung AABB dunia dari collider berdasarkan transformasi dunia badan pemilik:
    /// $T_{\text{world\_collider}} = T_{\text{body}} \times T_{\text{local}}$
    pub fn compute_world_aabb(&self, body_transform: &Transform) -> Result<Aabb, ShapeError> {
        let world_transform = body_transform.mul_transform(&self.local_transform);
        self.shape.compute_aabb(&world_transform)
    }
}
