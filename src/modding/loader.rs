use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::material::{MaterialDef, MaterialRegistry};
use crate::modding::definitions::{BlockDefinition, MaterialDefinition};
use crate::modding::discovery::DiscoveredMod;
use crate::modding::registry::BlockRegistry;
use crate::modding::resource_id::{ModId, ResourceId};

/// Error yang terjadi saat memuat data JSON content dari folder mod
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentError {
    IoError {
        path: PathBuf,
        message: String,
    },
    JsonParseError {
        path: PathBuf,
        message: String,
    },
    NamespaceMismatch {
        id: ResourceId,
        expected: ModId,
    },
    UnresolvedMaterial {
        block_id: ResourceId,
        material_id: ResourceId,
    },
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IoError { path, message } => {
                write!(f, "Gagal membaca file {:?}: {}", path, message)
            }
            Self::JsonParseError { path, message } => {
                write!(f, "Gagal parsing JSON di {:?}: {}", path, message)
            }
            Self::NamespaceMismatch { id, expected } => write!(
                f,
                "Namespace ID '{}' tidak cocok dengan namespace mod '{}'",
                id, expected
            ),
            Self::UnresolvedMaterial {
                block_id,
                material_id,
            } => write!(
                f,
                "Blok '{}' mereferensikan material '{}' yang belum terdaftar",
                block_id, material_id
            ),
        }
    }
}

impl std::error::Error for ContentError {}

/// Ringkasan konten yang berhasil dimuat dari suatu mod
#[derive(Debug, Clone, Default)]
pub struct ModContentSummary {
    pub materials_loaded: usize,
    pub blocks_loaded: usize,
}

/// ModLoader untuk memproses file JSON material dan blok
pub struct ModLoader;

impl ModLoader {
    /// Memuat seluruh file material (`materials/*.json`) dari folder mod
    pub fn load_materials_from_dir<P: AsRef<Path>>(
        materials_dir: P,
        mod_id: &ModId,
        registry: &mut MaterialRegistry,
    ) -> Result<usize, ContentError> {
        let dir = materials_dir.as_ref();
        if !dir.exists() || !dir.is_dir() {
            return Ok(0);
        }

        let mut entries: Vec<PathBuf> = fs::read_dir(dir)
            .map_err(|e| ContentError::IoError {
                path: dir.to_path_buf(),
                message: e.to_string(),
            })?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
            .collect();

        // Urutkan file secara deterministik
        entries.sort();

        let mut count = 0;
        for file_path in entries {
            let content = fs::read_to_string(&file_path).map_err(|e| ContentError::IoError {
                path: file_path.clone(),
                message: e.to_string(),
            })?;

            let def: MaterialDefinition =
                serde_json::from_str(&content).map_err(|e| ContentError::JsonParseError {
                    path: file_path.clone(),
                    message: e.to_string(),
                })?;

            // Validasi namespace kepemilikan
            if def.id.namespace != *mod_id {
                return Err(ContentError::NamespaceMismatch {
                    id: def.id,
                    expected: mod_id.clone(),
                });
            }

            registry.register_resource(
                def.id,
                MaterialDef {
                    name: def.name,
                    density_kg_m3: def.density,
                    shear_strength_mpa: def.shear_strength,
                    base_color: def.color,
                    is_solid: def.solid,
                    is_transparent: def.transparent,
                },
            );

            count += 1;
        }

        Ok(count)
    }

    /// Memuat seluruh file block (`blocks/*.json`) dari folder mod
    pub fn load_blocks_from_dir<P: AsRef<Path>>(
        blocks_dir: P,
        mod_id: &ModId,
        block_registry: &mut BlockRegistry,
        material_registry: &MaterialRegistry,
    ) -> Result<usize, ContentError> {
        let dir = blocks_dir.as_ref();
        if !dir.exists() || !dir.is_dir() {
            return Ok(0);
        }

        let mut entries: Vec<PathBuf> = fs::read_dir(dir)
            .map_err(|e| ContentError::IoError {
                path: dir.to_path_buf(),
                message: e.to_string(),
            })?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
            .collect();

        // Urutkan file secara deterministik
        entries.sort();

        let mut count = 0;
        for file_path in entries {
            let content = fs::read_to_string(&file_path).map_err(|e| ContentError::IoError {
                path: file_path.clone(),
                message: e.to_string(),
            })?;

            let def: BlockDefinition =
                serde_json::from_str(&content).map_err(|e| ContentError::JsonParseError {
                    path: file_path.clone(),
                    message: e.to_string(),
                })?;

            // Validasi namespace kepemilikan
            if def.id.namespace != *mod_id {
                return Err(ContentError::NamespaceMismatch {
                    id: def.id,
                    expected: mod_id.clone(),
                });
            }

            // Validasi referensi material harus sudah terdaftar
            if material_registry
                .get_by_resource_id(&def.material)
                .is_none()
            {
                return Err(ContentError::UnresolvedMaterial {
                    block_id: def.id,
                    material_id: def.material,
                });
            }

            block_registry
                .register_or_replace(def)
                .map_err(|e| ContentError::IoError {
                    path: file_path.clone(),
                    message: e.to_string(),
                })?;

            count += 1;
        }

        Ok(count)
    }

    /// Memuat seluruh konten dari mod yang telah terverifikasi
    pub fn load_mod(
        discovered: &DiscoveredMod,
        material_registry: &mut MaterialRegistry,
        block_registry: &mut BlockRegistry,
    ) -> Result<ModContentSummary, ContentError> {
        let materials_dir = discovered.root_dir.join("materials");
        let blocks_dir = discovered.root_dir.join("blocks");

        let mat_count = Self::load_materials_from_dir(
            materials_dir,
            &discovered.manifest.id,
            material_registry,
        )?;
        let blk_count = Self::load_blocks_from_dir(
            blocks_dir,
            &discovered.manifest.id,
            block_registry,
            material_registry,
        )?;

        Ok(ModContentSummary {
            materials_loaded: mat_count,
            blocks_loaded: blk_count,
        })
    }
}
