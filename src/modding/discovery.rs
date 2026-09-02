use std::fs;
use std::path::{Path, PathBuf};

use crate::modding::manifest::{ManifestError, ModManifest};

/// Representasi mod yang ditemukan pada filesystem
#[derive(Debug, Clone)]
pub struct DiscoveredMod {
    pub root_dir: PathBuf,
    pub manifest: ModManifest,
}

/// Sistem pencarian mod di filesystem secara deterministik
pub struct ModDiscovery;

impl ModDiscovery {
    /// Mencari seluruh mod dalam direktori yang ditentukan (misal: "mods/").
    ///
    /// Urutan pencarian dijamin DETERMINISTIK dengan menyortir nama folder secara alfabetis,
    /// terlepas dari urutan enumerasi filesystem sistem operasi.
    pub fn discover_from_dir<P: AsRef<Path>>(
        mods_dir: P,
    ) -> (Vec<DiscoveredMod>, Vec<(PathBuf, ManifestError)>) {
        let dir_path = mods_dir.as_ref();
        let mut discovered = Vec::new();
        let mut errors = Vec::new();

        if !dir_path.exists() || !dir_path.is_dir() {
            return (discovered, errors);
        }

        let read_dir = match fs::read_dir(dir_path) {
            Ok(entries) => entries,
            Err(e) => {
                errors.push((
                    dir_path.to_path_buf(),
                    ManifestError::TomlParseError(format!("Gagal membaca direktori mods: {}", e)),
                ));
                return (discovered, errors);
            }
        };

        // 1. Kumpulkan semua entri folder dan urutkan secara alfabetis (DETERMINISTIK)
        let mut subdirs: Vec<PathBuf> = Vec::new();
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir() {
                subdirs.push(path);
            }
        }
        subdirs.sort();

        // 2. Baca `mod.toml` di setiap folder
        for mod_folder in subdirs {
            let manifest_path = mod_folder.join("mod.toml");
            if !manifest_path.exists() {
                // Abaikan folder tanpa mod.toml atau catat error
                continue;
            }

            match fs::read_to_string(&manifest_path) {
                Ok(content) => match ModManifest::from_toml_str(&content) {
                    Ok(manifest) => {
                        discovered.push(DiscoveredMod {
                            root_dir: mod_folder,
                            manifest,
                        });
                    }
                    Err(e) => {
                        errors.push((manifest_path, e));
                    }
                },
                Err(e) => {
                    errors.push((
                        manifest_path,
                        ManifestError::TomlParseError(format!("Gagal membaca file: {}", e)),
                    ));
                }
            }
        }

        (discovered, errors)
    }
}
