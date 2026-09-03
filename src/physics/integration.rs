use glam::{Quat, Vec3};
use std::collections::BTreeMap;
use std::fmt;

use super::broadphase::{BodyType, RigidBodyId};
use super::rigid_body::RigidBody;

/// Kesalahan integrasi status fisik badan kaku.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrationError {
    /// Timestep delta time non-finite, <= 0.0, atau NaN/Infinity
    InvalidTimestep,
    /// Gravitasi memuat komponen non-finite (NaN atau Infinity)
    InvalidGravity,
    /// Status badan kaku memuat koordinat posisi atau kecepatan non-finite
    NonFiniteState,
    /// Orientasi rotasi quaternion tidak valid (non-finite atau zero-length)
    InvalidRotation,
}

impl fmt::Display for IntegrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTimestep => write!(f, "Timestep dt harus finite dan bernilai > 0.0"),
            Self::InvalidGravity => write!(f, "Vektor gravitasi memuat nilai non-finite"),
            Self::NonFiniteState => write!(f, "Status badan kaku memuat nilai non-finite"),
            Self::InvalidRotation => write!(
                f,
                "Rotasi quaternion tidak valid (non-finite atau panjang nol)"
            ),
        }
    }
}

impl std::error::Error for IntegrationError {}

/// Konfigurasi opsional/transien untuk integrasi status badan kaku.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntegrationConfig {
    /// Vektor gravitasi dunia (m/s^2)
    pub gravity: Vec3,
}

impl Default for IntegrationConfig {
    fn default() -> Self {
        Self {
            gravity: Vec3::new(0.0, -9.81, 0.0),
        }
    }
}

/// Mengintegrasikan rotasi quaternion menggunakan kecepatan sudut di ruang dunia (world-space).
///
/// FORMULA INTEGRASI (PERSAMAAN DIFERENSIAL QUATERNION RUANG DUNIA):
/// $$\frac{dq}{dt} = \frac{1}{2} \Omega(\vec{\omega}) \cdot q$$
/// di mana $\Omega(\vec{\omega}) = (w_x, w_y, w_z, 0)$ dan perkalian kuaternion adalah:
/// `omega_quat * q` (perkalian kiri, karena $\vec{\omega}$ berada dalam ruang dunia).
///
/// Nilai diskret orde pertama:
/// $$dq = \frac{1}{2} \Omega(\vec{\omega}) \cdot q \cdot \Delta t$$
/// $$q_{\text{candidate}} = q + dq$$
/// $$q_{\text{new}} = \text{normalize}(q_{\text{candidate}})$$
pub fn integrate_rotation(q: Quat, omega: Vec3, dt: f32) -> Result<Quat, IntegrationError> {
    if !dt.is_finite() || dt <= 0.0 {
        return Err(IntegrationError::InvalidTimestep);
    }
    if !omega.is_finite() {
        return Err(IntegrationError::NonFiniteState);
    }
    if !q.is_finite() || q.length_squared() < 1e-8 {
        return Err(IntegrationError::InvalidRotation);
    }

    // Jika kecepatan sudut nol, rotasi tidak berubah
    if omega.length_squared() <= 1e-12 {
        return Ok(q.normalize());
    }

    // Kuaternion kecepatan sudut murni (vektor imaginer = omega, skalar real = 0)
    let omega_quat = Quat::from_xyzw(omega.x, omega.y, omega.z, 0.0);

    // Perkalian kiri kuaternion dunia: dq = 0.5 * omega_quat * q * dt
    let half_dt = 0.5 * dt;
    let dq = (omega_quat * q) * half_dt;

    let q_cand = Quat::from_xyzw(q.x + dq.x, q.y + dq.y, q.z + dq.z, q.w + dq.w);
    if !q_cand.is_finite() || q_cand.length_squared() < 1e-8 {
        return Err(IntegrationError::InvalidRotation);
    }

    Ok(q_cand.normalize())
}

/// Mengintegrasikan kecepatan linier badan kaku dari gravitasi luar (semi-implicit Euler).
///
/// SEMANTIK TIPE BADAN:
/// - Static: Tidak pernah terakselerasi oleh gravitasi.
/// - Kinematic: Kecepatan authored secara eksternal; imun terhadap gravitasi solver.
/// - Dynamic: Kecepatan dimutasi oleh: $v_{\text{new}} = v_{\text{old}} + g \cdot \Delta t$.
pub fn integrate_velocity(
    body: &mut RigidBody,
    dt: f32,
    gravity: Vec3,
) -> Result<(), IntegrationError> {
    if !dt.is_finite() || dt <= 0.0 {
        return Err(IntegrationError::InvalidTimestep);
    }
    if !gravity.is_finite() {
        return Err(IntegrationError::InvalidGravity);
    }

    if body.body_type() != BodyType::Dynamic || body.is_sleeping() {
        return Ok(());
    }

    let cand_v = body.linear_velocity() + gravity * dt;
    if !cand_v.is_finite() {
        return Err(IntegrationError::NonFiniteState);
    }

    body.set_linear_velocity(cand_v)
        .map_err(|_| IntegrationError::NonFiniteState)?;
    Ok(())
}

/// Mengintegrasikan posisi dan rotasi badan kaku dari status kecepatannya saat ini.
///
/// SEMANTIK TIPE BADAN:
/// - Static: Posisi dan rotasi tidak pernah berubah.
/// - Kinematic: Posisi dan rotasi maju berdasarkan kecepatan eksternal.
/// - Dynamic: Posisi dan rotasi maju berdasarkan kecepatan hasil simulasi (dilewati jika Sleeping).
pub fn integrate_transform(body: &mut RigidBody, dt: f32) -> Result<(), IntegrationError> {
    if !dt.is_finite() || dt <= 0.0 {
        return Err(IntegrationError::InvalidTimestep);
    }

    if body.body_type() == BodyType::Static || body.is_sleeping() {
        return Ok(());
    }

    let cand_pos = body.position() + body.linear_velocity() * dt;
    if !cand_pos.is_finite() {
        return Err(IntegrationError::NonFiniteState);
    }

    let cand_rot = integrate_rotation(body.rotation(), body.angular_velocity(), dt)?;

    body.set_position(cand_pos)
        .map_err(|_| IntegrationError::NonFiniteState)?;
    body.set_rotation(cand_rot)
        .map_err(|_| IntegrationError::InvalidRotation)?;

    Ok(())
}

/// Mengintegrasikan satu badan kaku secara atomik (kecepatan dari gravitasi, lalu transform).
pub fn integrate_body(
    body: &mut RigidBody,
    dt: f32,
    gravity: Vec3,
) -> Result<(), IntegrationError> {
    if !dt.is_finite() || dt <= 0.0 {
        return Err(IntegrationError::InvalidTimestep);
    }
    if !gravity.is_finite() {
        return Err(IntegrationError::InvalidGravity);
    }

    if body.body_type() == BodyType::Static || body.is_sleeping() {
        return Ok(());
    }

    let (cand_v, cand_w) = match body.body_type() {
        BodyType::Dynamic => {
            let v = body.linear_velocity() + gravity * dt;
            if !v.is_finite() {
                return Err(IntegrationError::NonFiniteState);
            }
            (v, body.angular_velocity())
        }
        BodyType::Kinematic => (body.linear_velocity(), body.angular_velocity()),
        BodyType::Static => unreachable!(),
    };

    let cand_pos = body.position() + cand_v * dt;
    if !cand_pos.is_finite() {
        return Err(IntegrationError::NonFiniteState);
    }

    let cand_rot = integrate_rotation(body.rotation(), cand_w, dt)?;

    // Komit transaksional
    if body.body_type() == BodyType::Dynamic {
        body.set_linear_velocity(cand_v)
            .map_err(|_| IntegrationError::NonFiniteState)?;
    }
    body.set_position(cand_pos)
        .map_err(|_| IntegrationError::NonFiniteState)?;
    body.set_rotation(cand_rot)
        .map_err(|_| IntegrationError::InvalidRotation)?;

    Ok(())
}

/// Mengintegrasikan kecepatan seluruh badan kaku dari gravitasi secara dua-tahap transaksional.
pub fn integrate_velocities(
    bodies: &mut BTreeMap<RigidBodyId, RigidBody>,
    dt: f32,
    gravity: Vec3,
) -> Result<(), IntegrationError> {
    if !dt.is_finite() || dt <= 0.0 {
        return Err(IntegrationError::InvalidTimestep);
    }
    if !gravity.is_finite() {
        return Err(IntegrationError::InvalidGravity);
    }

    // PASS 1: Evaluasi & Validasi Seluruh Kandidat
    for body in bodies.values() {
        if body.body_type() == BodyType::Dynamic && !body.is_sleeping() {
            let cand_v = body.linear_velocity() + gravity * dt;
            if !cand_v.is_finite() {
                return Err(IntegrationError::NonFiniteState);
            }
        }
    }

    // PASS 2: Komit
    for body in bodies.values_mut() {
        if body.body_type() == BodyType::Dynamic && !body.is_sleeping() {
            let cand_v = body.linear_velocity() + gravity * dt;
            let _ = body.set_linear_velocity(cand_v);
        }
    }

    Ok(())
}

/// Mengintegrasikan posisi dan rotasi seluruh badan kaku secara dua-tahap transaksional.
pub fn integrate_transforms(
    bodies: &mut BTreeMap<RigidBodyId, RigidBody>,
    dt: f32,
) -> Result<(), IntegrationError> {
    if !dt.is_finite() || dt <= 0.0 {
        return Err(IntegrationError::InvalidTimestep);
    }

    // PASS 1: Evaluasi & Validasi Seluruh Kandidat Transform
    for body in bodies.values() {
        if body.body_type() != BodyType::Static && !body.is_sleeping() {
            let cand_pos = body.position() + body.linear_velocity() * dt;
            if !cand_pos.is_finite() {
                return Err(IntegrationError::NonFiniteState);
            }
            integrate_rotation(body.rotation(), body.angular_velocity(), dt)?;
        }
    }

    // PASS 2: Komit
    for body in bodies.values_mut() {
        if body.body_type() != BodyType::Static && !body.is_sleeping() {
            let cand_pos = body.position() + body.linear_velocity() * dt;
            let cand_rot = integrate_rotation(body.rotation(), body.angular_velocity(), dt)?;
            let _ = body.set_position(cand_pos);
            let _ = body.set_rotation(cand_rot);
        }
    }

    Ok(())
}

/// Mengintegrasikan seluruh badan kaku (kecepatan dari gravitasi, lalu posisi/rotasi) secara transaksional.
pub fn integrate_bodies(
    bodies: &mut BTreeMap<RigidBodyId, RigidBody>,
    dt: f32,
    gravity: Vec3,
) -> Result<(), IntegrationError> {
    if !dt.is_finite() || dt <= 0.0 {
        return Err(IntegrationError::InvalidTimestep);
    }
    if !gravity.is_finite() {
        return Err(IntegrationError::InvalidGravity);
    }

    // PASS 1: Validasi seluruh kandidat sebelum mutasi apa pun
    for body in bodies.values() {
        if body.body_type() != BodyType::Static && !body.is_sleeping() {
            let cand_v = if body.body_type() == BodyType::Dynamic {
                let v = body.linear_velocity() + gravity * dt;
                if !v.is_finite() {
                    return Err(IntegrationError::NonFiniteState);
                }
                v
            } else {
                body.linear_velocity()
            };

            let cand_pos = body.position() + cand_v * dt;
            if !cand_pos.is_finite() {
                return Err(IntegrationError::NonFiniteState);
            }

            integrate_rotation(body.rotation(), body.angular_velocity(), dt)?;
        }
    }

    // PASS 2: Komit perubahan
    for body in bodies.values_mut() {
        if body.body_type() != BodyType::Static && !body.is_sleeping() {
            if body.body_type() == BodyType::Dynamic {
                let cand_v = body.linear_velocity() + gravity * dt;
                let _ = body.set_linear_velocity(cand_v);
            }
            let cand_pos = body.position() + body.linear_velocity() * dt;
            let cand_rot = integrate_rotation(body.rotation(), body.angular_velocity(), dt)?;
            let _ = body.set_position(cand_pos);
            let _ = body.set_rotation(cand_rot);
        }
    }

    Ok(())
}
