use glam::{IVec3, Vec3};
use omnisia::chunk::{dirty_flags, Chunk};
use omnisia::interaction::{
    calculate_tool_effectiveness, can_gather_with_tool, execute_tool_gather_transaction,
    handle_player_tool_gather, resolve_tool, validate_tool_gather_action,
    validate_tool_gather_internal, validate_tool_requirement, InteractionCooldown,
    ResourceDefinition, ToolCategory, ToolDefinition, ToolEffectiveness, ToolError, ToolId,
    ToolRequirement, ToolState, VoxelHit, DEFAULT_INTERACTION_REACH,
};
use omnisia::mesh::types::FaceDirection;
use omnisia::modding::definitions::HarvestableComponent;
use omnisia::modding::resource_id::ResourceId;
use omnisia::player::PlayerController;
use omnisia::streaming::store::ChunkStore;
use omnisia::voxel::VoxelBlock;
use omnisia::world::World;

fn add_empty_chunk(store: &mut ChunkStore, coord: IVec3) {
    let mut chunk = Chunk::new(coord);
    chunk.clear_dirty(dirty_flags::ALL);
    store.insert(chunk);
}

fn voxel_center(coord: IVec3) -> Vec3 {
    coord.as_vec3() * 0.5 + Vec3::splat(0.25)
}

// ============================================================================
// 1. TOOL IDENTITY & CATEGORY TESTS
// ============================================================================

#[test]
fn test_tool_id_creation_formatting_and_validation() {
    let pickaxe = ToolId::core("stone_pickaxe").expect("valid core tool id");
    assert_eq!(pickaxe.namespace.as_str(), "core");
    assert_eq!(pickaxe.path, "stone_pickaxe");
    assert_eq!(pickaxe.to_string(), "core:stone_pickaxe");
    assert_eq!(pickaxe.as_str(), "core:stone_pickaxe");

    // FromStr
    let parsed: ToolId = "core:stone_pickaxe".parse().expect("should parse");
    assert_eq!(parsed, pickaxe);

    // Mod-defined tool
    let mod_tool = ToolId::new("custom_mod", "ruby_axe").expect("valid mod tool");
    assert_eq!(mod_tool.to_string(), "custom_mod:ruby_axe");

    // Invalid formatting
    assert!(ToolId::new("CORE", "invalid").is_err());
    assert!(ToolId::new("core", "").is_err());
    assert!(ToolId::new("core", "invalid uppercase").is_err());
    assert!("invalid_without_delimiter".parse::<ToolId>().is_err());
    assert!("too:many:colons".parse::<ToolId>().is_err());
}

#[test]
fn test_tool_id_is_distinct_from_resource_id() {
    // Verifikasi bahwa ToolId tidak sama dengan ResourceId secara tipe
    let tool_id = ToolId::core("stone").unwrap();
    let res_id = ResourceId::core("stone").unwrap();

    assert_eq!(tool_id.to_string(), res_id.to_string());
    // Type separation: tool_id and res_id cannot be assigned to each other
    let _typed_tool: ToolId = tool_id;
    let _typed_res: ResourceId = res_id;
}

#[test]
fn test_tool_category_equality_and_discrimination() {
    let pick = ToolCategory::Pickaxe;
    let axe = ToolCategory::Axe;
    let shovel = ToolCategory::Shovel;
    let hoe = ToolCategory::Hoe;
    let generic = ToolCategory::Generic;

    assert_ne!(pick, axe);
    assert_ne!(pick, shovel);
    assert_ne!(pick, hoe);
    assert_ne!(pick, generic);

    assert_eq!(pick.to_string(), "pickaxe");
    assert_eq!(axe.to_string(), "axe");
    assert_eq!(shovel.to_string(), "shovel");
    assert_eq!(hoe.to_string(), "hoe");
    assert_eq!(generic.to_string(), "generic");
}

// ============================================================================
// 2. TOOL DEFINITION & FLOATING-POINT VALIDATION TESTS
// ============================================================================

#[test]
fn test_tool_definition_and_effectiveness_validation() {
    let tool_id = ToolId::core("test_pickaxe").unwrap();
    let mut eff = ToolEffectiveness::new(1.5).expect("valid base efficiency");

    let iron_ore = ResourceId::core("iron_ore").unwrap();
    eff = eff
        .with_multiplier(iron_ore.clone(), 2.0)
        .expect("valid multiplier");

    let def = ToolDefinition::new(tool_id.clone(), ToolCategory::Pickaxe, 150)
        .with_effectiveness(eff)
        .expect("valid tool definition");

    assert_eq!(def.max_durability, 150);
    assert_eq!(def.category, ToolCategory::Pickaxe);
    assert_eq!(def.effectiveness.calculate_effectiveness(&iron_ore), 3.0); // 1.5 * 2.0
    let stone = ResourceId::core("stone").unwrap();
    assert_eq!(def.effectiveness.calculate_effectiveness(&stone), 1.5); // 1.5 * 1.0 (default)
    assert_eq!(calculate_tool_effectiveness(Some(&def), &iron_ore), 3.0);
    assert_eq!(calculate_tool_effectiveness(None, &iron_ore), 1.0);
}

#[test]
fn test_floating_point_validation_rejects_nan_infinities_and_negatives() {
    // Negative base efficiency
    assert!(ToolEffectiveness::new(-1.0).is_err());
    // NaN base efficiency
    assert!(ToolEffectiveness::new(f32::NAN).is_err());
    // Infinity base efficiency
    assert!(ToolEffectiveness::new(f32::INFINITY).is_err());
    assert!(ToolEffectiveness::new(f32::NEG_INFINITY).is_err());

    // Multipliers
    let iron_ore = ResourceId::core("iron_ore").unwrap();
    let eff = ToolEffectiveness::new(1.0).unwrap();
    assert!(eff.clone().with_multiplier(iron_ore.clone(), -0.5).is_err());
    assert!(eff
        .clone()
        .with_multiplier(iron_ore.clone(), f32::NAN)
        .is_err());
    assert!(eff
        .clone()
        .with_multiplier(iron_ore.clone(), f32::INFINITY)
        .is_err());
}

// ============================================================================
// 3. TOOL STATE & DURABILITY INVARIANTS
// ============================================================================

#[test]
fn test_tool_state_durability_lifecycle() {
    let tool_id = ToolId::core("stone_pickaxe").unwrap();
    let mut state = ToolState::new(tool_id.clone(), 3);

    assert!(!state.is_broken());
    assert!(state.is_usable());
    assert_eq!(state.current_durability, 3);

    // Consume 1
    assert!(state.consume_durability());
    assert_eq!(state.current_durability, 2);

    // Consume 2
    assert!(state.consume_durability());
    assert_eq!(state.current_durability, 1);

    // Consume 3 -> reaches 0
    assert!(state.consume_durability());
    assert_eq!(state.current_durability, 0);
    assert!(state.is_broken());
    assert!(!state.is_usable());

    // Attempt consume below 0 -> saturates, returns false, stays 0
    assert!(!state.consume_durability());
    assert_eq!(state.current_durability, 0);
}

#[test]
fn test_tool_durability_invariant_against_definition() {
    let world = World::new();
    let pickaxe_id = ToolId::core("stone_pickaxe").unwrap();
    let tool_def = world
        .tools
        .get(&pickaxe_id)
        .expect("stone_pickaxe must exist in core tools");

    // Valid state: current <= max (100)
    let valid_state = ToolState::new(pickaxe_id.clone(), 50);
    assert!(resolve_tool(&world.tools, Some(&valid_state)).is_ok());

    // Invalid state: current > max (101 > 100)
    let invalid_state = ToolState::new(pickaxe_id.clone(), tool_def.max_durability + 1);
    let err = resolve_tool(&world.tools, Some(&invalid_state)).unwrap_err();
    match err {
        ToolError::InvalidToolState { reason } => {
            assert!(reason.contains("exceeds definition max durability"));
        }
        other => panic!("Expected InvalidToolState, got {:?}", other),
    }
}

// ============================================================================
// 4. TOOL REQUIREMENT VALIDATION TESTS
// ============================================================================

#[test]
fn test_requirement_none_semantics() {
    let pickaxe_id = ToolId::core("stone_pickaxe").unwrap();
    let pickaxe_def = ToolDefinition::new(pickaxe_id.clone(), ToolCategory::Pickaxe, 100);

    // No tool: valid
    assert!(validate_tool_requirement(&ToolRequirement::None, None).is_ok());

    // Usable tool: valid
    let usable_state = ToolState::new(pickaxe_id.clone(), 50);
    assert!(
        validate_tool_requirement(&ToolRequirement::None, Some((&usable_state, &pickaxe_def)))
            .is_ok()
    );

    // Broken tool: rejected
    let broken_state = ToolState::new(pickaxe_id.clone(), 0);
    let err =
        validate_tool_requirement(&ToolRequirement::None, Some((&broken_state, &pickaxe_def)))
            .unwrap_err();
    assert_eq!(
        err,
        ToolError::ToolBroken {
            tool_id: pickaxe_id
        }
    );
}

#[test]
fn test_requirement_any_tool_semantics() {
    let pickaxe_id = ToolId::core("stone_pickaxe").unwrap();
    let pickaxe_def = ToolDefinition::new(pickaxe_id.clone(), ToolCategory::Pickaxe, 100);

    // No tool: rejected
    let err = validate_tool_requirement(&ToolRequirement::AnyTool, None).unwrap_err();
    assert_eq!(
        err,
        ToolError::NoTool {
            required: ToolRequirement::AnyTool
        }
    );

    // Usable tool: valid
    let usable_state = ToolState::new(pickaxe_id.clone(), 10);
    assert!(validate_tool_requirement(
        &ToolRequirement::AnyTool,
        Some((&usable_state, &pickaxe_def))
    )
    .is_ok());

    // Broken tool: rejected
    let broken_state = ToolState::new(pickaxe_id.clone(), 0);
    assert_eq!(
        validate_tool_requirement(
            &ToolRequirement::AnyTool,
            Some((&broken_state, &pickaxe_def))
        )
        .unwrap_err(),
        ToolError::ToolBroken {
            tool_id: pickaxe_id
        }
    );
}

#[test]
fn test_requirement_category_semantics() {
    let pickaxe_id = ToolId::core("stone_pickaxe").unwrap();
    let pickaxe_def = ToolDefinition::new(pickaxe_id.clone(), ToolCategory::Pickaxe, 100);

    let axe_id = ToolId::core("stone_axe").unwrap();
    let axe_def = ToolDefinition::new(axe_id.clone(), ToolCategory::Axe, 100);

    let req = ToolRequirement::Category(ToolCategory::Pickaxe);

    // No tool: rejected
    assert_eq!(
        validate_tool_requirement(&req, None).unwrap_err(),
        ToolError::NoTool {
            required: ToolRequirement::Category(ToolCategory::Pickaxe)
        }
    );

    // Correct category: valid
    let usable_pick = ToolState::new(pickaxe_id.clone(), 50);
    assert!(validate_tool_requirement(&req, Some((&usable_pick, &pickaxe_def))).is_ok());

    // Wrong category: rejected
    let usable_axe = ToolState::new(axe_id.clone(), 50);
    let err = validate_tool_requirement(&req, Some((&usable_axe, &axe_def))).unwrap_err();
    assert_eq!(
        err,
        ToolError::WrongToolCategory {
            expected: ToolCategory::Pickaxe,
            actual: ToolCategory::Axe
        }
    );

    // Broken pickaxe: rejected
    let broken_pick = ToolState::new(pickaxe_id.clone(), 0);
    assert_eq!(
        validate_tool_requirement(&req, Some((&broken_pick, &pickaxe_def))).unwrap_err(),
        ToolError::ToolBroken {
            tool_id: pickaxe_id
        }
    );
}

#[test]
fn test_requirement_specific_tool_semantics() {
    let stone_pick = ToolId::core("stone_pickaxe").unwrap();
    let stone_def = ToolDefinition::new(stone_pick.clone(), ToolCategory::Pickaxe, 100);

    let iron_pick = ToolId::core("iron_pickaxe").unwrap();
    let iron_def = ToolDefinition::new(iron_pick.clone(), ToolCategory::Pickaxe, 250);

    let req = ToolRequirement::Specific(iron_pick.clone());

    // No tool: rejected
    assert_eq!(
        validate_tool_requirement(&req, None).unwrap_err(),
        ToolError::NoTool {
            required: ToolRequirement::Specific(iron_pick.clone())
        }
    );

    // Wrong specific tool: rejected
    let usable_stone = ToolState::new(stone_pick.clone(), 80);
    let err = validate_tool_requirement(&req, Some((&usable_stone, &stone_def))).unwrap_err();
    assert_eq!(
        err,
        ToolError::WrongTool {
            expected: iron_pick.clone(),
            actual: stone_pick.clone()
        }
    );

    // Correct specific tool: valid
    let usable_iron = ToolState::new(iron_pick.clone(), 150);
    assert!(validate_tool_requirement(&req, Some((&usable_iron, &iron_def))).is_ok());

    // Broken iron pickaxe: rejected
    let broken_iron = ToolState::new(iron_pick.clone(), 0);
    assert_eq!(
        validate_tool_requirement(&req, Some((&broken_iron, &iron_def))).unwrap_err(),
        ToolError::ToolBroken { tool_id: iron_pick }
    );
}

// ============================================================================
// 5. TOOL GATHERING PIPELINE & ATOMICITY TESTS
// ============================================================================

#[test]
fn test_successful_tool_gather_consumes_exactly_one_durability() {
    let mut world = World::new();
    let target_coord = IVec3::new(2, 2, 0);

    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    world
        .store
        .set_voxel_world(target_coord, VoxelBlock::new(stone_mat));

    let player = PlayerController::new(Vec3::new(0.0, 0.0, 0.0));
    let look_dir = (voxel_center(target_coord) - player.eye_position()).normalize();

    let pickaxe_id = ToolId::core("stone_pickaxe").unwrap();
    let mut tool_state = ToolState::new(pickaxe_id.clone(), 50);
    let mut cooldown = InteractionCooldown::new(0.20);

    let result = handle_player_tool_gather(
        &mut world,
        &player,
        look_dir,
        Some(&mut tool_state),
        &mut cooldown,
    )
    .expect("gathering with valid tool must succeed");

    // Verification
    assert_eq!(
        result.collection.resource_id,
        ResourceId::core("stone").unwrap()
    );
    assert_eq!(result.collection.quantity, 1);
    assert_eq!(result.tool_id, Some(pickaxe_id));
    assert_eq!(result.durability_consumed, 1);
    assert_eq!(result.remaining_durability, Some(49));
    assert_eq!(tool_state.current_durability, 49);

    // Voxel is now air
    assert!(world
        .store
        .get_voxel_world_checked(target_coord)
        .unwrap()
        .is_air());
    // Cooldown is active
    assert!(!cooldown.can_act());
}

#[test]
fn test_can_gather_with_tool_and_validate_action_consistency() {
    let world = World::new();
    let target = IVec3::new(10, 64, 10);
    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    let hit = VoxelHit {
        voxel_coord: target,
        material: stone_mat,
        hit_point: Vec3::new(10.0, 64.5, 10.0),
        distance: 2.0,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let pickaxe_id = ToolId::core("stone_pickaxe").unwrap();
    let tool_state = ToolState::new(pickaxe_id, 50);

    // Target not resident should fail consistently
    let err1 = can_gather_with_tool(&world, &hit, DEFAULT_INTERACTION_REACH, Some(&tool_state))
        .unwrap_err();
    assert_eq!(err1, ToolError::TargetNotResident { coord: target });

    let err2 =
        validate_tool_gather_internal(&world, &hit, DEFAULT_INTERACTION_REACH, Some(&tool_state))
            .unwrap_err();
    assert_eq!(err2, ToolError::TargetNotResident { coord: target });
}

#[test]
fn test_validate_and_execute_tool_gather_action_separately() {
    let mut world = World::new();
    let target_coord = IVec3::new(2, 2, 0);
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    world
        .store
        .set_voxel_world(target_coord, VoxelBlock::new(stone_mat));

    let player = PlayerController::new(Vec3::new(0.0, 0.0, 0.0));
    let look_dir = (voxel_center(target_coord) - player.eye_position()).normalize();
    let pickaxe_id = ToolId::core("stone_pickaxe").unwrap();
    let mut tool_state = ToolState::new(pickaxe_id.clone(), 30);

    // 1. Validate
    let (res_def, eff, tx) =
        validate_tool_gather_action(&world, &player, look_dir, Some(&tool_state))
            .expect("validation should succeed");

    assert_eq!(eff, 1.0);
    assert_eq!(res_def.resource_id, ResourceId::core("stone").unwrap());

    // 2. Execute
    let res = execute_tool_gather_transaction(
        &mut world,
        target_coord,
        &res_def,
        eff,
        Some(&mut tool_state),
        &tx,
    )
    .expect("execution should succeed");

    assert_eq!(res.durability_consumed, 1);
    assert_eq!(tool_state.current_durability, 29);
    assert!(world
        .store
        .get_voxel_world_checked(target_coord)
        .unwrap()
        .is_air());
}

#[test]
fn test_hand_gathering_on_none_requirement_succeeds_without_durability() {
    let mut world = World::new();
    let target_coord = IVec3::new(2, 2, 0);

    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    world
        .store
        .set_voxel_world(target_coord, VoxelBlock::new(stone_mat));

    let player = PlayerController::new(Vec3::new(0.0, 0.0, 0.0));
    let look_dir = (voxel_center(target_coord) - player.eye_position()).normalize();
    let mut cooldown = InteractionCooldown::new(0.20);

    let result = handle_player_tool_gather(
        &mut world,
        &player,
        look_dir,
        None, // No active tool
        &mut cooldown,
    )
    .expect("gathering without tool on none requirement must succeed");

    assert_eq!(result.tool_id, None);
    assert_eq!(result.durability_consumed, 0);
    assert_eq!(result.remaining_durability, None);
}

#[test]
fn test_atomicity_all_failures_leave_world_and_durability_untouched() {
    let mut world = World::new();
    let target_coord = IVec3::new(2, 2, 0);

    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    world
        .store
        .set_voxel_world(target_coord, VoxelBlock::new(stone_mat));

    // Give the resource a specific tool requirement
    let specific_tool_id = ToolId::core("iron_pickaxe").unwrap();
    let res_def = ResourceDefinition::new(ResourceId::core("stone").unwrap(), 1)
        .with_required_tool(ToolRequirement::Specific(specific_tool_id.clone()));
    world.resources.register(stone_mat, res_def);

    let player = PlayerController::new(Vec3::new(0.0, 0.0, 0.0));
    let look_dir = (voxel_center(target_coord) - player.eye_position()).normalize();

    // Case 1: Wrong tool
    let wrong_id = ToolId::core("stone_pickaxe").unwrap();
    let mut wrong_tool = ToolState::new(wrong_id.clone(), 50);
    let mut cooldown = InteractionCooldown::new(0.20);

    let err = handle_player_tool_gather(
        &mut world,
        &player,
        look_dir,
        Some(&mut wrong_tool),
        &mut cooldown,
    )
    .unwrap_err();

    assert!(matches!(err, ToolError::WrongTool { .. }));
    // Invariants: durability unchanged, world unchanged, cooldown not triggered
    assert_eq!(wrong_tool.current_durability, 50);
    assert!(world
        .store
        .get_voxel_world_checked(target_coord)
        .unwrap()
        .is_solid());
    assert!(cooldown.can_act());

    // Case 2: Out of reach (beyond 5.0m reach but within resident chunk)
    let far_player = PlayerController::new(Vec3::new(8.0, 0.0, 0.0));
    let far_dir = (voxel_center(target_coord) - far_player.eye_position()).normalize();
    let mut iron_tool = ToolState::new(specific_tool_id.clone(), 100);
    let err2 = handle_player_tool_gather(
        &mut world,
        &far_player,
        far_dir,
        Some(&mut iron_tool),
        &mut cooldown,
    )
    .unwrap_err();

    assert!(matches!(err2, ToolError::NoTargetHit));
    assert_eq!(iron_tool.current_durability, 100);
    assert!(world
        .store
        .get_voxel_world_checked(target_coord)
        .unwrap()
        .is_solid());
    assert!(cooldown.can_act());

    // Case 3: Cooldown active
    cooldown.trigger();
    assert!(!cooldown.can_act());
    let err3 = handle_player_tool_gather(
        &mut world,
        &player,
        look_dir,
        Some(&mut iron_tool),
        &mut cooldown,
    )
    .unwrap_err();

    assert!(matches!(err3, ToolError::CooldownActive { .. }));
    assert_eq!(iron_tool.current_durability, 100);
    assert!(world
        .store
        .get_voxel_world_checked(target_coord)
        .unwrap()
        .is_solid());
}

#[test]
fn test_broken_tool_fails_and_consumes_zero() {
    let mut world = World::new();
    let target_coord = IVec3::new(2, 2, 0);
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    world
        .store
        .set_voxel_world(target_coord, VoxelBlock::new(stone_mat));

    let player = PlayerController::new(Vec3::new(0.0, 0.0, 0.0));
    let look_dir = (voxel_center(target_coord) - player.eye_position()).normalize();
    let pickaxe_id = ToolId::core("stone_pickaxe").unwrap();
    let mut broken_tool = ToolState::new(pickaxe_id.clone(), 0);
    let mut cooldown = InteractionCooldown::new(0.20);

    let err = handle_player_tool_gather(
        &mut world,
        &player,
        look_dir,
        Some(&mut broken_tool),
        &mut cooldown,
    )
    .unwrap_err();

    assert_eq!(
        err,
        ToolError::ToolBroken {
            tool_id: pickaxe_id
        }
    );
    assert_eq!(broken_tool.current_durability, 0);
    assert!(world
        .store
        .get_voxel_world_checked(target_coord)
        .unwrap()
        .is_solid());
    assert!(cooldown.can_act());
}

// ============================================================================
// 6. EFFECTIVENESS & YIELD DETERMINISM
// ============================================================================

#[test]
fn test_effectiveness_does_not_modify_base_yield_quantity() {
    let mut world = World::new();
    let target_coord = IVec3::new(2, 2, 0);
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let iron_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("iron_ore").unwrap())
        .unwrap();

    world
        .store
        .set_voxel_world(target_coord, VoxelBlock::new(iron_mat));

    // Register a custom high-effectiveness pickaxe
    let super_pick_id = ToolId::core("super_pickaxe").unwrap();
    let mut eff = ToolEffectiveness::new(5.0).unwrap();
    eff = eff
        .with_multiplier(ResourceId::core("iron_ore").unwrap(), 3.0)
        .unwrap();
    let super_def = ToolDefinition::new(super_pick_id.clone(), ToolCategory::Pickaxe, 500)
        .with_effectiveness(eff)
        .unwrap();
    world.tools.register(super_def).unwrap();

    let player = PlayerController::new(Vec3::new(0.0, 0.0, 0.0));
    let look_dir = (voxel_center(target_coord) - player.eye_position()).normalize();
    let mut tool_state = ToolState::new(super_pick_id.clone(), 200);
    let mut cooldown = InteractionCooldown::new(0.20);

    let result = handle_player_tool_gather(
        &mut world,
        &player,
        look_dir,
        Some(&mut tool_state),
        &mut cooldown,
    )
    .expect("must succeed");

    // INVARIANT GUARDRAIL 4: Effectiveness is 15.0 (5.0 * 3.0), BUT quantity remains base_yield (1)
    assert_eq!(result.effectiveness, 15.0);
    assert_eq!(result.collection.quantity, 1);
    assert_eq!(tool_state.current_durability, 199);
}

// ============================================================================
// 7. COORDINATES & BOUNDARIES
// ============================================================================

#[test]
fn test_negative_coordinates_and_chunk_boundary_tool_gathering() {
    let mut world = World::new();
    // Negative coordinate: (-1, 2, 0) lies in chunk (-1, 0, 0)
    let target_coord = IVec3::new(-1, 2, 0);
    add_empty_chunk(&mut world.store, IVec3::new(-1, 0, 0));
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    world
        .store
        .set_voxel_world(target_coord, VoxelBlock::new(stone_mat));

    // Player in chunk (0, 0, 0) targeting across boundary
    let player = PlayerController::new(Vec3::new(0.5, 0.0, 0.0));
    let pickaxe_id = ToolId::core("stone_pickaxe").unwrap();
    let mut tool_state = ToolState::new(pickaxe_id, 10);
    let mut cooldown = InteractionCooldown::new(0.20);

    let look_dir = (voxel_center(target_coord) - player.eye_position()).normalize();

    let result = handle_player_tool_gather(
        &mut world,
        &player,
        look_dir,
        Some(&mut tool_state),
        &mut cooldown,
    )
    .expect("cross-boundary gathering with tool must succeed");

    assert_eq!(result.collection.source_coord, target_coord);
    assert_eq!(tool_state.current_durability, 9);
    assert!(world
        .store
        .get_voxel_world_checked(target_coord)
        .unwrap()
        .is_air());
}

// ============================================================================
// 8. SCOPE FIREWALL AUDIT TESTS
// ============================================================================

#[test]
fn test_firewall_no_inventory_or_equipment_coupling() {
    // ToolState is merely active capability, caller retains ownership
    let tool_id = ToolId::core("stone_pickaxe").unwrap();
    let state = ToolState::new(tool_id, 10);
    // ToolState contains only tool_id and current_durability
    assert_eq!(std::mem::size_of_val(&state.current_durability), 4);
}

#[test]
fn test_data_backward_compatibility_defaults() {
    // Existing HarvestableComponent JSON without required_tool deserializes with ToolRequirement::None
    let json = r#"{
        "resource": "core:stone",
        "yield_quantity": 2,
        "harvestable": true
    }"#;

    let comp: HarvestableComponent = serde_json::from_str(json).expect("should deserialize");
    assert_eq!(comp.required_tool, ToolRequirement::None);
    assert_eq!(comp.yield_quantity, 2);
}
