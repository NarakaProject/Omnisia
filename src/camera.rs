use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Mat4, Vec3, Vec4};
use winit::event::{ElementState, KeyEvent, MouseButton};
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::coord::CHUNK_WORLD_SIZE;

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

/// Bidang frustum ternormalisasi: $a \cdot x + b \cdot y + c \cdot z + d = 0$
#[derive(Debug, Clone, Copy)]
pub struct FrustumPlane {
    pub normal: Vec3,
    pub distance: f32,
}

impl FrustumPlane {
    #[inline]
    pub fn from_vec4(v: Vec4) -> Self {
        let normal = Vec3::new(v.x, v.y, v.z);
        let length = normal.length();
        if length > 1e-6 {
            let inv_len = 1.0 / length;
            Self {
                normal: normal * inv_len,
                distance: v.w * inv_len,
            }
        } else {
            Self {
                normal: Vec3::ZERO,
                distance: 0.0,
            }
        }
    }

    /// Evaluasi jarak titik ke bidang frustum
    #[inline]
    pub fn distance_to_point(&self, point: Vec3) -> f32 {
        self.normal.dot(point) + self.distance
    }
}

/// Kamera view frustum yang terdiri dari 6 bidang (Left, Right, Bottom, Top, Near, Far)
#[derive(Debug, Clone, Copy)]
pub struct Frustum {
    pub planes: [FrustumPlane; 6],
}

impl Frustum {
    /// Ekstraksi 6 bidang frustum dari matriks View-Projection (wgpu NDC Depth [0, 1] standard)
    pub fn from_view_projection(view_proj: &Mat4) -> Self {
        let r0 = view_proj.row(0);
        let r1 = view_proj.row(1);
        let r2 = view_proj.row(2);
        let r3 = view_proj.row(3);

        Self {
            planes: [
                FrustumPlane::from_vec4(r3 + r0), // Left:   w + x >= 0
                FrustumPlane::from_vec4(r3 - r0), // Right:  w - x >= 0
                FrustumPlane::from_vec4(r3 + r1), // Bottom: w + y >= 0
                FrustumPlane::from_vec4(r3 - r1), // Top:    w - y >= 0
                FrustumPlane::from_vec4(r2),      // Near:   z >= 0 (wgpu depth 0..1)
                FrustumPlane::from_vec4(r3 - r2), // Far:    w - z >= 0
            ],
        }
    }

    /// Uji irisan AABB dengan 6 bidang frustum menggunakan evaluasi p-vertex (Zero Heap Allocation)
    #[inline]
    pub fn intersects_aabb(&self, min: Vec3, max: Vec3) -> bool {
        for plane in &self.planes {
            let p = Vec3::new(
                if plane.normal.x > 0.0 { max.x } else { min.x },
                if plane.normal.y > 0.0 { max.y } else { min.y },
                if plane.normal.z > 0.0 { max.z } else { min.z },
            );

            if plane.distance_to_point(p) < 0.0 {
                return false;
            }
        }
        true
    }

    /// Uji visibilitas AABB chunk terhadap frustum kamera
    #[inline]
    pub fn intersects_chunk(&self, chunk_coord: IVec3) -> bool {
        let min = Vec3::new(
            chunk_coord.x as f32 * CHUNK_WORLD_SIZE,
            chunk_coord.y as f32 * CHUNK_WORLD_SIZE,
            chunk_coord.z as f32 * CHUNK_WORLD_SIZE,
        );
        let max = min + Vec3::splat(CHUNK_WORLD_SIZE);
        self.intersects_aabb(min, max)
    }
}

/// Preset kecepatan kamera developer dalam satuan fisik meter per detik (m/s)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraSpeedPreset {
    /// 5 m/s - Inspeksi detail mikro/voxel
    Slow,
    /// 20 m/s - Penjelajahan standar lereng bukit dan hutan
    #[default]
    Normal,
    /// 100 m/s - Penjelajahan cepat antar-bioma
    Fast,
    /// 500 m/s - Stress-test streaming skala besar (kilometer)
    Extreme,
}

impl CameraSpeedPreset {
    pub fn speed_m_s(&self) -> f32 {
        match self {
            Self::Slow => 5.0,
            Self::Normal => 20.0,
            Self::Fast => 100.0,
            Self::Extreme => 500.0,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Slow => "Slow (5 m/s)",
            Self::Normal => "Normal (20 m/s)",
            Self::Fast => "Fast (100 m/s)",
            Self::Extreme => "Extreme (500 m/s)",
        }
    }
}

/// Kamera 3D FPS / Orbital terisolasi dari renderer (Developer Free-Flight Camera)
pub struct Camera {
    pub position: Vec3,
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub fov_y_rad: f32,
    pub z_near: f32,
    pub z_far: f32,

    pub speed: f32,
    pub active_preset: CameraSpeedPreset,
    pub sensitivity: f32,

    // Input state tracking
    is_forward: bool,
    is_backward: bool,
    is_left: bool,
    is_right: bool,
    is_up: bool,
    is_down: bool,
    pub is_mouse_dragging: bool,
    /// Mode free-look aktif: pergerakan mouse langsung mengubah yaw/pitch tanpa perlu menahan tombol mouse
    pub free_look: bool,
}

impl Camera {
    pub fn new(position: Vec3, yaw_deg: f32, pitch_deg: f32) -> Self {
        let default_preset = CameraSpeedPreset::Normal;
        Self {
            position,
            yaw_deg,
            pitch_deg,
            fov_y_rad: 60.0f32.to_radians(),
            z_near: 0.1,
            z_far: 1000.0,
            speed: default_preset.speed_m_s(), // 20.0 m/s
            active_preset: default_preset,
            sensitivity: 0.15, // Derajat per piksel gerak mouse
            is_forward: false,
            is_backward: false,
            is_left: false,
            is_right: false,
            is_up: false,
            is_down: false,
            is_mouse_dragging: false,
            free_look: false,
        }
    }

    /// Menetapkan preset kecepatan developer dalam meter/detik
    pub fn set_speed_preset(&mut self, preset: CameraSpeedPreset) {
        self.active_preset = preset;
        self.speed = preset.speed_m_s();
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

    /// Menghitung matriks View-Projection terisolasi dari translasi kamera (khusus untuk background/sky tak berhingga)
    pub fn build_sky_view_projection_matrix(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_at_rh(Vec3::ZERO, self.forward(), Vec3::Y);
        let proj = Mat4::perspective_rh(self.fov_y_rad, aspect, self.z_near, self.z_far);
        proj * view
    }

    /// Mengekstrak 6 bidang frustum dari kamera saat ini
    pub fn extract_frustum(&self, aspect: f32) -> Frustum {
        let vp = self.build_view_projection_matrix(aspect);
        Frustum::from_view_projection(&vp)
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
                KeyCode::Digit1 => {
                    if pressed {
                        self.set_speed_preset(CameraSpeedPreset::Slow);
                    }
                    true
                }
                KeyCode::Digit2 => {
                    if pressed {
                        self.set_speed_preset(CameraSpeedPreset::Normal);
                    }
                    true
                }
                KeyCode::Digit3 => {
                    if pressed {
                        self.set_speed_preset(CameraSpeedPreset::Fast);
                    }
                    true
                }
                KeyCode::Digit4 => {
                    if pressed {
                        self.set_speed_preset(CameraSpeedPreset::Extreme);
                    }
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

    /// Menangani perpindahan mouse untuk orientasi kamera (relative FPS look input).
    ///
    /// Berlaku seragam untuk Player FPS maupun Developer Free Camera:
    /// - Perpindahan horizontal (dx) memutar yaw
    /// - Perpindahan vertikal (dy) memutar pitch
    /// - Pitch dibatasi [-89.0, 89.0] derajat untuk mencegah pembalikan kamera / gimbal lock.
    pub fn handle_mouse_motion(&mut self, dx: f64, dy: f64) {
        self.yaw_deg += (dx as f32) * self.sensitivity;
        self.pitch_deg -= (dy as f32) * self.sensitivity;

        // Batasi pitch agar tidak gimbal-lock
        self.pitch_deg = self.pitch_deg.clamp(-89.0, 89.0);
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
