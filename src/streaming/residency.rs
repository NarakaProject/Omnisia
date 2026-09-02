use std::fmt;

/// Status kehadiran chunk dalam memori runtime
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ResidencyState {
    #[default]
    Unloaded,
    Queued,
    Loading,
    Generating,
    Resident,
    Saving,
    Evicting,
}

impl fmt::Display for ResidencyState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unloaded => write!(f, "Unloaded"),
            Self::Queued => write!(f, "Queued"),
            Self::Loading => write!(f, "Loading"),
            Self::Generating => write!(f, "Generating"),
            Self::Resident => write!(f, "Resident"),
            Self::Saving => write!(f, "Saving"),
            Self::Evicting => write!(f, "Evicting"),
        }
    }
}

/// Status sinkronisasi penyimpanan chunk ke disk
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PersistenceState {
    #[default]
    Clean,
    Dirty,
    Saving,
}

/// Status pembuatan mesh grafis chunk
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MeshState {
    #[default]
    Clean,
    Dirty,
    Meshing,
}

/// State machine validator untuk transisi residency chunk
pub struct ResidencyStateMachine;

impl ResidencyStateMachine {
    /// Memvalidasi apakah transisi dari `current` ke `target` diizinkan
    pub fn is_valid_transition(current: ResidencyState, target: ResidencyState) -> bool {
        match (current, target) {
            // Dari Unloaded: hanya boleh masuk ke antrean Queued atau langsung Loading/Generating
            (ResidencyState::Unloaded, ResidencyState::Queued) => true,
            (ResidencyState::Unloaded, ResidencyState::Loading) => true,
            (ResidencyState::Unloaded, ResidencyState::Generating) => true,

            // Dari Queued: boleh mulai Loading, Generating, atau dibatalkan kembali ke Unloaded
            (ResidencyState::Queued, ResidencyState::Loading) => true,
            (ResidencyState::Queued, ResidencyState::Generating) => true,
            (ResidencyState::Queued, ResidencyState::Unloaded) => true,

            // Dari Loading: berhasil menjadi Resident, atau gagal kembali ke Unloaded / Generating fallback
            (ResidencyState::Loading, ResidencyState::Resident) => true,
            (ResidencyState::Loading, ResidencyState::Generating) => true,
            (ResidencyState::Loading, ResidencyState::Unloaded) => true,

            // Dari Generating: berhasil menjadi Resident, atau gagal kembali ke Unloaded
            (ResidencyState::Generating, ResidencyState::Resident) => true,
            (ResidencyState::Generating, ResidencyState::Unloaded) => true,

            // Dari Resident: boleh masuk proses Saving atau Evicting
            (ResidencyState::Resident, ResidencyState::Saving) => true,
            (ResidencyState::Resident, ResidencyState::Evicting) => true,
            (ResidencyState::Resident, ResidencyState::Resident) => true,

            // Dari Saving: selesai save kembali ke Resident atau langsung dievict jika sedang dalam antrean eviksi
            (ResidencyState::Saving, ResidencyState::Resident) => true,
            (ResidencyState::Saving, ResidencyState::Evicting) => true,

            // Dari Evicting: selesai eviksi kembali ke Unloaded, atau jika evict gagal kembali ke Resident
            (ResidencyState::Evicting, ResidencyState::Unloaded) => true,
            (ResidencyState::Evicting, ResidencyState::Resident) => true,

            _ => false,
        }
    }
}
