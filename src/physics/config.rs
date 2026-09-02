use glam::Vec3;

/// Konfigurasi runtime fisika untuk Omnisia Phase 8A
#[derive(Debug, Clone)]
pub struct PhysicsConfig {
    /// Percepatan gravitasi dunia dalam m/s² (konvensi Y-up standar)
    pub world_gravity: Vec3,
    /// Frekuensi loop fisika fixed-timestep (default 30 Hz)
    pub fixed_timestep_hz: f32,
    /// Durasi satu tick fisika dalam detik (1.0 / 30.0 = 0.03333... detik)
    pub fixed_dt: f32,
    /// Ambang batas kecepatan maksimum untuk dianggap diam (m/s)
    pub sleep_velocity_threshold: f32,
    /// Jumlah ticks berturut-turut diam sebelum masuk ke status Sleeping (15 ticks = 0.5 detik)
    pub sleep_ticks_required: u32,
    /// Batas maksimum catch-up substeps per frame render untuk mencegah spiral of death
    pub max_substeps_per_frame: usize,
    /// Batas clamping akumulator delta time (detik) pada pathological stalls
    pub max_dt_clamp: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        let hz = 30.0;
        Self {
            world_gravity: Vec3::new(0.0, -9.81, 0.0),
            fixed_timestep_hz: hz,
            fixed_dt: 1.0 / hz,
            sleep_velocity_threshold: 0.05,
            sleep_ticks_required: 15,
            max_substeps_per_frame: 5,
            max_dt_clamp: 0.25,
        }
    }
}
