use glam::Vec3;

use crate::camera::Camera;
use crate::environment::EnvironmentState;

/// Camera mode distinguishing standard gameplay from free-flight developer inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraMode {
    #[default]
    Player,
    Developer,
}

impl CameraMode {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Player => "Player (Kinematic Capsule)",
            Self::Developer => "Developer (Free Camera)",
        }
    }
}

/// Developer camera context holding developer camera transform and read-only player reference.
///
/// INVARIANTS:
/// - Developer camera transform is completely decoupled from player physics and position (Amendment 2).
/// - Player state is strictly read-only from developer console execution contexts (Amendment 3).
pub struct DeveloperCameraContext {
    /// Active camera mode.
    pub mode: CameraMode,
    /// Developer free-flight camera instance (owns position, speed, orientation).
    pub dev_camera: Camera,
    /// Read-only snapshot of player position for status reporting.
    player_pos: Vec3,
    /// Read-only snapshot of player eye position.
    player_eye_pos: Vec3,
}

impl DeveloperCameraContext {
    pub fn new(player_spawn: Vec3, dev_spawn: Vec3) -> Self {
        Self {
            mode: CameraMode::Player,
            dev_camera: Camera::new(dev_spawn, -90.0, -10.0),
            player_pos: player_spawn,
            player_eye_pos: player_spawn,
        }
    }

    /// Updates read-only player position snapshot (called from main loop).
    #[inline]
    pub fn sync_player_snapshot(&mut self, player_feet: Vec3, player_eye: Vec3) {
        self.player_pos = player_feet;
        self.player_eye_pos = player_eye;
    }

    /// Sets camera mode.
    #[inline]
    pub fn set_mode(&mut self, mode: CameraMode) {
        // If switching to developer mode for the first time, sync to player eye position
        if self.mode == CameraMode::Player && mode == CameraMode::Developer {
            self.dev_camera.position = self.player_eye_pos;
        }
        self.mode = mode;
    }

    #[inline]
    pub fn mode(&self) -> CameraMode {
        self.mode
    }

    #[inline]
    pub fn is_developer(&self) -> bool {
        self.mode == CameraMode::Developer
    }

    /// Read-only snapshot of player position (feet).
    #[inline]
    pub fn player_position(&self) -> Vec3 {
        self.player_pos
    }

    /// Read-only snapshot of player eye position.
    #[inline]
    pub fn player_eye_position(&self) -> Vec3 {
        self.player_eye_pos
    }

    /// Returns current active camera position based on mode.
    #[inline]
    pub fn active_camera_position(&self) -> Vec3 {
        match self.mode {
            CameraMode::Player => self.player_eye_pos,
            CameraMode::Developer => self.dev_camera.position,
        }
    }

    /// Sets developer camera position (strictly affects Developer camera only).
    #[inline]
    pub fn set_dev_position(&mut self, pos: Vec3) {
        self.dev_camera.position = pos;
    }

    /// Sets developer camera speed in meters/second (finite, positive).
    pub fn set_dev_speed(&mut self, speed: f32) -> Result<(), &'static str> {
        if !speed.is_finite() || speed <= 0.0 {
            return Err("expected positive finite speed value");
        }
        if speed > 2000.0 {
            return Err("speed exceeds developer maximum of 2000 m/s");
        }
        self.dev_camera.speed = speed;
        Ok(())
    }
}

/// Execution context passed to console commands during dispatch.
///
/// FIREWALL:
/// - Commands can mutate developer camera state and environment time state.
/// - Commands CANNOT mutate player physics, ChunkStore, CSG, or persistence.
pub struct DeveloperExecutionContext<'a> {
    pub camera: &'a mut DeveloperCameraContext,
    pub environment: &'a mut EnvironmentState,
    pub resident_chunks: usize,
    pub fps: f32,
    pub frame_time_ms: f32,
}
