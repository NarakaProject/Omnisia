use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

use crate::modding::definitions::BlockDefinition;
use crate::modding::resource_id::{ModId, ResourceId};

/// Asal sumber dari resource yang terdaftar
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceSource {
    Core,
    Mod(ModId),
}

impl fmt::Display for ResourceSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core => write!(f, "Core"),
            Self::Mod(id) => write!(f, "Mod({})", id),
        }
    }
}

/// Metadata pelacakan untuk resource yang telah dioverride secara eksplisit
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverrideMetadata {
    pub target: ResourceId,
    pub replacement: ResourceId,
    pub source_mod: ModId,
}

/// Entri dalam ResourceRegistry yang menyimpan data bersama informasi provenance
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryEntry<T> {
    pub id: ResourceId,
    pub item: T,
    pub original_source: ResourceSource,
    pub active_source: ResourceSource,
    pub override_info: Option<OverrideMetadata>,
}

/// Error yang terjadi saat interaksi dengan registry
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    DuplicateRegistration(ResourceId),
    NotFound(ResourceId),
    CapacityExceeded(usize),
    OverrideConflict {
        target: ResourceId,
        existing_mod: ModId,
        new_mod: ModId,
    },
    InvalidOverrideOwnership {
        declaring_mod: ModId,
        replacement: ResourceId,
    },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateRegistration(id) => {
                write!(f, "Resource ID '{}' sudah terdaftar di registry", id)
            }
            Self::NotFound(id) => write!(f, "Resource ID '{}' tidak ditemukan dalam registry", id),
            Self::CapacityExceeded(cap) => {
                write!(f, "Kapasitas maksimum registry ({} elemen) terlampaui", cap)
            }
            Self::OverrideConflict {
                target,
                existing_mod,
                new_mod,
            } => write!(
                f,
                "Konflik override pada target '{}': Mod '{}' sudah meng-override target ini, mod '{}' ditolak",
                target, existing_mod, new_mod
            ),
            Self::InvalidOverrideOwnership {
                declaring_mod,
                replacement,
            } => write!(
                f,
                "Mod '{}' tidak boleh menggunakan replacement '{}' dari namespace lain",
                declaring_mod, replacement
            ),
        }
    }
}

impl std::error::Error for RegistryError {}

/// ID integer runtime kompak untuk blok di dunia (2 byte).
#[repr(transparent)]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
pub struct BlockId(pub u16);

impl BlockId {
    pub const AIR: Self = Self(0);
}

/// Generic Registry untuk memetakan identitas persisten (`ResourceId`) ke integer runtime ID secara deterministik
/// dengan pelacakan kepemilikan dan penanganan override aman.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRegistry<T> {
    entries: Vec<RegistryEntry<T>>,
    id_map: HashMap<ResourceId, u16>,
}

impl<T> Default for ResourceRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ResourceRegistry<T> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            id_map: HashMap::new(),
        }
    }

    /// Mendaftarkan resource baru ke dalam registry secara aman.
    ///
    /// Menolak duplikasi secara tegas: jika `id` sudah terdaftar, mengembalikan `DuplicateRegistration`.
    pub fn register(
        &mut self,
        id: ResourceId,
        item: T,
        source: ResourceSource,
    ) -> Result<u16, RegistryError> {
        if self.id_map.contains_key(&id) {
            return Err(RegistryError::DuplicateRegistration(id));
        }

        if self.entries.len() >= u16::MAX as usize {
            return Err(RegistryError::CapacityExceeded(u16::MAX as usize));
        }

        let runtime_index = self.entries.len() as u16;
        self.id_map.insert(id.clone(), runtime_index);
        self.entries.push(RegistryEntry {
            id,
            item,
            original_source: source.clone(),
            active_source: source,
            override_info: None,
        });
        Ok(runtime_index)
    }

    /// Mengambil item berdasarkan runtime integer ID O(1)
    #[inline(always)]
    pub fn get_by_index(&self, index: u16) -> Option<&T> {
        self.entries.get(index as usize).map(|e| &e.item)
    }

    /// Mengambil entri registry lengkap (termasuk provenance) berdasarkan runtime index
    #[inline(always)]
    pub fn get_entry_by_index(&self, index: u16) -> Option<&RegistryEntry<T>> {
        self.entries.get(index as usize)
    }

    /// Mengambil ResourceId berdasarkan runtime index O(1)
    #[inline(always)]
    pub fn get_resource_id_by_index(&self, index: u16) -> Option<&ResourceId> {
        self.entries.get(index as usize).map(|e| &e.id)
    }

    /// Mengambil item berdasarkan ResourceId persisten
    #[inline(always)]
    pub fn get(&self, id: &ResourceId) -> Option<&T> {
        self.id_map.get(id).and_then(|&idx| self.get_by_index(idx))
    }

    /// Mengambil entri lengkap (termasuk provenance) berdasarkan ResourceId
    #[inline(always)]
    pub fn get_entry(&self, id: &ResourceId) -> Option<&RegistryEntry<T>> {
        self.id_map
            .get(id)
            .and_then(|&idx| self.get_entry_by_index(idx))
    }

    /// Mengonversi ResourceId persisten ke runtime integer index
    #[inline(always)]
    pub fn resolve_runtime_id(&self, id: &ResourceId) -> Option<u16> {
        self.id_map.get(id).copied()
    }

    /// Jumlah total resource terdaftar
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterasi seluruh pasangan (ResourceId, &T) dalam urutan pendaftaran deterministik
    pub fn iter(&self) -> impl Iterator<Item = (&ResourceId, &T)> {
        self.entries.iter().map(|e| (&e.id, &e.item))
    }

    /// Iterasi seluruh entri lengkap (termasuk provenance)
    pub fn iter_entries(&self) -> impl Iterator<Item = &RegistryEntry<T>> {
        self.entries.iter()
    }
}

impl<T: Clone> ResourceRegistry<T> {
    /// Menerapkan deklarasi explicit override.
    ///
    /// Target resource tetap mempertahankan ResourceId aslinya (misal `core:stone`),
    /// namun data item aktif diperbarui ke definisi replacement dan provenance dicatat.
    pub fn apply_explicit_override(
        &mut self,
        target: &ResourceId,
        replacement_id: &ResourceId,
        source_mod: ModId,
    ) -> Result<(), RegistryError> {
        // 1. Validasi kepemilikan: mod hanya boleh memakai replacement miliknya sendiri
        if replacement_id.namespace != source_mod {
            return Err(RegistryError::InvalidOverrideOwnership {
                declaring_mod: source_mod,
                replacement: replacement_id.clone(),
            });
        }

        // 2. Ambil indeks target dan replacement
        let target_idx = *self
            .id_map
            .get(target)
            .ok_or_else(|| RegistryError::NotFound(target.clone()))?
            as usize;

        let replacement_idx = *self
            .id_map
            .get(replacement_id)
            .ok_or_else(|| RegistryError::NotFound(replacement_id.clone()))?
            as usize;

        // 3. Deteksi konflik override ganda
        if let Some(ref existing_ov) = self.entries[target_idx].override_info {
            return Err(RegistryError::OverrideConflict {
                target: target.clone(),
                existing_mod: existing_ov.source_mod.clone(),
                new_mod: source_mod,
            });
        }

        // 4. Salin definisi item dari replacement ke target entry
        let replacement_item = self.entries[replacement_idx].item.clone();
        let target_entry = &mut self.entries[target_idx];
        target_entry.item = replacement_item;
        target_entry.active_source = ResourceSource::Mod(source_mod.clone());
        target_entry.override_info = Some(OverrideMetadata {
            target: target.clone(),
            replacement: replacement_id.clone(),
            source_mod,
        });

        Ok(())
    }
}

/// Registry khusus untuk definisi Block
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlockRegistry {
    inner: ResourceRegistry<BlockDefinition>,
}

impl BlockRegistry {
    pub fn new() -> Self {
        Self {
            inner: ResourceRegistry::new(),
        }
    }

    pub fn register(
        &mut self,
        def: BlockDefinition,
        source: ResourceSource,
    ) -> Result<BlockId, RegistryError> {
        let id = def.id.clone();
        let idx = self.inner.register(id, def, source)?;
        Ok(BlockId(idx))
    }

    pub fn apply_explicit_override(
        &mut self,
        target: &ResourceId,
        replacement: &ResourceId,
        source_mod: ModId,
    ) -> Result<(), RegistryError> {
        self.inner
            .apply_explicit_override(target, replacement, source_mod)
    }

    #[inline(always)]
    pub fn get(&self, id: BlockId) -> Option<&BlockDefinition> {
        self.inner.get_by_index(id.0)
    }

    #[inline(always)]
    pub fn get_entry(&self, id: BlockId) -> Option<&RegistryEntry<BlockDefinition>> {
        self.inner.get_entry_by_index(id.0)
    }

    #[inline(always)]
    pub fn get_by_resource_id(&self, res_id: &ResourceId) -> Option<&BlockDefinition> {
        self.inner.get(res_id)
    }

    #[inline(always)]
    pub fn resolve_block_id(&self, res_id: &ResourceId) -> Option<BlockId> {
        self.inner.resolve_runtime_id(res_id).map(BlockId)
    }

    #[inline(always)]
    pub fn resolve_resource_id(&self, id: BlockId) -> Option<&ResourceId> {
        self.inner.get_resource_id_by_index(id.0)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ResourceId, &BlockDefinition)> {
        self.inner.iter()
    }

    pub fn iter_entries(&self) -> impl Iterator<Item = &RegistryEntry<BlockDefinition>> {
        self.inner.iter_entries()
    }
}
