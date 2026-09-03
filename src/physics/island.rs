use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use super::broadphase::RigidBodyId;
use super::contact::Contact;
use super::rigid_body::RigidBody;

/// Identifier deterministik unik untuk suatu Physics Island.
/// Ditetapkan secara kanonikal sebagai `RigidBodyId` minimum dari badan dinamis anggotanya.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysicsIslandId(pub u64);

impl fmt::Display for PhysicsIslandId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PhysicsIsland#{}", self.0)
    }
}

/// Status simulasi aktivasi dari suatu Physics Island.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IslandState {
    /// Seluruh atau sebagian anggota pulau aktif dan memerlukan penyelesaian solver/integrasi.
    Awake,
    /// Seluruh anggota pulau dinamis telah settled dan simulasi dilewati (skipped).
    Sleeping,
}

/// Konfigurasi ambang batas untuk transisi tidur (sleeping).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SleepConfig {
    /// Ambang batas kecepatan linier translasi di bawah mana badan dianggap tenang (meter/detik).
    /// Default: 0.05 m/s.
    pub linear_velocity_threshold: f32,
    /// Ambang batas kecepatan sudut rotasi di bawah mana badan dianggap tenang (radian/detik).
    /// Default: 0.05 rad/s.
    pub angular_velocity_threshold: f32,
    /// Durasi waktu berturut-turut dalam kondisi tenang sebelum transisi ke status tidur (detik).
    /// Default: 0.5 detik (15 ticks pada fixed 30 Hz).
    pub sleep_duration: f32,
}

impl Default for SleepConfig {
    fn default() -> Self {
        Self {
            linear_velocity_threshold: 0.05,
            angular_velocity_threshold: 0.05,
            sleep_duration: 0.5,
        }
    }
}

impl SleepConfig {
    /// Memvalidasi konfigurasi sleeping.
    pub fn validate(&self) -> Result<(), SleepError> {
        if !self.linear_velocity_threshold.is_finite() || self.linear_velocity_threshold < 0.0 {
            return Err(SleepError::InvalidConfig);
        }
        if !self.angular_velocity_threshold.is_finite() || self.angular_velocity_threshold < 0.0 {
            return Err(SleepError::InvalidConfig);
        }
        if !self.sleep_duration.is_finite() || self.sleep_duration <= 0.0 {
            return Err(SleepError::InvalidConfig);
        }
        Ok(())
    }
}

/// Kesalahan operasi atau validasi sleeping dan manajemen island.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepError {
    /// Konfigurasi sleeping tidak valid (non-finite, negatif, atau durasi <= 0)
    InvalidConfig,
    /// Badan kaku tidak ditemukan dalam registri
    BodyNotFound(RigidBodyId),
    /// Status badan kaku memuat koordinat atau kecepatan non-finite (NaN / Inf)
    NonFiniteState,
}

impl fmt::Display for SleepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig => write!(f, "Konfigurasi sleeping tidak valid"),
            Self::BodyNotFound(id) => write!(f, "Badan kaku {} tidak ditemukan", id),
            Self::NonFiniteState => write!(f, "Status badan kaku memuat nilai non-finite"),
        }
    }
}

impl std::error::Error for SleepError {}

/// Representasi kumpulan badan kaku yang terhubung melalui batasan kontak (Physics Island).
///
/// INVARIAN ARSITEKTURAL (PHASE 9.8):
/// 1. **Deterministik**: `id` ditetapkan dari `RigidBodyId` minimum anggota dinamis,
///    dan `bodies` terurut ascending.
/// 2. **Tanpa Duplikasi Kontak**: Kontak disimpan dalam array kontak otoritatif narrowphase;
///    pulau hanya memuat indeks `contact_indices` ke dalam array tersebut.
/// 3. **Isolasi Lantai Statis**: Kontak antara badan dinamis dan statis tidak pernah
///    menggabungkan dua badan dinamis independen ke dalam satu pulau yang sama.
/// 4. **Koherensi Status**: Seluruh badan dinamis dalam satu pulau terhubung memiliki
///    status tidur yang koheren (semua Awake atau semua Sleeping).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicsIsland {
    /// ID deterministik pulau (min RigidBodyId anggota dinamis)
    pub id: PhysicsIslandId,
    /// Daftar ID badan dinamis anggota pulau (terurut ascending)
    pub bodies: Vec<RigidBodyId>,
    /// Indeks kontak yang relevan terhadap pulau ini dalam slice kontak narrowphase
    pub contact_indices: Vec<usize>,
    /// Status aktivasi pulau saat ini
    pub state: IslandState,
}

/// Membangun partisi Physics Island secara deterministik dari kumpulan badan dan kontak.
///
/// ALGORITMA:
/// 1. Mengumpulkan seluruh badan Dinamis terdaftar (terurut ascending).
/// 2. Membangun graf ketetanggaan hanya dari kontak Dinamis ↔ Dinamis.
///    Kontak multipel antara pasangan yang sama dideduplikasi.
/// 3. Menjalankan traversal BFS iteratif (menggunakan `VecDeque`, bebas rekursi)
///    untuk menemukan komponen terhubung (connected components).
/// 4. Menghubungkan indeks kontak ke pulau yang relevan.
/// 5. Mengurutkan pulau berdasarkan `PhysicsIslandId` ascending.
pub fn build_islands(
    bodies: &BTreeMap<RigidBodyId, RigidBody>,
    contacts: &[Contact],
) -> Result<Vec<PhysicsIsland>, SleepError> {
    // 1. Kumpulkan seluruh badan dinamis secara terurut
    let dynamic_body_ids: Vec<RigidBodyId> = bodies
        .iter()
        .filter(|(_, b)| b.is_dynamic())
        .map(|(&id, _)| id)
        .collect();

    if dynamic_body_ids.is_empty() {
        return Ok(Vec::new());
    }

    // 2. Bangun graf ketetanggaan Dinamis <-> Dinamis
    let mut adj: BTreeMap<RigidBodyId, BTreeSet<RigidBodyId>> = BTreeMap::new();
    for &id in &dynamic_body_ids {
        adj.insert(id, BTreeSet::new());
    }

    for contact in contacts {
        let is_dyn_a = bodies.get(&contact.body_a).is_some_and(|b| b.is_dynamic());
        let is_dyn_b = bodies.get(&contact.body_b).is_some_and(|b| b.is_dynamic());

        // HANYA kontak Dinamis <-> Dinamis yang membentuk sisi (edge) penyambung pulau
        if is_dyn_a && is_dyn_b && contact.body_a != contact.body_b {
            adj.entry(contact.body_a)
                .or_default()
                .insert(contact.body_b);
            adj.entry(contact.body_b)
                .or_default()
                .insert(contact.body_a);
        }
    }

    // 3. Partisi komponen terhubung deterministik via BFS iteratif
    let mut visited: BTreeSet<RigidBodyId> = BTreeSet::new();
    let mut islands: Vec<PhysicsIsland> = Vec::new();

    for &root_id in &dynamic_body_ids {
        if visited.contains(&root_id) {
            continue;
        }

        let mut island_bodies = Vec::new();
        let mut queue = VecDeque::new();

        visited.insert(root_id);
        queue.push_back(root_id);

        while let Some(current_id) = queue.pop_front() {
            island_bodies.push(current_id);

            if let Some(neighbors) = adj.get(&current_id) {
                for &neighbor_id in neighbors {
                    if visited.insert(neighbor_id) {
                        queue.push_back(neighbor_id);
                    }
                }
            }
        }

        // Urutkan ID badan dalam pulau secara ascending
        island_bodies.sort();

        let island_id = PhysicsIslandId(island_bodies[0].0);

        // Status pulau turunan: jika SEMUA badan dinamis berstatus Sleeping, maka Sleeping; jika tidak, Awake.
        let all_sleeping = !island_bodies.is_empty()
            && island_bodies
                .iter()
                .all(|id| bodies.get(id).is_some_and(|b| b.is_sleeping()));
        let state = if all_sleeping {
            IslandState::Sleeping
        } else {
            IslandState::Awake
        };

        islands.push(PhysicsIsland {
            id: island_id,
            bodies: island_bodies,
            contact_indices: Vec::new(),
            state,
        });
    }

    // Urutkan pulau berdasarkan PhysicsIslandId ascending
    islands.sort_by_key(|island| island.id);

    // 4. Asosiasikan indeks kontak ke pulau yang relevan
    // Buat tabel pencarian body_id -> island_index
    let mut body_to_island: BTreeMap<RigidBodyId, usize> = BTreeMap::new();
    for (island_idx, island) in islands.iter().enumerate() {
        for &body_id in &island.bodies {
            body_to_island.insert(body_id, island_idx);
        }
    }

    for (contact_idx, contact) in contacts.iter().enumerate() {
        let island_a = body_to_island.get(&contact.body_a).copied();
        let island_b = body_to_island.get(&contact.body_b).copied();

        match (island_a, island_b) {
            (Some(ia), Some(ib)) => {
                // Dinamis <-> Dinamis: keduanya pasti berada dalam pulau yang sama
                debug_assert_eq!(ia, ib);
                islands[ia].contact_indices.push(contact_idx);
            }
            (Some(ia), None) => {
                // Dinamis A <-> Statis/Kinematik B: kontak terikat ke pulau A
                islands[ia].contact_indices.push(contact_idx);
            }
            (None, Some(ib)) => {
                // Statis/Kinematik A <-> Dinamis B: kontak terikat ke pulau B
                islands[ib].contact_indices.push(contact_idx);
            }
            (None, None) => {
                // Statis/Kinematik <-> Statis/Kinematik: diabaikan untuk pulau dinamis
            }
        }
    }

    Ok(islands)
}
