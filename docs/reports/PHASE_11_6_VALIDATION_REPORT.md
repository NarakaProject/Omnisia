# Omnisia Phase 11.6 — Generic Interactable World Objects & Feedback Validation Report

**Phase Identifier**: `PHASE_11_6_GENERIC_INTERACTABLES_AND_FEEDBACK`  
**Parent Milestone**: Phase 11 — Player ↔ World Interaction  
**Issue Reference**: #12 (Player ↔ World Interaction)  
**Status**: `COMPLETED / VALIDATED`  
**Verified Baseline HEAD**: `ebb4ea886c3daed40a22bde5ea60860e34ace193`  
**Test Suite Status**: 982 passed, 0 failed, 0 ignored across 23 test targets (28 new tests in `tests/interactable_tests.rs`)  

---

## 1. Executive Summary

Phase 11.6 introduces the **smallest useful semantic interaction seam for non-voxel-breaking world interactions** (switches, levers, doors, examine targets) to the Omnisia voxel engine. It establishes data-driven definitions, derived runtime lookup, genuinely read-only queries, atomic state commit, and pure semantic feedback without creating a generalized gameplay framework or introducing architecture creep.

The complete interaction pipeline is deterministic and verified:
```text
Player Look Direction + Generic Interaction Trigger
        ↓
Orchestration: handle_player_generic_interaction()
  1. Cooldown Gate: cooldown.can_act() (rejects early if debounce is active, zero mutation)
  2. Query Target: query_interactable_target(&World) (murni read-only)
     - Validates chunk residency in ChunkStore
     - Validates non-air voxel
     - Resolves BlockDefinition & InteractableDefinition
     - Inspects runtime instance:
       * Valid (interactable_id == def.id && expected_material == block.material): uses instance.state
       * Stale / mismatched: ignores entry (READ-ONLY: zero mutation), falls back to def.initial_state
       * None: uses def.initial_state
     - Filters definition allowed_actions by current_state -> available_actions
     - Preferred action derived from available_actions.first()
  3. Validate Interaction: validate_interaction(&World, &target, action, eye, reach)
     - Validates Euclidean AABB clamp distance <= max_reach
     - Validates chunk residency and non-air voxel
     - Validates target enabled (rejects Disabled with ObjectDisabled for mutating actions)
     - Computes target_state via deterministic transition matrix
  4. Build Proposal: InteractionProposal (captures expected_material, previous_state, target_state)
  5. Execute Proposal: execute_interaction(&mut World, &proposal)
     - Mandatory 3-part TOCTOU revalidation:
       1. current_interactable_id == proposal.interactable_id
       2. current_material == proposal.expected_material
       3. current_state == proposal.previous_state
     - Commits proposal.target_state to in-memory InteractableInstance
  6. Post-Commit Cooldown: cooldown.trigger() strictly AFTER successful state transition
  7. Return Result: Ok(InteractionResult) with pure data InteractionFeedback
```

---

## 2. Guardrail & Architectural Compliance Audit

All mandatory invariants and guardrails have been strictly implemented and verified:

| Invariant | Architectural Enforcement & Verification | Status |
|:---|:---|:---:|
| **InteractableId Isolation** | `InteractableId` is a distinct namespaced struct (`namespace: ModId, path: String`). Zero implicit conversions (`From<ResourceId>`, `From<ToolId>`, `From<MaterialId>`) exist. Verified by `test_interactable_id_distinct_from_resource_and_tool_id`. | **VERIFIED** |
| **Content Authority** | `BlockDefinition.components.interactable` is the sole content authority. `InteractableRegistry` is derived/cache lookup state built during world init. | **VERIFIED** |
| **MaterialId Non-Sufficiency** | `MaterialId` is treated strictly as a consistency sanity check, NOT object identity. State validity requires `current_interactable_id == instance.interactable_id && current_material == instance.expected_material`. Verified by `test_toctou_stale_proposal_after_same_material_definition_replacement`. | **VERIFIED** |
| **Truly Read-Only Query** | `detect_interactable_target` and `query_interactable_target` accept `&World`. Detecting a stale instance ignores it and falls back to `def.initial_state` without calling `instances.remove()`. Verified by `test_query_and_validate_perform_zero_mutation`. | **VERIFIED** |
| **Unambiguous Cooldown Ownership** | `handle_player_generic_interaction` exclusively gates on `cooldown.can_act()` and triggers `cooldown.trigger()` strictly post-commit. `execute_interaction()` does not receive or touch cooldown. Verified by `test_toctou_failed_stale_execution_leaves_cooldown_untouched`. | **VERIFIED** |
| **Semantic Feedback (Data Only)** | `InteractionFeedback` contains data only (`Option<AudioCue>`, `Option<VisualCue>`, `Option<FeedbackId>`). Zero UI text, lore strings, localization, audio devices, or VFX spawners exist. Verified by `test_interaction_feedback_returns_compact_semantic_data`. | **VERIFIED** |
| **Examine is Semantic Only** | `Examine` preserves state (`previous_state == new_state`), returns `FeedbackId::Examined`, and contains no lore strings or UI payload. Verified by `test_examine_preserves_state_and_returns_semantic_feedback`. | **VERIFIED** |
| **No Voxel Mutation in 11.6** | Phase 11.6 is strictly semantic interaction state transitions. CSG voxel transactions and physical mutations are omitted. | **VERIFIED** |
| **Atomic State Commit** | Pre-validated intent: failures leave instance state, world, and cooldown completely untouched. | **VERIFIED** |
| **Scope Firewalls** | Zero global event bus, zero inventory/items/crafting, zero creatures/entities/ECS, zero universal `WorldObject` hierarchy, zero persistence formats. | **VERIFIED** |

---

## 3. TOCTOU Protection & Final Revalidation

`execute_interaction()` implements the mandatory 3-part pre-commit revalidation:
```rust
// 1. Identity validation
if current_interactable_id != proposal.interactable_id {
    return Err(InteractionError::InteractableMismatch { expected, actual });
}

// 2. Physical material validation
if voxel.material != proposal.expected_material {
    return Err(InteractionError::MaterialMismatch { expected, actual });
}

// 3. Current state validation
if current_state != proposal.previous_state {
    return Err(InteractionError::StateMismatch { expected, actual });
}
```

If ANY condition fails:
- Execution fails immediately.
- Instance state remains completely untouched.
- Cooldown is NOT consumed.
- Zero unrelated world state changes occur.

---

## 4. Mandatory TOCTOU Behavioral Test Results

All four mandatory stale-proposal TOCTOU scenarios were implemented in `tests/interactable_tests.rs` and pass 100%:

1. **Test A — Stale Proposal After Interactable Replacement** (`test_toctou_stale_proposal_after_interactable_replacement`):
   - Setup: Object at coordinate replaced by another interactable definition (`lever_b`) between proposal creation and execution.
   - Result: `execute_interaction` rejects the stale proposal with `InteractionError::InteractableMismatch`. State of `lever_b` is unaffected; cooldown is not consumed.

2. **Test B — Stale Proposal After Same-Material Definition Replacement** (`test_toctou_stale_proposal_after_same_material_definition_replacement`):
   - Setup: Two distinct interactables share the exact same material (`MaterialId::Wood`). Definition at coordinate changed from `switch_a` to `switch_b`, with material remaining wood.
   - Result: Proposal execution fails with `InteractionError::InteractableMismatch`, proving that `MaterialId` alone is NOT sufficient identity.

3. **Test C — Stale Proposal After State Changed** (`test_toctou_stale_proposal_after_state_changed`):
   - Setup: Proposal generated for `Idle -> Active`. Live state advances to `Active` through another interaction before old proposal executes.
   - Result: Proposal execution fails with `InteractionError::StateMismatch { expected: Idle, actual: Active }`. State remains `Active` without being overwritten or repaired.

4. **Test D — Failed Stale Execution Leaves Cooldown Untouched** (`test_toctou_failed_stale_execution_leaves_cooldown_untouched`):
   - Setup: Stale proposal fails execution against replaced target.
   - Result: `cooldown.can_act()` remains `true`. A subsequent valid interaction with the new target succeeds and consumes cooldown post-commit.

---

## 5. Quality Gates & Regression Verification

Every repository quality gate has been executed and confirmed clean:

```bash
cargo fmt --all -- --check                                    # PASS (0 diffs)
cargo clippy --all-targets --all-features -- -D warnings       # PASS (0 warnings)
cargo check --bins                                            # PASS (0 errors)
cargo test --all-targets                                      # PASS (982 passed, 0 failed, 0 ignored)
cargo run --bin integration_validation                        # PASS (6/6 stages)
cargo run --bin player_validation                             # PASS (8/8 stages)
```

### Complete Test Count Matrix (982 Total Tests across 23 Targets)

- Baseline (Phase 11.5): **954 tests**
- New in Phase 11.6 (`tests/interactable_tests.rs`): **28 tests**
- Total passing workspace tests: **982 tests** (0 failures, 0 ignored)

---

## 6. Known Limitations

1. **Session-Only Memory State**: Runtime instance state is held in-memory in `InteractableRegistry.instances` and does not persist across save/load. Persistence is explicitly deferred to a dedicated world persistence milestone.
2. **No Visual Voxel Swap**: Semantic state transitions (`InteractableState`) occur in memory and emit semantic feedback cues, but do not modify voxel geometry or materials in `ChunkStore`. Visual representation and animations are deferred to future visual integration phases.
