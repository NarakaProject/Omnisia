# PHASE 11.3 VALIDATION REPORT: RESOURCE GATHERING PRIMITIVES

## Executive Summary

- **Feature / Subsystem**: Phase 11.3 — Resource Gathering Primitives
- **Repository**: `NarakaProject/Omnisia`
- **Engine / Game**: **Omnisia**
- **Status**: **VALIDATED**
- **Test Baseline**: 918 tests passing across 20 test targets (0 failed, 0 ignored)
- **Clippy / Lint Status**: Strict zero warnings (`cargo clippy --all-targets --all-features -- -D warnings` clean)
- **Formatting Status**: Strict rustfmt verified (`cargo fmt --all -- --check` clean)
- **Previous Phase**: Phase 11.2 (Voxel Removal & Interaction Mutations, 891 tests)
- **Subsequent Phase**: Phase 11.4 (Block Placement & Build Rules)

---

## 1. Objectives & Architectural Firewalls

Phase 11.3 establishes data-driven semantic resource gathering on top of the authoritative CSG and voxel interaction pipeline validated in Phase 11.2 and the continuous DDA raycaster validated in Phase 11.1.

### Core Architectural Invariants Preserved
1. **Authoritative ChunkStore (Invariant 1)**: The voxel grid in `ChunkStore` is the sole authoritative representation of matter. Gathering removes the target voxel via `VoxelEdit::remove(coord)`.
2. **Meshes as Transient Cache (Invariant 2)**: Chunk and neighbor mesh dirty flags (`dirty_mesh_chunks`) are properly set when border voxels are gathered.
3. **Data-Driven Semantic Mapping (Invariant 18)**: Voxel blocks define their resource yields through `HarvestableComponent` in JSON content definitions (`content/core/blocks/`), parsed through `BlockRegistry` and mapped to runtime `MaterialRegistry`.
4. **Structural Connectivity (Invariant 4 & 11)**: Removing a harvestable voxel that serves as structural support triggers localized, event-driven breadth-first search. Any newly unanchored voxels detach as `DynamicBody` aggregates into `PhysicsRuntime`.
5. **No Dropped Entities / Zero Inventory Mutation (Firewall Rule)**: Gathering outputs a pure semantic `CollectionResult` (containing `source_coord`, `resource_id`, and `quantity`). It does **not** create dropped physical item entities, modify inventory slots, or assume any tool requirements.

---

## 2. Implementation Details

### 2.1 Core Schema & Modding Integration
In `src/modding/definitions.rs`, `HarvestableComponent` was added to `BlockComponents`:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarvestableComponent {
    pub resource: ResourceId,
    pub yield_quantity: u32,
    #[serde(default = "default_true")]
    pub harvestable: bool,
}
```
Core JSON definitions in `content/core/blocks/` were updated with data-driven harvestable metadata:
- `stone_block.json`: `core:stone` $\to$ yield: 1
- `iron_ore_block.json`: `core:iron_ore` $\to$ yield: 1
- `coal_ore_block.json`: `core:coal_ore` $\to$ yield: 1
- `gold_ore_block.json`: `core:gold_ore` $\to$ yield: 1
- Blocks without harvestable components (such as `ag_core_casing_block.json`) remain strictly non-harvestable.

### 2.2 Semantic Types (`src/interaction/types.rs`)
- `ResourceDefinition`: Holds `resource_id`, `base_yield`, `harvestable`, and optional `source_block`.
- `CollectionResult`: Holds `source_coord: IVec3`, `resource_id: ResourceId`, and `quantity: u32`.
- `GatheringResult`: Combines `collection: CollectionResult` and `mutation: VoxelMutationResult`.
- `GatheringError`: Strongly typed enum handling:
  - `NoTargetHit`
  - `TargetNotResident { coord }`
  - `ExceedsReach { distance, max_reach }`
  - `TargetIsAir { coord }`
  - `NotHarvestable { coord, material, block_id }`
  - `CooldownActive { remaining }`
  - `TransactionError(VoxelEditError)`
  - `MutationError(InteractionMutationError)`

### 2.3 Runtime Gathering Pipeline (`src/interaction/gathering.rs`)
1. **`ResourceGatheringRegistry`**:
   - `by_material: HashMap<MaterialId, ResourceDefinition>` for $O(1)$ fast path during raycast hits.
   - `by_resource_id: HashMap<ResourceId, ResourceDefinition>` for persistent identity lookup.
   - `block_to_material: HashMap<ResourceId, MaterialId>` for cross-registry mapping.
   - Built dynamically via `ResourceGatheringRegistry::from_registries(&MaterialRegistry, &BlockRegistry)`.
2. **`can_gather`**:
   - Validates reach ($\le \text{max\_reach}$).
   - Validates chunk residency in `ChunkStore`.
   - Rejects air voxels (`TargetIsAir`).
   - Resolves `ResourceDefinition` from material and validates `harvestable == true`.
   - Constructs removal proposal `VoxelEdit::remove(coord)`.
3. **`validate_gather_action`**:
   - Performs player raycast interaction using Phase 11.1 semantics.
   - Constructs and validates `VoxelEditTransaction`.
4. **`execute_gather_transaction`**:
   - Commits transaction atomically via `execute_interaction_transaction`.
   - Produces `CollectionResult` only upon successful commit.
5. **`handle_player_gather`**:
   - Checks `InteractionCooldown` debounce.
   - Validates and executes gathering atomically.
   - Resets cooldown timer upon success.

### 2.4 World Integration (`src/world.rs`)
- Added `pub resources: ResourceGatheringRegistry` to `World`.
- Initialized in `World::with_content_and_config` directly from loaded content registries.

---

## 3. Verification & Test Matrix

A dedicated test suite `tests/gathering_tests.rs` with 27 comprehensive tests was added covering all required dimensions:

| Test Group | Test Name | Invariant Verified | Result |
| :--- | :--- | :--- | :--- |
| **Resource Identity** | `test_resource_definition_resolves_correctly` | Fast $O(1)$ material & persistent ResourceId lookup | PASS |
| | `test_resource_identity_is_stable` | Identical ResourceId across multiple lookups | PASS |
| | `test_resource_mapping_is_data_driven` | Loaded from JSON via `BlockRegistry` | PASS |
| | `test_unmapped_block_returns_no_resource` | Non-harvestable blocks return `None` | PASS |
| **Harvestability** | `test_harvestable_resource_can_be_gathered` | Valid harvestable voxel passes preflight | PASS |
| | `test_air_cannot_be_gathered` | Raycast hitting AIR rejected with `TargetIsAir` | PASS |
| | `test_non_harvestable_block_cannot_be_gathered` | Solid unmapped block rejected with `NotHarvestable` | PASS |
| | `test_unloaded_target_cannot_be_gathered` | Target in unresident chunk rejected | PASS |
| **Yield** | `test_yield_quantity_is_correct` | Base yield matches JSON definition | PASS |
| | `test_repeated_equivalent_gathers_produce_deterministic_yield` | 100 identical gathering validations produce constant yield | PASS |
| | `test_no_uncontrolled_randomness_exists` | No random fluctuations or variance | PASS |
| **Reach & Targeting** | `test_in_range_target_gathers` | Distance within reach succeeds ($4.99\text{m} \le 5.0\text{m}$) | PASS |
| | `test_beyond_reach_target_fails` | Distance exceeding reach fails ($5.01\text{m} > 5.0\text{m}$) | PASS |
| | `test_target_uses_phase_11_1_interaction_semantics` | Raycast computed from player camera eye | PASS |
| **Mutation & Atomicity** | `test_successful_gathering_removes_voxel` | Voxel replaced with AIR, single `CollectionResult` returned | PASS |
| | `test_failed_gathering_leaves_voxel_unchanged` | Failed reach/preflight preserves original voxel state | PASS |
| | `test_atomicity_preflight_failure_produces_zero_mutation_and_zero_collection` | Multi-edit partial failure leaves world completely untouched | PASS |
| **Coordinates** | `test_negative_world_coordinates` | Negative voxel in negative chunk gathers cleanly | PASS |
| | `test_chunk_boundary_coordinates` | Boundary voxel at X=31 marks neighbor chunk dirty | PASS |
| | `test_negative_chunk_boundary` | Boundary voxel at X=-32 across negative chunks marks neighbors dirty | PASS |
| **Structural Physics** | `test_gathering_preserves_structural_connectivity` | Removing support block triggers structural detachment into `PhysicsRuntime` | PASS |
| **Cooldown** | `test_cooldown_debounce_rate_limiting` | Immediate repeat call rejected with `CooldownActive` | PASS |
| **Determinism** | `test_determinism_repeated_gathering` | 100 runs on identical world states yield bitwise identical output | PASS |
| **Architectural Firewalls** | `test_firewall_no_inventory_or_item_stacks` | `CollectionResult` contains no inventory slots | PASS |
| | `test_firewall_no_tool_requirements_in_baseline` | Baseline gathering functions without tools | PASS |
| | `test_firewall_no_dropped_item_entities` | PhysicsRuntime body count is 0 (no dropped items) | PASS |
| | `test_firewall_no_renderer_dependency` | Pure headless execution without GPU/WGPU | PASS |

---

## 4. Test Suite Summary

- **Total Workspace Tests**: **918 passed, 0 failed, 0 ignored**
- **Test Targets**: 20 integration test targets + 10 unit test targets
- **Compilation**: All binaries (`cargo check --bins`) build cleanly.
- **Linters**: `cargo clippy --all-targets --all-features -- -D warnings` passed with 0 errors/warnings.
- **Format**: `cargo fmt --all -- --check` verified clean.
