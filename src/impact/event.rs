use glam::Vec3;
use std::fmt;

/// Pengidentifikasi unik sekuensial atau deterministik untuk suatu ImpactEvent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImpactId(pub u64);

impl fmt::Display for ImpactId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ImpactId({})", self.0)
    }
}

/// Kategori umum asal sumber terjadinya benturan (*impact*).
///
/// Dirancang sepenuhnya netral terhadap gameplay, mampu merepresentasikan
/// benturan dari berbagai sistem masa depan tanpa mengubah abstraksi inti.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImpactSourceKind {
    /// Sumber umum atau tidak terspesifikasi
    Generic,
    /// Proyektil (panah, peluru, batu terlontar)
    Projectile,
    /// Serangan atau tubrukan makhluk/hewan
    Creature,
    /// Peristiwa lingkungan (longsor, petir, meteor, gempa)
    Environment,
    /// Kemampuan aktif atau sihir pemain
    Ability,
    /// Pecahan atau puing runtuhan struktur
    Debris,
}

/// Asal sumber benturan yang menggabungkan kategori dan ID instansi sumber.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImpactSource {
    pub kind: ImpactSourceKind,
    pub id: u64,
}

impl ImpactSource {
    pub const fn new(kind: ImpactSourceKind, id: u64) -> Self {
        Self { kind, id }
    }

    pub const fn generic() -> Self {
        Self {
            kind: ImpactSourceKind::Generic,
            id: 0,
        }
    }

    pub const fn projectile(id: u64) -> Self {
        Self {
            kind: ImpactSourceKind::Projectile,
            id,
        }
    }

    pub const fn creature(id: u64) -> Self {
        Self {
            kind: ImpactSourceKind::Creature,
            id,
        }
    }

    pub const fn environment(id: u64) -> Self {
        Self {
            kind: ImpactSourceKind::Environment,
            id,
        }
    }

    pub const fn ability(id: u64) -> Self {
        Self {
            kind: ImpactSourceKind::Ability,
            id,
        }
    }

    pub const fn debris(id: u64) -> Self {
        Self {
            kind: ImpactSourceKind::Debris,
            id,
        }
    }
}

/// Representasi besaran benturan (*magnitude*) yang menjaga pemisahan semantik
/// antara Energi (Joule, skalar kerja) dan Impuls (Newton-detik, transfer momentum).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImpactMagnitude {
    /// Energi kinetik/ledakan murni dalam Joule (J = kg·m²/s² >= 0.0)
    Energy(f32),
    /// Impuls murni dalam Newton-detik (N·s = kg·m/s >= 0.0)
    Impulse(f32),
    /// Kombinasi eksplisit keduanya
    Combined { energy: f32, impulse: f32 },
}

impl ImpactMagnitude {
    pub fn try_energy(joules: f32) -> Result<Self, ImpactError> {
        if !joules.is_finite() {
            return Err(ImpactError::NonFiniteEnergy(joules));
        }
        if joules < 0.0 {
            return Err(ImpactError::NegativeEnergy(joules));
        }
        Ok(Self::Energy(joules))
    }

    pub fn try_impulse(newton_seconds: f32) -> Result<Self, ImpactError> {
        if !newton_seconds.is_finite() {
            return Err(ImpactError::NonFiniteImpulse(newton_seconds));
        }
        if newton_seconds < 0.0 {
            return Err(ImpactError::NegativeImpulse(newton_seconds));
        }
        Ok(Self::Impulse(newton_seconds))
    }

    pub fn try_combined(energy: f32, impulse: f32) -> Result<Self, ImpactError> {
        if !energy.is_finite() {
            return Err(ImpactError::NonFiniteEnergy(energy));
        }
        if energy < 0.0 {
            return Err(ImpactError::NegativeEnergy(energy));
        }
        if !impulse.is_finite() {
            return Err(ImpactError::NonFiniteImpulse(impulse));
        }
        if impulse < 0.0 {
            return Err(ImpactError::NegativeImpulse(impulse));
        }
        Ok(Self::Combined { energy, impulse })
    }

    #[inline]
    pub fn energy(&self) -> Option<f32> {
        match self {
            Self::Energy(e) => Some(*e),
            Self::Combined { energy, .. } => Some(*energy),
            Self::Impulse(_) => None,
        }
    }

    #[inline]
    pub fn impulse(&self) -> Option<f32> {
        match self {
            Self::Impulse(i) => Some(*i),
            Self::Combined { impulse, .. } => Some(*impulse),
            Self::Energy(_) => None,
        }
    }
}

/// Kesalahan validasi data ImpactEvent.
#[derive(Debug, Clone, PartialEq)]
pub enum ImpactError {
    NonFinitePosition(Vec3),
    NonFiniteDirection(Vec3),
    ZeroLengthDirection,
    NonFiniteNormal(Vec3),
    ZeroLengthNormal,
    NonFiniteEnergy(f32),
    NegativeEnergy(f32),
    NonFiniteImpulse(f32),
    NegativeImpulse(f32),
    NonFiniteRadius(f32),
    NegativeRadius(f32),
    MissingMagnitude,
    DuplicateEventId(ImpactId),
}

impl fmt::Display for ImpactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinitePosition(p) => write!(f, "Impact position is non-finite: {:?}", p),
            Self::NonFiniteDirection(d) => write!(f, "Impact direction is non-finite: {:?}", d),
            Self::ZeroLengthDirection => write!(f, "Impact direction is zero-length"),
            Self::NonFiniteNormal(n) => write!(f, "Impact normal is non-finite: {:?}", n),
            Self::ZeroLengthNormal => write!(f, "Impact normal is zero-length"),
            Self::NonFiniteEnergy(e) => write!(f, "Impact energy is non-finite: {}", e),
            Self::NegativeEnergy(e) => write!(f, "Impact energy cannot be negative: {}", e),
            Self::NonFiniteImpulse(i) => write!(f, "Impact impulse is non-finite: {}", i),
            Self::NegativeImpulse(i) => write!(f, "Impact impulse cannot be negative: {}", i),
            Self::NonFiniteRadius(r) => write!(f, "Impact radius is non-finite: {}", r),
            Self::NegativeRadius(r) => write!(f, "Impact radius cannot be negative: {}", r),
            Self::MissingMagnitude => write!(f, "Impact magnitude must have energy or impulse"),
            Self::DuplicateEventId(id) => {
                write!(f, "Duplicate impact event ID encountered: {}", id)
            }
        }
    }
}

impl std::error::Error for ImpactError {}

/// Representasi data murni (immutable input event) untuk suatu benturan fisik di dunia.
///
/// INVARIAN GEOMETRIS & NUMERIK:
/// - `position`: harus berhingga (*finite*).
/// - `direction`: jika ada, harus dinormalisasi menjadi vektor satuan terhingga (|d| == 1.0).
/// - `surface_normal`: jika ada, harus dinormalisasi menjadi vektor satuan terhingga (|n| == 1.0).
/// - `magnitude`: energi >= 0.0 dan/atau impuls >= 0.0, terhingga.
/// - `radius`: radius pengaruh benturan dalam meter, terhingga dan >= 0.0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImpactEvent {
    pub id: ImpactId,
    pub source: ImpactSource,
    pub position: Vec3,
    pub direction: Option<Vec3>,
    pub surface_normal: Option<Vec3>,
    pub magnitude: ImpactMagnitude,
    pub radius: f32,
}

impl ImpactEvent {
    /// Konstruktor tervalidasi langsung tanpa builder.
    pub fn try_new(
        id: ImpactId,
        source: ImpactSource,
        position: Vec3,
        direction: Option<Vec3>,
        surface_normal: Option<Vec3>,
        magnitude: ImpactMagnitude,
        radius: f32,
    ) -> Result<Self, ImpactError> {
        if !position.is_finite() {
            return Err(ImpactError::NonFinitePosition(position));
        }

        let validated_dir = match direction {
            Some(d) => {
                if !d.is_finite() {
                    return Err(ImpactError::NonFiniteDirection(d));
                }
                let len_sq = d.length_squared();
                if len_sq < 1e-12 {
                    return Err(ImpactError::ZeroLengthDirection);
                }
                Some(d / len_sq.sqrt())
            }
            None => None,
        };

        let validated_norm = match surface_normal {
            Some(n) => {
                if !n.is_finite() {
                    return Err(ImpactError::NonFiniteNormal(n));
                }
                let len_sq = n.length_squared();
                if len_sq < 1e-12 {
                    return Err(ImpactError::ZeroLengthNormal);
                }
                Some(n / len_sq.sqrt())
            }
            None => None,
        };

        if !radius.is_finite() {
            return Err(ImpactError::NonFiniteRadius(radius));
        }
        if radius < 0.0 {
            return Err(ImpactError::NegativeRadius(radius));
        }

        Ok(Self {
            id,
            source,
            position,
            direction: validated_dir,
            surface_normal: validated_norm,
            magnitude,
            radius,
        })
    }

    /// Memulai builder untuk menyusun ImpactEvent secara ergonomis.
    pub fn builder(id: ImpactId, position: Vec3, radius: f32) -> ImpactEventBuilder {
        ImpactEventBuilder::new(id, position, radius)
    }
}

impl Eq for ImpactEvent {}

impl PartialOrd for ImpactEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ImpactEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Pengurutan kanonikal deterministik total (bitwise float comparison aman karena sudah divalidasi finite)
        self.id
            .cmp(&other.id)
            .then_with(|| self.source.cmp(&other.source))
            .then_with(|| self.position.x.to_bits().cmp(&other.position.x.to_bits()))
            .then_with(|| self.position.y.to_bits().cmp(&other.position.y.to_bits()))
            .then_with(|| self.position.z.to_bits().cmp(&other.position.z.to_bits()))
            .then_with(|| self.radius.to_bits().cmp(&other.radius.to_bits()))
    }
}

/// Builder untuk menyusun ImpactEvent dengan validasi aman.
#[derive(Debug, Clone)]
pub struct ImpactEventBuilder {
    id: ImpactId,
    source: ImpactSource,
    position: Vec3,
    direction: Option<Vec3>,
    surface_normal: Option<Vec3>,
    energy: Option<f32>,
    impulse: Option<f32>,
    radius: f32,
}

impl ImpactEventBuilder {
    pub fn new(id: ImpactId, position: Vec3, radius: f32) -> Self {
        Self {
            id,
            source: ImpactSource::generic(),
            position,
            direction: None,
            surface_normal: None,
            energy: None,
            impulse: None,
            radius,
        }
    }

    pub fn source(mut self, source: ImpactSource) -> Self {
        self.source = source;
        self
    }

    pub fn direction(mut self, direction: Vec3) -> Self {
        self.direction = Some(direction);
        self
    }

    pub fn surface_normal(mut self, normal: Vec3) -> Self {
        self.surface_normal = Some(normal);
        self
    }

    pub fn energy(mut self, joules: f32) -> Self {
        self.energy = Some(joules);
        self
    }

    pub fn impulse(mut self, newton_seconds: f32) -> Self {
        self.impulse = Some(newton_seconds);
        self
    }

    pub fn build(self) -> Result<ImpactEvent, ImpactError> {
        let magnitude = match (self.energy, self.impulse) {
            (Some(e), Some(i)) => ImpactMagnitude::try_combined(e, i)?,
            (Some(e), None) => ImpactMagnitude::try_energy(e)?,
            (None, Some(i)) => ImpactMagnitude::try_impulse(i)?,
            (None, None) => return Err(ImpactError::MissingMagnitude),
        };

        ImpactEvent::try_new(
            self.id,
            self.source,
            self.position,
            self.direction,
            self.surface_normal,
            magnitude,
            self.radius,
        )
    }
}
