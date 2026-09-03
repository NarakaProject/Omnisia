use glam::{Mat3, Quat, Vec3};
use std::fmt;

use super::broadphase::{BodyType, RigidBodyId};
use super::transform::Transform;

/// Kesalahan validasi atau pembuatan RigidBody dan MassProperties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigidBodyError {
    /// Posisi memuat koordinat non-finite (NaN atau Infinity)
    NonFinitePosition,
    /// Rotasi tidak valid (non-finite atau zero-length quaternion)
    InvalidRotation,
    /// Kecepatan linier atau angular memuat komponen non-finite
    NonFiniteVelocity,
    /// Massa tidak valid (non-finite, <= 0.0 untuk dinamis, atau != 0.0 inverse untuk statis/kinematik)
    InvalidMass,
    /// Tensor inersia tidak valid (non-finite, asimetris, singular/non-positive-definite untuk dinamis, atau non-zero untuk statis/kinematik)
    InvalidInertia,
}

impl fmt::Display for RigidBodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinitePosition => write!(f, "Posisi memuat nilai non-finite"),
            Self::InvalidRotation => write!(f, "Rotasi tidak valid (non-finite atau zero-length)"),
            Self::NonFiniteVelocity => write!(f, "Kecepatan memuat nilai non-finite"),
            Self::InvalidMass => write!(f, "Massa tidak valid"),
            Self::InvalidInertia => write!(f, "Tensor inersia tidak valid"),
        }
    }
}

impl std::error::Error for RigidBodyError {}

/// Properti massa dan tensor inersia lokal dari badan kaku.
///
/// DOKUMENTASI ARSITEKTURAL TENSOR INERSIA:
/// - `local_inertia` dan `local_inverse_inertia` dinyatakan secara eksklusif dalam
///   **kerangka koordinat lokal badan** (body's local coordinate frame).
/// - Transformasi inersia ke ruang dunia ($I_{\text{world}} = R I_{\text{local}} R^T$)
///   secara tegas ditunda ke Phase 9.6 (Linear + Angular Integration).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassProperties {
    pub mass: f32,
    pub inverse_mass: f32,
    pub local_inertia: Mat3,
    pub local_inverse_inertia: Mat3,
}

impl MassProperties {
    /// Membuat MassProperties untuk badan dinamis.
    ///
    /// SYARAT VALIDASI DINAMIS:
    /// - `mass > 0.0` dan finite (`inverse_mass = 1.0 / mass`).
    /// - `local_inertia` harus bernilai finite, simetris, dan definit positif.
    ///   Definit positif diverifikasi menggunakan kriteria Sylvester:
    ///   1. $D_1 = m_{00} > \epsilon$
    ///   2. $D_2 = m_{00}m_{11} - m_{01}m_{10} > \epsilon$
    ///   3. $D_3 = \det(M) > \epsilon$
    pub fn new_dynamic(mass: f32, local_inertia: Mat3) -> Result<Self, RigidBodyError> {
        if !mass.is_finite() || mass <= 0.0 {
            return Err(RigidBodyError::InvalidMass);
        }

        // 1. Periksa keterhinggaan seluruh elemen tensor inersia
        let cols = [
            local_inertia.x_axis,
            local_inertia.y_axis,
            local_inertia.z_axis,
        ];
        for col in &cols {
            if !col.is_finite() {
                return Err(RigidBodyError::InvalidInertia);
            }
        }

        // 2. Periksa simetri tensor inersia (I_ij == I_ji dalam batas toleransi)
        const SYM_EPS: f32 = 1e-4;
        if (local_inertia.x_axis.y - local_inertia.y_axis.x).abs() > SYM_EPS
            || (local_inertia.x_axis.z - local_inertia.z_axis.x).abs() > SYM_EPS
            || (local_inertia.y_axis.z - local_inertia.z_axis.y).abs() > SYM_EPS
        {
            return Err(RigidBodyError::InvalidInertia);
        }

        // 3. Kriteria Sylvester untuk definit positif matriks simetris 3x3
        const POS_DEF_EPS: f32 = 1e-6;
        let m00 = local_inertia.x_axis.x;
        let m01 = local_inertia.y_axis.x;
        let m10 = local_inertia.x_axis.y;
        let m11 = local_inertia.y_axis.y;

        let d1 = m00;
        let d2 = m00 * m11 - m01 * m10;
        let d3 = local_inertia.determinant();

        if d1 <= POS_DEF_EPS || d2 <= POS_DEF_EPS || d3 <= POS_DEF_EPS {
            return Err(RigidBodyError::InvalidInertia);
        }

        let inverse_inertia = local_inertia.inverse();
        if !inverse_inertia.x_axis.is_finite()
            || !inverse_inertia.y_axis.is_finite()
            || !inverse_inertia.z_axis.is_finite()
        {
            return Err(RigidBodyError::InvalidInertia);
        }

        Ok(Self {
            mass,
            inverse_mass: 1.0 / mass,
            local_inertia,
            local_inverse_inertia: inverse_inertia,
        })
    }

    /// Membuat MassProperties untuk badan statis.
    ///
    /// SEMANTIK MASSA STATIS:
    /// Memiliki massa dan inersia efektif tak terhingga yang direpresentasikan
    /// secara kanonikal dengan `inverse_mass = 0.0` dan `local_inverse_inertia = Mat3::ZERO`.
    /// Nilai stored `mass` adalah `0.0` (BUKAN `f32::INFINITY`).
    pub fn new_static() -> Self {
        Self {
            mass: 0.0,
            inverse_mass: 0.0,
            local_inertia: Mat3::ZERO,
            local_inverse_inertia: Mat3::ZERO,
        }
    }

    /// Membuat MassProperties untuk badan kinematik.
    ///
    /// SEMANTIK MASSA KINEMATIK:
    /// Memiliki massa dan inersia efektif tak terhingga terhadap gaya/impuls luar
    /// (`inverse_mass = 0.0`, `local_inverse_inertia = Mat3::ZERO`), sehingga tidak
    /// dapat terakselerasi oleh kontak dinamis.
    pub fn new_kinematic() -> Self {
        Self {
            mass: 0.0,
            inverse_mass: 0.0,
            local_inertia: Mat3::ZERO,
            local_inverse_inertia: Mat3::ZERO,
        }
    }

    /// Helper pembantu untuk menghitung inersia diagonal murni (Ixx, Iyy, Izz).
    pub fn from_diagonal(mass: f32, diagonal: Vec3) -> Result<Self, RigidBodyError> {
        if !diagonal.is_finite() || diagonal.x <= 0.0 || diagonal.y <= 0.0 || diagonal.z <= 0.0 {
            return Err(RigidBodyError::InvalidInertia);
        }
        let inertia = Mat3::from_diagonal(diagonal);
        Self::new_dynamic(mass, inertia)
    }

    /// Helper pembantu inersia kotak seragam berukuran extents (dx, dy, dz).
    /// Rumus: $I_{xx} = \frac{1}{12} m (dy^2 + dz^2)$, dsb.
    pub fn from_box(mass: f32, extents: Vec3) -> Result<Self, RigidBodyError> {
        if !extents.is_finite() || extents.x <= 0.0 || extents.y <= 0.0 || extents.z <= 0.0 {
            return Err(RigidBodyError::InvalidInertia);
        }
        let factor = mass / 12.0;
        let ixx = factor * (extents.y * extents.y + extents.z * extents.z);
        let iyy = factor * (extents.x * extents.x + extents.z * extents.z);
        let izz = factor * (extents.x * extents.x + extents.y * extents.y);
        Self::from_diagonal(mass, Vec3::new(ixx, iyy, izz))
    }

    /// Helper pembantu inersia bola pejal seragam beradius r.
    /// Rumus: $I = \frac{2}{5} m r^2 \cdot I_{3\times 3}$.
    pub fn from_sphere(mass: f32, radius: f32) -> Result<Self, RigidBodyError> {
        if !radius.is_finite() || radius <= 0.0 {
            return Err(RigidBodyError::InvalidInertia);
        }
        let val = 0.4 * mass * radius * radius;
        Self::from_diagonal(mass, Vec3::splat(val))
    }
}

/// Status tidur badan kaku dinamis (Phase 9.8).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SleepState {
    /// Badan aktif berpartisipasi dalam simulasi solver dan integrasi.
    #[default]
    Awake = 0,
    /// Badan telah settled/tenang dan simulasi dilewati (skipped).
    Sleeping = 1,
}

/// Representasi model data fisik murni (Pure Rigid-Body Physical State Model) untuk Phase 9.2.
///
/// INVARIAN ARSITEKTURAL:
/// 1. **Pure Physical State**: Hanya memuat identitas, tipe badan, transform dunia (posisi, rotasi),
///    kecepatan (linier, angular), properti massa, dan status tidur (Phase 9.8).
/// 2. **Zero Voxel Ownership**: Tidak memiliki data voxel, array voxel, atau referensi chunk.
/// 3. **Zero GPU Resources**: Tidak memiliki mesh, buffer GPU, atau dependensi rendering (`wgpu`).
/// 4. **Zero Colliders in Phase 9.2**: Tidak memuat bentuk tabrakan (bola, kapsul, kotak, mesh).
///    Representasi bentuk tabrakan sepenuhnya ditunda ke Phase 9.3.
/// 5. **Zero Hidden Simulation**: Membuat atau mengakses `RigidBody` tidak pernah memodifikasi
///    posisi, mengintegrasikan kecepatan, atau memajukan waktu.
/// 6. **Rotation Normalization Invariant**: Rotasi (`Quat`) divalidasi dan selalu disimpan dalam
///    keadaan finite dan ternormalisasi ($|\text{rotation}| \approx 1.0$).
#[derive(Debug, Clone, PartialEq)]
pub struct RigidBody {
    id: RigidBodyId,
    body_type: BodyType,
    position: Vec3,
    rotation: Quat,
    linear_velocity: Vec3,
    angular_velocity: Vec3,
    mass_properties: MassProperties,
    sleep_state: SleepState,
    sleep_timer: f32,
}

impl RigidBody {
    /// Membuat badan dinamis baru dengan kecepatan awal nol.
    pub fn new_dynamic(
        id: RigidBodyId,
        position: Vec3,
        rotation: Quat,
        mass: f32,
        local_inertia: Mat3,
    ) -> Result<Self, RigidBodyError> {
        let mass_properties = MassProperties::new_dynamic(mass, local_inertia)?;
        Self::new(
            id,
            BodyType::Dynamic,
            position,
            rotation,
            Vec3::ZERO,
            Vec3::ZERO,
            mass_properties,
        )
    }

    /// Membuat badan statis baru dengan kecepatan nol dan massa efektif tak terhingga.
    pub fn new_static(
        id: RigidBodyId,
        position: Vec3,
        rotation: Quat,
    ) -> Result<Self, RigidBodyError> {
        let mass_properties = MassProperties::new_static();
        Self::new(
            id,
            BodyType::Static,
            position,
            rotation,
            Vec3::ZERO,
            Vec3::ZERO,
            mass_properties,
        )
    }

    /// Membuat badan kinematik baru dengan kecepatan awal yang ditentukan.
    pub fn new_kinematic(
        id: RigidBodyId,
        position: Vec3,
        rotation: Quat,
        linear_velocity: Vec3,
        angular_velocity: Vec3,
    ) -> Result<Self, RigidBodyError> {
        let mass_properties = MassProperties::new_kinematic();
        Self::new(
            id,
            BodyType::Kinematic,
            position,
            rotation,
            linear_velocity,
            angular_velocity,
            mass_properties,
        )
    }

    /// Konstruktor umum tervalidasi untuk RigidBody.
    pub fn new(
        id: RigidBodyId,
        body_type: BodyType,
        position: Vec3,
        rotation: Quat,
        linear_velocity: Vec3,
        angular_velocity: Vec3,
        mass_properties: MassProperties,
    ) -> Result<Self, RigidBodyError> {
        if !position.is_finite() {
            return Err(RigidBodyError::NonFinitePosition);
        }
        if !rotation.is_finite() || rotation.length_squared() < 1e-8 {
            return Err(RigidBodyError::InvalidRotation);
        }
        if !linear_velocity.is_finite() || !angular_velocity.is_finite() {
            return Err(RigidBodyError::NonFiniteVelocity);
        }

        // Validasi kesesuaian mass_properties dengan body_type
        match body_type {
            BodyType::Dynamic => {
                if mass_properties.mass <= 0.0
                    || !mass_properties.mass.is_finite()
                    || mass_properties.inverse_mass <= 0.0
                {
                    return Err(RigidBodyError::InvalidMass);
                }
            }
            BodyType::Static | BodyType::Kinematic => {
                if mass_properties.inverse_mass != 0.0 {
                    return Err(RigidBodyError::InvalidMass);
                }
                if mass_properties.local_inverse_inertia != Mat3::ZERO {
                    return Err(RigidBodyError::InvalidInertia);
                }
            }
        }

        Ok(Self {
            id,
            body_type,
            position,
            rotation: rotation.normalize(),
            linear_velocity,
            angular_velocity,
            mass_properties,
            sleep_state: SleepState::Awake,
            sleep_timer: 0.0,
        })
    }

    /// Identifier unik runtime dari badan ini
    #[inline(always)]
    pub fn id(&self) -> RigidBodyId {
        self.id
    }

    /// Kategori partisipasi fisik dari badan ini
    #[inline(always)]
    pub fn body_type(&self) -> BodyType {
        self.body_type
    }

    /// Posisi titik acuan asal badan dalam koordinat dunia (meter)
    #[inline(always)]
    pub fn position(&self) -> Vec3 {
        self.position
    }

    /// Mengubah posisi badan dunia (memvalidasi nilai finite)
    pub fn set_position(&mut self, pos: Vec3) -> Result<(), RigidBodyError> {
        if !pos.is_finite() {
            return Err(RigidBodyError::NonFinitePosition);
        }
        self.position = pos;
        Ok(())
    }

    /// Orientasi rotasi badan dalam koordinat dunia (quaternion ternormalisasi)
    #[inline(always)]
    pub fn rotation(&self) -> Quat {
        self.rotation
    }

    /// Mengubah orientasi rotasi badan (memvalidasi finite, non-zero length, dan menormalisasi)
    pub fn set_rotation(&mut self, rot: Quat) -> Result<(), RigidBodyError> {
        if !rot.is_finite() || rot.length_squared() < 1e-8 {
            return Err(RigidBodyError::InvalidRotation);
        }
        self.rotation = rot.normalize();
        Ok(())
    }

    /// Kecepatan linier translasi dalam koordinat dunia (meter/detik)
    #[inline(always)]
    pub fn linear_velocity(&self) -> Vec3 {
        self.linear_velocity
    }

    /// Mengubah kecepatan linier translasi (memvalidasi nilai finite).
    /// Jika badan sedang tidur dan diberikan kecepatan non-nol, badan otomatis bangun.
    pub fn set_linear_velocity(&mut self, vel: Vec3) -> Result<(), RigidBodyError> {
        if !vel.is_finite() {
            return Err(RigidBodyError::NonFiniteVelocity);
        }
        if self.sleep_state == SleepState::Sleeping && vel.length_squared() > 1e-8 {
            self.wake();
        }
        self.linear_velocity = vel;
        Ok(())
    }

    /// Kecepatan sudut rotasi dalam koordinat dunia (radian/detik)
    #[inline(always)]
    pub fn angular_velocity(&self) -> Vec3 {
        self.angular_velocity
    }

    /// Mengubah kecepatan sudut rotasi (memvalidasi nilai finite).
    /// Jika badan sedang tidur dan diberikan kecepatan sudut non-nol, badan otomatis bangun.
    pub fn set_angular_velocity(&mut self, vel: Vec3) -> Result<(), RigidBodyError> {
        if !vel.is_finite() {
            return Err(RigidBodyError::NonFiniteVelocity);
        }
        if self.sleep_state == SleepState::Sleeping && vel.length_squared() > 1e-8 {
            self.wake();
        }
        self.angular_velocity = vel;
        Ok(())
    }

    /// Properti massa dan tensor inersia lokal badan
    #[inline(always)]
    pub fn mass_properties(&self) -> &MassProperties {
        &self.mass_properties
    }

    /// Mengubah properti massa badan dengan validasi sesuai kategori badan
    pub fn set_mass_properties(&mut self, props: MassProperties) -> Result<(), RigidBodyError> {
        match self.body_type {
            BodyType::Dynamic => {
                if props.mass <= 0.0 || !props.mass.is_finite() || props.inverse_mass <= 0.0 {
                    return Err(RigidBodyError::InvalidMass);
                }
            }
            BodyType::Static | BodyType::Kinematic => {
                if props.inverse_mass != 0.0 {
                    return Err(RigidBodyError::InvalidMass);
                }
                if props.local_inverse_inertia != Mat3::ZERO {
                    return Err(RigidBodyError::InvalidInertia);
                }
            }
        }
        self.mass_properties = props;
        Ok(())
    }

    /// Apakah badan berpartisipasi sebagai badan dinamis
    #[inline(always)]
    pub fn is_dynamic(&self) -> bool {
        self.body_type == BodyType::Dynamic
    }

    /// Apakah badan berpartisipasi sebagai badan statis
    #[inline(always)]
    pub fn is_static(&self) -> bool {
        self.body_type == BodyType::Static
    }

    /// Apakah badan berpartisipasi sebagai badan kinematik
    #[inline(always)]
    pub fn is_kinematic(&self) -> bool {
        self.body_type == BodyType::Kinematic
    }

    /// Mengambil representasi Transform spasial kaku dari badan (posisi dan rotasi ternormalisasi).
    #[inline(always)]
    pub fn transform(&self) -> Transform {
        Transform {
            position: self.position,
            rotation: self.rotation,
        }
    }

    /// Status tidur badan kaku (selalu Awake untuk Static dan Kinematic).
    #[inline(always)]
    pub fn sleep_state(&self) -> SleepState {
        self.sleep_state
    }

    /// Apakah badan kaku sedang dalam status tidur (Sleeping).
    #[inline(always)]
    pub fn is_sleeping(&self) -> bool {
        self.sleep_state == SleepState::Sleeping
    }

    /// Apakah badan kaku sedang dalam status aktif (Awake).
    #[inline(always)]
    pub fn is_awake(&self) -> bool {
        self.sleep_state == SleepState::Awake
    }

    /// Waktu akumulasi kondisi tenang berturut-turut (detik).
    #[inline(always)]
    pub fn sleep_timer(&self) -> f32 {
        self.sleep_timer
    }

    /// Membangunkan badan kaku dinamis dan mereset timer tidur ke 0.0.
    pub fn wake(&mut self) {
        self.sleep_state = SleepState::Awake;
        self.sleep_timer = 0.0;
    }

    /// Menidurkan badan dinamis: mengkanonikan kecepatan linier dan angular ke nol.
    /// Tidak memodifikasi posisi atau rotasi. Badan statis dan kinematik tidak pernah tidur.
    pub fn put_to_sleep(&mut self) {
        if self.body_type == BodyType::Dynamic {
            self.sleep_state = SleepState::Sleeping;
            self.linear_velocity = Vec3::ZERO;
            self.angular_velocity = Vec3::ZERO;
        }
    }

    /// Memperbarui timer tidur berdasarkan status tenang (is_quiet).
    pub fn update_sleep_timer(&mut self, dt: f32, is_quiet: bool) {
        if self.body_type != BodyType::Dynamic {
            return;
        }
        if is_quiet {
            self.sleep_timer += dt;
        } else {
            self.sleep_timer = 0.0;
            self.sleep_state = SleepState::Awake;
        }
    }
}
