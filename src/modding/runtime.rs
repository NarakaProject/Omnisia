use std::collections::BTreeMap;
use std::path::Path;

use crate::material::MaterialRegistry;
use crate::modding::asset::AssetResolver;
use crate::modding::dependency::DependencyResolver;
use crate::modding::discovery::ModDiscovery;
use crate::modding::loader::{ContentError, ModLoader};
use crate::modding::registry::BlockRegistry;
use crate::modding::resource_id::ModId;
use crate::modding::validation::ValidationReport;

/// Struktur hasil akhir dari resolusi seluruh konten (Core + Mods + Explicit Overrides)
pub struct ResolvedContent {
    pub materials: MaterialRegistry,
    pub blocks: BlockRegistry,
    pub assets: AssetResolver,
    pub report: ValidationReport,
}

/// ContentRuntime bertindak sebagai orkestrator pipeline pemuatan data, resolusi dependensi, dan penanganan override
pub struct ContentRuntime;

impl ContentRuntime {
    /// Memuat konten Core dan Mod secara deterministik dan menghasilkan `ResolvedContent` yang terverifikasi
    pub fn build_runtime<P1: AsRef<Path>, P2: AsRef<Path>>(
        core_dir: P1,
        mods_dir: P2,
    ) -> Result<ResolvedContent, ContentError> {
        let mut materials = MaterialRegistry::new();
        let mut blocks = BlockRegistry::new();
        let mut assets = AssetResolver::new();
        let mut report = ValidationReport::default();

        let core_path = core_dir.as_ref();
        let mods_path = mods_dir.as_ref();

        // 1. Registrasi Asset Root untuk Core
        assets.register_root(ModId::core(), core_path);

        // 2. Muat Core Content (Wajib ada!)
        match ModLoader::load_core_content(core_path, &mut materials, &mut blocks) {
            Ok(summary) => {
                report.core_materials_loaded = summary.materials_loaded;
                report.core_blocks_loaded = summary.blocks_loaded;
            }
            Err(e) => {
                report.core_error = Some(e.to_string());
                return Err(e);
            }
        }

        // 3. Discovery Mods
        let (discovered, discovery_errors) = ModDiscovery::discover_from_dir(mods_path);
        report.total_discovered = discovered.len();

        for (path, err) in discovery_errors {
            let fake_id = ModId::new("unreadable_mod").unwrap_or_else(|_| ModId::core());
            report
                .failed_mods
                .insert(fake_id, format!("File {:?}: {}", path, err));
        }

        // 4. Resolusi Dependensi & Siklus
        let manifests: Vec<_> = discovered.iter().map(|d| d.manifest.clone()).collect();
        let resolution = DependencyResolver::resolve(&manifests);

        for (failed_id, err) in resolution.failed_mods {
            report.failed_mods.insert(failed_id, err.to_string());
        }

        let discovered_map: BTreeMap<ModId, &crate::modding::discovery::DiscoveredMod> = discovered
            .iter()
            .map(|d| (d.manifest.id.clone(), d))
            .collect();

        // 5. Muat Konten Mod berdasarkan Deterministic Load Order
        let mut successfully_loaded_mods = Vec::new();
        for mod_id in &resolution.load_order {
            if let Some(&disc) = discovered_map.get(mod_id) {
                // Daftarkan root asset untuk mod ini
                assets.register_root(mod_id.clone(), &disc.root_dir);

                match ModLoader::load_mod(disc, &mut materials, &mut blocks) {
                    Ok(summary) => {
                        successfully_loaded_mods.push(disc);
                        report.loaded_mods.insert(mod_id.clone(), summary);
                    }
                    Err(e) => {
                        report.failed_mods.insert(mod_id.clone(), e.to_string());
                    }
                }
            }
        }

        // 6. Terapkan Explicit Overrides secara deterministik setelah seluruh definisi resource selesai dimuat
        for disc in successfully_loaded_mods {
            let mod_id = &disc.manifest.id;
            match ModLoader::apply_mod_overrides(disc, &mut materials, &mut blocks) {
                Ok(applied_count) => {
                    if let Some(summary) = report.loaded_mods.get_mut(mod_id) {
                        summary.overrides_applied = applied_count;
                    }
                    for ov in &disc.manifest.overrides {
                        report.applied_overrides.push(format!(
                            "Mod '{}' meng-override '{}' -> '{}'",
                            mod_id, ov.target, ov.replacement
                        ));
                    }
                }
                Err(e) => {
                    report.failed_mods.insert(mod_id.clone(), e.to_string());
                }
            }
        }

        report.total_materials = materials.len();
        report.total_blocks = blocks.len();

        Ok(ResolvedContent {
            materials,
            blocks,
            assets,
            report,
        })
    }
}
