use glam::{Mat3, Vec3};
use std::fmt;

use super::collider::Collider;
use super::contact::Contact;
use super::shape::{BoxShape, Capsule, Shape, ShapeError, Sphere};
use super::transform::Transform;

/// Toleransi spasial sentuhan (touching) vs terpisah (0.1 milimeter).
pub const CONTACT_EPSILON: f32 = 1e-4;

/// Ambang batas magnitudo kuadrat minimum untuk normalisasi vektor secara aman.
pub const NORMAL_EPSILON: f32 = 1e-6;

/// Ambang batas degenerasi kuadrat untuk sumbu perkalian silang SAT OBB.
pub const SAT_EPSILON: f32 = 1e-6;

/// Kesalahan evaluasi narrowphase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NarrowphaseError {
    /// Geometri bentuk tidak valid
    InvalidGeometry(ShapeError),
    /// Transformasi memuat koordinat non-finite
    NonFiniteCoordinates,
    /// Transformasi tidak valid atau quaternion zero-length
    InvalidTransform,
    /// Koefisien material collider tidak valid (non-finite, e di luar [0..1], mu < 0)
    InvalidMaterial,
}

impl fmt::Display for NarrowphaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGeometry(e) => write!(f, "Kesalahan geometri narrowphase: {}", e),
            Self::NonFiniteCoordinates => {
                write!(f, "Koordinat narrowphase memuat nilai non-finite")
            }
            Self::InvalidTransform => write!(f, "Transformasi narrowphase tidak valid"),
            Self::InvalidMaterial => {
                write!(f, "Koefisien material collider narrowphase tidak valid")
            }
        }
    }
}

impl std::error::Error for NarrowphaseError {}

impl From<ShapeError> for NarrowphaseError {
    fn from(err: ShapeError) -> Self {
        Self::InvalidGeometry(err)
    }
}

/// Evaluasi tabrakan geometris narrowphase kanonikal antara dua collider pada transformasi dunia masing-masing.
///
/// KONVENSI NORMAL KANONIKAL:
/// - Vektor normal kontak hasil fungsi ini **SELALU berarah dari Collider A ke Collider B** ($A \to B$).
/// - Kueri terbalik `collide(B, T_B, A, T_A)` menghasilkan kontak dengan normal berkebalikan arah ($\vec{n}_{BA} = -\vec{n}_{AB}$).
pub fn collide(
    collider_a: &Collider,
    transform_a: &Transform,
    collider_b: &Collider,
    transform_b: &Transform,
) -> Result<Option<Contact>, NarrowphaseError> {
    if !transform_a.position.is_finite() || !transform_b.position.is_finite() {
        return Err(NarrowphaseError::NonFiniteCoordinates);
    }

    let id_a = collider_a.id();
    let id_b = collider_b.id();
    let body_a = collider_a.rigid_body_id();
    let body_b = collider_b.rigid_body_id();

    let contact_opt = match (collider_a.shape(), collider_b.shape()) {
        // --- 1. Sphere ↔ Sphere ---
        (Shape::Sphere(s_a), Shape::Sphere(s_b)) => collide_sphere_sphere(
            id_a,
            body_a,
            s_a,
            transform_a,
            id_b,
            body_b,
            s_b,
            transform_b,
        ),

        // --- 2. Sphere ↔ Box ---
        (Shape::Sphere(s_a), Shape::Box(b_b)) => collide_sphere_box(
            id_a,
            body_a,
            s_a,
            transform_a,
            id_b,
            body_b,
            b_b,
            transform_b,
        ),
        (Shape::Box(b_a), Shape::Sphere(s_b)) => {
            // Kueri simetri terbalik: Box A ↔ Sphere B
            let c = collide_sphere_box(
                id_b,
                body_b,
                s_b,
                transform_b,
                id_a,
                body_a,
                b_a,
                transform_a,
            )?;
            Ok(c.map(|contact| contact.flipped()))
        }

        // --- 3. Sphere ↔ Capsule ---
        (Shape::Sphere(s_a), Shape::Capsule(c_b)) => collide_sphere_capsule(
            id_a,
            body_a,
            s_a,
            transform_a,
            id_b,
            body_b,
            c_b,
            transform_b,
        ),
        (Shape::Capsule(c_a), Shape::Sphere(s_b)) => {
            // Kueri simetri terbalik: Capsule A ↔ Sphere B
            let c = collide_sphere_capsule(
                id_b,
                body_b,
                s_b,
                transform_b,
                id_a,
                body_a,
                c_a,
                transform_a,
            )?;
            Ok(c.map(|contact| contact.flipped()))
        }

        // --- 4. Capsule ↔ Capsule ---
        (Shape::Capsule(c_a), Shape::Capsule(c_b)) => collide_capsule_capsule(
            id_a,
            body_a,
            c_a,
            transform_a,
            id_b,
            body_b,
            c_b,
            transform_b,
        ),

        // --- 5. Box ↔ Box ---
        (Shape::Box(b_a), Shape::Box(b_b)) => collide_box_box(
            id_a,
            body_a,
            b_a,
            transform_a,
            id_b,
            body_b,
            b_b,
            transform_b,
        ),

        // --- 6. Box ↔ Capsule ---
        (Shape::Box(b_a), Shape::Capsule(c_b)) => collide_box_capsule(
            id_a,
            body_a,
            b_a,
            transform_a,
            id_b,
            body_b,
            c_b,
            transform_b,
        ),
        (Shape::Capsule(c_a), Shape::Box(b_b)) => {
            // Kueri simetri terbalik: Capsule A ↔ Box B
            let c = collide_box_capsule(
                id_b,
                body_b,
                b_b,
                transform_b,
                id_a,
                body_a,
                c_a,
                transform_a,
            )?;
            Ok(c.map(|contact| contact.flipped()))
        }
    }?;

    if let Some(mut contact) = contact_opt {
        let combined = collider_a
            .material()
            .combine(&collider_b.material())
            .map_err(|_| NarrowphaseError::InvalidMaterial)?;
        contact.restitution = combined.restitution;
        contact.friction = combined.friction;
        Ok(Some(contact))
    } else {
        Ok(None)
    }
}

// ============================================================================
// 1. SPHERE ↔ SPHERE ALGORITHM
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn collide_sphere_sphere(
    id_a: super::collider::ColliderId,
    body_a: super::broadphase::RigidBodyId,
    sphere_a: &Sphere,
    transform_a: &Transform,
    id_b: super::collider::ColliderId,
    body_b: super::broadphase::RigidBodyId,
    sphere_b: &Sphere,
    transform_b: &Transform,
) -> Result<Option<Contact>, NarrowphaseError> {
    let center_a = transform_a.position;
    let center_b = transform_b.position;
    let r_a = sphere_a.radius();
    let r_b = sphere_b.radius();

    let delta = center_b - center_a;
    let dist_sq = delta.length_squared();
    let r_sum = r_a + r_b;

    // Uji pemisahan dengan CONTACT_EPSILON
    if dist_sq > (r_sum + CONTACT_EPSILON).powi(2) {
        return Ok(None);
    }

    let dist = dist_sq.sqrt();
    let penetration = (r_sum - dist).max(0.0);

    // Penanganan degenerasi pusat berimpit (coincident centers):
    // Jika jarak kuadrat <= NORMAL_EPSILON, pilih sumbu fallback deterministik Vec3::X
    // berorientasi kanonikal berdasarkan perbandingan ID agar simetri A <-> B tetap terjaga.
    let (normal, point) = if dist_sq > NORMAL_EPSILON {
        let n = delta / dist;
        let pt_a = center_a + n * r_a;
        let pt_b = center_b - n * r_b;
        (n, 0.5 * (pt_a + pt_b))
    } else {
        let sign = if id_a <= id_b { 1.0 } else { -1.0 };
        (Vec3::X * sign, center_a)
    };

    Ok(Some(Contact::new(
        id_a,
        id_b,
        body_a,
        body_b,
        point,
        normal,
        penetration,
    )))
}

// ============================================================================
// 2. SPHERE ↔ BOX ALGORITHM
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn collide_sphere_box(
    id_a: super::collider::ColliderId,
    body_a: super::broadphase::RigidBodyId,
    sphere_a: &Sphere,
    transform_a: &Transform,
    id_b: super::collider::ColliderId,
    body_b: super::broadphase::RigidBodyId,
    box_b: &BoxShape,
    transform_b: &Transform,
) -> Result<Option<Contact>, NarrowphaseError> {
    let center_a = transform_a.position;
    let r_a = sphere_a.radius();
    let h_b = box_b.half_extents();

    // Transformasi pusat bola ke ruang lokal boks B:
    // c_loc = R_b^T * (center_a - p_b)
    let center_local = transform_b.rotation.conjugate() * (center_a - transform_b.position);

    // Titik terdekat pada AABB lokal boks:
    let clamped = center_local.clamp(-h_b, h_b);
    let delta_local = center_local - clamped;
    let dist_sq = delta_local.length_squared();

    if dist_sq > NORMAL_EPSILON {
        // --- KASUS A: Pusat bola berada di LUAR boks B ---
        let dist = dist_sq.sqrt();
        if dist > r_a + CONTACT_EPSILON {
            return Ok(None);
        }

        let penetration = (r_a - dist).max(0.0);

        // Vektor delta_local berarah dari permukaan boks ke pusat bola.
        // Normal kanonikal harus berarah dari Sphere A ke Box B (kebalikan delta_local).
        let normal_local = -delta_local / dist;
        let normal_world = transform_b.rotation * normal_local;

        let point_box_world = transform_b.transform_point(clamped);
        let point_sphere_world = center_a + normal_world * r_a;
        let point = 0.5 * (point_box_world + point_sphere_world);

        Ok(Some(Contact::new(
            id_a,
            id_b,
            body_a,
            body_b,
            point,
            normal_world,
            penetration,
        )))
    } else {
        // --- KASUS B: Pusat bola berada di DALAM boks B (clamped == center_local) ---
        // Cari jarak ke 6 muka boks untuk menentukan sumbu penetrasi terpendek secara deterministik.
        let d_xp = h_b.x - center_local.x;
        let d_xn = h_b.x + center_local.x;
        let d_yp = h_b.y - center_local.y;
        let d_yn = h_b.y + center_local.y;
        let d_zp = h_b.z - center_local.z;
        let d_zn = h_b.z + center_local.z;

        // Urutan deterministik: +X, -X, +Y, -Y, +Z, -Z
        let mut min_dist = d_xp;
        let mut face_normal_local = Vec3::X;
        let mut surface_local = Vec3::new(h_b.x, center_local.y, center_local.z);

        if d_xn < min_dist {
            min_dist = d_xn;
            face_normal_local = Vec3::NEG_X;
            surface_local = Vec3::new(-h_b.x, center_local.y, center_local.z);
        }
        if d_yp < min_dist {
            min_dist = d_yp;
            face_normal_local = Vec3::Y;
            surface_local = Vec3::new(center_local.x, h_b.y, center_local.z);
        }
        if d_yn < min_dist {
            min_dist = d_yn;
            face_normal_local = Vec3::NEG_Y;
            surface_local = Vec3::new(center_local.x, -h_b.y, center_local.z);
        }
        if d_zp < min_dist {
            min_dist = d_zp;
            face_normal_local = Vec3::Z;
            surface_local = Vec3::new(center_local.x, center_local.y, h_b.z);
        }
        if d_zn < min_dist {
            min_dist = d_zn;
            face_normal_local = Vec3::NEG_Z;
            surface_local = Vec3::new(center_local.x, center_local.y, -h_b.z);
        }

        // face_normal_local berarah ke luar boks.
        // Normal kanonikal dari Sphere A ke Box B berarah ke dalam muka boks (-face_normal_local).
        let normal_world = transform_b.rotation * (-face_normal_local);
        let penetration = (r_a + min_dist).max(0.0);
        let point = transform_b.transform_point(surface_local);

        Ok(Some(Contact::new(
            id_a,
            id_b,
            body_a,
            body_b,
            point,
            normal_world,
            penetration,
        )))
    }
}

// ============================================================================
// 3. SPHERE ↔ CAPSULE ALGORITHM
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn collide_sphere_capsule(
    id_a: super::collider::ColliderId,
    body_a: super::broadphase::RigidBodyId,
    sphere_a: &Sphere,
    transform_a: &Transform,
    id_b: super::collider::ColliderId,
    body_b: super::broadphase::RigidBodyId,
    capsule_b: &Capsule,
    transform_b: &Transform,
) -> Result<Option<Contact>, NarrowphaseError> {
    let center_a = transform_a.position;
    let r_a = sphere_a.radius();
    let r_b = capsule_b.radius();
    let h_b = capsule_b.half_height();

    // Titik ujung segmen garis tengah kapsul di ruang dunia (sumbu lokal Y):
    let axis_offset = transform_b.rotation * Vec3::new(0.0, h_b, 0.0);
    let s0 = transform_b.position - axis_offset;
    let s1 = transform_b.position + axis_offset;
    let seg_vec = s1 - s0;
    let seg_len_sq = seg_vec.length_squared();

    // Proyeksikan pusat bola ke segmen kapsul
    let closest_on_capsule = if seg_len_sq > NORMAL_EPSILON {
        let t = ((center_a - s0).dot(seg_vec) / seg_len_sq).clamp(0.0, 1.0);
        s0 + seg_vec * t
    } else {
        transform_b.position
    };

    // Reduksi menjadi masalah Sphere-Sphere antara Sphere A dan Sphere maya di closest_on_capsule
    let delta = closest_on_capsule - center_a;
    let dist_sq = delta.length_squared();
    let r_sum = r_a + r_b;

    if dist_sq > (r_sum + CONTACT_EPSILON).powi(2) {
        return Ok(None);
    }

    let dist = dist_sq.sqrt();
    let penetration = (r_sum - dist).max(0.0);

    // Penanganan degenerasi: jika pusat bola berimpit dengan segmen kapsul
    let normal = if dist_sq > NORMAL_EPSILON {
        delta / dist
    } else if seg_len_sq > NORMAL_EPSILON {
        // Cari vektor tegak lurus terhadap segmen kapsul
        let seg_dir = seg_vec / seg_len_sq.sqrt();
        let trial = if seg_dir.x.abs() < 0.9 {
            Vec3::X
        } else {
            Vec3::Y
        };
        seg_dir.cross(trial).normalize()
    } else {
        Vec3::X
    };

    let point_a = center_a + normal * r_a;
    let point_b = closest_on_capsule - normal * r_b;
    let point = 0.5 * (point_a + point_b);

    Ok(Some(Contact::new(
        id_a,
        id_b,
        body_a,
        body_b,
        point,
        normal,
        penetration,
    )))
}

// ============================================================================
// 4. CAPSULE ↔ CAPSULE ALGORITHM
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn collide_capsule_capsule(
    id_a: super::collider::ColliderId,
    body_a: super::broadphase::RigidBodyId,
    capsule_a: &Capsule,
    transform_a: &Transform,
    id_b: super::collider::ColliderId,
    body_b: super::broadphase::RigidBodyId,
    capsule_b: &Capsule,
    transform_b: &Transform,
) -> Result<Option<Contact>, NarrowphaseError> {
    let r_a = capsule_a.radius();
    let r_b = capsule_b.radius();

    // Segmen A di ruang dunia
    let offset_a = transform_a.rotation * Vec3::new(0.0, capsule_a.half_height(), 0.0);
    let a0 = transform_a.position - offset_a;
    let a1 = transform_a.position + offset_a;

    // Segmen B di ruang dunia
    let offset_b = transform_b.rotation * Vec3::new(0.0, capsule_b.half_height(), 0.0);
    let b0 = transform_b.position - offset_b;
    let b1 = transform_b.position + offset_b;

    // Hitung titik terdekat analitis antara dua segmen garis 3D
    let (p_a, p_b) = closest_points_segment_segment(a0, a1, b0, b1);

    let delta = p_b - p_a;
    let dist_sq = delta.length_squared();
    let r_sum = r_a + r_b;

    if dist_sq > (r_sum + CONTACT_EPSILON).powi(2) {
        return Ok(None);
    }

    let dist = dist_sq.sqrt();
    let penetration = (r_sum - dist).max(0.0);

    // Penanganan degenerasi segmen berimpit / titik terdekat berimpit:
    let (normal, point) = if dist_sq > NORMAL_EPSILON {
        let n = delta / dist;
        let pt_a = p_a + n * r_a;
        let pt_b = p_b - n * r_b;
        (n, 0.5 * (pt_a + pt_b))
    } else {
        let sign = if id_a <= id_b { 1.0 } else { -1.0 };
        // Coba perkalian silang arah segmen A dan B
        let dir_a = a1 - a0;
        let dir_b = b1 - b0;
        let cross = dir_a.cross(dir_b);
        let fallback = if cross.length_squared() > NORMAL_EPSILON {
            cross.normalize()
        } else if dir_a.length_squared() > NORMAL_EPSILON {
            // Paralel berimpit: pilih vektor tegak lurus dir_a
            let d = dir_a.normalize();
            let trial = if d.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
            d.cross(trial).normalize()
        } else {
            Vec3::X
        };
        (fallback * sign, p_a)
    };

    Ok(Some(Contact::new(
        id_a,
        id_b,
        body_a,
        body_b,
        point,
        normal,
        penetration,
    )))
}

/// Menghitung pasangan titik terdekat antara dua segmen garis 3D [p1, q1] dan [p2, q2].
/// Menerapkan algoritma analitis deterministik Lumelsky/Eberly yang tahan kasus paralel & nol.
fn closest_points_segment_segment(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> (Vec3, Vec3) {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;

    let a = d1.length_squared();
    let e = d2.length_squared();
    let f = d2.dot(r);

    if a <= NORMAL_EPSILON && e <= NORMAL_EPSILON {
        // Kedua segmen adalah titik
        return (p1, p2);
    }
    if a <= NORMAL_EPSILON {
        // Segmen pertama adalah titik tunggal
        let t = (f / e).clamp(0.0, 1.0);
        return (p1, p2 + d2 * t);
    }
    if e <= NORMAL_EPSILON {
        // Segmen kedua adalah titik tunggal
        let c = d1.dot(r);
        let s = (-c / a).clamp(0.0, 1.0);
        return (p1 + d1 * s, p2);
    }

    let c = d1.dot(r);
    let b = d1.dot(d2);
    let denom = a * e - b * b;

    let s = if denom > NORMAL_EPSILON {
        ((b * f - c * e) / denom).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let t = (b * s + f) / e;
    if t < 0.0 {
        let s_clamped = (-c / a).clamp(0.0, 1.0);
        return (p1 + d1 * s_clamped, p2);
    } else if t > 1.0 {
        let s_clamped = ((b - c) / a).clamp(0.0, 1.0);
        return (p1 + d1 * s_clamped, p2 + d2);
    }

    (p1 + d1 * s, p2 + d2 * t)
}

// ============================================================================
// 5. BOX ↔ BOX ALGORITHM (SAT 15 AXES)
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn collide_box_box(
    id_a: super::collider::ColliderId,
    body_a: super::broadphase::RigidBodyId,
    box_a: &BoxShape,
    transform_a: &Transform,
    id_b: super::collider::ColliderId,
    body_b: super::broadphase::RigidBodyId,
    box_b: &BoxShape,
    transform_b: &Transform,
) -> Result<Option<Contact>, NarrowphaseError> {
    let p_a = transform_a.position;
    let p_b = transform_b.position;
    let h_a = box_a.half_extents();
    let h_b = box_b.half_extents();

    let rot_a = Mat3::from_quat(transform_a.rotation);
    let rot_b = Mat3::from_quat(transform_b.rotation);

    let a_axes = [rot_a.x_axis, rot_a.y_axis, rot_a.z_axis];
    let b_axes = [rot_b.x_axis, rot_b.y_axis, rot_b.z_axis];

    let d_ab = p_b - p_a;

    // 15 sumbu kandidat SAT:
    // 0..3: 3 muka Box A
    // 3..6: 3 muka Box B
    // 6..15: 9 perkalian silang tepi A_i x B_j
    let mut min_penetration = f32::INFINITY;
    let mut best_axis = Vec3::ZERO;

    // Helper untuk menguji satu sumbu pemisah
    let mut test_axis = |axis: Vec3| -> bool {
        let len_sq = axis.length_squared();
        if len_sq < SAT_EPSILON {
            // Sumbu degenerat (perkalian silang tepi paralel), abaikan
            return true;
        }

        let l = axis / len_sq.sqrt();

        // Proyeksi setengah bentang A dan B ke sumbu L
        let r_a = h_a.x * a_axes[0].dot(l).abs()
            + h_a.y * a_axes[1].dot(l).abs()
            + h_a.z * a_axes[2].dot(l).abs();
        let r_b = h_b.x * b_axes[0].dot(l).abs()
            + h_b.y * b_axes[1].dot(l).abs()
            + h_b.z * b_axes[2].dot(l).abs();

        let s = d_ab.dot(l).abs();
        let overlap = (r_a + r_b) - s;

        if overlap < -CONTACT_EPSILON {
            // Terpisah pada sumbu ini! Tidak ada tabrakan.
            return false;
        }

        // Tie-breaking deterministik: gunakan '<' ketat dengan urutan iterasi tetap
        if overlap < min_penetration {
            min_penetration = overlap;
            best_axis = l;
        }

        true
    };

    // Uji 3 sumbu muka Box A
    for axis in a_axes {
        if !test_axis(axis) {
            return Ok(None);
        }
    }

    // Uji 3 sumbu muka Box B
    for axis in b_axes {
        if !test_axis(axis) {
            return Ok(None);
        }
    }

    // Uji 9 sumbu perkalian silang tepi
    for a in a_axes {
        for b in b_axes {
            if !test_axis(a.cross(b)) {
                return Ok(None);
            }
        }
    }

    // Pastikan normal kontak berarah kanonikal dari Box A ke Box B (d_ab . normal >= 0)
    let normal = if d_ab.dot(best_axis) < 0.0 {
        -best_axis
    } else {
        best_axis
    };

    let penetration = min_penetration.max(0.0);

    // Titik kontak representatif deterministik:
    // Titik penyangga pada Box A searah +normal:
    let mut s_a = p_a;
    for (i, axis) in a_axes.iter().enumerate() {
        let proj = axis.dot(normal);
        if proj > 1e-4 {
            s_a += *axis * h_a[i];
        } else if proj < -1e-4 {
            s_a -= *axis * h_a[i];
        }
    }

    // Titik penyangga pada Box B searah -normal:
    let mut s_b = p_b;
    for (j, axis) in b_axes.iter().enumerate() {
        let proj = axis.dot(-normal);
        if proj > 1e-4 {
            s_b += *axis * h_b[j];
        } else if proj < -1e-4 {
            s_b -= *axis * h_b[j];
        } else {
            // Sumbu ortogonal terhadap normal: proyeksikan s_a ke sumbu Box B dan klem
            let d = (s_a - p_b).dot(*axis);
            let clamped_d = d.clamp(-h_b[j], h_b[j]);
            s_b += *axis * clamped_d;
        }
    }

    let point = 0.5 * (s_a + s_b);

    Ok(Some(Contact::new(
        id_a,
        id_b,
        body_a,
        body_b,
        point,
        normal,
        penetration,
    )))
}

// ============================================================================
// 6. BOX ↔ CAPSULE ALGORITHM (SPECIAL HARDENING SECTION 13)
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn collide_box_capsule(
    id_a: super::collider::ColliderId,
    body_a: super::broadphase::RigidBodyId,
    box_a: &BoxShape,
    transform_a: &Transform,
    id_b: super::collider::ColliderId,
    body_b: super::broadphase::RigidBodyId,
    capsule_b: &Capsule,
    transform_b: &Transform,
) -> Result<Option<Contact>, NarrowphaseError> {
    let h_a = box_a.half_extents();
    let r_b = capsule_b.radius();
    let h_b = capsule_b.half_height();

    // Segmen kapsul di ruang dunia:
    let axis_offset = transform_b.rotation * Vec3::new(0.0, h_b, 0.0);
    let world_s0 = transform_b.position - axis_offset;
    let world_s1 = transform_b.position + axis_offset;

    // Transformasikan segmen kapsul ke ruang lokal Box A:
    let rot_a_inv = transform_a.rotation.conjugate();
    let s0 = rot_a_inv * (world_s0 - transform_a.position);
    let s1 = rot_a_inv * (world_s1 - transform_a.position);

    // Cari titik terdekat antara segmen garis [s0, s1] dan AABB [-h_a, h_a] di ruang lokal Box A
    let (closest_seg_local, closest_box_local) = closest_segment_aabb_local(s0, s1, h_a);

    let delta_local = closest_seg_local - closest_box_local;
    let dist_sq = delta_local.length_squared();

    if dist_sq > NORMAL_EPSILON {
        // --- KASUS A: Segmen kapsul berada di LUAR boks A ---
        let dist = dist_sq.sqrt();
        if dist > r_b + CONTACT_EPSILON {
            return Ok(None);
        }

        let penetration = (r_b - dist).max(0.0);

        // delta_local berarah dari boks ke kapsul.
        // Normal kanonikal Box A -> Capsule B adalah delta_local yang dinormalisasi:
        let normal_local = delta_local / dist;
        let normal_world = transform_a.rotation * normal_local;

        let pt_box_world = transform_a.transform_point(closest_box_local);
        let pt_seg_world = transform_a.transform_point(closest_seg_local);
        let pt_capsule_world = pt_seg_world - normal_world * r_b;
        let point = 0.5 * (pt_box_world + pt_capsule_world);

        Ok(Some(Contact::new(
            id_a,
            id_b,
            body_a,
            body_b,
            point,
            normal_world,
            penetration,
        )))
    } else {
        // --- KASUS B: Segmen kapsul MEMOTONG atau BERADA DI DALAM boks A (HARDENING SECTION 13) ---
        // Penetrasi BUKAN sekadar r_b! Kita harus menghitung kedalaman penetrasi sebenarnya
        // ke muka boks terdekat untuk mendorong segmen keluar dari boks, ditambah radius kapsul.
        //
        // Muka boks yang diuji: +X, -X, +Y, -Y, +Z, -Z.
        // Untuk setiap muka i, jarak minimum segmen ke muka i adalah:
        // d_face = min_{t in [0,1]} (h_a[i] - sign * s[i](t)).
        // Karena s(t) linier, nilai minimum selalu terjadi pada salah satu ujung s0 atau s1!
        let s_inside = closest_seg_local.clamp(-h_a, h_a);
        let mut min_face_dist = f32::INFINITY;
        let mut best_face_normal = Vec3::X;
        let mut best_box_pt_local = Vec3::ZERO;

        let test_faces = [
            (
                Vec3::X,
                h_a.x - s_inside.x,
                Vec3::new(h_a.x, s_inside.y, s_inside.z),
            ),
            (
                Vec3::NEG_X,
                h_a.x + s_inside.x,
                Vec3::new(-h_a.x, s_inside.y, s_inside.z),
            ),
            (
                Vec3::Y,
                h_a.y - s_inside.y,
                Vec3::new(s_inside.x, h_a.y, s_inside.z),
            ),
            (
                Vec3::NEG_Y,
                h_a.y + s_inside.y,
                Vec3::new(s_inside.x, -h_a.y, s_inside.z),
            ),
            (
                Vec3::Z,
                h_a.z - s_inside.z,
                Vec3::new(s_inside.x, s_inside.y, h_a.z),
            ),
            (
                Vec3::NEG_Z,
                h_a.z + s_inside.z,
                Vec3::new(s_inside.x, s_inside.y, -h_a.z),
            ),
        ];

        for (face_normal, depth, pt_local) in test_faces {
            if depth < min_face_dist {
                min_face_dist = depth;
                best_face_normal = face_normal;
                best_box_pt_local = pt_local;
            }
        }

        // Normal Box A -> Capsule B berarah ke luar muka boks (mendorong kapsul keluar dari boks A)
        let normal_world = transform_a.rotation * best_face_normal;
        let penetration = (r_b + min_face_dist).max(0.0);
        let point = transform_a.transform_point(best_box_pt_local);

        Ok(Some(Contact::new(
            id_a,
            id_b,
            body_a,
            body_b,
            point,
            normal_world,
            penetration,
        )))
    }
}

/// Menemukan titik terdekat pada segmen garis 3D [s0, s1] dan titik terdekat pada AABB [-half_extents, half_extents].
fn closest_segment_aabb_local(s0: Vec3, s1: Vec3, h: Vec3) -> (Vec3, Vec3) {
    let seg = s1 - s0;
    let seg_len_sq = seg.length_squared();

    if seg_len_sq <= NORMAL_EPSILON {
        let p_box = s0.clamp(-h, h);
        return (s0, p_box);
    }

    // Evaluasi fungsi kuadrat jarak terpotong f(t) = |s(t) - clamp(s(t), -h, h)|^2 pada t in [0, 1].
    // Nilai t kritis terjadi pada t = 0.0, t = 1.0, dan saat s(t) memotong bidang batas muka boks (+-h).
    let mut candidate_t = [0.0; 8];
    let mut count = 2;
    candidate_t[0] = 0.0;
    candidate_t[1] = 1.0;

    for i in 0..3 {
        let d = seg[i];
        if d.abs() > NORMAL_EPSILON {
            let t_pos = (h[i] - s0[i]) / d;
            if (0.0..=1.0).contains(&t_pos) {
                candidate_t[count] = t_pos;
                count += 1;
            }
            let t_neg = (-h[i] - s0[i]) / d;
            if (0.0..=1.0).contains(&t_neg) {
                candidate_t[count] = t_neg;
                count += 1;
            }
        }
    }

    let mut best_t = 0.0;
    let mut best_dist_sq = f32::INFINITY;
    let mut best_box_pt = s0.clamp(-h, h);

    for &t in &candidate_t[..count] {
        let pt_seg = s0 + seg * t;
        let pt_box = pt_seg.clamp(-h, h);
        let d_sq = (pt_seg - pt_box).length_squared();
        if d_sq < best_dist_sq {
            best_dist_sq = d_sq;
            best_t = t;
            best_box_pt = pt_box;
        }
    }

    (s0 + seg * best_t, best_box_pt)
}
