use glam::Vec3;

use super::broadphase::RigidBodyId;
use super::collider::{ColliderId, MaterialError, PhysicsMaterial};

/// Representasi data geometris kontak fisik hasil dari narrowphase (Phase 9.4)
/// beserta snapshot koefisien material gabungan untuk solver (Phase 9.7).
///
/// INVARIAN ARSITEKTURAL:
/// 1. **Pure Data Snapshot**: Memuat identitas, geometri kontak, dan snapshot material gabungan.
///    Tidak memuat status solver, impuls terakumulasi, riwayat waktu (lifetime), cache persistensi,
///    atau warm-starting.
/// 2. **Canonical Normal Orientation**:
///    Vektor `normal` **SELALU berarah dari Collider A ke Collider B** ($A \to B$).
///    Kueri simetri terbalik ($B \leftrightarrow A$) menghasilkan normal yang dinegasikan:
///    $$\vec{n}_{BA} = -\vec{n}_{AB}$$
/// 3. **Penetrasi Non-Negatif**:
///    `penetration >= 0.0`. Objek yang bersentuhan (touching) memiliki `penetration ≈ 0.0`.
/// 4. **Finiteness**: Seluruh komponen `point`, `normal`, `penetration`, `restitution`, dan `friction`
///    dijamin bernilai finite (bukan NaN atau Infinity), dan `normal` mendekati panjang satu unit ($|\vec{n}| \approx 1.0$).
/// 5. **Symmetric Material Snapshot**:
///    `restitution` ($0.0 \le e \le 1.0$) dan `friction` ($\mu \ge 0.0$) tidak berubah pada pembalikan arah kontak
///    karena kombinasi material bersifat skalar simetris.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contact {
    /// ID collider pertama (A)
    pub collider_a: ColliderId,
    /// ID collider kedua (B)
    pub collider_b: ColliderId,
    /// ID badan kaku pemilik collider A
    pub body_a: RigidBodyId,
    /// ID badan kaku pemilik collider B
    pub body_b: RigidBodyId,
    /// Titik kontak geometris dalam koordinat dunia (meter)
    pub point: Vec3,
    /// Vektor normal kontak kanonikal dalam koordinat dunia (berarah dari A ke B)
    pub normal: Vec3,
    /// Kedalaman penetrasi overlap (meter, >= 0.0)
    pub penetration: f32,
    /// Snapshot koefisien restitusi tabrakan (0.0 <= e <= 1.0)
    pub restitution: f32,
    /// Snapshot koefisien friksi Coulomb permukaan (mu >= 0.0)
    pub friction: f32,
}

impl Contact {
    /// Membuat instance Contact baru dengan normal berarah A -> B dan default material (0.0, 0.0).
    pub fn new(
        collider_a: ColliderId,
        collider_b: ColliderId,
        body_a: RigidBodyId,
        body_b: RigidBodyId,
        point: Vec3,
        normal: Vec3,
        penetration: f32,
    ) -> Self {
        Self {
            collider_a,
            collider_b,
            body_a,
            body_b,
            point,
            normal,
            penetration: penetration.max(0.0),
            restitution: 0.0,
            friction: 0.0,
        }
    }

    /// Menetapkan snapshot material fisik pada kontak.
    pub fn with_material(mut self, material: &PhysicsMaterial) -> Result<Self, MaterialError> {
        material.validate()?;
        self.restitution = material.restitution;
        self.friction = material.friction;
        Ok(self)
    }

    /// Menetapkan koefisien material fisik secara eksplisit dengan validasi.
    pub fn with_coefficients(
        mut self,
        restitution: f32,
        friction: f32,
    ) -> Result<Self, MaterialError> {
        let mat = PhysicsMaterial::new(restitution, friction)?;
        self.restitution = mat.restitution;
        self.friction = mat.friction;
        Ok(self)
    }

    /// Menghasilkan kontak simetris terbalik ($B \leftrightarrow A$).
    ///
    /// Menukar identitas A dan B serta membalik arah vektor normal ($\vec{n}_{BA} = -\vec{n}_{AB}$),
    /// dengan titik kontak, penetrasi, restitusi, dan friksi yang tetap sama.
    pub fn flipped(&self) -> Self {
        Self {
            collider_a: self.collider_b,
            collider_b: self.collider_a,
            body_a: self.body_b,
            body_b: self.body_a,
            point: self.point,
            normal: -self.normal,
            penetration: self.penetration,
            restitution: self.restitution,
            friction: self.friction,
        }
    }

    /// Alias simetris kanonikal untuk `flipped(&self)`.
    #[inline(always)]
    pub fn reverse_symmetry(&self) -> Self {
        self.flipped()
    }
}
