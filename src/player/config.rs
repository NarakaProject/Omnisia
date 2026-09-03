use glam::Vec3;

/// Konfigurasi terpusat untuk Kinematic Capsule Player Controller (Phase 8B)
#[derive(Debug, Clone, Copy)]
pub struct PlayerConfig {
    /// Tinggi total kapsul saat berdiri dalam meter (default: 1.8m = 3.6 voxel)
    pub standing_height: f32,
    /// Tinggi total kapsul saat jongkok dalam meter (default: 1.2m = 2.4 voxel)
    pub crouching_height: f32,
    /// Radius kapsul dan belahan bola dalam meter (default: 0.30m = 0.6 voxel)
    pub capsule_radius: f32,

    /// Kecepatan jalan normal horizontal dalam m/s (default: 3.0 m/s)
    pub walk_speed: f32,
    /// Kecepatan lari cepat (sprint) horizontal dalam m/s (default: 6.0 m/s)
    pub sprint_speed: f32,
    /// Kecepatan jalan saat jongkok horizontal dalam m/s (default: 1.6 m/s)
    pub crouch_speed: f32,

    /// Kecepatan vertikal awal saat lompat dalam m/s (default: 6.0 m/s)
    pub jump_velocity: f32,
    /// Akselerasi gravitasi pemain dalam m/s² (default: -9.81 m/s²)
    pub gravity: f32,

    /// Epsilon toleransi kontak tumpuan tanah dalam meter (default: 0.05m)
    pub ground_contact_epsilon: f32,

    /// Parameter batas tinggi maksimum auto-step traversal dalam meter (default: 0.55m = ~1 voxel + toleransi).
    pub step_height: f32,
    /// Apakah auto-step traversal aktif saat pemain grounded (default: true)
    pub auto_step_enabled: bool,

    /// Apakah mekanik melayang terikat (bounded glide) saat airborne aktif (default: true)
    pub glide_enabled: bool,
    /// Pengali akselerasi gravitasi saat meluncur/glide (default: 0.35)
    pub glide_gravity_multiplier: f32,
    /// Batas maksimum kecepatan jatuh ke bawah saat glide dalam m/s (default: 2.5 m/s)
    pub glide_max_downward_speed: f32,
    /// Pengali kontrol pergerakan horizontal di udara saat glide (default: 0.85)
    pub glide_air_control: f32,

    /// Interval waktu simulasi tetap dalam detik (default: 1/30 detik)
    pub fixed_timestep: f32,
    /// Batas maksimal substep per frame render untuk mencegah spiral of death (default: 5)
    pub max_substeps_per_frame: usize,
    /// Batas maksimal delta time render sebelum di-clamp (default: 0.25s)
    pub max_dt_clamp: f32,

    /// Tinggi mata kamera dari telapak kaki saat berdiri dalam meter (default: 1.62m)
    pub eye_height_standing: f32,
    /// Tinggi mata kamera dari telapak kaki saat jongkok dalam meter (default: 1.08m)
    pub eye_height_crouching: f32,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            standing_height: 1.8,
            crouching_height: 1.2,
            capsule_radius: 0.30,

            walk_speed: 3.0,
            sprint_speed: 6.0,
            crouch_speed: 1.6,

            jump_velocity: 6.0,
            gravity: -9.81,

            ground_contact_epsilon: 0.05,
            step_height: 0.55,
            auto_step_enabled: true,

            glide_enabled: true,
            glide_gravity_multiplier: 0.35,
            glide_max_downward_speed: 2.5,
            glide_air_control: 0.85,

            fixed_timestep: 1.0 / 30.0,
            max_substeps_per_frame: 5,
            max_dt_clamp: 0.25,

            eye_height_standing: 1.62,
            eye_height_crouching: 1.08,
        }
    }
}

impl PlayerConfig {
    /// Menghitung panjang segmen garis tengah kapsul saat berdiri
    #[inline(always)]
    pub fn standing_segment_length(&self) -> f32 {
        (self.standing_height - 2.0 * self.capsule_radius).max(0.0)
    }

    /// Menghitung panjang segmen garis tengah kapsul saat jongkok
    #[inline(always)]
    pub fn crouching_segment_length(&self) -> f32 {
        (self.crouching_height - 2.0 * self.capsule_radius).max(0.0)
    }

    /// Offset mata relatif terhadap posisi telapak kaki
    #[inline(always)]
    pub fn eye_offset(&self, crouching: bool) -> Vec3 {
        Vec3::new(
            0.0,
            if crouching {
                self.eye_height_crouching
            } else {
                self.eye_height_standing
            },
            0.0,
        )
    }
}
