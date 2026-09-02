use glam::{Vec2, Vec3};

use super::collider::Capsule;
use super::config::PlayerConfig;
use super::state::PlayerState;

/// Input pergerakan pemain yang disampel pada cadence render / window event
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PlayerInput {
    /// Input maju / mundur: +1.0 = maju (W), -1.0 = mundur (S)
    pub move_forward: f32,
    /// Input kanan / kiri: +1.0 = kanan (D), -1.0 = kiri (A)
    pub move_right: f32,
    /// Apakah tombol sprint (Shift) sedang ditekan
    pub sprint: bool,
    /// Apakah tombol crouch (C / Ctrl) sedang ditekan
    pub crouch: bool,
    /// Permintaan lompat (Space) yang akan dikonsumsi secara edge-triggered
    pub jump: bool,
}

impl PlayerInput {
    /// Mengonversi input keyboard biner ke nilai float [-1.0..1.0]
    pub fn from_raw(
        is_w: bool,
        is_s: bool,
        is_a: bool,
        is_d: bool,
        sprint: bool,
        crouch: bool,
        jump: bool,
    ) -> Self {
        let mut forward = 0.0f32;
        if is_w {
            forward += 1.0;
        }
        if is_s {
            forward -= 1.0;
        }

        let mut right = 0.0f32;
        if is_d {
            right += 1.0;
        }
        if is_a {
            right -= 1.0;
        }

        Self {
            move_forward: forward,
            move_right: right,
            sprint,
            crouch,
            jump,
        }
    }
}

/// Kinematic Capsule Character Controller (Phase 8B)
pub struct PlayerController {
    pub state: PlayerState,
    pub config: PlayerConfig,
    pub input: PlayerInput,

    /// Akumulator waktu untuk simulasi fixed-timestep 30 Hz
    pub time_accumulator: f32,

    // Metrik telemetri runtime
    pub collision_queries_total: u64,
    pub collision_hits_total: u64,
    pub unknown_blocked_total: u64,
    pub last_tick_duration_us: f32,
}

impl Default for PlayerController {
    fn default() -> Self {
        Self::new(Vec3::ZERO)
    }
}

impl PlayerController {
    pub fn new(spawn_position: Vec3) -> Self {
        Self {
            state: PlayerState::new(spawn_position),
            config: PlayerConfig::default(),
            input: PlayerInput::default(),
            time_accumulator: 0.0,
            collision_queries_total: 0,
            collision_hits_total: 0,
            unknown_blocked_total: 0,
            last_tick_duration_us: 0.0,
        }
    }

    pub fn with_config(spawn_position: Vec3, config: PlayerConfig) -> Self {
        Self {
            state: PlayerState::new(spawn_position),
            config,
            input: PlayerInput::default(),
            time_accumulator: 0.0,
            collision_queries_total: 0,
            collision_hits_total: 0,
            unknown_blocked_total: 0,
            last_tick_duration_us: 0.0,
        }
    }

    /// Mendapatkan kapsul geometris pemain saat ini berdasarkan status berdiri/jongkok
    #[inline(always)]
    pub fn current_capsule(&self) -> Capsule {
        let height = if self.state.crouching {
            self.config.crouching_height
        } else {
            self.config.standing_height
        };
        Capsule::new(self.state.position, self.config.capsule_radius, height)
    }

    /// Koordinat posisi mata kamera (eye position) pemain
    #[inline(always)]
    pub fn eye_position(&self) -> Vec3 {
        self.state.position + self.config.eye_offset(self.state.crouching)
    }

    /// Menerima sampel input baru dari loop window/render
    pub fn set_input(&mut self, input: PlayerInput) {
        // Jika input meminta lompat, catat permintaan (edge-triggered)
        if input.jump {
            self.state.jump_requested = true;
        }
        self.input = input;
    }

    /// Menghitung vektor niat gerak horizontal di bidang XZ relatif terhadap arah hadap kamera (8B.3).
    ///
    /// INVARIANTS:
    /// - Pitch vertikal kamera diabaikan sepenuhnya (tidak ada penerbangan vertikal dari gerak mouse).
    /// - Vektor input diagonal dinormalisasi sehingga kecepatan W+D sama dengan W (tidak ada speed exploit 5*sqrt(2)).
    pub fn compute_horizontal_intent(&self, camera_yaw_deg: f32) -> Vec3 {
        let raw_input = Vec2::new(self.input.move_right, self.input.move_forward);
        if raw_input.length_squared() < 0.0001 {
            return Vec3::ZERO;
        }

        // Normalisasi diagonal (W+D menghasilkan panjang maks 1.0)
        let input_dir = if raw_input.length_squared() > 1.0 {
            raw_input.normalize()
        } else {
            raw_input
        };

        let yaw_rad = camera_yaw_deg.to_radians();
        // Forward pada bidang XZ (Y = 0)
        let forward_xz = Vec3::new(yaw_rad.cos(), 0.0, yaw_rad.sin()).normalize();
        // Right pada bidang XZ = forward x Vec3::Y
        let right_xz = forward_xz.cross(Vec3::Y).normalize();

        (forward_xz * input_dir.y + right_xz * input_dir.x).normalize_or_zero()
    }

    /// Menghitung kecepatan target horizontal saat ini berdasarkan status jalan / lari / jongkok
    pub fn current_target_speed(&self) -> f32 {
        if self.state.crouching {
            // Prioritas: Crouching > Sprinting
            self.config.crouch_speed
        } else if self.input.sprint && self.input_has_movement() {
            self.config.sprint_speed
        } else {
            self.config.walk_speed
        }
    }

    /// Memeriksa apakah ada input pergerakan yang signifikan
    #[inline(always)]
    pub fn input_has_movement(&self) -> bool {
        (self.input.move_forward.abs() > 0.01) || (self.input.move_right.abs() > 0.01)
    }

    /// Memperbarui state sprinting berdasarkan input dan prioritas (8B.4).
    ///
    /// INVARIANTS:
    /// - Crouching > Sprinting: jika sedang jongkok, sprint otomatis ditekan/dibatalkan.
    /// - Sprint membutuhkan input pergerakan nyata (Shift saja tidak mengaktifkan sprint).
    pub fn update_movement_states(&mut self) {
        if self.state.crouching {
            self.state.sprinting = false;
        } else {
            self.state.sprinting = self.input.sprint && self.input_has_movement();
        }
    }
}
