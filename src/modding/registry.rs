use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

use crate::modding::definitions::BlockDefinition;
use crate::modding::resource_id::ResourceId;

/// Error yang terjadi saat interaksi dengan registry
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    DuplicateRegistration(ResourceId),
    NotFound(ResourceId),
    CapacityExceeded(usize),
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

/// Generic Registry untuk memetakan identitas persisten (`ResourceId`) ke integer runtime ID secara deterministik.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRegistry<T> {
    entries: Vec<(ResourceId, T)>,
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

    /// Mendaftarkan resource baru ke dalam registry secara deterministik.
    /// Mengembalikan runtime index `u16`.
    pub fn register(&mut self, id: ResourceId, item: T) -> Result<u16, RegistryError> {
        if self.id_map.contains_key(&id) {
            return Err(RegistryError::DuplicateRegistration(id));
        }

        if self.entries.len() >= u16::MAX as usize {
            return Err(RegistryError::CapacityExceeded(u16::MAX as usize));
        }

        let runtime_index = self.entries.len() as u16;
        self.id_map.insert(id.clone(), runtime_index);
        self.entries.push((id, item));
        Ok(runtime_index)
    }

    /// Mendaftarkan atau menimpa resource (berguna untuk update/patching mod).
    pub fn register_or_replace(&mut self, id: ResourceId, item: T) -> Result<u16, RegistryError> {
        if let Some(&runtime_idx) = self.id_map.get(&id) {
            self.entries[runtime_idx as usize] = (id, item);
            Ok(runtime_idx)
        } else {
            self.register(id, item)
        }
    }

    /// Mengambil item berdasarkan runtime integer ID O(1)
    #[inline(always)]
    pub fn get_by_index(&self, index: u16) -> Option<&T> {
        self.entries.get(index as usize).map(|(_, item)| item)
    }

    /// Mengambil ResourceId berdasarkan runtime index O(1)
    #[inline(always)]
    pub fn get_resource_id_by_index(&self, index: u16) -> Option<&ResourceId> {
        self.entries.get(index as usize).map(|(id, _)| id)
    }

    /// Mengambil item berdasarkan ResourceId persisten
    #[inline(always)]
    pub fn get(&self, id: &ResourceId) -> Option<&T> {
        self.id_map.get(id).and_then(|&idx| self.get_by_index(idx))
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
        self.entries.iter().map(|(id, item)| (id, item))
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

    pub fn register(&mut self, def: BlockDefinition) -> Result<BlockId, RegistryError> {
        let id = def.id.clone();
        let idx = self.inner.register(id, def)?;
        Ok(BlockId(idx))
    }

    pub fn register_or_replace(&mut self, def: BlockDefinition) -> Result<BlockId, RegistryError> {
        let id = def.id.clone();
        let idx = self.inner.register_or_replace(id, def)?;
        Ok(BlockId(idx))
    }

    #[inline(always)]
    pub fn get(&self, id: BlockId) -> Option<&BlockDefinition> {
        self.inner.get_by_index(id.0)
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
}
