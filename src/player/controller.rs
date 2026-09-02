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

    /// State tombol lompat pada frame sebelumnya untuk deteksi rising edge (edge-triggered)
    pub prev_input_jump: bool,

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
            prev_input_jump: false,
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
            prev_input_jump: false,
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
        // Edge-triggered: hanya mencatat permintaan jika transisi dari false -> true (rising edge)!
        if input.jump && !self.prev_input_jump {
            self.state.jump_requested = true;
        }
        self.prev_input_jump = input.jump;
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

    /// Mengevaluasi transisi status jongkok dan clearance langit-langit (8B.5).
    ///
    /// INVARIANTS:
    /// - Saat input.crouch = true: pemain langsung berjongkok.
    /// - Saat input.crouch = false: pemain mencoba berdiri; diperiksa clearance kapsul berdiri penuh.
    /// - Jika terhalang langit-langit rendah: pemain tetap jongkok (forced_crouch = true).
    /// - Jika clearance bebas: pemain berdiri (crouching = false, forced_crouch = false).
    /// - Telapak kaki (feet_pos) tetap stabil (zero foot teleportation)!
    pub fn update_crouch_state(&mut self, store: &crate::streaming::store::ChunkStore) {
        if self.input.crouch {
            self.state.crouching = true;
            self.state.forced_crouch = false;
        } else if self.state.crouching {
            let has_clearance = super::collision::check_capsule_clearance(
                self.state.position,
                self.config.standing_height,
                self.config.capsule_radius,
                store,
            );

            if has_clearance {
                self.state.crouching = false;
                self.state.forced_crouch = false;
            } else {
                self.state.crouching = true;
                self.state.forced_crouch = true;
            }
        }
    }

    /// Mengeksekusi permintaan lompat jika memenuhi syarat grounded (8B.6).
    ///
    /// INVARIANTS:
    /// - Lompat hanya diizinkan jika dan hanya jika `grounded == true`.
    /// - Single-consumption: `jump_requested` langsung dikonsumsi (reset ke false).
    /// - Menahan tombol spasi tidak akan memicu lompatan berulang saat mendarat.
    /// - Memberikan kecepatan vertikal ke atas: `velocity.y = config.jump_velocity` (6.0 m/s).
    /// - Seketika mengubah status menjadi lepas landas: `grounded = false`.
    pub fn try_execute_jump(&mut self) -> bool {
        if self.state.jump_requested {
            // Konsumsi permintaan lompat (single-consumption)
            self.state.jump_requested = false;

            if self.state.grounded {
                self.state.velocity.y = self.config.jump_velocity;
                self.state.grounded = false;
                return true;
            }
        }
        false
    }

    /// Menjalankan satu langkah simulasi fisika kinematik berwaktu tetap (30 Hz substep).
    pub fn step_simulation(
        &mut self,
        fixed_dt: f32,
        store: &crate::streaming::store::ChunkStore,
        camera_yaw_deg: f32,
    ) {
        // 1. Evaluasi transisi jongkok dan clearance
        self.update_crouch_state(store);

        // 2. Evaluasi status sprinting
        self.update_movement_states();

        // 3. Eksekusi lompat jika ada permintaan dan sedang grounded
        self.try_execute_jump();

        // 4. Hitung niat gerak horizontal di bidang XZ
        let move_intent = self.compute_horizontal_intent(camera_yaw_deg);
        let target_speed = self.current_target_speed();

        let start_tick = std::time::Instant::now();

        self.state.velocity.x = move_intent.x * target_speed;
        self.state.velocity.z = move_intent.z * target_speed;

        // 5. Integrasi gravitasi kinematik jika di udara (airborne)
        if !self.state.grounded {
            self.state.velocity.y += self.config.gravity * fixed_dt;
        }

        // 6. Swept collision resolution kontinu per sumbu X -> Z -> Y (8B.8)
        let mut capsule = self.current_capsule();
        let desired_delta = self.state.velocity * fixed_dt;
        let stats = super::collision::resolve_swept_step(
            &mut capsule,
            &mut self.state.velocity,
            desired_delta,
            store,
        );

        self.state.position = capsule.base;
        self.collision_queries_total += stats.queries_count;
        self.collision_hits_total += stats.hits_count;
        self.unknown_blocked_total += stats.unknown_hits_count;

        // 7. Evaluasi tumpuan tanah (Ground Detection)
        let ground = super::collision::check_ground_support(
            self.state.position,
            self.config.capsule_radius,
            self.config.ground_contact_epsilon,
            store,
        );

        if ground.grounded && self.state.velocity.y <= 0.0 {
            self.state.grounded = true;
            self.state.ground_normal = ground.ground_normal;
            self.state.ground_distance = ground.ground_distance;
            self.state.velocity.y = 0.0;
            if let Some(surf_y) = ground.ground_y_surface {
                self.state.position.y = surf_y;
            }
        } else {
            self.state.grounded = false;
            self.state.ground_distance = ground.ground_distance;
        }

        // 8. Stationary ticks tracking
        if self.state.speed() < 0.01 {
            self.state.ticks_stationary = self.state.ticks_stationary.saturating_add(1);
        } else {
            self.state.ticks_stationary = 0;
        }

        self.last_tick_duration_us = start_tick.elapsed().as_micros() as f32;
    }

    /// Memperbarui simulasi pemain dengan akumulator fixed-timestep 30 Hz (8B.7).
    ///
    /// INVARIANTS:
    /// - Simulasi selalu dieksekusi dengan langkah fixed_timestep (1/30 detik).
    /// - Deterministik dan frame-rate independent di bawah cadence normal (30, 60, 120 FPS).
    /// - Memiliki batas kompensasi substep per frame (bounded catch-up) untuk mencegah spiral of death.
    pub fn update_fixed_time(
        &mut self,
        delta_seconds: f32,
        store: &crate::streaming::store::ChunkStore,
        camera_yaw_deg: f32,
    ) {
        let clamped_dt = delta_seconds.min(self.config.max_dt_clamp);
        self.time_accumulator += clamped_dt;

        let fixed_dt = self.config.fixed_timestep;
        let mut substeps = 0;

        while self.time_accumulator >= fixed_dt && substeps < self.config.max_substeps_per_frame {
            self.step_simulation(fixed_dt, store, camera_yaw_deg);
            self.time_accumulator -= fixed_dt;
            substeps += 1;
        }

        // Bounded catch-up: cegah akumulasi tak berbatas jika frame stall ekstrim
        if self.time_accumulator >= fixed_dt {
            self.time_accumulator = 0.0;
        }
    }
}
