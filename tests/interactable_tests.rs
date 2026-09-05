use glam::{IVec3, Vec3};
use omnisia::chunk::{dirty_flags, Chunk};
use omnisia::interaction::{
    detect_interactable_target, execute_interaction, handle_player_generic_interaction,
    query_interactable_target, validate_interaction, AudioCue, FeedbackId, InteractableAction,
    InteractableDefinition, InteractableId, InteractableState, InteractionCooldown,
    InteractionError, InteractionFeedback, VisualCue, VoxelHit, DEFAULT_INTERACTION_REACH,
};
use omnisia::mesh::types::FaceDirection;
use omnisia::modding::definitions::BlockDefinition;
use omnisia::modding::resource_id::ResourceId;
use omnisia::player::PlayerController;
use omnisia::streaming::store::ChunkStore;
use omnisia::voxel::{VoxelBlock, VOXEL_SIZE};
use omnisia::world::World;

fn add_empty_chunk(store: &mut ChunkStore, coord: IVec3) {
    let mut chunk = Chunk::new(coord);
    chunk.clear_dirty(dirty_flags::ALL);
    store.insert(chunk);
}

fn voxel_center(coord: IVec3) -> Vec3 {
    coord.as_vec3() * VOXEL_SIZE + Vec3::splat(VOXEL_SIZE * 0.5)
}

fn create_test_switch_definition(
    id: &str,
    source_block: &str,
    material: omnisia::material::MaterialId,
) -> InteractableDefinition {
    InteractableDefinition {
        id: InteractableId::core(id).unwrap(),
        source_block: ResourceId::core(source_block).unwrap(),
        expected_material: material,
        allowed_actions: vec![
            InteractableAction::Activate,
            InteractableAction::Toggle,
            InteractableAction::Examine,
        ],
        initial_state: InteractableState::Idle,
        audio_cue: Some(AudioCue::SwitchToggle),
        visual_cue: Some(VisualCue::Pulse),
    }
}

fn create_test_door_definition(
    id: &str,
    source_block: &str,
    material: omnisia::material::MaterialId,
) -> InteractableDefinition {
    InteractableDefinition {
        id: InteractableId::core(id).unwrap(),
        source_block: ResourceId::core(source_block).unwrap(),
        expected_material: material,
        allowed_actions: vec![
            InteractableAction::Open,
            InteractableAction::Close,
            InteractableAction::Toggle,
            InteractableAction::Examine,
        ],
        initial_state: InteractableState::Closed,
        audio_cue: Some(AudioCue::DoorOpen),
        visual_cue: Some(VisualCue::StateTransition),
    }
}

// ============================================================================
// 1. IDENTITY & TYPING TESTS
// ============================================================================

#[test]
fn test_interactable_id_creation_formatting_and_validation() {
    let switch = InteractableId::core("ancient_switch").expect("valid core id");
    assert_eq!(switch.namespace.as_str(), "core");
    assert_eq!(switch.path, "ancient_switch");
    assert_eq!(switch.to_string(), "core:ancient_switch");
    assert_eq!(switch.as_str(), "core:ancient_switch");

    // FromStr roundtrip
    let parsed: InteractableId = "core:ancient_switch".parse().expect("should parse");
    assert_eq!(parsed, switch);

    // Mod-namespaced interactable
    let mod_obj = InteractableId::new("relics_mod", "vault_door").expect("valid mod interactable");
    assert_eq!(mod_obj.to_string(), "relics_mod:vault_door");

    // Formatting errors
    assert!(InteractableId::new("CORE", "invalid").is_err());
    assert!(InteractableId::new("core", "").is_err());
    assert!(InteractableId::new("core", "invalid space").is_err());
    assert!("missing_delimiter".parse::<InteractableId>().is_err());
    assert!("too:many:delimiters:here"
        .parse::<InteractableId>()
        .is_err());
}

#[test]
fn test_interactable_id_distinct_from_resource_and_tool_id() {
    let id_str = "ancient_mechanism";
    let interactable_id = InteractableId::core(id_str).unwrap();
    let resource_id = ResourceId::core(id_str).unwrap();
    let tool_id = omnisia::interaction::ToolId::core(id_str).unwrap();

    assert_eq!(interactable_id.to_string(), resource_id.to_string());
    assert_eq!(interactable_id.to_string(), tool_id.to_string());

    // Type separation: compiler guarantees distinct types without implicit conversion
    let _typed_inter: InteractableId = interactable_id;
    let _typed_res: ResourceId = resource_id;
    let _typed_tool: omnisia::interaction::ToolId = tool_id;
}

// ============================================================================
// 2. RESOLUTION & FALLBACK TESTS
// ============================================================================

#[test]
fn test_interactable_resolution_succeeds_on_valid_block() {
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

    let def = create_test_switch_definition("ancient_switch", "ancient_switch_block", stone_mat);
    world.interactables.register_definition(def.clone());
    world
        .interactables
        .set_interactable_at(target_coord, def.id.clone());

    let hit = VoxelHit {
        voxel_coord: target_coord,
        material: stone_mat,
        hit_point: voxel_center(target_coord),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };

    let target = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH)
        .expect("should resolve valid interactable");

    assert_eq!(target.interactable_id, def.id);
    assert_eq!(target.current_state, InteractableState::Idle);
    assert_eq!(target.expected_material, stone_mat);
    assert_eq!(target.preferred_action, Some(InteractableAction::Activate));
}

#[test]
fn test_non_interactable_block_rejected_with_not_interactable() {
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

    // Notice: NOT registered in world.interactables
    let hit = VoxelHit {
        voxel_coord: target_coord,
        material: stone_mat,
        hit_point: voxel_center(target_coord),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };

    let err = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH)
        .expect_err("plain block must not resolve to interactable");

    assert_eq!(
        err,
        InteractionError::NotInteractable {
            coord: target_coord
        }
    );
}

#[test]
fn test_target_air_rejected_with_target_is_air() {
    let mut world = World::new();
    let target_coord = IVec3::new(2, 2, 0);
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let hit = VoxelHit {
        voxel_coord: target_coord,
        material: omnisia::material::MaterialId::AIR,
        hit_point: voxel_center(target_coord),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };

    let err = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH)
        .expect_err("air must be rejected");

    assert_eq!(
        err,
        InteractionError::TargetIsAir {
            coord: target_coord
        }
    );
}

#[test]
fn test_unloaded_chunk_target_rejected_with_target_not_resident() {
    let world = World::new();
    let target_coord = IVec3::new(500, 500, 500); // Unloaded chunk

    let hit = VoxelHit {
        voxel_coord: target_coord,
        material: omnisia::material::MaterialId(1),
        hit_point: target_coord.as_vec3(),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };

    let err = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH)
        .expect_err("unloaded chunk must be rejected");

    assert_eq!(
        err,
        InteractionError::TargetNotResident {
            coord: target_coord
        }
    );
}

#[test]
fn test_raycast_miss_rejected_with_no_target_hit() {
    let mut world = World::new();
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    // Player well inside chunk (0..16m), looking through empty air for 5m
    let player = PlayerController::new(Vec3::new(2.0, 2.0, 2.0));
    let look_dir = Vec3::new(1.0, 0.0, 0.0);

    let err = query_interactable_target(&world, &player, look_dir)
        .expect_err("raycast miss must yield NoTargetHit");

    assert_eq!(err, InteractionError::NoTargetHit);
}

#[test]
fn test_stale_instance_ignored_on_query_and_falls_back_to_initial_state() {
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

    let def = create_test_switch_definition("switch_a", "switch_block_a", stone_mat);
    world.interactables.register_definition(def.clone());
    world
        .interactables
        .set_interactable_at(target_coord, def.id.clone());

    // Inject a stale instance with a DIFFERENT expected material
    let foreign_mat = omnisia::material::MaterialId(999);
    world.interactables.set_instance_state(
        target_coord,
        def.id.clone(),
        foreign_mat,
        InteractableState::Active,
    );

    let hit = VoxelHit {
        voxel_coord: target_coord,
        material: stone_mat,
        hit_point: voxel_center(target_coord),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };

    let target = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH)
        .expect("should resolve with fallback");

    // Stale instance was ignored: fell back to def.initial_state (Idle)
    assert_eq!(target.current_state, InteractableState::Idle);

    // INVARIANT: Read-only query did NOT delete the instance
    assert!(world.interactables.instances.contains_key(&target_coord));
}

// ============================================================================
// 3. REACH BOUNDARIES
// ============================================================================

#[test]
fn test_reach_inside_and_exact_boundary() {
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

    let def = create_test_switch_definition("switch_reach", "switch_reach_block", stone_mat);
    world.interactables.register_definition(def.clone());
    world
        .interactables
        .set_interactable_at(target_coord, def.id.clone());

    let hit_inside = VoxelHit {
        voxel_coord: target_coord,
        material: stone_mat,
        hit_point: voxel_center(target_coord),
        distance: 3.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };
    assert!(detect_interactable_target(&world, &hit_inside, 5.0).is_ok());

    let hit_exact = VoxelHit {
        voxel_coord: target_coord,
        material: stone_mat,
        hit_point: voxel_center(target_coord),
        distance: 5.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };
    assert!(detect_interactable_target(&world, &hit_exact, 5.0).is_ok());
}

#[test]
fn test_reach_beyond_boundary_rejected() {
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

    let def = create_test_switch_definition("switch_far", "switch_far_block", stone_mat);
    world.interactables.register_definition(def.clone());
    world
        .interactables
        .set_interactable_at(target_coord, def.id.clone());

    let hit_far = VoxelHit {
        voxel_coord: target_coord,
        material: stone_mat,
        hit_point: voxel_center(target_coord),
        distance: 5.5,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };

    let err = detect_interactable_target(&world, &hit_far, 5.0)
        .expect_err("hit beyond max_reach must fail");

    assert_eq!(
        err,
        InteractionError::ExceedsReach {
            distance: 5.5,
            max_reach: 5.0,
        }
    );
}

// ============================================================================
// 4. ACTION FILTERING & PREFERENCE
// ============================================================================

#[test]
fn test_available_actions_filtered_by_current_state() {
    let allowed = vec![
        InteractableAction::Open,
        InteractableAction::Close,
        InteractableAction::Toggle,
        InteractableAction::Examine,
    ];

    let closed_actions =
        omnisia::interaction::filter_available_actions(&allowed, InteractableState::Closed);
    assert_eq!(
        closed_actions,
        vec![
            InteractableAction::Open,
            InteractableAction::Toggle,
            InteractableAction::Examine
        ]
    );

    let open_actions =
        omnisia::interaction::filter_available_actions(&allowed, InteractableState::Open);
    assert_eq!(
        open_actions,
        vec![
            InteractableAction::Close,
            InteractableAction::Toggle,
            InteractableAction::Examine
        ]
    );

    let disabled_actions =
        omnisia::interaction::filter_available_actions(&allowed, InteractableState::Disabled);
    assert_eq!(disabled_actions, vec![InteractableAction::Examine]);
}

#[test]
fn test_preferred_action_equals_first_valid_action() {
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

    let door_def = create_test_door_definition("vault_door", "vault_door_block", stone_mat);
    world.interactables.register_definition(door_def.clone());
    world
        .interactables
        .set_interactable_at(target_coord, door_def.id.clone());

    let hit = VoxelHit {
        voxel_coord: target_coord,
        material: stone_mat,
        hit_point: voxel_center(target_coord),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };

    // When Closed: preferred is Open
    let target = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
    assert_eq!(target.current_state, InteractableState::Closed);
    assert_eq!(target.preferred_action, Some(InteractableAction::Open));

    // When Open: preferred is Close
    world.interactables.set_instance_state(
        target_coord,
        door_def.id.clone(),
        stone_mat,
        InteractableState::Open,
    );
    let target_open = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
    assert_eq!(target_open.current_state, InteractableState::Open);
    assert_eq!(
        target_open.preferred_action,
        Some(InteractableAction::Close)
    );
}

#[test]
fn test_invalid_action_rejected() {
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

    let door_def = create_test_door_definition("vault_door", "vault_door_block", stone_mat);
    world.interactables.register_definition(door_def.clone());
    world
        .interactables
        .set_interactable_at(target_coord, door_def.id.clone());

    // Door is Closed; trying to Close it must fail
    let hit = VoxelHit {
        voxel_coord: target_coord,
        material: stone_mat,
        hit_point: voxel_center(target_coord),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };
    let target = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();

    let err = validate_interaction(
        &world,
        &target,
        InteractableAction::Close,
        Vec3::ZERO,
        DEFAULT_INTERACTION_REACH,
    )
    .expect_err("closing a closed door must fail");

    assert_eq!(
        err,
        InteractionError::InvalidActionForState {
            action: InteractableAction::Close,
            state: InteractableState::Closed,
        }
    );
}

#[test]
fn test_disabled_target_rejects_mutating_actions_allows_examine() {
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

    let switch_def = create_test_switch_definition("broken_switch", "switch_block", stone_mat);
    world.interactables.register_definition(switch_def.clone());
    world
        .interactables
        .set_interactable_at(target_coord, switch_def.id.clone());
    world.interactables.set_instance_state(
        target_coord,
        switch_def.id.clone(),
        stone_mat,
        InteractableState::Disabled,
    );

    let hit = VoxelHit {
        voxel_coord: target_coord,
        material: stone_mat,
        hit_point: voxel_center(target_coord),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };
    let target = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();

    // Mutating action rejected with ObjectDisabled
    let err = validate_interaction(
        &world,
        &target,
        InteractableAction::Activate,
        Vec3::ZERO,
        DEFAULT_INTERACTION_REACH,
    )
    .expect_err("mutating action on disabled object must fail");

    assert_eq!(
        err,
        InteractionError::ObjectDisabled {
            interactable_id: switch_def.id.clone(),
        }
    );

    // Examine action succeeds even when Disabled!
    let proposal = validate_interaction(
        &world,
        &target,
        InteractableAction::Examine,
        Vec3::ZERO,
        DEFAULT_INTERACTION_REACH,
    )
    .expect("examine must be allowed on disabled object");

    assert_eq!(proposal.previous_state, InteractableState::Disabled);
    assert_eq!(proposal.target_state, InteractableState::Disabled);
    assert_eq!(proposal.feedback.feedback_id, Some(FeedbackId::Examined));
}

// ============================================================================
// 5. DETERMINISTIC STATE TRANSITIONS
// ============================================================================

#[test]
fn test_activate_transitions_idle_to_active() {
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

    let def = create_test_switch_definition("switch_act", "switch_act_block", stone_mat);
    world.interactables.register_definition(def.clone());
    world
        .interactables
        .set_interactable_at(target_coord, def.id.clone());

    let hit = VoxelHit {
        voxel_coord: target_coord,
        material: stone_mat,
        hit_point: voxel_center(target_coord),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };
    let target = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();

    let proposal = validate_interaction(
        &world,
        &target,
        InteractableAction::Activate,
        Vec3::ZERO,
        DEFAULT_INTERACTION_REACH,
    )
    .unwrap();
    assert_eq!(proposal.previous_state, InteractableState::Idle);
    assert_eq!(proposal.target_state, InteractableState::Active);

    let res = execute_interaction(&mut world, &proposal).unwrap();
    assert_eq!(res.previous_state, InteractableState::Idle);
    assert_eq!(res.new_state, InteractableState::Active);
    assert_eq!(res.feedback.feedback_id, Some(FeedbackId::Activated));

    // Second Activate must fail because it's already Active
    let target_active =
        detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
    assert_eq!(target_active.current_state, InteractableState::Active);
    let err = validate_interaction(
        &world,
        &target_active,
        InteractableAction::Activate,
        Vec3::ZERO,
        DEFAULT_INTERACTION_REACH,
    )
    .expect_err("cannot activate an already active switch");
    assert_eq!(
        err,
        InteractionError::InvalidActionForState {
            action: InteractableAction::Activate,
            state: InteractableState::Active,
        }
    );
}

#[test]
fn test_toggle_transitions_idle_active_and_closed_open() {
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

    let def = create_test_switch_definition("switch_tog", "switch_tog_block", stone_mat);
    world.interactables.register_definition(def.clone());
    world
        .interactables
        .set_interactable_at(target_coord, def.id.clone());

    let hit = VoxelHit {
        voxel_coord: target_coord,
        material: stone_mat,
        hit_point: voxel_center(target_coord),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };

    // 1. Idle -> Active
    let target = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
    let prop1 = validate_interaction(
        &world,
        &target,
        InteractableAction::Toggle,
        Vec3::ZERO,
        DEFAULT_INTERACTION_REACH,
    )
    .unwrap();
    let res1 = execute_interaction(&mut world, &prop1).unwrap();
    assert_eq!(res1.previous_state, InteractableState::Idle);
    assert_eq!(res1.new_state, InteractableState::Active);
    assert_eq!(res1.feedback.feedback_id, Some(FeedbackId::Activated));

    // 2. Active -> Idle
    let target2 = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
    let prop2 = validate_interaction(
        &world,
        &target2,
        InteractableAction::Toggle,
        Vec3::ZERO,
        DEFAULT_INTERACTION_REACH,
    )
    .unwrap();
    let res2 = execute_interaction(&mut world, &prop2).unwrap();
    assert_eq!(res2.previous_state, InteractableState::Active);
    assert_eq!(res2.new_state, InteractableState::Idle);
    assert_eq!(res2.feedback.feedback_id, Some(FeedbackId::Deactivated));
}

#[test]
fn test_open_and_close_transitions() {
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

    let def = create_test_door_definition("door_test", "door_block", stone_mat);
    world.interactables.register_definition(def.clone());
    world
        .interactables
        .set_interactable_at(target_coord, def.id.clone());

    let hit = VoxelHit {
        voxel_coord: target_coord,
        material: stone_mat,
        hit_point: voxel_center(target_coord),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };

    // Open: Closed -> Open
    let target = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
    let prop_open = validate_interaction(
        &world,
        &target,
        InteractableAction::Open,
        Vec3::ZERO,
        DEFAULT_INTERACTION_REACH,
    )
    .unwrap();
    let res_open = execute_interaction(&mut world, &prop_open).unwrap();
    assert_eq!(res_open.previous_state, InteractableState::Closed);
    assert_eq!(res_open.new_state, InteractableState::Open);
    assert_eq!(res_open.feedback.feedback_id, Some(FeedbackId::Opened));

    // Close: Open -> Closed
    let target_open = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
    let prop_close = validate_interaction(
        &world,
        &target_open,
        InteractableAction::Close,
        Vec3::ZERO,
        DEFAULT_INTERACTION_REACH,
    )
    .unwrap();
    let res_close = execute_interaction(&mut world, &prop_close).unwrap();
    assert_eq!(res_close.previous_state, InteractableState::Open);
    assert_eq!(res_close.new_state, InteractableState::Closed);
    assert_eq!(res_close.feedback.feedback_id, Some(FeedbackId::Closed));
}

#[test]
fn test_examine_preserves_state_and_returns_semantic_feedback() {
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

    let def = create_test_switch_definition("switch_exam", "switch_exam_block", stone_mat);
    world.interactables.register_definition(def.clone());
    world
        .interactables
        .set_interactable_at(target_coord, def.id.clone());

    let hit = VoxelHit {
        voxel_coord: target_coord,
        material: stone_mat,
        hit_point: voxel_center(target_coord),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };
    let target = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();

    let proposal = validate_interaction(
        &world,
        &target,
        InteractableAction::Examine,
        Vec3::ZERO,
        DEFAULT_INTERACTION_REACH,
    )
    .unwrap();

    // Semantic invariant: previous_state == target_state
    assert_eq!(proposal.previous_state, InteractableState::Idle);
    assert_eq!(proposal.target_state, InteractableState::Idle);
    assert_eq!(proposal.feedback.feedback_id, Some(FeedbackId::Examined));

    let res = execute_interaction(&mut world, &proposal).unwrap();
    assert_eq!(res.previous_state, InteractableState::Idle);
    assert_eq!(res.new_state, InteractableState::Idle);
    assert_eq!(res.feedback.feedback_id, Some(FeedbackId::Examined));
    assert_eq!(res.feedback.audio_cue, None);
    assert_eq!(res.feedback.visual_cue, None);
}

// ============================================================================
// 6. QUERY PURITY TESTS
// ============================================================================

#[test]
fn test_query_and_validate_perform_zero_mutation() {
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

    let def = create_test_switch_definition("switch_pure", "switch_pure_block", stone_mat);
    world.interactables.register_definition(def.clone());
    world
        .interactables
        .set_interactable_at(target_coord, def.id.clone());

    // Inject a stale instance
    let wrong_mat = omnisia::material::MaterialId(999);
    world.interactables.set_instance_state(
        target_coord,
        def.id.clone(),
        wrong_mat,
        InteractableState::Active,
    );

    let instances_before = world.interactables.instances.clone();

    let hit = VoxelHit {
        voxel_coord: target_coord,
        material: stone_mat,
        hit_point: voxel_center(target_coord),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };

    // Run query
    let target = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();

    // Run validation
    let _proposal = validate_interaction(
        &world,
        &target,
        InteractableAction::Activate,
        Vec3::ZERO,
        DEFAULT_INTERACTION_REACH,
    )
    .unwrap();

    // Invariant: instances map must be 100% IDENTICAL before and after read-only queries
    assert_eq!(world.interactables.instances, instances_before);
}

#[test]
fn test_validation_failure_leaves_state_and_cooldown_untouched() {
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

    let def = create_test_switch_definition("switch_fail", "switch_fail_block", stone_mat);
    world.interactables.register_definition(def.clone());
    world
        .interactables
        .set_interactable_at(target_coord, def.id.clone());

    let mut cooldown = InteractionCooldown::new(0.20);
    // Player well inside chunk looking at air (misses target)
    let player = PlayerController::new(Vec3::new(2.0, 2.0, 2.0));

    let err = handle_player_generic_interaction(
        &mut world,
        &player,
        Vec3::new(0.0, 0.0, 1.0),
        Some(InteractableAction::Activate),
        &mut cooldown,
    )
    .expect_err("missed interaction must fail");

    assert!(matches!(err, InteractionError::NoTargetHit));
    // Cooldown was NOT triggered
    assert!(cooldown.can_act());
    // World instances unmodified
    assert!(world.interactables.instances.is_empty());
}

// ============================================================================
// 7. COOLDOWN INTEGRATION
// ============================================================================

#[test]
fn test_active_cooldown_blocks_interaction() {
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

    let def = create_test_switch_definition("switch_cd", "switch_cd_block", stone_mat);
    world.interactables.register_definition(def.clone());
    world
        .interactables
        .set_interactable_at(target_coord, def.id.clone());

    let player = PlayerController::new(Vec3::new(0.0, 0.0, 0.0));
    let look_dir = (voxel_center(target_coord) - player.eye_position()).normalize();

    let mut cooldown = InteractionCooldown::new(0.20);
    cooldown.trigger(); // Make cooldown active

    let err = handle_player_generic_interaction(
        &mut world,
        &player,
        look_dir,
        Some(InteractableAction::Activate),
        &mut cooldown,
    )
    .expect_err("active cooldown must block interaction");

    assert!(matches!(err, InteractionError::CooldownActive { .. }));
}

#[test]
fn test_successful_interaction_triggers_cooldown() {
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

    let def = create_test_switch_definition("switch_succ", "switch_succ_block", stone_mat);
    world.interactables.register_definition(def.clone());
    world
        .interactables
        .set_interactable_at(target_coord, def.id.clone());

    let player = PlayerController::new(Vec3::new(0.0, 0.0, 0.0));
    let look_dir = (voxel_center(target_coord) - player.eye_position()).normalize();

    let mut cooldown = InteractionCooldown::new(0.20);
    assert!(cooldown.can_act());

    let res = handle_player_generic_interaction(
        &mut world,
        &player,
        look_dir,
        Some(InteractableAction::Activate),
        &mut cooldown,
    )
    .expect("interaction should succeed");

    assert_eq!(res.new_state, InteractableState::Active);
    // Cooldown is now active
    assert!(!cooldown.can_act());
}

// ============================================================================
// 8. SEMANTIC FEEDBACK DATA
// ============================================================================

#[test]
fn test_interaction_feedback_returns_compact_semantic_data() {
    let feedback = InteractionFeedback::new(
        Some(AudioCue::SwitchToggle),
        Some(VisualCue::Pulse),
        Some(FeedbackId::Activated),
    );

    assert_eq!(feedback.audio_cue, Some(AudioCue::SwitchToggle));
    assert_eq!(feedback.visual_cue, Some(VisualCue::Pulse));
    assert_eq!(feedback.feedback_id, Some(FeedbackId::Activated));

    // Zero lore/strings, purely typed primitives
    assert_eq!(std::mem::size_of::<InteractionFeedback>(), 3);
}

// ============================================================================
// 9. SERDE COMPATIBILITY
// ============================================================================

#[test]
fn test_block_definitions_serde_default_none_interactable() {
    let json_data = r#"{
        "id": "core:plain_stone",
        "material": "core:stone",
        "components": {}
    }"#;

    let def: BlockDefinition = serde_json::from_str(json_data).expect("should deserialize");
    assert_eq!(def.components.interactable, None);
}

// ============================================================================
// 10. MANDATORY TOCTOU / STALE PROPOSAL TESTS
// ============================================================================

/// Test A — Stale proposal after interactable replacement:
/// T0: Interactable A exists, material M, state Idle
/// T1: Query + validate -> proposal for A / M / Idle -> Active
/// T2: Voxel/object at same coordinate is replaced by Interactable B
/// T3: Execute old proposal
/// Expected: Execution fails with InteractableMismatch, state does not mutate, B unaffected, cooldown untouched.
#[test]
fn test_toctou_stale_proposal_after_interactable_replacement() {
    let mut world = World::new();
    let target_coord = IVec3::new(2, 2, 0);
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let wood_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("wood_oak").unwrap())
        .unwrap();
    let iron_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("iron_ore").unwrap())
        .unwrap();

    // T0: Register Interactable A and place it
    let def_a = create_test_switch_definition("lever_a", "lever_block_a", wood_mat);
    let def_b = create_test_switch_definition("lever_b", "lever_block_b", iron_mat);
    world.interactables.register_definition(def_a.clone());
    world.interactables.register_definition(def_b.clone());

    world
        .store
        .set_voxel_world(target_coord, VoxelBlock::new(wood_mat));
    world
        .interactables
        .set_interactable_at(target_coord, def_a.id.clone());

    // T1: Query & validate -> proposal generated for A
    let hit = VoxelHit {
        voxel_coord: target_coord,
        material: wood_mat,
        hit_point: voxel_center(target_coord),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };
    let target = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
    let proposal = validate_interaction(
        &world,
        &target,
        InteractableAction::Activate,
        Vec3::ZERO,
        DEFAULT_INTERACTION_REACH,
    )
    .unwrap();
    assert_eq!(proposal.interactable_id, def_a.id);
    assert_eq!(proposal.expected_material, wood_mat);

    // T2: Replace with Interactable B at same coordinate
    world
        .store
        .set_voxel_world(target_coord, VoxelBlock::new(iron_mat));
    world
        .interactables
        .set_interactable_at(target_coord, def_b.id.clone());

    // T3: Execute old proposal
    let err = execute_interaction(&mut world, &proposal)
        .expect_err("stale proposal must be rejected after replacement");

    assert_eq!(
        err,
        InteractionError::InteractableMismatch {
            expected: def_a.id.clone(),
            actual: Some(def_b.id.clone()),
        }
    );

    // B remains unaffected: no instance created
    assert_eq!(world.interactables.get_instance(&target_coord), None);
}

/// Test B — Stale proposal after same-material definition replacement:
/// T0: Interactable A, material M (wood), state Idle
/// T1: Proposal generated for A
/// T2: Target definition changes to Interactable B, material remains M (wood)
/// T3: Execute old proposal
/// Expected: Execution fails with InteractableMismatch because MaterialId is NOT sufficient identity!
#[test]
fn test_toctou_stale_proposal_after_same_material_definition_replacement() {
    let mut world = World::new();
    let target_coord = IVec3::new(2, 2, 0);
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let wood_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("wood_oak").unwrap())
        .unwrap();

    // Two distinct interactables sharing the EXACT SAME material (wood)
    let def_a = create_test_switch_definition("wooden_switch_a", "switch_block_a", wood_mat);
    let def_b = create_test_switch_definition("wooden_switch_b", "switch_block_b", wood_mat);
    world.interactables.register_definition(def_a.clone());
    world.interactables.register_definition(def_b.clone());

    // T0: Wooden switch A placed
    world
        .store
        .set_voxel_world(target_coord, VoxelBlock::new(wood_mat));
    world
        .interactables
        .set_interactable_at(target_coord, def_a.id.clone());

    // T1: Proposal generated for switch A
    let hit = VoxelHit {
        voxel_coord: target_coord,
        material: wood_mat,
        hit_point: voxel_center(target_coord),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };
    let target = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
    let proposal = validate_interaction(
        &world,
        &target,
        InteractableAction::Activate,
        Vec3::ZERO,
        DEFAULT_INTERACTION_REACH,
    )
    .unwrap();
    assert_eq!(proposal.interactable_id, def_a.id);
    assert_eq!(proposal.expected_material, wood_mat);

    // T2: Target definition changes to B (material remains wood_mat!)
    world
        .interactables
        .set_interactable_at(target_coord, def_b.id.clone());

    // T3: Execute old proposal
    let err = execute_interaction(&mut world, &proposal).expect_err(
        "stale proposal must fail even if material matches (MaterialId is not sufficient identity)",
    );

    assert_eq!(
        err,
        InteractionError::InteractableMismatch {
            expected: def_a.id.clone(),
            actual: Some(def_b.id.clone()),
        }
    );

    // Invariant: B's state was NOT mutated
    assert_eq!(world.interactables.get_instance(&target_coord), None);
}

/// Test C — Stale proposal after state changed:
/// T0: Interactable A, material M, state Idle
/// T1: Proposal generated: Idle -> Active
/// T2: Target state becomes Active through another valid interaction
/// T3: Execute old Idle -> Active proposal
/// Expected: Execution fails with StateMismatch, state remains Active
#[test]
fn test_toctou_stale_proposal_after_state_changed() {
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

    let def = create_test_switch_definition("switch_state", "switch_block", stone_mat);
    world.interactables.register_definition(def.clone());
    world
        .interactables
        .set_interactable_at(target_coord, def.id.clone());

    // T0: Idle
    let hit = VoxelHit {
        voxel_coord: target_coord,
        material: stone_mat,
        hit_point: voxel_center(target_coord),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };

    // T1: Generate Idle -> Active proposal
    let target = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
    let old_proposal = validate_interaction(
        &world,
        &target,
        InteractableAction::Activate,
        Vec3::ZERO,
        DEFAULT_INTERACTION_REACH,
    )
    .unwrap();
    assert_eq!(old_proposal.previous_state, InteractableState::Idle);
    assert_eq!(old_proposal.target_state, InteractableState::Active);

    // T2: Another interaction advances the state to Active
    world.interactables.set_instance_state(
        target_coord,
        def.id.clone(),
        stone_mat,
        InteractableState::Active,
    );

    // T3: Execute old Idle -> Active proposal
    let err = execute_interaction(&mut world, &old_proposal)
        .expect_err("stale proposal with outdated previous_state must fail");

    assert_eq!(
        err,
        InteractionError::StateMismatch {
            expected: InteractableState::Idle,
            actual: InteractableState::Active,
        }
    );

    // State remains Active, not overwritten or repaired
    let current_inst = world.interactables.get_instance(&target_coord).unwrap();
    assert_eq!(current_inst.state, InteractableState::Active);
}

/// Test D — Failed stale execution leaves cooldown untouched:
/// Stale proposal -> execution failure -> cooldown remains available.
/// Subsequent valid interaction can still consume the cooldown normally.
#[test]
fn test_toctou_failed_stale_execution_leaves_cooldown_untouched() {
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

    let def_a = create_test_switch_definition("switch_cd_a", "switch_cd_a_block", stone_mat);
    let def_b = create_test_switch_definition("switch_cd_b", "switch_cd_b_block", stone_mat);
    world.interactables.register_definition(def_a.clone());
    world.interactables.register_definition(def_b.clone());

    world
        .interactables
        .set_interactable_at(target_coord, def_a.id.clone());

    let hit = VoxelHit {
        voxel_coord: target_coord,
        material: stone_mat,
        hit_point: voxel_center(target_coord),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };
    let target = detect_interactable_target(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
    let stale_proposal = validate_interaction(
        &world,
        &target,
        InteractableAction::Activate,
        Vec3::ZERO,
        DEFAULT_INTERACTION_REACH,
    )
    .unwrap();

    // Replace A with B
    world
        .interactables
        .set_interactable_at(target_coord, def_b.id.clone());

    let mut cooldown = InteractionCooldown::new(0.20);
    assert!(cooldown.can_act());

    // Executing stale proposal fails
    let err = execute_interaction(&mut world, &stale_proposal);
    assert!(err.is_err());

    // Invariant: Failed stale execution did NOT consume cooldown
    assert!(
        cooldown.can_act(),
        "cooldown must remain available after stale execution failure"
    );

    // Now perform a valid interaction with B
    let player = PlayerController::new(Vec3::new(0.0, 0.0, 0.0));
    let look_dir = (voxel_center(target_coord) - player.eye_position()).normalize();

    let valid_result = handle_player_generic_interaction(
        &mut world,
        &player,
        look_dir,
        Some(InteractableAction::Activate),
        &mut cooldown,
    )
    .expect("valid interaction with B should succeed");

    assert_eq!(valid_result.interactable_id, def_b.id);
    assert_eq!(valid_result.new_state, InteractableState::Active);

    // And now cooldown IS consumed strictly post-commit!
    assert!(!cooldown.can_act());
}
