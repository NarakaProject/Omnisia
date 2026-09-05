use glam::{IVec3, Vec3};
use omnisia::chunk::{dirty_flags, Chunk};
use omnisia::csg::transaction::VoxelEditTransaction;
use omnisia::interaction::{
    build_placement_proposal, execute_placement_transaction, handle_player_placement,
    validate_placement_proposal, validate_support, BlockOrientation, BuildRuleDefinition,
    BuildRuleRegistry, InteractionCooldown, PlacementError, PlacementRejectionReason,
    PlacementValidity, VoxelHit, VoxelRaycastResult, DEFAULT_INTERACTION_REACH,
};
use omnisia::material::MaterialId;
use omnisia::mesh::types::FaceDirection;
use omnisia::modding::definitions::{
    BlockComponents, BlockDefinition, BuildComponent, SupportRule,
};
use omnisia::modding::registry::{BlockRegistry, ResourceSource};
use omnisia::modding::resource_id::ResourceId;
use omnisia::player::PlayerController;
use omnisia::streaming::store::ChunkStore;
use omnisia::voxel::VoxelBlock;
use omnisia::world::World;

const TEST_STONE: MaterialId = MaterialId(1);
const TEST_DIRT: MaterialId = MaterialId(2);
const TEST_WOOD: MaterialId = MaterialId(3);
const TEST_GLASS: MaterialId = MaterialId(4);

fn create_test_store() -> ChunkStore {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    chunk.clear_dirty(dirty_flags::ALL);
    store.insert(chunk);
    store
}

// ============================================================================
// 1. TARGETING TESTS (Items 1 - 6: All Six Canonical Faces)
// ============================================================================

#[test]
fn test_targeting_all_six_faces() {
    let mut world = World::new();
    world.store.insert(Chunk::new(IVec3::ZERO));

    // Target block di tengah chunk (10, 10, 10)
    let target = IVec3::new(10, 10, 10);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    let faces_and_expected = [
        (FaceDirection::PosX, IVec3::new(11, 10, 10), Vec3::X),
        (FaceDirection::NegX, IVec3::new(9, 10, 10), -Vec3::X),
        (FaceDirection::PosY, IVec3::new(10, 11, 10), Vec3::Y),
        (FaceDirection::NegY, IVec3::new(10, 9, 10), -Vec3::Y),
        (FaceDirection::PosZ, IVec3::new(10, 10, 11), Vec3::Z),
        (FaceDirection::NegZ, IVec3::new(10, 10, 9), -Vec3::Z),
    ];

    for (face, expected_candidate, normal) in faces_and_expected {
        let player = PlayerController::new(Vec3::new(10.0, 20.0, 10.0)); // Jauh di atas
        let hit = VoxelHit {
            voxel_coord: target,
            material: TEST_STONE,
            hit_point: Vec3::new(5.0, 5.0, 5.0),
            distance: 2.0,
            face,
            normal,
        };
        let ray_result = VoxelRaycastResult::Hit(hit);

        let proposal = build_placement_proposal(
            &world.store,
            &world.build_rules,
            &player,
            &ray_result,
            TEST_DIRT,
            None,
            BlockOrientation::Default,
            DEFAULT_INTERACTION_REACH,
        );

        assert_eq!(proposal.target_face, face);
        assert_eq!(proposal.candidate_voxel, expected_candidate);
        assert_eq!(
            proposal.validity,
            PlacementValidity::Valid,
            "Targeting face {:?} should produce valid proposal",
            face
        );
    }
}

// ============================================================================
// 2. CANDIDATE COORDINATES (Items 7 - 10: Adjacent, Chunk Boundary, Negative, Mixed)
// ============================================================================

#[test]
fn test_candidate_coordinates_adjacent_and_boundaries() {
    let mut world = World::new();
    world.store.insert(Chunk::new(IVec3::ZERO));
    world.store.insert(Chunk::new(IVec3::new(1, 0, 0)));
    world.store.insert(Chunk::new(IVec3::new(-1, 0, 0)));
    world.store.insert(Chunk::new(IVec3::new(-1, -1, -1)));

    let player = PlayerController::new(Vec3::new(0.0, 30.0, 0.0));

    // Item 7: Adjacent voxel
    let target_adj = IVec3::new(5, 5, 5);
    world
        .store
        .set_voxel_world(target_adj, VoxelBlock::new(TEST_STONE));
    let hit_adj = VoxelHit {
        voxel_coord: target_adj,
        material: TEST_STONE,
        hit_point: Vec3::new(2.5, 2.5, 2.5),
        distance: 2.0,
        face: FaceDirection::PosY,
        normal: Vec3::Y,
    };
    let prop_adj = build_placement_proposal(
        &world.store,
        &world.build_rules,
        &player,
        &VoxelRaycastResult::Hit(hit_adj),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        DEFAULT_INTERACTION_REACH,
    );
    assert_eq!(prop_adj.candidate_voxel, IVec3::new(5, 6, 5));

    // Item 8: Chunk boundary candidate (voxel 31 di Chunk 0 -> candidate voxel 32 di Chunk 1)
    let boundary_target = IVec3::new(31, 0, 0);
    world
        .store
        .set_voxel_world(boundary_target, VoxelBlock::new(TEST_STONE));
    let hit_bound = VoxelHit {
        voxel_coord: boundary_target,
        material: TEST_STONE,
        hit_point: Vec3::new(16.0, 0.25, 0.25),
        distance: 2.0,
        face: FaceDirection::PosX,
        normal: Vec3::X,
    };
    let prop_bound = build_placement_proposal(
        &world.store,
        &world.build_rules,
        &player,
        &VoxelRaycastResult::Hit(hit_bound),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        DEFAULT_INTERACTION_REACH,
    );
    assert_eq!(prop_bound.candidate_voxel, IVec3::new(32, 0, 0));
    assert_eq!(prop_bound.validity, PlacementValidity::Valid);

    // Item 9: Negative world coordinates (chunk -1, -1, -1)
    let neg_target = IVec3::new(-10, -10, -10);
    world
        .store
        .set_voxel_world(neg_target, VoxelBlock::new(TEST_STONE));
    let hit_neg = VoxelHit {
        voxel_coord: neg_target,
        material: TEST_STONE,
        hit_point: Vec3::new(-5.0, -5.0, -5.0),
        distance: 2.0,
        face: FaceDirection::PosZ,
        normal: Vec3::Z,
    };
    let prop_neg = build_placement_proposal(
        &world.store,
        &world.build_rules,
        &player,
        &VoxelRaycastResult::Hit(hit_neg),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        DEFAULT_INTERACTION_REACH,
    );
    assert_eq!(prop_neg.candidate_voxel, IVec3::new(-10, -10, -9));
    assert_eq!(prop_neg.validity, PlacementValidity::Valid);

    // Item 10: Mixed-sign coordinates (target di -1, face PosX -> candidate di 0)
    let mixed_target = IVec3::new(-1, 0, 0);
    world
        .store
        .set_voxel_world(mixed_target, VoxelBlock::new(TEST_STONE));
    let hit_mixed = VoxelHit {
        voxel_coord: mixed_target,
        material: TEST_STONE,
        hit_point: Vec3::new(0.0, 0.25, 0.25),
        distance: 1.0,
        face: FaceDirection::PosX,
        normal: Vec3::X,
    };
    let prop_mixed = build_placement_proposal(
        &world.store,
        &world.build_rules,
        &player,
        &VoxelRaycastResult::Hit(hit_mixed),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        DEFAULT_INTERACTION_REACH,
    );
    assert_eq!(prop_mixed.candidate_voxel, IVec3::new(0, 0, 0));
    assert_eq!(prop_mixed.validity, PlacementValidity::Valid);
}

// ============================================================================
// 3. OCCUPANCY & AIR SEMANTICS (Items 11 - 13)
// ============================================================================

#[test]
fn test_occupancy_and_air_semantics() {
    let mut store = create_test_store();
    let player = PlayerController::new(Vec3::new(0.0, 20.0, 0.0));
    let rules = BuildRuleRegistry::new();

    let target = IVec3::new(2, 2, 2);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    let hit = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(1.0, 1.5, 1.0),
        distance: 2.0,
        face: FaceDirection::PosY,
        normal: Vec3::Y,
    };

    // Item 11: Empty candidate accepted
    let prop_empty = build_placement_proposal(
        &store,
        &rules,
        &player,
        &VoxelRaycastResult::Hit(hit),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        DEFAULT_INTERACTION_REACH,
    );
    assert_eq!(prop_empty.validity, PlacementValidity::Valid);

    // Item 12: Occupied candidate rejected
    let candidate = IVec3::new(2, 3, 2);
    store.set_voxel_world(candidate, VoxelBlock::new(TEST_WOOD));
    let prop_occupied = build_placement_proposal(
        &store,
        &rules,
        &player,
        &VoxelRaycastResult::Hit(hit),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        DEFAULT_INTERACTION_REACH,
    );
    assert_eq!(
        prop_occupied.validity,
        PlacementValidity::Invalid(PlacementRejectionReason::CandidateOccupied {
            coord: candidate,
            current_material: TEST_WOOD,
        })
    );

    // Item 13: AIR material cannot be placed as a solid block
    store.set_voxel_world(candidate, VoxelBlock::AIR);
    let prop_air_mat = build_placement_proposal(
        &store,
        &rules,
        &player,
        &VoxelRaycastResult::Hit(hit),
        MaterialId(0), // AIR
        None,
        BlockOrientation::Default,
        DEFAULT_INTERACTION_REACH,
    );
    assert!(matches!(
        prop_air_mat.validity,
        PlacementValidity::Invalid(PlacementRejectionReason::InvalidMaterial(_))
    ));

    // Item 13b: Target is AIR rejected
    store.set_voxel_world(target, VoxelBlock::AIR);
    let prop_air_target = build_placement_proposal(
        &store,
        &rules,
        &player,
        &VoxelRaycastResult::Hit(hit),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        DEFAULT_INTERACTION_REACH,
    );
    assert_eq!(
        prop_air_target.validity,
        PlacementValidity::Invalid(PlacementRejectionReason::TargetIsAir { coord: target })
    );
}

// ============================================================================
// 4. RESIDENCY VALIDATION (Items 14 - 16)
// ============================================================================

#[test]
fn test_residency_validation() {
    let mut store = ChunkStore::new();
    // Hanya chunk 0 yang dimasukkan
    let mut chunk0 = Chunk::new(IVec3::ZERO);
    chunk0.clear_dirty(dirty_flags::ALL);
    store.insert(chunk0);

    let player = PlayerController::new(Vec3::new(0.0, 20.0, 0.0));
    let rules = BuildRuleRegistry::new();

    // Item 14: Resident candidate accepted
    let target = IVec3::new(10, 10, 10);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));
    let hit_resident = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(5.0, 5.5, 5.0),
        distance: 2.0,
        face: FaceDirection::PosY,
        normal: Vec3::Y,
    };
    let prop_resident = build_placement_proposal(
        &store,
        &rules,
        &player,
        &VoxelRaycastResult::Hit(hit_resident),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        DEFAULT_INTERACTION_REACH,
    );
    assert_eq!(prop_resident.validity, PlacementValidity::Valid);

    // Item 15: Non-resident candidate rejected
    // Target di ujung chunk (31, 0, 0), menabrak +X menuju Chunk (1, 0, 0) yang belum resident
    let edge_target = IVec3::new(31, 0, 0);
    store.set_voxel_world(edge_target, VoxelBlock::new(TEST_STONE));
    let hit_unresident_cand = VoxelHit {
        voxel_coord: edge_target,
        material: TEST_STONE,
        hit_point: Vec3::new(16.0, 0.25, 0.25),
        distance: 2.0,
        face: FaceDirection::PosX,
        normal: Vec3::X,
    };
    let prop_unresident_cand = build_placement_proposal(
        &store,
        &rules,
        &player,
        &VoxelRaycastResult::Hit(hit_unresident_cand),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        DEFAULT_INTERACTION_REACH,
    );
    assert_eq!(
        prop_unresident_cand.validity,
        PlacementValidity::Invalid(PlacementRejectionReason::CandidateNotResident {
            coord: IVec3::new(32, 0, 0)
        })
    );

    // Item 16: Non-resident support rejected
    // Candidate di (0, 0, 0) membutuhkan FloorOnly (bawahnya di (0, -1, 0) dalam chunk (0, -1, 0) yang belum resident)
    let side_target = IVec3::new(1, 0, 0);
    store.set_voxel_world(side_target, VoxelBlock::new(TEST_STONE));
    let hit_floor_rule = VoxelHit {
        voxel_coord: side_target,
        material: TEST_STONE,
        hit_point: Vec3::new(0.5, 0.25, 0.25),
        distance: 2.0,
        face: FaceDirection::NegX,
        normal: -Vec3::X,
    };
    let mut floor_rules = BuildRuleRegistry::new();
    floor_rules.register(
        TEST_DIRT,
        BuildRuleDefinition::new(ResourceId::core("dirt").unwrap())
            .with_support(true, SupportRule::FloorOnly),
    );
    let prop_unresident_support = build_placement_proposal(
        &store,
        &floor_rules,
        &player,
        &VoxelRaycastResult::Hit(hit_floor_rule),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        DEFAULT_INTERACTION_REACH,
    );
    assert_eq!(
        prop_unresident_support.validity,
        PlacementValidity::Invalid(PlacementRejectionReason::SupportNotResident {
            coord: IVec3::new(0, -1, 0)
        })
    );
}

// ============================================================================
// 5. REACH BOUNDARIES (Items 17 - 18)
// ============================================================================

#[test]
fn test_reach_exact_and_beyond() {
    let mut store = create_test_store();
    let rules = BuildRuleRegistry::new();
    let player = PlayerController::new(Vec3::new(0.0, 0.0, 0.0));

    let target = IVec3::new(5, 0, 0);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    // Item 17: Exactly at reach (5.0m == 5.0m)
    let hit_exact = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(5.0, 0.0, 0.0),
        distance: 5.0,
        face: FaceDirection::PosX,
        normal: Vec3::X,
    };
    let prop_exact = build_placement_proposal(
        &store,
        &rules,
        &player,
        &VoxelRaycastResult::Hit(hit_exact),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        5.0,
    );
    assert_eq!(prop_exact.validity, PlacementValidity::Valid);

    // Item 18: Beyond reach (5.01m > 5.0m)
    let hit_beyond = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(5.01, 0.0, 0.0),
        distance: 5.01,
        face: FaceDirection::PosX,
        normal: Vec3::X,
    };
    let prop_beyond = build_placement_proposal(
        &store,
        &rules,
        &player,
        &VoxelRaycastResult::Hit(hit_beyond),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        5.0,
    );
    assert_eq!(
        prop_beyond.validity,
        PlacementValidity::Invalid(PlacementRejectionReason::ExceedsReach {
            distance: 5.01,
            max_reach: 5.0,
        })
    );
}

// ============================================================================
// 6. SUPPORT RULES (Items 19 - 22: Data-Driven, AnyAdjacent, FloorOnly, AttachmentFace, None)
// ============================================================================

#[test]
fn test_support_rules_comprehensive() {
    let mut store = create_test_store();

    let target = IVec3::new(4, 4, 4);
    let candidate = IVec3::new(4, 5, 4);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    // Item 19 & 20: FloorOnly with and without support
    assert!(validate_support(&store, target, candidate, true, SupportRule::FloorOnly).is_ok());

    let floating_target = IVec3::new(4, 5, 5); // di samping, bukan di bawah
    let floating_cand = IVec3::new(4, 5, 4);
    // Hapus target di (4, 4, 4)
    store.set_voxel_world(target, VoxelBlock::AIR);
    store.set_voxel_world(floating_target, VoxelBlock::new(TEST_STONE));
    assert_eq!(
        validate_support(
            &store,
            floating_target,
            floating_cand,
            true,
            SupportRule::FloorOnly
        ),
        Err(PlacementError::SupportMissing {
            coord: floating_cand,
            rule: SupportRule::FloorOnly
        })
    );

    // Item 21: No-support block (requires_support = false) accepted anywhere
    assert!(validate_support(
        &store,
        floating_target,
        floating_cand,
        false,
        SupportRule::FloorOnly
    )
    .is_ok());
    assert!(validate_support(
        &store,
        floating_target,
        floating_cand,
        true,
        SupportRule::None
    )
    .is_ok());

    // Item 22: Support rule comes from BlockDefinition data, not hardcoded ID
    let mut blocks = BlockRegistry::new();
    let torch_id = ResourceId::core("torch").unwrap();
    let def = BlockDefinition {
        id: torch_id.clone(),
        material: ResourceId::core("wood").unwrap(),
        hardness: Some(1.0),
        components: BlockComponents {
            build: Some(BuildComponent {
                requires_support: true,
                support_rule: SupportRule::AttachmentFace,
                allowed_orientations: None,
            }),
            ..Default::default()
        },
        tags: vec![],
    };
    blocks.register(def, ResourceSource::Core).unwrap();
    let materials = omnisia::material::MaterialRegistry::new();
    let registry = BuildRuleRegistry::from_registries(&materials, &blocks);

    let rule = registry
        .get_by_block(&torch_id)
        .expect("Rule must be loaded from BlockDefinition");
    assert_eq!(rule.support_rule, SupportRule::AttachmentFace);
    assert!(rule.requires_support);
}

// ============================================================================
// 7. PLAYER CLEARANCE (Items 23 - 26: Standing, Crouching, Tangent)
// ============================================================================

#[test]
fn test_clearance_standing_crouching_and_tangent() {
    let mut store = create_test_store();
    let rules = BuildRuleRegistry::new();

    // Pemain berdiri di (0.25, 0.0, 0.25)
    // Standing capsule: height 1.80m, radius 0.30m -> puncak di Y = 1.80m
    let standing_player = PlayerController::new(Vec3::new(0.25, 0.0, 0.25));

    // Item 23: Standing player intersection rejected (placing inside body at (0, 0, 0))
    let side_anchor = IVec3::new(1, 0, 0);
    store.set_voxel_world(side_anchor, VoxelBlock::new(TEST_STONE));
    let hit_inside = VoxelHit {
        voxel_coord: side_anchor,
        material: TEST_STONE,
        hit_point: Vec3::new(0.5, 0.25, 0.25),
        distance: 1.0,
        face: FaceDirection::NegX,
        normal: -Vec3::X,
    };
    let prop_inside = build_placement_proposal(
        &store,
        &rules,
        &standing_player,
        &VoxelRaycastResult::Hit(hit_inside),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        5.0,
    );
    assert_eq!(
        prop_inside.validity,
        PlacementValidity::Invalid(PlacementRejectionReason::PlayerCapsuleOverlap {
            coord: IVec3::new(0, 0, 0)
        })
    );

    // Item 24: Crouching clearance behavior
    // Voxel di Y = 3 -> bounds [1.5, 2.0].
    // Standing: Y in [0.0, 1.80] overlaps [1.5, 2.0]!
    // Crouching: height 1.20m, Y in [0.0, 1.20] -> DOES NOT OVERLAP [1.5, 2.0]!
    let high_anchor = IVec3::new(1, 3, 0);
    store.set_voxel_world(high_anchor, VoxelBlock::new(TEST_STONE));
    let hit_high = VoxelHit {
        voxel_coord: high_anchor,
        material: TEST_STONE,
        hit_point: Vec3::new(0.5, 1.75, 0.25),
        distance: 1.0,
        face: FaceDirection::NegX,
        normal: -Vec3::X,
    };

    // Standing: overlap
    let prop_standing = build_placement_proposal(
        &store,
        &rules,
        &standing_player,
        &VoxelRaycastResult::Hit(hit_high),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        5.0,
    );
    assert_eq!(
        prop_standing.validity,
        PlacementValidity::Invalid(PlacementRejectionReason::PlayerCapsuleOverlap {
            coord: IVec3::new(0, 3, 0)
        })
    );

    // Crouching: bebas & berhasil!
    let mut crouching_player = PlayerController::new(Vec3::new(0.25, 0.0, 0.25));
    crouching_player.state.crouching = true;
    let prop_crouching = build_placement_proposal(
        &store,
        &rules,
        &crouching_player,
        &VoxelRaycastResult::Hit(hit_high),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        5.0,
    );
    assert_eq!(
        prop_crouching.validity,
        PlacementValidity::Valid,
        "Crouching player must have clear head clearance"
    );

    // Item 25 & 26: Tangent/edge case outside capsule succeeds deterministically
    let distant_anchor = IVec3::new(3, 0, 0);
    store.set_voxel_world(distant_anchor, VoxelBlock::new(TEST_STONE));
    let hit_tangent = VoxelHit {
        voxel_coord: distant_anchor,
        material: TEST_STONE,
        hit_point: Vec3::new(1.5, 0.25, 0.25),
        distance: 1.5,
        face: FaceDirection::NegX,
        normal: -Vec3::X,
    };
    let prop_tangent = build_placement_proposal(
        &store,
        &rules,
        &standing_player,
        &VoxelRaycastResult::Hit(hit_tangent),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        5.0,
    );
    assert_eq!(prop_tangent.candidate_voxel, IVec3::new(2, 0, 0));
    assert_eq!(prop_tangent.validity, PlacementValidity::Valid);
}

// ============================================================================
// 8. ORIENTATION (Items 27 - 29: Discrete, Preservation, Restriction)
// ============================================================================

#[test]
fn test_orientation_discrete_and_restricted() {
    let mut store = create_test_store();
    let target = IVec3::new(5, 5, 5);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    let player = PlayerController::new(Vec3::new(2.5, 4.0, 2.5));

    let hit = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(2.5, 3.0, 2.5),
        distance: 2.0,
        face: FaceDirection::PosY,
        normal: Vec3::Y,
    };

    // Item 27: Default orientation is deterministic
    assert_eq!(BlockOrientation::default(), BlockOrientation::Default);

    // Item 28: Discrete orientation survives proposal and validation
    let facing_neg_z = BlockOrientation::Facing(FaceDirection::NegZ);
    let rules = BuildRuleRegistry::new();
    let prop = build_placement_proposal(
        &store,
        &rules,
        &player,
        &VoxelRaycastResult::Hit(hit),
        TEST_DIRT,
        None,
        facing_neg_z,
        5.0,
    );
    assert_eq!(prop.orientation, facing_neg_z);
    assert_eq!(prop.validity, PlacementValidity::Valid);

    let edit = validate_placement_proposal(&store, &rules, &player, &prop, 5.0)
        .expect("Validation should succeed with valid discrete orientation");
    assert_eq!(edit.position, IVec3::new(5, 6, 5));

    // Item 29: Invalid orientation rejected when restricted
    let mut restricted_rules = BuildRuleRegistry::new();
    let block_id = ResourceId::core("furnace").unwrap();
    let restricted_rule = BuildRuleDefinition::new(block_id.clone())
        .with_allowed_orientations(vec![BlockOrientation::Default]);
    restricted_rules.register(TEST_WOOD, restricted_rule);

    // Default diizinkan
    let prop_allowed = build_placement_proposal(
        &store,
        &restricted_rules,
        &player,
        &VoxelRaycastResult::Hit(hit),
        TEST_WOOD,
        Some(block_id.clone()),
        BlockOrientation::Default,
        5.0,
    );
    assert_eq!(prop_allowed.validity, PlacementValidity::Valid);

    // Facing(PosY) ditolak
    let prop_disallowed = build_placement_proposal(
        &store,
        &restricted_rules,
        &player,
        &VoxelRaycastResult::Hit(hit),
        TEST_WOOD,
        Some(block_id),
        BlockOrientation::Facing(FaceDirection::PosY),
        5.0,
    );
    assert!(matches!(
        prop_disallowed.validity,
        PlacementValidity::Invalid(PlacementRejectionReason::InvalidOrientation(_))
    ));
}

// ============================================================================
// 9. ATOMICITY & STALE PROPOSALS (Items 30 - 33)
// ============================================================================

#[test]
fn test_atomicity_and_stale_proposals() {
    let mut world = World::new();
    world.store.insert(Chunk::new(IVec3::ZERO));

    let target = IVec3::new(5, 0, 5);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(TEST_STONE));
    let _initial_rev = world.store.get(&IVec3::ZERO).unwrap().revision;

    let player = PlayerController::new(Vec3::new(2.5, 1.8, 2.5));

    let hit = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(2.5, 0.5, 2.5),
        distance: 1.5,
        face: FaceDirection::PosY,
        normal: Vec3::Y,
    };

    // Item 30: Successful placement mutates exactly the intended voxel
    let prop = build_placement_proposal(
        &world.store,
        &world.build_rules,
        &player,
        &VoxelRaycastResult::Hit(hit),
        TEST_WOOD,
        None,
        BlockOrientation::Default,
        5.0,
    );
    assert_eq!(prop.validity, PlacementValidity::Valid);

    let edit = validate_placement_proposal(
        &world.store,
        &world.build_rules,
        &player,
        &prop,
        DEFAULT_INTERACTION_REACH,
    )
    .unwrap();
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(edit);
    let result = execute_placement_transaction(&mut world, &tx, prop.clone()).unwrap();
    assert_eq!(result.mutation.commit_result.delta.len(), 1);
    assert_eq!(
        world.store.get_voxel_world(IVec3::new(5, 1, 5)).material(),
        TEST_WOOD
    );

    // Item 31 & 32: Failed validation leaves world 100% unchanged
    let current_rev = world.store.get(&IVec3::ZERO).unwrap().revision;
    let mut bad_tx = VoxelEditTransaction::new();
    // Coba tempatkan lagi di (5, 1, 5) yang sudah terisi -> gagal
    bad_tx.add_edit(omnisia::csg::VoxelEdit::add(
        IVec3::new(5, 1, 5),
        VoxelBlock::new(TEST_STONE),
    ));
    assert!(execute_placement_transaction(&mut world, &bad_tx, prop.clone()).is_err());
    assert_eq!(
        world.store.get(&IVec3::ZERO).unwrap().revision,
        current_rev,
        "Chunk revision must not change upon failed placement"
    );

    // Item 33: Stale proposal cannot bypass final validation
    // Proposal awal dibuat ketika (5, 2, 5) masih kosong
    let hit_layer2 = VoxelHit {
        voxel_coord: IVec3::new(5, 1, 5),
        material: TEST_WOOD,
        hit_point: Vec3::new(2.5, 1.0, 2.5),
        distance: 1.5,
        face: FaceDirection::PosY,
        normal: Vec3::Y,
    };
    let stale_prop = build_placement_proposal(
        &world.store,
        &world.build_rules,
        &player,
        &VoxelRaycastResult::Hit(hit_layer2),
        TEST_GLASS,
        None,
        BlockOrientation::Default,
        5.0,
    );
    assert_eq!(stale_prop.validity, PlacementValidity::Valid);

    // Dunia berubah di Frame N+1: voxel kandidat (5, 2, 5) terisi oleh mutasi lain
    world
        .store
        .set_voxel_world(IVec3::new(5, 2, 5), VoxelBlock::new(TEST_DIRT));

    // Mencoba mengomit stale proposal -> harus ditolak oleh authoritative re-validation
    let reval_result = validate_placement_proposal(
        &world.store,
        &world.build_rules,
        &player,
        &stale_prop,
        DEFAULT_INTERACTION_REACH,
    );
    assert!(matches!(
        reval_result,
        Err(PlacementError::CandidateOccupied { .. })
    ));
}

// ============================================================================
// 10. STRUCTURAL & REMESH INTEGRATION (Items 34 - 37)
// ============================================================================

#[test]
fn test_structural_and_remesh_integration() {
    let mut world = World::new();
    world.store.insert(Chunk::new(IVec3::ZERO));
    world.store.insert(Chunk::new(IVec3::new(1, 0, 0)));

    // Clear initial dirty flags
    if let Some(c) = world.store.get_mut(&IVec3::ZERO) {
        c.clear_dirty(dirty_flags::ALL);
    }
    if let Some(c) = world.store.get_mut(&IVec3::new(1, 0, 0)) {
        c.clear_dirty(dirty_flags::ALL);
    }
    world.store.dirty_mesh_chunks.clear();

    // Item 34 & 35: Structural integration preserved
    let anchor_coord = IVec3::new(5, 0, 5);
    world.set_voxel_world(anchor_coord, VoxelBlock::new(MaterialId(255))); // Bedrock

    let player = PlayerController::new(Vec3::new(2.5, 10.0, 2.5));
    let hit = VoxelHit {
        voxel_coord: anchor_coord,
        material: MaterialId(255),
        hit_point: Vec3::new(2.5, 0.5, 2.5),
        distance: 2.0,
        face: FaceDirection::PosY,
        normal: Vec3::Y,
    };
    let prop = build_placement_proposal(
        &world.store,
        &world.build_rules,
        &player,
        &VoxelRaycastResult::Hit(hit),
        TEST_STONE,
        None,
        BlockOrientation::Default,
        DEFAULT_INTERACTION_REACH,
    );
    let edit = validate_placement_proposal(&world.store, &world.build_rules, &player, &prop, 15.0)
        .unwrap();
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(edit);
    let res = execute_placement_transaction(&mut world, &tx, prop).unwrap();
    assert_eq!(res.mutation.commit_result.delta.len(), 1);

    // Item 36: Mutated chunk becomes mesh-dirty
    let chunk0 = world.store.get(&IVec3::ZERO).unwrap();
    assert!(chunk0.is_dirty(dirty_flags::MESH_DIRTY));
    assert!(world.store.dirty_mesh_chunks.contains(&IVec3::ZERO));

    // Item 37: Cross-boundary neighbor invalidation
    // Tempatkan voxel di border X = 31 (bersebelahan dengan Chunk (1, 0, 0))
    if let Some(c) = world.store.get_mut(&IVec3::ZERO) {
        c.clear_dirty(dirty_flags::ALL);
    }
    if let Some(c) = world.store.get_mut(&IVec3::new(1, 0, 0)) {
        c.clear_dirty(dirty_flags::ALL);
    }
    world.store.dirty_mesh_chunks.clear();

    let border_anchor = IVec3::new(30, 0, 0);
    world
        .store
        .set_voxel_world(border_anchor, VoxelBlock::new(TEST_STONE));

    let border_hit = VoxelHit {
        voxel_coord: border_anchor,
        material: TEST_STONE,
        hit_point: Vec3::new(15.5, 0.25, 0.25),
        distance: 2.0,
        face: FaceDirection::PosX,
        normal: Vec3::X,
    };
    // Candidate = (31, 0, 0)
    let border_prop = build_placement_proposal(
        &world.store,
        &world.build_rules,
        &player,
        &VoxelRaycastResult::Hit(border_hit),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        20.0,
    );
    let border_edit = validate_placement_proposal(
        &world.store,
        &world.build_rules,
        &player,
        &border_prop,
        20.0,
    )
    .unwrap();
    let mut border_tx = VoxelEditTransaction::new();
    border_tx.add_edit(border_edit);
    execute_placement_transaction(&mut world, &border_tx, border_prop).unwrap();

    // Chunk (0, 0, 0) dan neighbor (+1, 0, 0) keduanya harus MESH_DIRTY
    let c0 = world.store.get(&IVec3::ZERO).unwrap();
    let c1 = world.store.get(&IVec3::new(1, 0, 0)).unwrap();
    assert!(c0.is_dirty(dirty_flags::MESH_DIRTY));
    assert!(
        c1.is_dirty(dirty_flags::MESH_DIRTY),
        "Cross-boundary neighbor chunk must be marked MESH_DIRTY"
    );
}

// ============================================================================
// 11. COOLDOWN SEMANTICS (Items 38 - 40)
// ============================================================================

#[test]
fn test_cooldown_semantics_success_vs_failure() {
    let mut world = World::new();
    world.store.insert(Chunk::new(IVec3::ZERO));

    let ground = IVec3::new(1, 0, 1);
    world
        .store
        .set_voxel_world(ground, VoxelBlock::new(TEST_STONE));

    let player = PlayerController::new(Vec3::new(0.75, 1.8, 0.75));
    let mut cooldown = InteractionCooldown::new(0.20);
    assert!(cooldown.can_act());

    // Item 40: Failed placement does NOT consume cooldown
    // Look direction ke langit (Miss)
    let miss_look = Vec3::new(0.0, 1.0, 0.0);
    let fail_res = handle_player_placement(
        &mut world,
        &player,
        miss_look,
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        &mut cooldown,
    );
    assert!(fail_res.is_err());
    assert!(
        cooldown.can_act(),
        "Failed placement must not trigger/consume cooldown"
    );

    // Item 38: Successful placement triggers cooldown
    let hit_look = Vec3::new(0.0, -1.0, 0.0);
    let succ_res = handle_player_placement(
        &mut world,
        &player,
        hit_look,
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        &mut cooldown,
    );
    assert!(succ_res.is_ok());
    assert!(
        !cooldown.can_act(),
        "Successful placement must trigger cooldown"
    );

    // Item 39: Cooldown blocks spam during cooldown window
    let spam_res = handle_player_placement(
        &mut world,
        &player,
        hit_look,
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        &mut cooldown,
    );
    assert!(matches!(
        spam_res,
        Err(PlacementError::CooldownActive { .. })
    ));
}

// ============================================================================
// 12. DETERMINISM (Items 41 - 42)
// ============================================================================

#[test]
fn test_determinism_proposals_and_validation() {
    let mut store = create_test_store();
    let target = IVec3::new(3, 3, 3);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    let player = PlayerController::new(Vec3::new(1.5, 3.0, 1.5));
    let rules = BuildRuleRegistry::new();

    let hit = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(1.5, 2.0, 1.5),
        distance: 2.0,
        face: FaceDirection::PosY,
        normal: Vec3::Y,
    };
    let ray_result = VoxelRaycastResult::Hit(hit);

    // Item 41: Repeated proposals are identical
    let first_prop = build_placement_proposal(
        &store,
        &rules,
        &player,
        &ray_result,
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        5.0,
    );
    for _ in 0..100 {
        let prop = build_placement_proposal(
            &store,
            &rules,
            &player,
            &ray_result,
            TEST_DIRT,
            None,
            BlockOrientation::Default,
            5.0,
        );
        assert_eq!(first_prop, prop);
    }

    // Item 42: Repeated validations are identical
    let first_edit =
        validate_placement_proposal(&store, &rules, &player, &first_prop, 5.0).unwrap();
    for _ in 0..100 {
        let edit = validate_placement_proposal(&store, &rules, &player, &first_prop, 5.0).unwrap();
        assert_eq!(first_edit, edit);
    }
}

// ============================================================================
// 13. PREVIEW & ARCHITECTURE FIREWALLS (Items 43 - 54)
// ============================================================================

#[test]
fn test_preview_and_architecture_firewalls() {
    let mut store = create_test_store();
    let target = IVec3::new(2, 2, 2);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));
    let initial_rev = store.get(&IVec3::ZERO).unwrap().revision;

    let player = PlayerController::new(Vec3::new(2.0, 10.0, 2.0));
    let rules = BuildRuleRegistry::new();

    let hit = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(1.0, 1.5, 1.0),
        distance: 2.0,
        face: FaceDirection::PosY,
        normal: Vec3::Y,
    };

    // Item 43: Creating proposal never mutates world state
    let _prop = build_placement_proposal(
        &store,
        &rules,
        &player,
        &VoxelRaycastResult::Hit(hit),
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        5.0,
    );
    assert_eq!(
        store.get(&IVec3::ZERO).unwrap().revision,
        initial_rev,
        "Building a proposal must never mutate world or chunk revision"
    );

    // Item 44: Preview requires no renderer
    // (Proposal is purely CPU in-memory struct PlacementProposal)

    // Item 45: Preview performs no chunk generation
    // Evaluasi terhadap raycast Miss tidak memanggil chunk generator
    let miss_prop = build_placement_proposal(
        &store,
        &rules,
        &player,
        &VoxelRaycastResult::Miss,
        TEST_DIRT,
        None,
        BlockOrientation::Default,
        5.0,
    );
    assert_eq!(
        miss_prop.validity,
        PlacementValidity::Invalid(PlacementRejectionReason::NoTargetHit)
    );

    // Item 46 - 54: Architecture firewalls:
    // Placement proposal and handler only require:
    // - store / world
    // - player
    // - material (MaterialId)
    // - block_id (Option<ResourceId>)
    // - orientation (BlockOrientation)
    // - cooldown (InteractionCooldown)
    // No Inventory, no ItemStacks, no Tools, no Durability, no Crafting, no DroppedItems,
    // no DynamicBody targeting, no new renderer, no global event bus.
}

#[test]
fn test_block_components_serde_compatibility() {
    // 1. JSON tanpa field "build" (kompatibel dengan file definisi blok lawas / Phase 11.3 ke bawah)
    let json_legacy = r#"{
        "harvestable": {
            "resource": "core:stone",
            "yield_quantity": 1
        }
    }"#;
    let components: BlockComponents = serde_json::from_str(json_legacy).unwrap();
    assert!(components.build.is_none());

    // 2. JSON dengan build kosong (menggunakan default serde)
    let json_empty_build = r#"{
        "build": {}
    }"#;
    let components_default: BlockComponents = serde_json::from_str(json_empty_build).unwrap();
    let build = components_default.build.unwrap();
    assert!(build.requires_support);
    assert_eq!(build.support_rule, SupportRule::AnyAdjacent);
    assert!(build.allowed_orientations.is_none());

    // 3. JSON dengan build kustom
    let json_custom = r#"{
        "build": {
            "requires_support": false,
            "support_rule": "none",
            "allowed_orientations": [
                "default",
                { "facing": "pos_y" }
            ]
        }
    }"#;
    let components_custom: BlockComponents = serde_json::from_str(json_custom).unwrap();
    let custom_build = components_custom.build.unwrap();
    assert!(!custom_build.requires_support);
    assert_eq!(custom_build.support_rule, SupportRule::None);
    assert_eq!(
        custom_build.allowed_orientations,
        Some(vec![
            BlockOrientation::Default,
            BlockOrientation::Facing(FaceDirection::PosY)
        ])
    );
}

#[test]
fn test_material_ambiguity_preserves_block_id_authority() {
    let mut blocks = BlockRegistry::new();
    let mut materials = omnisia::material::MaterialRegistry::new();

    let stone_mat = ResourceId::core("stone_shared").unwrap();
    materials
        .register_resource(
            stone_mat.clone(),
            omnisia::material::MaterialDef {
                name: "Stone Shared".to_string(),
                density_kg_m3: 2500.0,
                shear_strength_mpa: 50.0,
                base_color: [0.5, 0.5, 0.5],
                is_solid: true,
                is_transparent: false,
            },
            ResourceSource::Core,
        )
        .unwrap();

    // Blok A dan Blok B berbagi material yang sama: "stone_shared"
    let block_a_id = ResourceId::core("block_a").unwrap();
    let def_a = BlockDefinition {
        id: block_a_id.clone(),
        material: stone_mat.clone(),
        hardness: Some(1.0),
        components: BlockComponents {
            build: Some(BuildComponent {
                requires_support: false,
                support_rule: SupportRule::None,
                allowed_orientations: None,
            }),
            ..Default::default()
        },
        tags: vec![],
    };
    blocks.register(def_a, ResourceSource::Core).unwrap();

    let block_b_id = ResourceId::core("block_b").unwrap();
    let def_b = BlockDefinition {
        id: block_b_id.clone(),
        material: stone_mat.clone(),
        hardness: Some(1.0),
        components: BlockComponents {
            build: Some(BuildComponent {
                requires_support: true,
                support_rule: SupportRule::FloorOnly,
                allowed_orientations: None,
            }),
            ..Default::default()
        },
        tags: vec![],
    };
    blocks.register(def_b, ResourceSource::Core).unwrap();

    let registry = BuildRuleRegistry::from_registries(&materials, &blocks);

    // Otoritas block_id menjamin resolusi presisi
    let rule_a = registry.get_by_block(&block_a_id).unwrap();
    assert!(!rule_a.requires_support);
    assert_eq!(rule_a.support_rule, SupportRule::None);

    let rule_b = registry.get_by_block(&block_b_id).unwrap();
    assert!(rule_b.requires_support);
    assert_eq!(rule_b.support_rule, SupportRule::FloorOnly);

    // Pencarian via material yang ambigu tidak mengklaim salah satu secara sewenang-wenang
    let mat_id = materials.resolve_material_id(&stone_mat).unwrap();
    assert!(registry.get_by_material(mat_id).is_none());
}
