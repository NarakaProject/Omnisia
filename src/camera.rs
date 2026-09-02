use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use winit::event::{ElementState, KeyEvent, MouseButton};
use winit::keyboard::{KeyCode, PhysicalKey};

/// Uniform struct untuk binding ke GPU
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_proj: [f32; 16],
    pub camera_pos: [f32; 3],
    pub _pad0: f32,
}

impl Default for CameraUniform {
    fn default() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array(),
            camera_pos: [0.0; 3],
            _pad0: 0.0,
        }
    }
}

/// Kamera 3D FPS / Orbital terisolasi dari renderer
pub struct Camera {
    pub position: Vec3,
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub fov_y_rad: f32,
    pub z_near: f32,
    pub z_far: f32,

    pub speed: f32,
    pub sensitivity: f32,

    // Input state tracking
    is_forward: bool,
    is_backward: bool,
    is_left: bool,
    is_right: bool,
    is_up: bool,
    is_down: bool,
    pub is_mouse_dragging: bool,
}

impl Camera {
    pub fn new(position: Vec3, yaw_deg: f32, pitch_deg: f32) -> Self {
        Self {
            position,
            yaw_deg,
            pitch_deg,
            fov_y_rad: 60.0f32.to_radians(),
            z_near: 0.1,
            z_far: 1000.0,
            speed: 18.0,        // 18 m/s
            sensitivity: 0.15, // Derajat per piksel gerak mouse
            is_forward: false,
            is_backward: false,
            is_left: false,
            is_right: false,
            is_up: false,
            is_down: false,
            is_mouse_dragging: false,
        }
    }

    /// Menghitung vektor arah hadap (forward vector)
    pub fn forward(&self) -> Vec3 {
        let yaw_rad = self.yaw_deg.to_radians();
        let pitch_rad = self.pitch_deg.to_radians();

        Vec3::new(
            yaw_rad.cos() * pitch_rad.cos(),
            pitch_rad.sin(),
            yaw_rad.sin() * pitch_rad.cos(),
        )
        .normalize()
    }

    /// Menghitung vektor kanan (right vector)
    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize()
    }

    /// Menghitung matriks View-Projection
    pub fn build_view_projection_matrix(&self, aspect: f32) -> Mat4 {
        let target = self.position + self.forward();
        let view = Mat4::look_at_rh(self.position, target, Vec3::Y);
        let proj = Mat4::perspective_rh(self.fov_y_rad, aspect, self.z_near, self.z_far);
        proj * view
    }

    /// Menghasilkan CameraUniform siap upload ke GPU
    pub fn build_uniform(&self, aspect: f32) -> CameraUniform {
        let view_proj = self.build_view_projection_matrix(aspect);
        CameraUniform {
            view_proj: view_proj.to_cols_array(),
            camera_pos: [self.position.x, self.position.y, self.position.z],
            _pad0: 0.0,
        }
    }

    /// Menangani event keyboard
    pub fn handle_keyboard(&mut self, key_event: &KeyEvent) -> bool {
        let pressed = key_event.state == ElementState::Pressed;
        if let PhysicalKey::Code(key) = key_event.physical_key {
            match key {
                KeyCode::KeyW => {
                    self.is_forward = pressed;
                    true
                }
                KeyCode::KeyS => {
                    self.is_backward = pressed;
                    true
                }
                KeyCode::KeyA => {
                    self.is_left = pressed;
                    true
                }
                KeyCode::KeyD => {
                    self.is_right = pressed;
                    true
                }
                KeyCode::Space => {
                    self.is_up = pressed;
                    true
                }
                KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                    self.is_down = pressed;
                    true
                }
                _ => false,
            }
        } else {
            false
        }
    }

    /// Menangani klik mouse
    pub fn handle_mouse_button(&mut self, button: MouseButton, state: ElementState) {
        if button == MouseButton::Left || button == MouseButton::Right {
            self.is_mouse_dragging = state == ElementState::Pressed;
        }
    }

    /// Menangani perpindahan mouse
    pub fn handle_mouse_motion(&mut self, dx: f64, dy: f64) {
        if self.is_mouse_dragging {
            self.yaw_deg += (dx as f32) * self.sensitivity;
            self.pitch_deg -= (dy as f32) * self.sensitivity;

            // Batasi pitch agar tidak gimbal-lock
            self.pitch_deg = self.pitch_deg.clamp(-89.0, 89.0);
        }
    }

    /// Update pergerakan kamera berdasarkan delta time (detik)
    pub fn update(&mut self, dt_secs: f32) {
        let forward = self.forward();
        let right = self.right();
        let up = Vec3::Y;

        let mut move_dir = Vec3::ZERO;
        if self.is_forward {
            move_dir += forward;
        }
        if self.is_backward {
            move_dir -= forward;
        }
        if self.is_right {
            move_dir += right;
        }
        if self.is_left {
            move_dir -= right;
        }
        if self.is_up {
            move_dir += up;
        }
        if self.is_down {
            move_dir -= up;
        }

        if move_dir.length_squared() > 0.001 {
            self.position += move_dir.normalize() * (self.speed * dt_secs);
        }
    }
}
