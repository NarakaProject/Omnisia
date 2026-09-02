use std::collections::BTreeMap;
use std::path::Path;

use crate::modding::loader::ModContentSummary;
use crate::modding::resource_id::ModId;
use crate::modding::runtime::ContentRuntime;

/// Laporan hasil validasi seluruh konten Core dan Mod
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub total_discovered: usize,
    pub core_materials_loaded: usize,
    pub core_blocks_loaded: usize,
    pub core_error: Option<String>,
    pub loaded_mods: BTreeMap<ModId, ModContentSummary>,
    pub failed_mods: BTreeMap<ModId, String>,
    pub applied_overrides: Vec<String>,
    pub warnings: Vec<String>,
    pub total_materials: usize,
    pub total_blocks: usize,
}

impl ValidationReport {
    /// Mencetak laporan ke stdout dalam format bersih
    pub fn print_summary(&self) {
        println!("============================================================");
        println!("           OMNISIA CONTENT & MOD VALIDATION REPORT          ");
        println!("============================================================");

        // 1. Status Core Content
        if let Some(ref err) = self.core_error {
            println!("[ERROR] core (Core Content Failed): {}", err);
        } else {
            println!(
                "[OK] core (Built-in Core: {} materials, {} blocks)",
                self.core_materials_loaded, self.core_blocks_loaded
            );
        }

        println!();
        println!("Mod Eksternal Ditemukan: {}", self.total_discovered);

        // 2. Mod yang Berhasil Dimuat
        for (mod_id, summary) in &self.loaded_mods {
            println!(
                "[OK] {} (Materials: {}, Blocks: {}, Overrides: {})",
                mod_id, summary.materials_loaded, summary.blocks_loaded, summary.overrides_applied
            );
        }

        // 3. Explicit Overrides yang Berhasil Diterapkan
        if !self.applied_overrides.is_empty() {
            println!();
            println!("Explicit Overrides Diterapkan:");
            for ov in &self.applied_overrides {
                println!("  [OVERRIDE] {}", ov);
            }
        }

        // 4. Mod / Konten yang Gagal
        if !self.failed_mods.is_empty() {
            println!();
            println!("Errors:");
            for (mod_id, reason) in &self.failed_mods {
                println!("  [ERROR] {}: {}", mod_id, reason);
            }
        }

        // 5. Warnings
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
        self.core_error.is_none() && self.failed_mods.is_empty()
    }
}

/// Menjalankan validasi penuh terhadap Core Content dan direktori mods
pub fn validate_mods_directory<P1: AsRef<Path>, P2: AsRef<Path>>(
    core_dir: P1,
    mods_dir: P2,
) -> ValidationReport {
    match ContentRuntime::build_runtime(core_dir, mods_dir) {
        Ok(resolved) => resolved.report,
        Err(_) => ValidationReport {
            core_error: Some("Gagal memuat Core Content atau Mod".to_string()),
            ..Default::default()
        },
    }
}
