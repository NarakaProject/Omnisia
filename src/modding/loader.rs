use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::material::{MaterialDef, MaterialRegistry};
use crate::modding::definitions::{BlockDefinition, MaterialDefinition};
use crate::modding::discovery::DiscoveredMod;
use crate::modding::registry::{BlockRegistry, RegistryError, ResourceSource};
use crate::modding::resource_id::{ModId, ResourceId};

/// Error yang terjadi saat memuat data JSON content dari folder core atau mod
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
    ReservedNamespaceViolation {
        id: ResourceId,
        mod_id: ModId,
    },
    UnresolvedMaterial {
        block_id: ResourceId,
        material_id: ResourceId,
    },
    MissingCoreDirectory(PathBuf),
    UnresolvedOverrideTarget(ResourceId),
    UnresolvedOverrideReplacement(ResourceId),
    Registry(RegistryError),
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
            Self::ReservedNamespaceViolation { id, mod_id } => write!(
                f,
                "Mod '{}' dilarang mendaftarkan resource pada reserved namespace 'core' ('{}')",
                mod_id, id
            ),
            Self::UnresolvedMaterial {
                block_id,
                material_id,
            } => write!(
                f,
                "Blok '{}' mereferensikan material '{}' yang belum terdaftar",
                block_id, material_id
            ),
            Self::MissingCoreDirectory(path) => {
                write!(f, "Direktori Core Content tidak ditemukan di {:?}", path)
            }
            Self::UnresolvedOverrideTarget(target) => {
                write!(
                    f,
                    "Target override '{}' tidak ditemukan di Core Registry",
                    target
                )
            }
            Self::UnresolvedOverrideReplacement(rep) => {
                write!(
                    f,
                    "Replacement override '{}' belum terdaftar di registry",
                    rep
                )
            }
            Self::Registry(e) => write!(f, "Registry error: {}", e),
        }
    }
}

impl std::error::Error for ContentError {}

impl From<RegistryError> for ContentError {
    fn from(e: RegistryError) -> Self {
        Self::Registry(e)
    }
}

/// Ringkasan konten yang berhasil dimuat dari suatu mod atau core
#[derive(Debug, Clone, Default)]
pub struct ModContentSummary {
    pub materials_loaded: usize,
    pub blocks_loaded: usize,
    pub overrides_applied: usize,
}

/// ModLoader untuk memproses file JSON core content dan mod content
pub struct ModLoader;

impl ModLoader {
    /// Memuat konten bawaan engine dari direktori `content/core/`
    pub fn load_core_content<P: AsRef<Path>>(
        core_dir: P,
        material_registry: &mut MaterialRegistry,
        block_registry: &mut BlockRegistry,
    ) -> Result<ModContentSummary, ContentError> {
        let dir = core_dir.as_ref();
        if !dir.exists() || !dir.is_dir() {
            return Err(ContentError::MissingCoreDirectory(dir.to_path_buf()));
        }

        let core_id = ModId::core();
        let materials_dir = dir.join("materials");
        let blocks_dir = dir.join("blocks");

        let mat_count = Self::load_materials_internal(
            &materials_dir,
            &core_id,
            material_registry,
            ResourceSource::Core,
            false,
        )?;

        let blk_count = Self::load_blocks_internal(
            &blocks_dir,
            &core_id,
            block_registry,
            material_registry,
            ResourceSource::Core,
            false,
        )?;

        Ok(ModContentSummary {
            materials_loaded: mat_count,
            blocks_loaded: blk_count,
            overrides_applied: 0,
        })
    }

    /// Memuat seluruh file material (`materials/*.json`) dari folder mod dengan proteksi namespace
    pub fn load_materials_from_dir<P: AsRef<Path>>(
        materials_dir: P,
        mod_id: &ModId,
        registry: &mut MaterialRegistry,
    ) -> Result<usize, ContentError> {
        Self::load_materials_internal(
            materials_dir,
            mod_id,
            registry,
            ResourceSource::Mod(mod_id.clone()),
            true,
        )
    }

    fn load_materials_internal<P: AsRef<Path>>(
        materials_dir: P,
        mod_id: &ModId,
        registry: &mut MaterialRegistry,
        source: ResourceSource,
        enforce_mod_restrictions: bool,
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

            // 1. Proteksi Reserved Namespace: Mod dilarang mendaftarkan namespace core:*
            if enforce_mod_restrictions && def.id.namespace.as_str() == ModId::CORE {
                return Err(ContentError::ReservedNamespaceViolation {
                    id: def.id,
                    mod_id: mod_id.clone(),
                });
            }

            // 2. Validasi namespace kepemilikan
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
                source.clone(),
            )?;

            count += 1;
        }

        Ok(count)
    }

    /// Memuat seluruh file block (`blocks/*.json`) dari folder mod dengan proteksi namespace
    pub fn load_blocks_from_dir<P: AsRef<Path>>(
        blocks_dir: P,
        mod_id: &ModId,
        block_registry: &mut BlockRegistry,
        material_registry: &MaterialRegistry,
    ) -> Result<usize, ContentError> {
        Self::load_blocks_internal(
            blocks_dir,
            mod_id,
            block_registry,
            material_registry,
            ResourceSource::Mod(mod_id.clone()),
            true,
        )
    }

    fn load_blocks_internal<P: AsRef<Path>>(
        blocks_dir: P,
        mod_id: &ModId,
        block_registry: &mut BlockRegistry,
        material_registry: &MaterialRegistry,
        source: ResourceSource,
        enforce_mod_restrictions: bool,
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

            // 1. Proteksi Reserved Namespace: Mod dilarang mendaftarkan namespace core:*
            if enforce_mod_restrictions && def.id.namespace.as_str() == ModId::CORE {
                return Err(ContentError::ReservedNamespaceViolation {
                    id: def.id,
                    mod_id: mod_id.clone(),
                });
            }

            // 2. Validasi namespace kepemilikan
            if def.id.namespace != *mod_id {
                return Err(ContentError::NamespaceMismatch {
                    id: def.id,
                    expected: mod_id.clone(),
                });
            }

            // 3. Validasi referensi material harus sudah terdaftar
            if material_registry
                .get_by_resource_id(&def.material)
                .is_none()
            {
                return Err(ContentError::UnresolvedMaterial {
                    block_id: def.id,
                    material_id: def.material,
                });
            }

            block_registry.register(def, source.clone())?;

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
            overrides_applied: 0,
        })
    }

    /// Menerapkan seluruh deklarasi override yang tertera pada manifest mod
    pub fn apply_mod_overrides(
        discovered: &DiscoveredMod,
        material_registry: &mut MaterialRegistry,
        block_registry: &mut BlockRegistry,
    ) -> Result<usize, ContentError> {
        let mut applied_count = 0;

        for ov in &discovered.manifest.overrides {
            let mut resolved = false;

            // Coba terapkan ke MaterialRegistry jika target adalah material
            if material_registry.get_by_resource_id(&ov.target).is_some() {
                if material_registry
                    .get_by_resource_id(&ov.replacement)
                    .is_none()
                {
                    return Err(ContentError::UnresolvedOverrideReplacement(
                        ov.replacement.clone(),
                    ));
                }
                material_registry.apply_explicit_override(
                    &ov.target,
                    &ov.replacement,
                    discovered.manifest.id.clone(),
                )?;
                resolved = true;
            }

            // Coba terapkan ke BlockRegistry jika target adalah block
            if block_registry.get_by_resource_id(&ov.target).is_some() {
                if block_registry.get_by_resource_id(&ov.replacement).is_none() {
                    return Err(ContentError::UnresolvedOverrideReplacement(
                        ov.replacement.clone(),
                    ));
                }
                block_registry.apply_explicit_override(
                    &ov.target,
                    &ov.replacement,
                    discovered.manifest.id.clone(),
                )?;
                resolved = true;
            }

            if !resolved {
                return Err(ContentError::UnresolvedOverrideTarget(ov.target.clone()));
            }

            applied_count += 1;
        }

        Ok(applied_count)
    }
}
