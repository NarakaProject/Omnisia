use glam::Vec3;

use super::broadphase::RigidBodyId;
use super::collider::ColliderId;

/// Representasi data geometris kontak fisik hasil dari narrowphase (Phase 9.4).
///
/// INVARIAN ARSITEKTURAL:
/// 1. **Pure Data**: Hanya memuat identitas dan geometri kontak. Tidak memuat status solver,
///    impuls terakumulasi, riwayat waktu (lifetime), cache persistensi, atau warm-starting.
/// 2. **Canonical Normal Orientation**:
///    Vektor `normal` **SELALU berarah dari Collider A ke Collider B** ($A \to B$).
///    Kueri simetri terbalik ($B \leftrightarrow A$) menghasilkan normal yang dinegasikan:
///    $$\vec{n}_{BA} = -\vec{n}_{AB}$$
/// 3. **Penetrasi Non-Negatif**:
///    `penetration >= 0.0`. Objek yang bersentuhan (touching) memiliki `penetration ≈ 0.0`.
/// 4. **Finiteness**: Seluruh komponen `point`, `normal`, dan `penetration` dijamin bernilai finite
///    (bukan NaN atau Infinity), dan `normal` mendekati panjang satu unit ($|\vec{n}| \approx 1.0$).
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
}

impl Contact {
    /// Membuat instance Contact baru dengan normal berarah A -> B.
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
        }
    }

    /// Menghasilkan kontak simetris terbalik ($B \leftrightarrow A$).
    ///
    /// Menukar identitas A dan B serta membalik arah vektor normal ($\vec{n}_{BA} = -\vec{n}_{AB}$),
    /// dengan titik kontak dan nilai penetrasi yang tetap sama.
    pub fn flipped(&self) -> Self {
        Self {
            collider_a: self.collider_b,
            collider_b: self.collider_a,
            body_a: self.body_b,
            body_b: self.body_a,
            point: self.point,
            normal: -self.normal,
            penetration: self.penetration,
        }
    }
}
