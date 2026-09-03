use super::aabb::Aabb;
use super::broadphase::RigidBodyId;
use super::shape::{Shape, ShapeError};
use super::transform::Transform;

/// Identifier unik runtime untuk sebuah instance Collider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColliderId(pub u64);

/// Abstraksi instance bentuk tabrakan (Collider) yang terpasang pada sebuah badan kaku (RigidBody).
///
/// PEMISAHAN PERAN ARSITEKTURAL:
/// - `Shape`: Definisi geometri bentuk murni (Sphere, Box, Capsule).
/// - `Collider`: Instance bentuk geometris yang terikat pada `RigidBodyId` tertentu dengan transformasi lokal.
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
}

impl Collider {
    /// Membuat Collider baru.
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

    /// Menghitung AABB dunia dari collider berdasarkan transformasi dunia badan pemilik:
    /// $T_{\text{world\_collider}} = T_{\text{body}} \times T_{\text{local}}$
    pub fn compute_world_aabb(&self, body_transform: &Transform) -> Result<Aabb, ShapeError> {
        let world_transform = body_transform.mul_transform(&self.local_transform);
        self.shape.compute_aabb(&world_transform)
    }
}
