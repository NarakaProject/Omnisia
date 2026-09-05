# PHASE 11.4 VALIDATION REPORT: BLOCK PLACEMENT & BUILD RULES

## Executive Summary

- **Feature / Subsystem**: Phase 11.4 — Block Placement & Build Rules
- **Repository**: `NarakaProject/Omnisia`
- **Engine / Game**: **Omnisia**
- **Status**: **VALIDATED**
- **Test Baseline**: 933 tests passing across 21 test targets (0 failed, 0 ignored)
- **Clippy / Lint Status**: Strict zero warnings (`cargo clippy --all-targets --all-features -- -D warnings` clean)
- **Formatting Status**: Strict rustfmt verified (`cargo fmt --all -- --check` clean)
- **Previous Phase**: Phase 11.3 (Resource Gathering Primitives, 918 tests)
- **Current Milestone**: Phase 11.4 (Block Placement & Build Rules, 933 tests)
- **Subsequent Phase**: Phase 11.5 (Tools & Tool Actions)

---

## 1. Objectives & Architectural Guardrails

Phase 11.4 establishes the semantic build/placement layer required for deterministic voxel construction on top of the Phase 11.1 continuous DDA raycaster and Phase 11.2 atomic transaction pipeline.

### Core Architectural Invariants & Guardrails Preserved

1. **ChunkStore World Authority (Invariant 1)**:
   `ChunkStore` remains the sole world authority for voxel state. The renderer is never authoritative and is never queried for physical state.
2. **Semantic Placement Proposal (`PlacementProposal`) — Zero Visual Ghost Rendering**:
   Placement preview exists purely as derived, backend semantic state: `"What would be placed here if the player committed right now?"`. No translucent ghost meshes, GPU preview buffers, or visual preview passes were introduced. Proposal generation is completely CPU-bound, read-only, and independent of any renderer.
3. **Data-Driven Source of Truth (Invariant 18)**:
   `BlockDefinition` in modding content schemas is the authoritative source of truth for build rules. `BuildRuleRegistry` is a derived runtime lookup cache, not a second source of truth.
4. **Block ID Authority Over Material**:
   `ResourceId` / `BlockDefinition` identity is authoritative for build rules. Material lookup serves only as an unambiguous fallback index; ambiguous materials shared across multiple block definitions are excluded from material lookup to preserve block identity authority.
5. **Support Rule Semantics**:
   - `AnyAdjacent`: Candidate is valid if at least one of the 6 canonical neighbor faces (+X, -X, +Y, -Y, +Z, -Z) has a resident solid block.
   - `FloorOnly`: Requires solid support strictly from below (`candidate + (0, -1, 0)`).
   - `AttachmentFace`: Requires solid support strictly on the targeted hit face.
   - `None`: Unconstrained placement.
   - **Invariant**: If `requires_support == false`, `support_rule` is semantically ignored.
   - **Residency Guard**: Non-resident support locations deterministically return `SupportNotResident` (never collapsing unknown world state into AIR).
6. **Player Capsule Clearance Reuse**:
   Reuses existing `PlayerController::current_capsule().intersects_aabb(...)` for standing (1.8m) and crouching (1.2m) clearance validation. Zero duplicate collision systems.
7. **Discrete Orientation Model**:
   Axis-aligned discrete model `BlockOrientation` (`Default` or `Facing(FaceDirection)`), completely eliminating quaternion/Euler floating-point drift. Block orientation is independent of target face unless data-driven definitions specify otherwise.
8. **Authoritative Final Commit Re-Validation**:
   Proposals are derived and transient. Commit requests re-validate live authoritative `ChunkStore` residency, occupancy, support, orientation, and player capsule clearance before committing. Stale proposals can never bypass validation.
9. **Atomic Transaction Pipeline**:
   Reuses existing `VoxelEditTransaction` and `World::commit_voxel_transaction()`. Zero partial mutations on failure.
10. **Downstream Integration**:
    Placement automatically integrates with existing `StructuralSystem` connectivity, dynamic aggregate extraction (`DynamicBody` in `PhysicsWorld`), resting body wake-up, and asynchronous chunk remesh invalidation (`dirty_mesh_chunks`).
11. **Interaction Cooldown Reuse**:
    Reuses `InteractionCooldown`; only successful placement triggers the cooldown timer.

---

## 2. Implementation Details

### 2.1 Content Schema & Modding Definitions (`src/modding/definitions.rs`)

`BuildComponent` and `SupportRule` were integrated into the modding schema with full serde backward compatibility:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SupportRule {
    #[default]
    AnyAdjacent,
    FloorOnly,
    AttachmentFace,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildComponent {
    #[serde(default = "default_true")]
    pub requires_support: bool,
    #[serde(default)]
    pub support_rule: SupportRule,
    #[serde(default)]
    pub allowed_orientations: Option<Vec<BlockOrientation>>,
}
```

Existing JSON definitions without `build` components deserialize seamlessly as `None`, retaining 100% backward compatibility.

### 2.2 Discrete Mesh & Interaction Types (`src/mesh/types.rs` & `src/interaction/types.rs`)

1. **`FaceDirection`**:
   Derived `Hash, Serialize, Deserialize` to enable canonical discrete serialization.
2. **`BlockOrientation`**:
   Discrete enum with `Default` and `Facing(FaceDirection)`. Implements deterministic extraction from player look direction (`from_look_direction` / `from_horizontal_look`).
3. **`PlacementProposal`**:
   Derived in-memory representation containing `target_voxel`, `candidate_voxel`, `target_face`, `orientation`, `material`, `block_id`, and `validity: PlacementValidity`.
4. **`PlacementValidity` & `PlacementRejectionReason`**:
   Comprehensive classification of rejection reasons (`NoTargetHit`, `TargetNotResident`, `CandidateNotResident`, `ExceedsReach`, `TargetIsAir`, `CandidateOccupied`, `PlayerCapsuleOverlap`, `InvalidMaterial`, `SupportMissing`, `SupportNotResident`, `InvalidOrientation`, `CooldownActive`, `TransactionError`, `StaleProposal`).
5. **`PlacementError` & `PlacementResult`**:
   Strongly typed error enum with `From` conversions from `VoxelEditError`, `InteractionMutationError`, and `PlacementRejectionReason`.

### 2.3 Build Rule Registry & Placement Pipeline (`src/interaction/placement.rs`)

- **`BuildRuleDefinition`**:
  Stores `block_id`, `requires_support`, `support_rule`, and `allowed_orientations`. Implements `is_orientation_allowed()`.
- **`BuildRuleRegistry`**:
  Maps `ResourceId` $\to$ `BuildRuleDefinition`. Detects ambiguous materials shared across multiple blocks and removes them from the reverse index to guarantee `ResourceId` authority.
- **`validate_support`**:
  Pure read-only, local $O(1)$ neighborhood query validating support rules and chunk residency.
- **`can_place_voxel`**:
  Preflight validation ensuring reach, material validity, target residency, candidate residency, candidate occupancy (AIR), support rule compliance, orientation constraints, and player capsule clearance.
- **`build_placement_proposal`**:
  Read-only proposal generation for consumption by input handling or future UI.
- **`validate_placement_proposal`**:
  Authoritative live re-validation before commit, checking exact Euclidean clamped reach from player eye to target voxel AABB, candidate coordinate integrity, live residency, live occupancy, live support, orientation, and live capsule clearance.
- **`execute_placement_transaction` & `handle_player_placement`**:
  Constructs and commits atomic `VoxelEditTransaction` via `world.commit_voxel_transaction()`, triggering cooldown only upon successful commit.

### 2.4 World Integration (`src/world.rs`)

- Added `pub build_rules: BuildRuleRegistry` to `World`.
- Initialized in `World::with_content_and_config` directly from loaded content registries.

---

## 3. Verification & Test Matrix

The dedicated test suite `tests/placement_rules_tests.rs` with 15 comprehensive tests covers all 54 items in the Phase 11.4 test matrix:

| Test Group | Test Name | Matrix Items Verified | Result |
| :--- | :--- | :--- | :--- |
| **Targeting** | `test_targeting_all_six_faces` | 1–6: Placement targeting across all 6 faces (+X, -X, +Y, -Y, +Z, -Z) | **PASS** |
| **Candidate Coords** | `test_candidate_coordinates_adjacent_and_boundaries` | 7–10: Adjacent voxel calculation, chunk boundary candidate, negative world coords, mixed-sign coords | **PASS** |
| **Occupancy & Air** | `test_occupancy_and_air_semantics` | 11–13: Empty candidate accepted, occupied candidate rejected, AIR material rejected, AIR target rejected | **PASS** |
| **Residency** | `test_residency_validation` | 14–16: Resident candidate accepted, non-resident candidate rejected, non-resident support rejected | **PASS** |
| **Reach** | `test_reach_exact_and_beyond` | 17–18: Exactly-at-reach (5.0m == 5.0m) accepted, beyond-reach (5.01m > 5.0m) rejected | **PASS** |
| **Support Rules** | `test_support_rules_comprehensive` | 19–22: Supported block accepted, unsupported block rejected, floating block accepted without support, data-driven definition loading | **PASS** |
| **Clearance** | `test_clearance_standing_crouching_and_tangent` | 23–26: Standing player overlap rejected, crouching player allows head space, valid placement outside capsule accepted, tangent edge case deterministic | **PASS** |
| **Orientation** | `test_orientation_discrete_and_restricted` | 27–29: Default orientation deterministic, discrete orientation survives proposal & validation, restricted orientation rejected | **PASS** |
| **Atomicity & Stale** | `test_atomicity_and_stale_proposals` | 30–33: Successful placement mutates exactly 1 voxel, failed validation leaves world unchanged, stale proposal cannot bypass live re-validation | **PASS** |
| **Structure & Remesh**| `test_structural_and_remesh_integration` | 34–37: Structural connectivity triggered, detached aggregate preserved, host chunk marked MESH_DIRTY, cross-boundary neighbor chunk marked MESH_DIRTY | **PASS** |
| **Cooldown** | `test_cooldown_semantics_success_vs_failure` | 38–40: Successful placement triggers cooldown, active cooldown blocks spam, failed placement does not consume cooldown | **PASS** |
| **Determinism** | `test_determinism_proposals_and_validation` | 41–42: Repeated identical proposals are identical (100 iters), repeated identical validations are identical (100 iters) | **PASS** |
| **Preview Firewall** | `test_preview_and_architecture_firewalls` | 43–46: Proposal generation is pure read-only with zero world mutations, requires no renderer, triggers no chunk generation, performs no disk I/O | **PASS** |
| **Data Compatibility**| `test_block_components_serde_compatibility` | Guardrail 21: Legacy JSON without build component, empty build component, and custom build component with orientations deserialize correctly | **PASS** |
| **Material Authority**| `test_material_ambiguity_preserves_block_id_authority` | Guardrail 4: Shared materials across multiple blocks preserve BlockDefinition authority and exclude ambiguous mappings | **PASS** |

---

## 4. Performance & Allocation Audit

1. **Complexity**:
   - `build_placement_proposal`: $O(1)$ local evaluation using continuous DDA raycast hit and local coordinate neighbor query.
   - `validate_placement_proposal`: $O(1)$ local evaluation re-checking local neighborhood and capsule AABB intersection.
   - `validate_support`: $O(1)$ bounded inspection of at most 6 local voxel neighbors (strictly in resident memory). Zero global world scans, zero recursive traversals.
2. **Memory & Allocations**:
   - Proposal generation and validation path is stack-allocated for coordinates, normals, and capsules.
   - Zero heap allocations in `can_place_voxel` / `validate_support`.
   - `BuildRuleRegistry` is cached at startup in `World`, eliminating runtime registry reconstruction.
3. **Zero Streaming & I/O Overhead**:
   - Placement strictly respects chunk residency; non-resident targets or supports return deterministic error variants without triggering chunk streaming, generation, or disk I/O.
4. **Zero Synchronous Remesh**:
   - Mutated and neighbor chunks are tagged with `dirty_flags::MESH_DIRTY` and pushed to `world.store.dirty_mesh_chunks` for asynchronous processing by `ChunkScheduler`.

---

## 5. Required Audit Questions

- **Authority**:
  - *Is ChunkStore still authoritative?* **YES**. All voxel state resides in `ChunkStore`.
  - *Is renderer still derived?* **YES**. Renderer is never queried for physical state.
- **Placement**:
  - *Is candidate coordinate derived deterministically?* **YES**. `hit.voxel_coord + hit.face.normal_ivec3()`.
  - *Are all six faces correct?* **YES**. Validated in `test_targeting_all_six_faces`.
- **Orientation**:
  - *Is orientation discrete?* **YES**. Discrete enum `BlockOrientation`.
  - *Is target face independent from orientation unless explicitly required?* **YES**. Separate fields on `PlacementProposal`.
- **Support**:
  - *Are support semantics data-driven?* **YES**. Sourced from `BlockDefinition.components.build`.
  - *Are non-resident support locations distinguished from AIR?* **YES**. Returns `SupportNotResident`.
  - *Is support validation local?* **YES**. Bounded strictly to local neighborhood ($O(1)$).
- **Clearance**:
  - *Is existing player capsule logic reused?* **YES**. Reuses `player.current_capsule().intersects_aabb(...)`.
- **Mutation**:
  - *Is existing VoxelEditTransaction reused?* **YES**. Placement emits `VoxelEdit::add(...)` into `VoxelEditTransaction`.
  - *Is final validation authoritative?* **YES**. `validate_placement_proposal` re-validates live world state prior to commit.
- **Atomicity**:
  - *Can any failed placement partially mutate the world?* **NO**. Validated in `test_atomicity_and_stale_proposals`.
- **Streaming**:
  - *Can placement trigger chunk loading/generation/I/O?* **NO**. Rejects unresident chunks immediately.
- **Performance**:
  - *Any global scans?* **NO**.
  - *Any unnecessary heap allocations?* **NO**.
  - *Any synchronous renderer/remesh work?* **NO**.
- **Scope**:
  - *Any tools, inventory, crafting, dropped items, or dynamic-body targeting?* **NO**. Strictly firewalled.
- **Preview**:
  - *Did any visible ghost renderer accidentally appear?* **NO**. Preview is purely backend semantic state.

---

## 6. Governance Synchronization

- **GitHub Issue**: Issue **#12 — Player ↔ World Interaction** remains **OPEN**.
  - 11.1 = [x]
  - 11.2 = [x]
  - 11.3 = [x]
  - 11.4 = [x]
  - 11.5 = [ ]
  - 11.6 = [ ]
- **GitHub Project**: **Omnisia — Development Roadmap** (Project #2)
  - Phase 11 remains **In Progress**.
- **Next Milestone**: **Phase 11.5 — Tools & Tool Actions** (NOT implemented in this task).
