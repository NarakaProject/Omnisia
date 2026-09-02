use omnisia::modding::definitions::{BlockComponents, BlockDefinition, LiftCapacityComponent};
use omnisia::modding::dependency::{DependencyError, DependencyResolver};
use omnisia::modding::manifest::{ManifestError, ModManifest};
use omnisia::modding::registry::{BlockId, BlockRegistry, ResourceRegistry};
use omnisia::modding::resource_id::{ModId, ResourceId, ResourceIdError};
use omnisia::modding::validation::validate_mods_directory;
use std::collections::HashMap;

// ============================================================================
// 1. MOD MANIFEST TESTS
// ============================================================================

#[test]
fn test_valid_manifest_parsing() {
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
    "#;

    let manifest =
        ModManifest::from_toml_str(toml_str).expect("Valid manifest harus berhasil diparsing");
    assert_eq!(manifest.id.as_str(), "test_mod");
    assert_eq!(manifest.name, "Test Mod");
    assert_eq!(manifest.version, "1.2.3");
    assert_eq!(manifest.engine_api, "0.2");
    assert_eq!(manifest.dependencies.get("core"), Some(&"0.2".to_string()));
}

#[test]
fn test_manifest_missing_required_fields() {
    // Missing id
    let toml_missing_id = r#"
        name = "No ID Mod"
        version = "1.0.0"
        engine_api = "0.2"
    "#;
    assert!(ModManifest::from_toml_str(toml_missing_id).is_err());

    // Missing version
    let toml_missing_ver = r#"
        id = "no_ver"
        name = "No Version"
        engine_api = "0.2"
    "#;
    assert!(ModManifest::from_toml_str(toml_missing_ver).is_err());

    // Missing engine_api
    let toml_missing_api = r#"
        id = "no_api"
        name = "No API"
        version = "1.0.0"
    "#;
    assert!(ModManifest::from_toml_str(toml_missing_api).is_err());
}

#[test]
fn test_manifest_invalid_version_or_engine_api() {
    // Invalid semver
    let toml_invalid_ver = r#"
        id = "bad_ver"
        name = "Bad Ver"
        version = "not-a-semver"
        engine_api = "0.2"
    "#;
    assert!(matches!(
        ModManifest::from_toml_str(toml_invalid_ver),
        Err(ManifestError::InvalidVersion(_))
    ));

    // Incompatible Engine API
    let toml_incompatible_api = r#"
        id = "future_mod"
        name = "Future Mod"
        version = "1.0.0"
        engine_api = "99.0"
    "#;
    assert!(matches!(
        ModManifest::from_toml_str(toml_incompatible_api),
        Err(ManifestError::IncompatibleApi { .. })
    ));
}

// ============================================================================
// 2. NAMESPACE & RESOURCE ID TESTS
// ============================================================================

#[test]
fn test_resource_id_format_validity() {
    assert!(ResourceId::parse("core:stone").is_ok());
    assert!(ResourceId::parse("my_mod:steel_plate").is_ok());
    assert!(ResourceId::parse("tech_v2:sub_dir/item").is_ok());

    // Invalid namespace (uppercase / symbol)
    assert!(matches!(
        ResourceId::parse("MyMod:stone"),
        Err(ResourceIdError::InvalidNamespace(_))
    ));
    assert!(matches!(
        ResourceId::parse("mod!name:stone"),
        Err(ResourceIdError::InvalidNamespace(_))
    ));

    // Invalid format (no colon or multiple colons)
    assert!(matches!(
        ResourceId::parse("only_name"),
        Err(ResourceIdError::MissingDelimiter)
    ));
    assert!(matches!(
        ResourceId::parse("a:b:c"),
        Err(ResourceIdError::TooManyDelimiters)
    ));
}

#[test]
fn test_resource_id_canonical_serialization() {
    let res_id = ResourceId::new("tech_mod", "reinforced_alloy").unwrap();
    let serialized = serde_json::to_string(&res_id).unwrap();
    assert_eq!(serialized, "\"tech_mod:reinforced_alloy\"");

    let deserialized: ResourceId = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, res_id);
}

// ============================================================================
// 3. DEPENDENCY RESOLUTION & DETERMINISM TESTS
// ============================================================================

#[test]
fn test_deterministic_dependency_topological_sort() {
    // base_mod -> machines_mod -> advanced_reactor_mod
    let manifest_base = ModManifest {
        id: ModId::new("base_mod").unwrap(),
        name: "Base Mod".to_string(),
        version: "1.0.0".to_string(),
        engine_api: "0.2".to_string(),
        description: None,
        author: None,
        dependencies: HashMap::new(),
    };

    let manifest_machines = ModManifest {
        id: ModId::new("machines_mod").unwrap(),
        name: "Machines".to_string(),
        version: "1.0.0".to_string(),
        engine_api: "0.2".to_string(),
        description: None,
        author: None,
        dependencies: [("base_mod".to_string(), "^1.0".to_string())]
            .into_iter()
            .collect(),
    };

    let manifest_reactor = ModManifest {
        id: ModId::new("advanced_reactor_mod").unwrap(),
        name: "Advanced Reactor".to_string(),
        version: "1.0.0".to_string(),
        engine_api: "0.2".to_string(),
        description: None,
        author: None,
        dependencies: [("machines_mod".to_string(), "^1.0".to_string())]
            .into_iter()
            .collect(),
    };

    // Urutan input sengaja diacak
    let manifests = vec![manifest_reactor, manifest_machines, manifest_base];
    let result = DependencyResolver::resolve(&manifests);

    assert!(result.failed_mods.is_empty());
    assert_eq!(
        result.load_order,
        vec![
            ModId::new("base_mod").unwrap(),
            ModId::new("machines_mod").unwrap(),
            ModId::new("advanced_reactor_mod").unwrap(),
        ]
    );
}

#[test]
fn test_dependency_cycle_detection_and_isolation() {
    // Mod A butuh B, Mod B butuh A (Cycle)
    // Mod C independen (harus tetap berhasil dimuat!)
    let mod_a = ModManifest {
        id: ModId::new("mod_a").unwrap(),
        name: "Mod A".to_string(),
        version: "1.0.0".to_string(),
        engine_api: "0.2".to_string(),
        description: None,
        author: None,
        dependencies: [("mod_b".to_string(), "*".to_string())]
            .into_iter()
            .collect(),
    };

    let mod_b = ModManifest {
        id: ModId::new("mod_b").unwrap(),
        name: "Mod B".to_string(),
        version: "1.0.0".to_string(),
        engine_api: "0.2".to_string(),
        description: None,
        author: None,
        dependencies: [("mod_a".to_string(), "*".to_string())]
            .into_iter()
            .collect(),
    };

    let mod_c = ModManifest {
        id: ModId::new("mod_c").unwrap(),
        name: "Mod C Independent".to_string(),
        version: "1.0.0".to_string(),
        engine_api: "0.2".to_string(),
        description: None,
        author: None,
        dependencies: HashMap::new(),
    };

    let result = DependencyResolver::resolve(&[mod_a, mod_b, mod_c]);

    // Mod C harus tetap berhasil dimuat
    assert_eq!(result.load_order, vec![ModId::new("mod_c").unwrap()]);

    // Mod A dan Mod B harus diisolasi dan dilaporkan sebagai error
    assert!(result
        .failed_mods
        .contains_key(&ModId::new("mod_a").unwrap()));
    assert!(result
        .failed_mods
        .contains_key(&ModId::new("mod_b").unwrap()));
}

#[test]
fn test_missing_dependency_isolation() {
    let mod_good = ModManifest {
        id: ModId::new("mod_good").unwrap(),
        name: "Good Mod".to_string(),
        version: "1.0.0".to_string(),
        engine_api: "0.2".to_string(),
        description: None,
        author: None,
        dependencies: HashMap::new(),
    };

    let mod_broken = ModManifest {
        id: ModId::new("mod_broken").unwrap(),
        name: "Broken Mod".to_string(),
        version: "1.0.0".to_string(),
        engine_api: "0.2".to_string(),
        description: None,
        author: None,
        dependencies: [("non_existent_mod".to_string(), "1.0".to_string())]
            .into_iter()
            .collect(),
    };

    let result = DependencyResolver::resolve(&[mod_good, mod_broken]);
    assert_eq!(result.load_order, vec![ModId::new("mod_good").unwrap()]);
    assert!(matches!(
        result.failed_mods.get(&ModId::new("mod_broken").unwrap()),
        Some(DependencyError::MissingDependency { .. })
    ));
}

// ============================================================================
// 4. RESOURCE REGISTRY & BLOCK REGISTRY TESTS
// ============================================================================

#[test]
fn test_generic_resource_registry_bidirectional_mapping() {
    let mut reg = ResourceRegistry::<String>::new();
    let res_a = ResourceId::parse("core:stone").unwrap();
    let res_b = ResourceId::parse("custom:alloy").unwrap();

    let id_a = reg
        .register(res_a.clone(), "Stone Object".to_string())
        .unwrap();
    let id_b = reg
        .register(res_b.clone(), "Alloy Object".to_string())
        .unwrap();

    assert_eq!(id_a, 0);
    assert_eq!(id_b, 1);

    // Fast O(1) Index Lookup
    assert_eq!(reg.get_by_index(0), Some(&"Stone Object".to_string()));
    assert_eq!(reg.get_by_index(1), Some(&"Alloy Object".to_string()));

    // ResourceId Lookup
    assert_eq!(reg.get(&res_a), Some(&"Stone Object".to_string()));
    assert_eq!(reg.get(&res_b), Some(&"Alloy Object".to_string()));

    // Resolution
    assert_eq!(reg.resolve_runtime_id(&res_a), Some(0));
    assert_eq!(reg.resolve_runtime_id(&res_b), Some(1));
    assert_eq!(reg.get_resource_id_by_index(0), Some(&res_a));
    assert_eq!(reg.get_resource_id_by_index(1), Some(&res_b));
}

#[test]
fn test_block_registry_with_generic_components() {
    let mut block_reg = BlockRegistry::new();
    let block_id = ResourceId::parse("energy_mod:grav_core").unwrap();
    let mat_id = ResourceId::parse("energy_mod:dark_metal").unwrap();

    let def = BlockDefinition {
        id: block_id.clone(),
        material: mat_id,
        hardness: Some(100.0),
        components: BlockComponents {
            structural_anchor: None,
            lift_capacity: Some(LiftCapacityComponent {
                capacity_kg: 5_000_000.0,
                radius_m: 64.0,
                power_consumption_w: 100_000.0,
            }),
            extra: HashMap::new(),
        },
        tags: vec!["anti_gravity".to_string(), "core".to_string()],
    };

    let runtime_id = block_reg.register(def).unwrap();
    assert_eq!(runtime_id, BlockId(0));

    let fetched = block_reg.get(runtime_id).unwrap();
    assert_eq!(fetched.id, block_id);
    assert!(fetched.components.lift_capacity.is_some());
    assert_eq!(
        fetched
            .components
            .lift_capacity
            .as_ref()
            .unwrap()
            .capacity_kg,
        5_000_000.0
    );
}

// ============================================================================
// 5. EXAMPLE MOD VERIFICATION TEST
// ============================================================================

#[test]
fn test_example_mod_discovery_and_loading() {
    let report = validate_mods_directory("mods");
    assert!(
        report.total_discovered >= 1,
        "Example mod harus ditemukan di folder mods/"
    );

    let example_mod_id = ModId::new("example_mod").unwrap();
    assert!(
        report.loaded_mods.contains_key(&example_mod_id),
        "example_mod harus berhasil dimuat tanpa error"
    );

    let summary = report.loaded_mods.get(&example_mod_id).unwrap();
    assert!(
        summary.materials_loaded >= 2,
        "Harus memuat minimal 2 material (steel, reinforced_concrete)"
    );
    assert!(
        summary.blocks_loaded >= 2,
        "Harus memuat minimal 2 blok (steel_block, reactor_core)"
    );
    assert!(report.is_all_ok());
}
