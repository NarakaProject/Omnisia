use std::collections::BTreeMap;
use std::path::Path;

use crate::material::MaterialRegistry;
use crate::modding::dependency::DependencyResolver;
use crate::modding::discovery::ModDiscovery;
use crate::modding::loader::{ModContentSummary, ModLoader};
use crate::modding::registry::BlockRegistry;
use crate::modding::resource_id::ModId;

/// Laporan hasil validasi seluruh mod
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub total_discovered: usize,
    pub loaded_mods: BTreeMap<ModId, ModContentSummary>,
    pub failed_mods: BTreeMap<ModId, String>,
    pub warnings: Vec<String>,
    pub total_materials: usize,
    pub total_blocks: usize,
}

impl ValidationReport {
    /// Mencetak laporan ke stdout dalam format bersih
    pub fn print_summary(&self) {
        println!("============================================================");
        println!("           OMNISIA MODDING VALIDATION REPORT                ");
        println!("============================================================");
        println!("Mod Ditemukan: {}", self.total_discovered);
        println!();

        // 1. Built-in Core Status
        println!("[OK] core (Built-in Core Content)");

        // 2. Mod yang Berhasil Dimuat
        for (mod_id, summary) in &self.loaded_mods {
            println!(
                "[OK] {} (Materials: {}, Blocks: {})",
                mod_id, summary.materials_loaded, summary.blocks_loaded
            );
        }

        // 3. Mod yang Gagal
        if !self.failed_mods.is_empty() {
            println!();
            println!("Errors:");
            for (mod_id, reason) in &self.failed_mods {
                println!("  [ERROR] {}: {}", mod_id, reason);
            }
        }

        // 4. Warnings
        if !self.warnings.is_empty() {
            println!();
            println!("Warnings:");
            for warn in &self.warnings {
                println!("  [WARN] {}", warn);
            }
        }

        println!();
        println!("Total Registry Terdaftar:");
        println!("  Materials: {}", self.total_materials);
        println!("  Blocks:    {}", self.total_blocks);
        println!("============================================================");
    }

    pub fn is_all_ok(&self) -> bool {
        self.failed_mods.is_empty()
    }
}

/// Menjalankan validasi penuh terhadap folder mods
pub fn validate_mods_directory<P: AsRef<Path>>(mods_dir: P) -> ValidationReport {
    let mut report = ValidationReport::default();
    let mut material_reg = MaterialRegistry::with_builtin_materials();
    let mut block_reg = BlockRegistry::new();

    // 1. Discovery
    let (discovered, discovery_errors) = ModDiscovery::discover_from_dir(mods_dir);
    report.total_discovered = discovered.len();

    for (path, err) in discovery_errors {
        let fake_id = ModId::new("unreadable_mod").unwrap_or_else(|_| ModId::core());
        report
            .failed_mods
            .insert(fake_id, format!("File {:?}: {}", path, err));
    }

    // 2. Dependency Resolution
    let manifests: Vec<_> = discovered.iter().map(|d| d.manifest.clone()).collect();
    let res = DependencyResolver::resolve(&manifests);

    for (failed_id, err) in res.failed_mods {
        report.failed_mods.insert(failed_id, err.to_string());
    }

    // 3. Content Loading berdasarkan Deterministic Load Order
    let discovered_map: BTreeMap<ModId, &crate::modding::discovery::DiscoveredMod> = discovered
        .iter()
        .map(|d| (d.manifest.id.clone(), d))
        .collect();

    for mod_id in res.load_order {
        if let Some(&disc) = discovered_map.get(&mod_id) {
            match ModLoader::load_mod(disc, &mut material_reg, &mut block_reg) {
                Ok(summary) => {
                    report.loaded_mods.insert(mod_id, summary);
                }
                Err(e) => {
                    report.failed_mods.insert(mod_id, e.to_string());
                }
            }
        }
    }

    report.total_materials = material_reg.len();
    report.total_blocks = block_reg.len();

    report
}
