use glam::Vec3;

/// State pergerakan otoritatif pemain dalam simulasi kinematik (Phase 8D.5)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MovementState {
    #[default]
    Grounded,
    Sprinting,
    JumpAscending,
    JumpDescending,
    Falling,
    Gliding,
}

/// Asal muasal status di udara (airborne sequence origin) (Phase 8D.5)
/// INVARIAN: Tidak dapat bermutasi di tengah urutan airborne kecuali saat mendarat atau external displacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AirborneOrigin {
    #[default]
    None,
    NormalJump,
    SprintJump,
    FellFromEdge,
    ExternalDisplacement,
}

/// State runtime lengkap pemain dalam simulasi kinematik
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerState {
    /// Posisi titik dasar telapak kaki pemain (y_feet) dalam meter
    pub position: Vec3,
    /// Kecepatan linier pemain dalam meter per detik (m/s)
    pub velocity: Vec3,

    /// State pergerakan tingkat tinggi otoritatif (Phase 8D.5)
    pub movement_state: MovementState,
    /// Asal muasal mengapa pemain berada di udara (Phase 8D.5)
    pub airborne_origin: AirborneOrigin,

    /// Apakah pemain saat ini bertumpu stabil di atas tanah solid yang dimuat
    pub grounded: bool,
    /// Vektor normal permukaan tumpuan tanah (default: (0, 1, 0) ke atas)
    pub ground_normal: Vec3,
    /// Jarak vertikal ke permukaan tumpuan terdekat dalam meter
    pub ground_distance: f32,

    /// Status aktif berjongkok (crouch)
    pub crouching: bool,
    /// Status aktif berlari cepat (sprint)
    pub sprinting: bool,
    /// Status aktif melayang terikat saat di udara (airborne glide)
    pub gliding: bool,
    /// Status tertahan jongkok karena langit-langit rendah (ceiling clearance check)
    pub forced_crouch: bool,

    /// Flag permintaan lompat satu kali konsumsi (edge-triggered)
    pub jump_requested: bool,

    /// Jumlah tick berturut-turut pemain diam
    pub ticks_stationary: u32,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            movement_state: MovementState::Grounded,
            airborne_origin: AirborneOrigin::None,
            grounded: false,
            ground_normal: Vec3::Y,
            ground_distance: 0.0,
            crouching: false,
            sprinting: false,
            gliding: false,
            forced_crouch: false,
            jump_requested: false,
            ticks_stationary: 0,
        }
    }
}

impl PlayerState {
    pub fn new(spawn_position: Vec3) -> Self {
        Self {
            position: spawn_position,
            ..Default::default()
        }
    }

    /// Kecepatan horizontal (magnitude di bidang XZ) dalam m/s
    #[inline(always)]
    pub fn horizontal_speed(&self) -> f32 {
        Vec3::new(self.velocity.x, 0.0, self.velocity.z).length()
    }

    /// Total kecepatan skalar (3D magnitude) dalam m/s
    #[inline(always)]
    pub fn speed(&self) -> f32 {
        self.velocity.length()
    }
}
