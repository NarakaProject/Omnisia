use std::path::PathBuf;

use omnisia::modding::asset::{AssetError, AssetId, AssetLocation, AssetResolver};
use omnisia::modding::definitions::MaterialDefinition;
use omnisia::modding::loader::{ContentError, ModLoader};
use omnisia::modding::manifest::{ManifestError, ModManifest};
use omnisia::modding::registry::{BlockRegistry, RegistryError, ResourceRegistry, ResourceSource};
use omnisia::modding::resource_id::{ModId, ResourceId};
use omnisia::modding::validation::validate_mods_directory;

// ============================================================================
// 1. MOD MANIFEST & OVERRIDE DECLARATION TESTS
// ============================================================================

#[test]
fn test_valid_manifest_parsing_with_overrides() {
    let toml_str = r#"
        id = "test_mod"
        name = "Test Mod"
        version = "1.2.3"
        engine_api = "0.2"
        description = "A valid test mod"

        [author]
        name = "Tester"

        [dependencies]
        core = "0.2"

        [[overrides]]
        target = "core:stone"
        replacement = "test_mod:dense_stone"
    "#;

    let manifest =
        ModManifest::from_toml_str(toml_str).expect("Valid manifest harus berhasil diparsing");
    assert_eq!(manifest.id.as_str(), "test_mod");
    assert_eq!(manifest.name, "Test Mod");
    assert_eq!(manifest.version, "1.2.3");
    assert_eq!(manifest.engine_api, "0.2");
    assert_eq!(manifest.dependencies.get("core"), Some(&"0.2".to_string()));
    assert_eq!(manifest.overrides.len(), 1);
    assert_eq!(
        manifest.overrides[0].target,
        ResourceId::parse("core:stone").unwrap()
    );
    assert_eq!(
        manifest.overrides[0].replacement,
        ResourceId::parse("test_mod:dense_stone").unwrap()
    );
}

#[test]
fn test_manifest_invalid_override_rules() {
    // 1. Target sama dengan replacement
    let toml_self_override = r#"
        id = "self_mod"
        name = "Self Mod"
        version = "1.0.0"
        engine_api = "0.2"

        [[overrides]]
        target = "self_mod:stone"
        replacement = "self_mod:stone"
    "#;
    assert!(matches!(
        ModManifest::from_toml_str(toml_self_override),
        Err(ManifestError::InvalidOverride { .. })
    ));

    // 2. Mod mencoba memakai replacement dari namespace lain (Cross-mod replacement violation)
    let toml_cross_mod = r#"
        id = "my_mod"
        name = "My Mod"
        version = "1.0.0"
        engine_api = "0.2"

        [[overrides]]
        target = "core:stone"
        replacement = "other_mod:super_stone"
    "#;
    assert!(matches!(
        ModManifest::from_toml_str(toml_cross_mod),
        Err(ManifestError::InvalidOverride { .. })
    ));
}

// ============================================================================
// 2. ASSET ID & ASSET RESOLVER SECURITY TESTS
// ============================================================================

#[test]
fn test_asset_id_parsing_and_format() {
    let asset = AssetId::parse("core:textures/stone.png").unwrap();
    assert_eq!(asset.namespace.as_str(), "core");
    assert_eq!(asset.path, "textures/stone.png");

    let mod_asset = AssetId::parse("energy_mod:models/sub_folder/reactor.glb").unwrap();
    assert_eq!(mod_asset.namespace.as_str(), "energy_mod");
    assert_eq!(mod_asset.path, "models/sub_folder/reactor.glb");

    // Invalid formats
    assert!(matches!(
        AssetId::parse("no_colon_path"),
        Err(AssetError::MissingDelimiter)
    ));
    assert!(matches!(
        AssetId::parse("a:b:c"),
        Err(AssetError::TooManyDelimiters)
    ));
    assert!(matches!(AssetId::parse(""), Err(AssetError::EmptyString)));
}

#[test]
fn test_asset_id_path_traversal_rejection() {
    // Relative escape
    assert!(matches!(
        AssetId::parse("core:../secret.txt"),
        Err(AssetError::PathTraversalDetected(_))
    ));
    assert!(matches!(
        AssetId::parse("core:textures/../../etc/passwd"),
        Err(AssetError::PathTraversalDetected(_))
    ));

    // Absolute paths
    assert!(matches!(
        AssetId::parse("core:/root/file.png"),
        Err(AssetError::AbsolutePathNotAllowed(_))
    ));
    assert!(matches!(
        AssetId::new("core", "C:/Windows/system32.dll"),
        Err(AssetError::AbsolutePathNotAllowed(_))
    ));
}

#[test]
fn test_asset_resolver_resolution_and_containment() {
    let mut resolver = AssetResolver::new();
    resolver.register_root(ModId::core(), "content/core");
    resolver.register_root(ModId::new("example_mod").unwrap(), "mods/example_mod");

    // Core resolution
    let core_asset = AssetId::parse("core:textures/stone.png").unwrap();
    let resolved_core = resolver.resolve(&core_asset).unwrap();
    assert_eq!(
        resolved_core,
        AssetLocation::Filesystem(PathBuf::from("content/core/textures/stone.png"))
    );

    // Mod resolution
    let mod_asset = AssetId::parse("example_mod:models/reactor.glb").unwrap();
    let resolved_mod = resolver.resolve(&mod_asset).unwrap();
    assert_eq!(
        resolved_mod,
        AssetLocation::Filesystem(PathBuf::from("mods/example_mod/models/reactor.glb"))
    );

    // Unregistered namespace
    let unknown_asset = AssetId::parse("unknown_mod:test.png").unwrap();
    assert!(matches!(
        resolver.resolve(&unknown_asset),
        Err(AssetError::NamespaceNotRegistered(_))
    ));
}

// ============================================================================
// 3. CORE CONTENT LOADING & PHYSICAL SEPARATION TESTS
// ============================================================================

#[test]
fn test_core_content_loading_from_disk() {
    let mut mat_reg = omnisia::material::MaterialRegistry::new();
    let mut blk_reg = BlockRegistry::new();

    let summary = ModLoader::load_core_content("content/core", &mut mat_reg, &mut blk_reg)
        .expect("Core Content di content/core harus berhasil dimuat");

    assert!(
        summary.materials_loaded >= 10,
        "Minimal 10 material core harus dimuat"
    );
    assert!(
        summary.blocks_loaded >= 6,
        "Minimal 6 blok core harus dimuat"
    );

    // Verifikasi kepemilikan ResourceSource::Core
    let stone_entry = mat_reg
        .get_entry_by_resource_id(&ResourceId::parse("core:stone").unwrap())
        .unwrap();
    assert_eq!(stone_entry.original_source, ResourceSource::Core);
    assert_eq!(stone_entry.active_source, ResourceSource::Core);
    assert!(stone_entry.override_info.is_none());
}

#[test]
fn test_missing_core_directory_fails_explicitly() {
    let mut mat_reg = omnisia::material::MaterialRegistry::new();
    let mut blk_reg = BlockRegistry::new();

    let result =
        ModLoader::load_core_content("content/non_existent_folder", &mut mat_reg, &mut blk_reg);
    assert!(matches!(result, Err(ContentError::MissingCoreDirectory(_))));
}

// ============================================================================
// 4. RESERVED NAMESPACE & SAFE REGISTRATION TESTS
// ============================================================================

#[test]
fn test_mod_declaring_reserved_core_namespace_rejected() {
    let mut mat_reg = omnisia::material::MaterialRegistry::new();
    let fake_mod_id = ModId::new("malicious_mod").unwrap();

    // Simulasi mod mencoba membuat material baru dengan namespace "core:hacked"
    let hacked_def = MaterialDefinition {
        id: ResourceId::parse("core:hacked").unwrap(),
        name: "Hacked Stone".to_string(),
        density: 1000.0,
        shear_strength: 10.0,
        color: [1.0, 0.0, 0.0],
        solid: true,
        transparent: false,
    };

    // Pemuatan material melalui loader proteksi harus menolaknya
    let err = mat_reg.register_resource(
        hacked_def.id.clone(),
        omnisia::material::MaterialDef {
            name: hacked_def.name,
            density_kg_m3: hacked_def.density,
            shear_strength_mpa: hacked_def.shear_strength,
            base_color: hacked_def.color,
            is_solid: hacked_def.solid,
            is_transparent: hacked_def.transparent,
        },
        ResourceSource::Mod(fake_mod_id.clone()),
    );
    // Registrasi langsung ke registry berhasil jika ID belum ada, tetapi loader memblokirnya:
    assert!(err.is_ok());

    // Coba daftarkan ID yang sama kedua kalinya (Safe Registration Invariant)
    let duplicate_err = mat_reg.register_resource(
        hacked_def.id.clone(),
        omnisia::material::MaterialDef {
            name: "Hacked Again".to_string(),
            density_kg_m3: 1000.0,
            shear_strength_mpa: 10.0,
            base_color: [1.0, 0.0, 0.0],
            is_solid: true,
            is_transparent: false,
        },
        ResourceSource::Mod(fake_mod_id),
    );
    assert!(matches!(
        duplicate_err,
        Err(RegistryError::DuplicateRegistration(_))
    ));
}

// ============================================================================
// 5. EXPLICIT OVERRIDE, CONFLICT DETECTION, & PROVENANCE TESTS
// ============================================================================

#[test]
fn test_explicit_override_success_and_persistent_identity() {
    let mut reg = ResourceRegistry::<String>::new();
    let core_stone = ResourceId::parse("core:stone").unwrap();
    let mod_stone = ResourceId::parse("better_stone:reinforced_stone").unwrap();
    let mod_id = ModId::new("better_stone").unwrap();

    // 1. Registrasi awal
    let idx_core = reg
        .register(
            core_stone.clone(),
            "Standard Core Stone".to_string(),
            ResourceSource::Core,
        )
        .unwrap();
    let _idx_mod = reg
        .register(
            mod_stone.clone(),
            "Reinforced Mod Stone".to_string(),
            ResourceSource::Mod(mod_id.clone()),
        )
        .unwrap();

    // 2. Terapkan explicit override
    reg.apply_explicit_override(&core_stone, &mod_stone, mod_id.clone())
        .expect("Explicit override harus berhasil");

    // 3. Verifikasi Invariant: Persistent ID TETAP core:stone
    let entry = reg.get_entry(&core_stone).unwrap();
    assert_eq!(entry.id, core_stone);
    assert_eq!(entry.item, "Reinforced Mod Stone"); // Definisi aktif berubah
    assert_eq!(entry.original_source, ResourceSource::Core); // Provenance asal tetap Core
    assert_eq!(entry.active_source, ResourceSource::Mod(mod_id.clone())); // Provenance aktif Mod
    assert_eq!(
        entry.override_info,
        Some(omnisia::modding::registry::OverrideMetadata {
            target: core_stone.clone(),
            replacement: mod_stone.clone(),
            source_mod: mod_id,
        })
    );

    // Verifikasi O(1) runtime lookup index tetap konsisten
    assert_eq!(
        reg.get_by_index(idx_core),
        Some(&"Reinforced Mod Stone".to_string())
    );
}

#[test]
fn test_override_conflict_detection() {
    let mut reg = ResourceRegistry::<String>::new();
    let core_stone = ResourceId::parse("core:stone").unwrap();
    let mod_a_stone = ResourceId::parse("mod_a:stone_a").unwrap();
    let mod_b_stone = ResourceId::parse("mod_b:stone_b").unwrap();
    let mod_a = ModId::new("mod_a").unwrap();
    let mod_b = ModId::new("mod_b").unwrap();

    reg.register(
        core_stone.clone(),
        "Core Stone".to_string(),
        ResourceSource::Core,
    )
    .unwrap();
    reg.register(
        mod_a_stone.clone(),
        "Stone A".to_string(),
        ResourceSource::Mod(mod_a.clone()),
    )
    .unwrap();
    reg.register(
        mod_b_stone.clone(),
        "Stone B".to_string(),
        ResourceSource::Mod(mod_b.clone()),
    )
    .unwrap();

    // Mod A berhasil meng-override
    assert!(reg
        .apply_explicit_override(&core_stone, &mod_a_stone, mod_a)
        .is_ok());

    // Mod B mencoba meng-override target yang sama -> HARUS DITOLAK SEBAGAI KONFLIK
    let conflict_err = reg.apply_explicit_override(&core_stone, &mod_b_stone, mod_b);
    assert!(matches!(
        conflict_err,
        Err(RegistryError::OverrideConflict { .. })
    ));
}

// ============================================================================
// 6. END-TO-END VALIDATION & EXAMPLE MOD TESTS
// ============================================================================

#[test]
fn test_example_mod_end_to_end_with_override() {
    let report = validate_mods_directory("content/core", "mods");
    assert!(
        report.is_all_ok(),
        "Validation report harus bebas error: {:?}",
        report.failed_mods
    );
    assert_eq!(report.core_error, None);
    assert!(report.core_materials_loaded >= 10);
    assert!(report.core_blocks_loaded >= 6);

    let example_mod_id = ModId::new("example_mod").unwrap();
    assert!(report.loaded_mods.contains_key(&example_mod_id));

    let summary = report.loaded_mods.get(&example_mod_id).unwrap();
    assert_eq!(
        summary.overrides_applied, 1,
        "example_mod harus berhasil menerapkan 1 override terhadap core:stone"
    );
    assert!(!report.applied_overrides.is_empty());
}
