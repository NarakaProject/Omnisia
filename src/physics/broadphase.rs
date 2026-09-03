use glam::{IVec3, Vec3};
use std::collections::BTreeMap;
use std::fmt;

use super::aabb::{Aabb, AabbError};

/// Identifier runtime unik dan stabil untuk setiap badan kaku (RigidBody).
/// Terurut secara alami (`Ord`, `PartialOrd`) untuk menjamin generasi pasangan
/// kandidat dan iterasi yang sepenuhnya deterministik.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RigidBodyId(pub u64);

impl fmt::Display for RigidBodyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RigidBody#{}", self.0)
    }
}

/// Kategori partisipasi fisik dari suatu badan dalam sistem simulasi.
///
/// CATATAN ARSITEKTURAL:
/// Kategori `Kinematic` disediakan untuk entitas seperti platform bergerak atau mesin kinematik.
/// Kategori ini **TIDAK** mengubah Player menjadi `RigidBody`. Player tetap menggunakan
/// `PlayerController` kinematik khusus (Phase 8D).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BodyType {
    /// Objek statis (tidak bergerak oleh gaya atau gravitasi)
    Static,
    /// Objek dinamis penuh (digerakkan oleh gaya, gravitasi, dan impuls)
    Dynamic,
    /// Objek yang dikontrol secara kinematik (digerakkan oleh skrip/kecepatan eksplisit)
    Kinematic,
}

impl BodyType {
    /// Memeriksa apakah dua kategori badan diizinkan untuk saling bertabrakan
    /// dalam broadphase. Objek Static ↔ Static tidak pernah berpartisipasi dalam deteksi kontak.
    #[inline(always)]
    pub fn can_collide_with(&self, other: BodyType) -> bool {
        !matches!((self, other), (BodyType::Static, BodyType::Static))
    }
}

/// Representasi proxy spasial dari suatu badan dalam struktur broadphase.
#[derive(Debug, Clone, PartialEq)]
pub struct BroadphaseProxy {
    pub body_id: RigidBodyId,
    pub body_type: BodyType,
    pub aabb: Aabb,
}

impl BroadphaseProxy {
    pub fn new(body_id: RigidBodyId, body_type: BodyType, aabb: Aabb) -> Self {
        Self {
            body_id,
            body_type,
            aabb,
        }
    }
}

/// Pasangan kandidat tabrakan kanonikal antara dua badan kaku.
///
/// INVARIAN KANONIKAL:
/// - `body_a < body_b` selalu terjamin (tidak ada pasangan terbalik seperti (B, A)).
/// - `body_a != body_b` selalu terjamin (tidak ada self-collision (A, A)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BroadphasePair {
    pub body_a: RigidBodyId,
    pub body_b: RigidBodyId,
}

impl BroadphasePair {
    /// Membuat pasangan kanonikal baru di mana `body_a < body_b`.
    /// Mengembalikan `None` jika `id_1 == id_2` (mencegah tabrakan mandiri).
    pub fn new(id_1: RigidBodyId, id_2: RigidBodyId) -> Option<Self> {
        if id_1 == id_2 {
            None
        } else if id_1 < id_2 {
            Some(Self {
                body_a: id_1,
                body_b: id_2,
            })
        } else {
            Some(Self {
                body_a: id_2,
                body_b: id_1,
            })
        }
    }
}

/// Kesalahan operasi pada broadphase dan registri fisika.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BroadphaseError {
    /// Badan dengan ID tersebut sudah terdaftar
    BodyAlreadyExists(RigidBodyId),
    /// Badan dengan ID tersebut tidak ditemukan
    BodyNotFound(RigidBodyId),
    /// AABB tidak valid (non-finite atau min > max)
    InvalidAabb(AabbError),
    /// Collider dengan ID tersebut sudah terdaftar
    ColliderAlreadyExists(super::collider::ColliderId),
    /// Collider dengan ID tersebut tidak ditemukan
    ColliderNotFound(super::collider::ColliderId),
    /// Kesalahan geometri bentuk
    ShapeError(super::shape::ShapeError),
}

impl fmt::Display for BroadphaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BodyAlreadyExists(id) => write!(f, "Badan {} sudah terdaftar di broadphase", id),
            Self::BodyNotFound(id) => write!(f, "Badan {} tidak ditemukan di broadphase", id),
            Self::InvalidAabb(err) => write!(f, "AABB tidak valid: {}", err),
            Self::ColliderAlreadyExists(id) => write!(f, "Collider {:?} sudah terdaftar", id),
            Self::ColliderNotFound(id) => write!(f, "Collider {:?} tidak ditemukan", id),
            Self::ShapeError(err) => write!(f, "Kesalahan bentuk tabrakan: {}", err),
        }
    }
}

impl std::error::Error for BroadphaseError {}

impl From<super::shape::ShapeError> for BroadphaseError {
    fn from(err: super::shape::ShapeError) -> Self {
        Self::ShapeError(err)
    }
}

/// Koordinat sel 3D integer spatial hash yang memiliki urutan total (`Ord`, `PartialOrd`)
/// untuk menjamin determinisme saat digunakan sebagai kunci dalam struktur `BTreeMap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl CellCoord {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }
}

impl From<IVec3> for CellCoord {
    #[inline(always)]
    fn from(v: IVec3) -> Self {
        Self {
            x: v.x,
            y: v.y,
            z: v.z,
        }
    }
}

impl From<CellCoord> for IVec3 {
    #[inline(always)]
    fn from(c: CellCoord) -> Self {
        Self::new(c.x, c.y, c.z)
    }
}

/// Konversi posisi dunia (meter) ke koordinat sel integer spatial hash.
///
/// SEMANTIK EUCLIDEAN FLOOR:
/// Menggunakan pembagian floor matematis sejati untuk menangani koordinat negatif
/// secara kontinu tanpa diskontinuitas atau pemotongan (truncation) di sekitar x=0.
#[inline(always)]
pub fn world_pos_to_cell(pos: Vec3, cell_size: f32) -> CellCoord {
    CellCoord {
        x: (pos.x / cell_size).floor() as i32,
        y: (pos.y / cell_size).floor() as i32,
        z: (pos.z / cell_size).floor() as i32,
    }
}

/// Broadphase berbasis Uniform Spatial Hash Grid 3D yang sepenuhnya deterministik.
///
/// PRINSIP ARSITEKTUR & DESAIN:
/// 1. **Zero Voxel Iteration**: Broadphase beroperasi secara eksklusif pada bounding box (`Aabb`),
///    tidak pernah melakukan pemindaian voxel di dalam chunk atau badan selama pembaruan.
/// 2. **Conservative Cell-Boundary Inclusion**: Objek AABB yang menyentuh atau melintasi batas sel
///    diindeks secara konservatif di seluruh sel yang bersinggungan (`[min_cell ..= max_cell]`),
///    memastikan tidak ada potensi kontak yang terlewat.
/// 3. **False Positives Allowed**: Tumpang tindih AABB pada broadphase adalah estimasi kasar;
///    kontak geometris aktual akan disaring oleh narrowphase (Phase 9.4).
/// 4. **Determinisme Mutlak**: Menggunakan `BTreeMap` untuk penyimpanan sel dan proxy, menjamin
///    urutan iterasi dan generasi pasangan kandidat selalu identik untuk input yang sama.
/// 5. **Dukungan Multi-Cell**: Badan besar yang melintasi beberapa sel diindeks di seluruh sel tersebut,
///    dan pasangan kandidat dideduplikasi secara kanonikal.
pub struct SpatialHashBroadphase {
    cell_size: f32,
    proxies: BTreeMap<RigidBodyId, BroadphaseProxy>,
    grid: BTreeMap<CellCoord, Vec<RigidBodyId>>,
}

impl Default for SpatialHashBroadphase {
    fn default() -> Self {
        // Ukuran sel default 4.0 meter (8 voxel x 0.5m), seimbang untuk objek dinamis 1-8m
        Self::new(4.0)
    }
}

impl SpatialHashBroadphase {
    /// Membuat instance spatial hash broadphase baru dengan ukuran sel tertentu (meter).
    pub fn new(cell_size: f32) -> Self {
        assert!(
            cell_size > 0.0 && cell_size.is_finite(),
            "Ukuran sel broadphase harus positif dan terhingga"
        );
        Self {
            cell_size,
            proxies: BTreeMap::new(),
            grid: BTreeMap::new(),
        }
    }

    /// Mengembalikan ukuran sel grid dalam satuan meter
    #[inline(always)]
    pub fn cell_size(&self) -> f32 {
        self.cell_size
    }

    /// Jumlah total proxy yang terdaftar dalam broadphase
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.proxies.len()
    }

    /// Apakah broadphase kosong
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.proxies.is_empty()
    }

    /// Memeriksa apakah suatu badan terdaftar dalam broadphase
    #[inline(always)]
    pub fn contains(&self, id: RigidBodyId) -> bool {
        self.proxies.contains_key(&id)
    }

    /// Mengambil referensi ke proxy berdasarkan ID
    #[inline(always)]
    pub fn get_proxy(&self, id: RigidBodyId) -> Option<&BroadphaseProxy> {
        self.proxies.get(&id)
    }

    /// Menghitung rentang sel inklusif [min_cell, max_cell] yang dicakup oleh suatu AABB
    #[inline]
    fn compute_cell_range(&self, aabb: &Aabb) -> (CellCoord, CellCoord) {
        let min_cell = world_pos_to_cell(aabb.min, self.cell_size);
        let max_cell = world_pos_to_cell(aabb.max, self.cell_size);
        (min_cell, max_cell)
    }

    /// Mendaftarkan proxy badan baru ke dalam broadphase.
    pub fn insert(&mut self, proxy: BroadphaseProxy) -> Result<(), BroadphaseError> {
        if self.proxies.contains_key(&proxy.body_id) {
            return Err(BroadphaseError::BodyAlreadyExists(proxy.body_id));
        }
        if !proxy.aabb.is_valid() {
            return Err(BroadphaseError::InvalidAabb(
                AabbError::NonFiniteCoordinates,
            ));
        }

        let (min_cell, max_cell) = self.compute_cell_range(&proxy.aabb);
        let id = proxy.body_id;

        for cy in min_cell.y..=max_cell.y {
            for cz in min_cell.z..=max_cell.z {
                for cx in min_cell.x..=max_cell.x {
                    let cell_coord = CellCoord::new(cx, cy, cz);
                    let cell_vec = self.grid.entry(cell_coord).or_default();
                    cell_vec.push(id);
                }
            }
        }

        self.proxies.insert(id, proxy);
        Ok(())
    }

    /// Menghapus proxy badan dari broadphase.
    pub fn remove(&mut self, id: RigidBodyId) -> Option<BroadphaseProxy> {
        let proxy = self.proxies.remove(&id)?;
        let (min_cell, max_cell) = self.compute_cell_range(&proxy.aabb);

        for cy in min_cell.y..=max_cell.y {
            for cz in min_cell.z..=max_cell.z {
                for cx in min_cell.x..=max_cell.x {
                    let cell_coord = CellCoord::new(cx, cy, cz);
                    if let Some(cell_vec) = self.grid.get_mut(&cell_coord) {
                        cell_vec.retain(|&entry_id| entry_id != id);
                        if cell_vec.is_empty() {
                            self.grid.remove(&cell_coord);
                        }
                    }
                }
            }
        }

        Some(proxy)
    }

    /// Memperbarui AABB dunia dari suatu badan terdaftar.
    /// Memiliki jalur cepat (*fast path*) jika rentang sel tidak berubah.
    pub fn update(&mut self, id: RigidBodyId, new_aabb: Aabb) -> Result<(), BroadphaseError> {
        if !new_aabb.is_valid() {
            return Err(BroadphaseError::InvalidAabb(
                AabbError::NonFiniteCoordinates,
            ));
        }

        let old_aabb = match self.proxies.get(&id) {
            Some(p) => p.aabb,
            None => return Err(BroadphaseError::BodyNotFound(id)),
        };

        let old_range = self.compute_cell_range(&old_aabb);
        let new_range = self.compute_cell_range(&new_aabb);

        let proxy = self.proxies.get_mut(&id).unwrap();
        proxy.aabb = new_aabb;

        // Jalur cepat: jika rentang sel grid identik, tidak perlu memodifikasi isi sel
        if old_range == new_range {
            return Ok(());
        }

        // Hapus dari sel lama
        let (old_min, old_max) = old_range;
        for cy in old_min.y..=old_max.y {
            for cz in old_min.z..=old_max.z {
                for cx in old_min.x..=old_max.x {
                    let cell_coord = CellCoord::new(cx, cy, cz);
                    if let Some(cell_vec) = self.grid.get_mut(&cell_coord) {
                        cell_vec.retain(|&entry_id| entry_id != id);
                        if cell_vec.is_empty() {
                            self.grid.remove(&cell_coord);
                        }
                    }
                }
            }
        }

        // Masukkan ke sel baru
        let (new_min, new_max) = new_range;
        for cy in new_min.y..=new_max.y {
            for cz in new_min.z..=new_max.z {
                for cx in new_min.x..=new_max.x {
                    let cell_coord = CellCoord::new(cx, cy, cz);
                    let cell_vec = self.grid.entry(cell_coord).or_default();
                    cell_vec.push(id);
                }
            }
        }

        Ok(())
    }

    /// Menemukan seluruh badan terdaftar yang AABB-nya bertumpukan dengan AABB kueri.
    /// Hasil dikembalikan terurut secara deterministik berdasarkan `RigidBodyId`.
    pub fn query_aabb(&self, query_box: &Aabb) -> Vec<RigidBodyId> {
        if !query_box.is_valid() || self.proxies.is_empty() {
            return Vec::new();
        }

        let (min_cell, max_cell) = self.compute_cell_range(query_box);
        let mut seen = std::collections::BTreeSet::new();

        for cy in min_cell.y..=max_cell.y {
            for cz in min_cell.z..=max_cell.z {
                for cx in min_cell.x..=max_cell.x {
                    let cell_coord = CellCoord::new(cx, cy, cz);
                    if let Some(cell_vec) = self.grid.get(&cell_coord) {
                        for &body_id in cell_vec {
                            if seen.contains(&body_id) {
                                continue;
                            }
                            if let Some(proxy) = self.proxies.get(&body_id) {
                                if proxy.aabb.overlaps(query_box) {
                                    seen.insert(body_id);
                                }
                            }
                        }
                    }
                }
            }
        }

        seen.into_iter().collect()
    }

    /// Menghasilkan seluruh pasangan kandidat tabrakan kanonikal (`body_a < body_b`).
    ///
    /// JAMINAN DETERMINISTIK & FILTERING:
    /// 1. Pasangan `Static ↔ Static` tidak pernah dihasilkan.
    /// 2. Pasangan terduplikasi (akibat badan melintasi banyak sel) dideduplikasi secara otomatis.
    /// 3. Tidak ada tabrakan mandiri (`body_a != body_b`).
    /// 4. Pasangan terurut secara kanonikal (`body_a < body_b`) dan terurut deterministik dalam vektor keluaran.
    pub fn generate_candidate_pairs(&self) -> Vec<BroadphasePair> {
        let mut candidate_set = std::collections::BTreeSet::new();

        for cell_vec in self.grid.values() {
            if cell_vec.len() < 2 {
                continue;
            }

            for i in 0..cell_vec.len() {
                let id_i = cell_vec[i];
                let proxy_i = match self.proxies.get(&id_i) {
                    Some(p) => p,
                    None => continue,
                };

                for &id_j in &cell_vec[(i + 1)..] {
                    if id_i == id_j {
                        continue;
                    }

                    let proxy_j = match self.proxies.get(&id_j) {
                        Some(p) => p,
                        None => continue,
                    };

                    // Filter kategori partisipasi (lewati Static ↔ Static)
                    if !proxy_i.body_type.can_collide_with(proxy_j.body_type) {
                        continue;
                    }

                    // Uji overlap AABB kasar
                    if proxy_i.aabb.overlaps(&proxy_j.aabb) {
                        if let Some(pair) = BroadphasePair::new(id_i, id_j) {
                            candidate_set.insert(pair);
                        }
                    }
                }
            }
        }

        candidate_set.into_iter().collect()
    }

    /// Mengosongkan seluruh proxy dan sel broadphase
    pub fn clear(&mut self) {
        self.proxies.clear();
        self.grid.clear();
    }
}
