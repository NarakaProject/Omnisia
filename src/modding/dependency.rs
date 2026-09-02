use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fmt;

use crate::modding::manifest::ModManifest;
use crate::modding::resource_id::ModId;

/// Error terkait resolusi dependensi mod
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyError {
    DuplicateModId(ModId),
    MissingDependency {
        mod_id: ModId,
        dependency: ModId,
    },
    VersionMismatch {
        mod_id: ModId,
        dependency: ModId,
        required: String,
        installed: String,
    },
    CircularDependency {
        cycle: Vec<ModId>,
    },
    CascadeFailure {
        mod_id: ModId,
        failed_dependency: ModId,
    },
}

impl fmt::Display for DependencyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateModId(id) => write!(f, "Duplikat Mod ID terdeteksi: '{}'", id),
            Self::MissingDependency { mod_id, dependency } => write!(
                f,
                "Mod '{}' membutuhkan dependensi '{}' yang tidak ditemukan",
                mod_id, dependency
            ),
            Self::VersionMismatch {
                mod_id,
                dependency,
                required,
                installed,
            } => write!(
                f,
                "Mod '{}' membutuhkan dependensi '{}' versi '{}', namun versi terpasang '{}'",
                mod_id, dependency, required, installed
            ),
            Self::CircularDependency { cycle } => {
                let cycle_str = cycle
                    .iter()
                    .map(|id| id.as_str())
                    .collect::<Vec<_>>()
                    .join(" -> ");
                write!(
                    f,
                    "Siklus dependensi terdeteksi (Circular Dependency): {}",
                    cycle_str
                )
            }
            Self::CascadeFailure {
                mod_id,
                failed_dependency,
            } => write!(
                f,
                "Mod '{}' gagal dimuat karena dependensi '{}' mengalami kegagalan",
                mod_id, failed_dependency
            ),
        }
    }
}

impl std::error::Error for DependencyError {}

/// Hasil resolusi dependensi dengan isolasi error
#[derive(Debug, Clone)]
pub struct DependencyResolutionResult {
    /// Urutan pemuatan deterministik untuk mod yang berhasil lolos validasi dependensi
    pub load_order: Vec<ModId>,
    /// Daftar mod yang gagal beserta alasan error-nya
    pub failed_mods: BTreeMap<ModId, DependencyError>,
}

/// Sistem resolusi grafik dependensi
pub struct DependencyResolver;

impl DependencyResolver {
    /// Menyelesaikan urutan pemuatan (load order) secara deterministik dan mengisolasi mod yang rusak
    pub fn resolve(manifests: &[ModManifest]) -> DependencyResolutionResult {
        let mut manifest_map: BTreeMap<ModId, &ModManifest> = BTreeMap::new();
        let mut failed_mods: BTreeMap<ModId, DependencyError> = BTreeMap::new();

        // 1. Deteksi Duplikat Mod ID
        for manifest in manifests {
            if manifest_map.contains_key(&manifest.id) {
                failed_mods.insert(
                    manifest.id.clone(),
                    DependencyError::DuplicateModId(manifest.id.clone()),
                );
            } else {
                manifest_map.insert(manifest.id.clone(), manifest);
            }
        }

        // Hapus mod yang duplikat dari manifest_map
        for failed_id in failed_mods.keys() {
            manifest_map.remove(failed_id);
        }

        // 2. Validasi Ketersediaan & Versi Dependensi
        let mut valid_manifests: BTreeMap<ModId, &ModManifest> = BTreeMap::new();
        for (mod_id, &manifest) in &manifest_map {
            let mut mod_valid = true;

            if let Ok(deps) = manifest.parsed_dependencies() {
                for req in deps {
                    // built-in "core" selalu tersedia implisit
                    if req.mod_id.as_str() == ModId::CORE {
                        continue;
                    }

                    if let Some(target_manifest) = manifest_map.get(&req.mod_id) {
                        match req.matches(&target_manifest.version) {
                            Ok(true) => {}
                            Ok(false) => {
                                failed_mods.insert(
                                    mod_id.clone(),
                                    DependencyError::VersionMismatch {
                                        mod_id: mod_id.clone(),
                                        dependency: req.mod_id.clone(),
                                        required: req.version_req.clone(),
                                        installed: target_manifest.version.clone(),
                                    },
                                );
                                mod_valid = false;
                                break;
                            }
                            Err(_) => {
                                failed_mods.insert(
                                    mod_id.clone(),
                                    DependencyError::VersionMismatch {
                                        mod_id: mod_id.clone(),
                                        dependency: req.mod_id.clone(),
                                        required: req.version_req.clone(),
                                        installed: target_manifest.version.clone(),
                                    },
                                );
                                mod_valid = false;
                                break;
                            }
                        }
                    } else {
                        failed_mods.insert(
                            mod_id.clone(),
                            DependencyError::MissingDependency {
                                mod_id: mod_id.clone(),
                                dependency: req.mod_id.clone(),
                            },
                        );
                        mod_valid = false;
                        break;
                    }
                }
            } else {
                mod_valid = false;
            }

            if mod_valid {
                valid_manifests.insert(mod_id.clone(), manifest);
            }
        }

        // 3. Bangun Dependency Graph (Kahn's Topological Sort)
        let mut in_degree: BTreeMap<ModId, usize> = BTreeMap::new();
        let mut dependents: BTreeMap<ModId, Vec<ModId>> = BTreeMap::new();

        for mod_id in valid_manifests.keys() {
            in_degree.insert(mod_id.clone(), 0);
            dependents.insert(mod_id.clone(), Vec::new());
        }

        for (mod_id, manifest) in &valid_manifests {
            if let Ok(deps) = manifest.parsed_dependencies() {
                for req in deps {
                    if req.mod_id.as_str() == ModId::CORE {
                        continue;
                    }

                    if valid_manifests.contains_key(&req.mod_id) {
                        *in_degree.entry(mod_id.clone()).or_insert(0) += 1;
                        dependents
                            .entry(req.mod_id.clone())
                            .or_default()
                            .push(mod_id.clone());
                    }
                }
            }
        }

        // 4. Inisialisasi Queue dengan Mod yang Memiliki in_degree == 0
        let mut ready_queue: VecDeque<ModId> = VecDeque::new();
        let mut sorted_candidates: Vec<ModId> = in_degree
            .iter()
            .filter(|&(_, &deg)| deg == 0)
            .map(|(id, _)| id.clone())
            .collect();
        sorted_candidates.sort(); // Deterministik alfabetis

        for id in sorted_candidates {
            ready_queue.push_back(id);
        }

        let mut load_order = Vec::new();

        while let Some(current) = ready_queue.pop_front() {
            load_order.push(current.clone());

            if let Some(next_mods) = dependents.get(&current) {
                let mut newly_ready = Vec::new();
                for next_id in next_mods {
                    if let Some(deg) = in_degree.get_mut(next_id) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            newly_ready.push(next_id.clone());
                        }
                    }
                }
                newly_ready.sort(); // Deterministik
                for id in newly_ready {
                    ready_queue.push_back(id);
                }
            }
        }

        // 5. Deteksi Circular Dependency untuk mod yang tersisa (in_degree > 0)
        let unvisited: HashSet<ModId> = in_degree
            .iter()
            .filter(|&(_, &deg)| deg > 0)
            .map(|(id, _)| id.clone())
            .collect();

        if !unvisited.is_empty() {
            let mut cycle_list: Vec<ModId> = unvisited.into_iter().collect();
            cycle_list.sort();
            for id in &cycle_list {
                failed_mods.insert(
                    id.clone(),
                    DependencyError::CircularDependency {
                        cycle: cycle_list.clone(),
                    },
                );
            }
        }

        DependencyResolutionResult {
            load_order,
            failed_mods,
        }
    }
}
