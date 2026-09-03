use glam::{Mat3, Vec3};
use std::collections::BTreeMap;
use std::fmt;

use super::broadphase::{BodyType, RigidBodyId};
use super::contact::Contact;
use super::rigid_body::RigidBody;

/// Toleransi numerik pembagi inersia/massa efektif minimum.
pub const SOLVER_MASS_EPSILON: f32 = 1e-6;

/// Konfigurasi solver kontak sequential impulse (Phase 9.5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolverConfig {
    /// Jumlah iterasi sequential impulse per pemanggilan solver (default: 10)
    pub iterations: u32,
    /// Koefisien bias Baumgarte untuk stabilisasi penetrasi (default: 0.2)
    pub beta: f32,
    /// Slop penetrasi yang ditoleransi sebelum bias diaktifkan (meter, default: 0.001 = 1mm)
    pub penetration_slop: f32,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            iterations: 10,
            beta: 0.2,
            penetration_slop: 0.001,
        }
    }
}

impl SolverConfig {
    /// Memvalidasi konfigurasi solver.
    pub fn validate(&self) -> Result<(), SolverError> {
        if self.iterations == 0 {
            return Err(SolverError::InvalidConfiguration);
        }
        if !self.beta.is_finite() || self.beta < 0.0 {
            return Err(SolverError::InvalidConfiguration);
        }
        if !self.penetration_slop.is_finite() || self.penetration_slop < 0.0 {
            return Err(SolverError::InvalidConfiguration);
        }
        Ok(())
    }
}

/// Kesalahan eksekusi solver kontak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolverError {
    /// Timestep delta time non-finite, <= 0.0, atau NaN/Infinity
    InvalidTimestep,
    /// Konfigurasi solver tidak valid (iterations == 0, beta/slop negatif atau non-finite)
    InvalidConfiguration,
    /// Badan kaku memuat status kecepatan atau posisi non-finite
    NonFiniteBodyState,
    /// Data kontak tidak valid (normal non-unit, penetrasi < 0, koordinat non-finite)
    InvalidContact,
    /// Badan kaku yang dirujuk oleh kontak tidak ditemukan di registri
    BodyNotFound(RigidBodyId),
    /// Kontak mandiri terdeteksi (body_a == body_b)
    SameBodyContact(RigidBodyId),
}

impl fmt::Display for SolverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTimestep => write!(f, "Timestep dt harus finite dan bernilai > 0.0"),
            Self::InvalidConfiguration => write!(f, "Konfigurasi solver tidak valid"),
            Self::NonFiniteBodyState => write!(f, "Status badan kaku memuat nilai non-finite"),
            Self::InvalidContact => write!(f, "Data kontak tidak valid"),
            Self::BodyNotFound(id) => write!(f, "RigidBody {:?} tidak ditemukan di registri", id),
            Self::SameBodyContact(id) => write!(f, "Kontak mandiri pada badan {:?}", id),
        }
    }
}

impl std::error::Error for SolverError {}

/// Struktur internal solver untuk batasan kontak (ContactConstraint).
///
/// INVARIAN ARSITEKTURAL:
/// 1. `Contact` geometris tetap murni tidak dimutasi.
/// 2. `accumulated_impulse` bersifat **lokal terhadap satu pemanggilan solver ini**
///    dan DIBUANG setelah solver selesai (TIDAK ADA warm-starting antar-frame di Phase 9.5).
#[derive(Debug, Clone, PartialEq)]
pub struct ContactConstraint {
    /// Data kontak geometris asli dari Phase 9.4
    pub contact: Contact,
    /// Vektor lengan tuas dari pusat massa Badan A ke titik kontak ($P - \text{pos}_A$)
    pub r_a: Vec3,
    /// Vektor lengan tuas dari pusat massa Badan B ke titik kontak ($P - \text{pos}_B$)
    pub r_b: Vec3,
    /// Tensor inersia invers ruang dunia Badan A ($R_A I_{\text{local},A}^{-1} R_A^T$)
    pub inv_inertia_world_a: Mat3,
    /// Tensor inersia invers ruang dunia Badan B ($R_B I_{\text{local},B}^{-1} R_B^T$)
    pub inv_inertia_world_b: Mat3,
    /// Skalar massa efektif $1 / K$ sepanjang vektor normal kontak
    pub effective_mass: f32,
    /// Target bias kecepatan Baumgarte untuk stabilisasi penetrasi
    pub bias: f32,
    /// Akumulasi impuls normal selama iterasi solver saat ini (selalu >= 0.0)
    pub accumulated_impulse: f32,
}

/// Menghitung tensor inersia invers di ruang dunia: $I_{\text{world}}^{-1} = R I_{\text{local}}^{-1} R^T$.
/// Untuk badan statis atau kinematik, mengembalikan `Mat3::ZERO`.
#[inline]
pub fn compute_world_inv_inertia(body: &RigidBody) -> Mat3 {
    if body.body_type() == BodyType::Static || body.body_type() == BodyType::Kinematic {
        Mat3::ZERO
    } else {
        let rot_mat = Mat3::from_quat(body.rotation());
        rot_mat * body.mass_properties().local_inverse_inertia * rot_mat.transpose()
    }
}

/// Menyelesaikan seluruh batasan kontak menggunakan Sequential Impulse (Phase 9.5).
///
/// INVARIAN UTAMA:
/// - Hanya memutasi `linear_velocity` dan `angular_velocity` pada badan Dinamis.
/// - **TIDAK PERNAH** memutasi `position` atau `rotation` (integrasi ditunda ke Phase 9.6).
/// - Badan Statis dan Kinematik tidak pernah menerima akselerasi impuls.
/// - Urutan pemrosesan batasan kontak deterministik.
pub fn solve_contacts(
    bodies: &mut BTreeMap<RigidBodyId, RigidBody>,
    contacts: &[Contact],
    dt: f32,
    config: &SolverConfig,
) -> Result<(), SolverError> {
    // 1. Validasi timestep dan konfigurasi
    if !dt.is_finite() || dt <= 0.0 {
        return Err(SolverError::InvalidTimestep);
    }
    config.validate()?;

    if contacts.is_empty() {
        return Ok(());
    }

    // 2. Sortir kontak secara deterministik (body_a, body_b, collider_a, collider_b)
    let mut sorted_contacts: Vec<Contact> = contacts.to_vec();
    sorted_contacts.sort_by(|c1, c2| {
        c1.body_a
            .cmp(&c2.body_a)
            .then_with(|| c1.body_b.cmp(&c2.body_b))
            .then_with(|| c1.collider_a.cmp(&c2.collider_a))
            .then_with(|| c1.collider_b.cmp(&c2.collider_b))
    });

    // 3. Persiapan batasan kontak (ContactConstraint preparation)
    let mut constraints: Vec<ContactConstraint> = Vec::with_capacity(sorted_contacts.len());

    for contact in &sorted_contacts {
        // Validasi pertahanan kontak mandiri
        if contact.body_a == contact.body_b {
            return Err(SolverError::SameBodyContact(contact.body_a));
        }

        // Validasi keterhinggaan geometri kontak
        if !contact.point.is_finite()
            || !contact.normal.is_finite()
            || !contact.penetration.is_finite()
            || contact.penetration < 0.0
        {
            return Err(SolverError::InvalidContact);
        }

        // Validasi normal mendekati unit length
        let norm_len_sq = contact.normal.length_squared();
        if (norm_len_sq - 1.0).abs() > 1e-3 {
            return Err(SolverError::InvalidContact);
        }

        // Ambil referensi badan A dan B
        let body_a = bodies
            .get(&contact.body_a)
            .ok_or(SolverError::BodyNotFound(contact.body_a))?;
        let body_b = bodies
            .get(&contact.body_b)
            .ok_or(SolverError::BodyNotFound(contact.body_b))?;

        // Validasi status badan awal
        if !body_a.position().is_finite()
            || !body_a.linear_velocity().is_finite()
            || !body_a.angular_velocity().is_finite()
            || !body_b.position().is_finite()
            || !body_b.linear_velocity().is_finite()
            || !body_b.angular_velocity().is_finite()
        {
            return Err(SolverError::NonFiniteBodyState);
        }

        let r_a = contact.point - body_a.position();
        let r_b = contact.point - body_b.position();

        let inv_inertia_world_a = compute_world_inv_inertia(body_a);
        let inv_inertia_world_b = compute_world_inv_inertia(body_b);

        let inv_mass_a = if body_a.body_type() == BodyType::Dynamic {
            body_a.mass_properties().inverse_mass
        } else {
            0.0
        };

        let inv_mass_b = if body_b.body_type() == BodyType::Dynamic {
            body_b.mass_properties().inverse_mass
        } else {
            0.0
        };

        let ra_cross_n = r_a.cross(contact.normal);
        let rb_cross_n = r_b.cross(contact.normal);

        let rot_term_a = ra_cross_n.dot(inv_inertia_world_a * ra_cross_n);
        let rot_term_b = rb_cross_n.dot(inv_inertia_world_b * rb_cross_n);

        let k = inv_mass_a + inv_mass_b + rot_term_a + rot_term_b;
        let effective_mass = if k > SOLVER_MASS_EPSILON {
            1.0 / k
        } else {
            0.0
        };

        // Stabilisasi Baumgarte untuk konvensi normal A -> B:
        // non-penetration mensyaratkan v_n >= bias di mana bias = (beta / dt) * max(penetration - slop, 0).
        let penetration_error = (contact.penetration - config.penetration_slop).max(0.0);
        let bias = (config.beta / dt) * penetration_error;

        constraints.push(ContactConstraint {
            contact: *contact,
            r_a,
            r_b,
            inv_inertia_world_a,
            inv_inertia_world_b,
            effective_mass,
            bias,
            accumulated_impulse: 0.0,
        });
    }

    // 4. Iterasi Sequential Impulse
    for _iter in 0..config.iterations {
        for constraint in &mut constraints {
            if constraint.effective_mass <= 0.0 {
                continue;
            }

            let id_a = constraint.contact.body_a;
            let id_b = constraint.contact.body_b;

            // Dapatkan kecepatan saat ini dari kedua badan
            let (v_a, w_a, is_dyn_a, inv_mass_a) = {
                let b_a = bodies.get(&id_a).ok_or(SolverError::BodyNotFound(id_a))?;
                (
                    b_a.linear_velocity(),
                    b_a.angular_velocity(),
                    b_a.body_type() == BodyType::Dynamic,
                    b_a.mass_properties().inverse_mass,
                )
            };

            let (v_b, w_b, is_dyn_b, inv_mass_b) = {
                let b_b = bodies.get(&id_b).ok_or(SolverError::BodyNotFound(id_b))?;
                (
                    b_b.linear_velocity(),
                    b_b.angular_velocity(),
                    b_b.body_type() == BodyType::Dynamic,
                    b_b.mass_properties().inverse_mass,
                )
            };

            // Hitung kecepatan relatif pada titik kontak:
            // vA(P) = vA + wA x rA
            // vB(P) = vB + wB x rB
            let v_a_contact = v_a + w_a.cross(constraint.r_a);
            let v_b_contact = v_b + w_b.cross(constraint.r_b);
            let v_rel = v_b_contact - v_a_contact;
            let v_n = v_rel.dot(constraint.contact.normal);

            // FORMULA BAUMGARTE KRITIS (KONVENSI A -> B):
            // delta_lambda = (bias - v_n) * effective_mass
            let delta_lambda_raw = (constraint.bias - v_n) * constraint.effective_mass;

            // Klem impuls akumulasi agar tidak pernah menarik (unilateral constraint: lambda >= 0)
            let lambda_new = (constraint.accumulated_impulse + delta_lambda_raw).max(0.0);
            let delta_lambda = lambda_new - constraint.accumulated_impulse;
            constraint.accumulated_impulse = lambda_new;

            if delta_lambda.abs() <= 1e-8 {
                continue;
            }

            let impulse_vec = delta_lambda * constraint.contact.normal;

            // Terapkan impuls pada Badan A (menerima -J) jika Dinamis
            if is_dyn_a {
                let b_a = bodies
                    .get_mut(&id_a)
                    .ok_or(SolverError::BodyNotFound(id_a))?;
                let new_v_a = b_a.linear_velocity() - inv_mass_a * impulse_vec;
                let new_w_a = b_a.angular_velocity()
                    - constraint.inv_inertia_world_a * (constraint.r_a.cross(impulse_vec));

                if !new_v_a.is_finite() || !new_w_a.is_finite() {
                    return Err(SolverError::NonFiniteBodyState);
                }

                b_a.set_linear_velocity(new_v_a)
                    .map_err(|_| SolverError::NonFiniteBodyState)?;
                b_a.set_angular_velocity(new_w_a)
                    .map_err(|_| SolverError::NonFiniteBodyState)?;
            }

            // Terapkan impuls pada Badan B (menerima +J) jika Dinamis
            if is_dyn_b {
                let b_b = bodies
                    .get_mut(&id_b)
                    .ok_or(SolverError::BodyNotFound(id_b))?;
                let new_v_b = b_b.linear_velocity() + inv_mass_b * impulse_vec;
                let new_w_b = b_b.angular_velocity()
                    + constraint.inv_inertia_world_b * (constraint.r_b.cross(impulse_vec));

                if !new_v_b.is_finite() || !new_w_b.is_finite() {
                    return Err(SolverError::NonFiniteBodyState);
                }

                b_b.set_linear_velocity(new_v_b)
                    .map_err(|_| SolverError::NonFiniteBodyState)?;
                b_b.set_angular_velocity(new_w_b)
                    .map_err(|_| SolverError::NonFiniteBodyState)?;
            }
        }
    }

    Ok(())
}
