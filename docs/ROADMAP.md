# Omnisia — Master Roadmap & Project Vision

> **Vision Statement**: *"Revive Chimeraland with a voxel soul."*  
> **Core Substrate**: Procedural, structural, and rigid-body voxel world.  
> **Core Gameplay Identity**: Creature ecosystem, taming, acquisition, pet companions, and modular devour/evolution.  
> **Current Completed Phase**: **Phase 11.6 — Generic Interactable World Objects & Feedback** (`VALIDATED`)  
> **Next Active Phase**: **Phase 11.7 — Interaction Audio / Feedback Integration** (`PLANNED / NEXT`)

---

## 1. Project Vision & Gameplay Identity

Omnisia is an ambitious, high-performance voxel sandbox game built from scratch in pure Rust.

### What Omnisia IS
- A living procedural voxel world where terrain has metric physical reality ($1\text{ voxel} = 0.5\text{m}$).
- A structural simulation where terrain breaks into physical dynamic bodies under gravitational and impact forces.
- A deep creature sandbox combining wildlife ecology, creature taming, pet raising, and chimera evolution.
- A progression system centered on **devour and mutation**: capturing beasts, acquiring anatomical traits (wings, horns, heads, tails), and mutating hybrid creatures.
- A modular architecture where player transformation, modular creature meshes, and physical destruction coexist smoothly.

### What Omnisia IS NOT
- **NOT a Minecraft clone with creatures**: Terrain destruction produces rigid-body physical aggregates, not floating disconnected blocks; movement features fluid kinematic locomotion, auto-stepping, and gliding.
- **NOT a generic voxel engine demo**: Engine features are built strictly in service of gameplay, progression, and ecology.
- **NOT a physics sandbox without gameplay**: Physics is a deterministic substrate, not the game itself.
- **NOT a direct copy of Chimeraland**: Omnisia reimagines the modular creature evolution and fantasy beast fantasy with the physical mutability and emergent gameplay of an authoritative voxel world.

---

## 2. Historical Completed Roadmap (Phases 1 — 9.12)

### PHASE 1 — FOUNDATION (`COMPLETED`)
- Pure Rust 2021 workspace, `wgpu` v24 graphics pipeline targeting macOS Metal / Vulkan / DirectX 12.
- 32³ dense chunk storage with inlined canonical index calculations ($0.26\text{ ns/op}$).
- Base Ambient Occlusion (AO) and dual meshing architectures:
  - Culled mesher for immediate visualization.
  - Greedy mesher delivering 29.1x quad reduction ($0.82\text{ ms/chunk}$).
- Headless automated testing foundation.

### PHASE 2 — MODDING & DATA DEFINITION (`COMPLETED`)
- Data-driven content architecture via JSON schemas in `content/core/`.
- Dynamic block and material registries eliminating hardcoded engine constants.
- Decoupled `MaterialId` and `BlockId` lookup paths ($< 1.0\text{ ns/op}$).

### PHASE 2.5 — CONTENT BOUNDARY & ASSET IDENTITY (`COMPLETED`)
- Strict namespaced `ResourceIdentifier` (`namespace:path`, e.g., `core:stone_block`).
- Safe override policy: external mods in `mods/` can override or extend assets without mutating engine core files.
- SemVer-compliant dependency resolution and manifest validation.

### PHASE 3 — HIERARCHICAL STREAMING & CHUNK RESIDENCY (`COMPLETED`)
- Sparse world model backed by authoritative `ChunkStore`.
- Multithreaded background chunk generation and meshing using Rayon thread-pools.
- Explicit memory budget enforcement, generational handles, and LRU eviction lifecycles.
- Strict firewall between authoritative voxel data (`ChunkStore`) and derived transient render state (`MeshCache`).

### PHASE 4 — WORLD GENERATION FOUNDATION (`COMPLETED`)
- Deterministic seed pipeline (`WorldSeed(u64)`) ensuring reproducible world generation across runs.
- Multi-scale volumetric noise fields evaluating continentalness, temperature, and moisture.
- Biome classification matrix (Forest, Plains, Desert, Mountains, Tundra).
- Hydrology simulation: coherent river networks, sea levels, and lake basins.

### PHASE 5 — VOXEL WORLD FEATURES (`COMPLETED`)
- 3D density evaluation for non-columnar overhangs, steep cliffs, and floating arches.
- Volumetric cave carving using dual-noise worm tunnels and spherical cheese caverns.
- Stratified subsurface layers: Topsoil $\to$ Subsoil $\to$ Stone $\to$ Deepslate.
- Resource distribution: Coal, Iron, Gold, and Lumina Crystal ore veins.
- Surface natural rock formations and boulders.

### PHASE 6 — VEGETATION & CANONICAL ECOLOGY (`COMPLETED`)
- Canonical cross-chunk vegetation stamping (Oak trees, Pine trees, Desert shrubs, Grass).
- Neighbor-independent generation guaranteeing deterministic tree structures across chunk boundaries.
- Biome-specific vegetation density and foliage placement.

### PHASE 7 — STRUCTURAL CONNECTIVITY & METRIC SCALE (`COMPLETED`)
- Event-driven localized structural connectivity graph.
- 6-connected cubic adjacency model (diagonal edge/corner touches strictly rejected).
- Data-driven anchor policy via `BlockComponents::structural_anchor` (Stone, Deepslate).
- Early-exit localized BFS: scans average 15 voxels before reaching an anchor; zero full-world BFS.
- Detached aggregate extraction: unanchored voxels atomic transfer into `DetachedAggregate`.
- Metric scale validation: 1 voxel $= 0.5\text{m}$, human height reference $\approx 1.8\text{m}$ (3.6 voxels).
- Multi-kilometer flight validation ($100\text{m}$ to $1\text{km}$, negative coordinates, memory $< 85\text{ MB}$).

### PHASE 8 — ANTIGRAVITY, DYNAMIC ISLANDS & PLAYER KINEMATICS (`COMPLETED`)
- **8A — Dynamic Aggregate Runtime**:
  - `DynamicBody` representation for detached voxels.
  - 30 Hz fixed-timestep physics accumulator with deterministic `BTreeMap` storage.
  - Swept vertical collision detection against static terrain with integer voxel lattice snapping.
  - Two-phase transactional reintegration (`prepare` $\to$ `commit`) restoring settled voxels to `ChunkStore`.
  - AntiGravity floating aggregates (`gravity_scale = 0.0`).
- **8B — Kinematic Player Controller**:
  - Kinematic capsule controller (height $1.8\text{m}$, radius $0.3\text{m}$, crouch $1.2\text{m}$).
  - Stand-up clearance guard preventing standing under low ceilings.
  - Single-consumption jump edge-trigger suppressing bunny-hopping.
  - Continuous swept anti-tunneling ($50\text{ m/s}$ and $100\text{ m/s}$).
  - Unloaded chunk boundary guard (`Unknown != Air`).
- **8C — Integration Layer**:
  - Full player ↔ static world ↔ `DynamicBody` interaction.
  - Voxel ownership conservation: zero double-ownership audited across transitions.
  - Runtime structural detachment triggered by player mining or terrain collapse.
- **8D — Movement & Traversal Hardening**:
  - Auto-step traversal solver (up to $0.55\text{m}$ obstacle height).
  - Slope sliding and ground contact normal resolution.
  - Bounded airborne glide mechanics.
  - **Authoritative Player Movement Semantics**:
    - `W` = Walk ($3.0\text{ m/s}$ production default).
    - `Shift + W` = Sprint ($6.0\text{ m/s}$ production default).
    - `Space` while grounded = Normal Jump ($6.0\text{ m/s}$ upward impulse).
    - `Shift + W + Space` while grounded = Sprint-Jump, which may transition into Glide during descent.
    - Running off a cliff = Fall, **NOT** Glide (`AirborneOrigin::FellFromEdge` is immutable).
    - Standing still + `Shift + Space` = Normal Jump, **NOT** Glide.
    - Normal Jump does not become Glide merely because `Shift` is pressed later.
    - Airborne origin is authoritative for determining whether Glide is eligible.

### PHASE 9 — RIGIDBODY PHYSICS & AGGREGATE INTEGRATION (`COMPLETED`)
- **9.1 Physics World & Broadphase**: Spatial hash grid broadphase, dynamic AABB synchronization, candidate pair generation ($< 0.18\text{ ms}$ for 1,000 bodies).
- **9.2 RigidBody Data Model**: Mass properties, center of mass, 3x3 inertia tensor, linear/angular velocity, state flags.
- **9.3 Shape Representation**: Box colliders, sphere, capsule, compound colliders, local/world transforms.
- **9.4 Contact Generation**: SAT (Separating Axis Theorem) box-box narrowphase, contact manifold generation, contact normals, penetration depths.
- **9.5 Contact Solver**: Sequential impulse constraint solver, normal non-penetration impulse, tangent Coulomb friction impulse.
- **9.6 Linear & Angular Integration**: Symplectic Euler integration, torque resolution, quaternion angular velocity integration with gyroscopic stability.
- **9.7 Friction & Restitution**: Dynamic/static Coulomb friction, restitution bounciness, restitution velocity thresholding.
- **9.8 Sleeping & Island Management**: Union-Find graph partitioning, kinetic energy quietness tracking, atomic island sleep transitions, static anchor wake isolation.
- **9.9 Dynamic ↔ Dynamic Collision**: Momentum-conserving dynamic body pairs, deep penetration resolution, finite guard rails against non-finite values.
- **9.10 Player ↔ RigidBody Refinement**: `PlayerRigidBodyBridge`: kinematic pushing, dynamic platform carrying; player strictly excluded from `rigid_bodies` and `PhysicsIsland`.
- **9.11 Structural Aggregate ↔ RigidBody**: 1 aggregate = 1 `RigidBody` owning $M$ greedy merged colliders; Parallel Axis Theorem inertia tensor; 24 proper cubic lattice rotations for two-phase transactional reintegration.
- **9.12 Stress & Performance Validation**: Linear scaling up to 10,000 bodies (12.65 ms); 40–45% sleeping CPU reduction; 10,000-step long-run stability (drift $< 1.3\text{ mm}$); bitwise identical deterministic replay across runs; verified at commit `4f60bd6` with 660 passing tests.

---

## 3. Completed Milestone: Phase 10 — World Impact & Atmosphere

> **Status**: `COMPLETED / VALIDATED` (Track A: Phase 10.1 `VALIDATED`, Phase 10.2 `VALIDATED`, Phase 10.3 `VALIDATED`, Phase 10.4 `VALIDATED`; Track B: Phase 10.5 `VALIDATED`, Phase 10.5.x `VALIDATED`, Phase 10.6 `VALIDATED`, Phase 10.6.1 `VALIDATED`, Phase 10.6.1R `VALIDATED`, Phase 10.7 `VALIDATED`)  
> **Objective**: Connect the physical simulation substrate to destruction events (Track A) and atmospheric visuals (Track B).  
> **Scope Firewall**: Phase 10 strictly maintained isolation from creature AI, taming, devour, inventory, and full weather simulation.

```
                                  PHASE 10
                         WORLD IMPACT & ATMOSPHERE
                                       │
           ┌───────────────────────────┴───────────────────────────┐
           ▼                                                       ▼
        TRACK A:                                                TRACK B:
 WORLD IMPACT & CSG DESTRUCTION                          SKY & ATMOSPHERE
           │                                                       │
    10.1 Impact Foundation (VALIDATED)                      10.5 Sky Foundation (VALIDATED)
    10.2 Terrain Mutation / CSG (VALIDATED)                 10.5.x Debug Console & Camera (VALIDATED)
    10.3 Impact -> Structure -> Physics (VALIDATED)         10.6 Procedural Aurora V1 (VALIDATED)
    10.4 CSG Hardening & Revert (VALIDATED)                 10.6.1 Procedural Aurora V2 (VALIDATED)
           │                                                10.6.1R Sky Shader Opt (VALIDATED)
           │                                                10.7 Integration Stress (VALIDATED)
           │                                                       │
           └───────────────────────────┬───────────────────────────┘
                                       ▼
                        10.7 Final Phase 10 Validation (PASS)
```

### Track A: World Impact & Destruction
- **Phase 10.1 — Impact Foundation (`COMPLETED / VALIDATED`)**:
  - `ImpactEvent` representation: source type (`ImpactSource`), world position, impact normal and direction, magnitude (`ImpactMagnitude`: energy vs impulse distinction), and bounded radius.
  - Generic spatial query (`AffectedVolume`) for affected voxel volumes and chunk bounds using authoritative Euclidean floor division (`div_euclid`) for seamless negative coordinate handling.
  - `DeterministicImpactPipeline`: deterministic sorting, deduplication, and pure observational queries without mutating world state.
  - Verified with 17 focused unit tests (`tests/impact_tests.rs`) and Benchmark 50 ($21.9\text{ ns}$ construction, $40.4\text{ ns}$ volume query).
- **Phase 10.2 — Terrain Mutation & CSG Foundation (`COMPLETED / VALIDATED`)**:
  - `VoxelEdit` and `VoxelEditOperation`: `Add` (on air), `Remove` (on solid), and `Replace` (with optional precondition).
  - `VoxelEditTransaction`: Atomic multi-chunk commit guarantee; non-mutating `validate(&store)` returns inspectable `ProposedDelta`.
  - Infallible commit phase with rollback guarantee: zero partial mutations on failure.
  - `CraterGenerator`: Bounded, deterministic $O(r^3)$ spherical crater generation with Euclidean floor division across negative coordinates.
  - `MaterialDestructionPolicy`: Configurable indestructible material preservation (e.g. `AG_CORE_CASING`).
  - Dual invalidation signals: in-memory `MESH_DIRTY` (including border neighbor propagation) and `StructuralEvent` notifications without triggering structural BFS or physics.
  - Verified with 27 focused unit tests (`tests/csg_tests.rs`) and Benchmark 51.
- **Phase 10.3 — Impact $\to$ Structure $\to$ Physics Pipeline (`COMPLETED / VALIDATED`)**:
  - `ImpactBridge`: Coordinates two-phase pipeline between `VoxelEditCommitResult`, `StructuralSystem`, and `PhysicsWorld`.
  - Phase A: Authoritative structural detachment extraction without physics coupling.
  - Phase B: Physicalization into `DynamicAggregateRecord`, generating `RigidBody` with greedy colliders, and distance-attenuated blast impulse application.
  - Reintegration guards preserving single dynamic ownership and transactional settlement.
  - Verified at commit `a7bf5f3` with 34 integration tests (`tests/impact_physics_integration_tests.rs`) and Benchmark 52.
- **Phase 10.4 — CSG / Destruction Hardening (`COMPLETED / VALIDATED`)**:
  - Arbitrary multi-chunk boundary mutation (Add, Remove, Replace).
  - Negative coordinate boundary stability ($x = -1, -32, -33$) with zero coordinate-sign asymmetry.
  - 6-face and corner boundary mesh invalidation with unloaded neighbor protection (`UNLOADED != AIR`).
  - Transactional pre-commit state capture (`ChunkPreState`: voxels, `non_air_count`, `dirty_flags`, `revision`) and preflight-safe `revert(&mut store)`.
  - Deterministic replay verification and strict LastWriteWins transaction-order preservation.
  - Zero coupling to downstream physics, structural BFS, or persistence workers.
  - Verified with 45 hardening tests (`tests/csg_hardening_tests.rs`) and Benchmark 53 (4 measurement profiles).

### Track B: Sky & Atmosphere
- **Phase 10.5 — Sky & Atmosphere Foundation (`COMPLETED / VALIDATED`)**:
  - Fullscreen procedural sky rendered after opaque voxel geometry in the primary render pass with `LessEqual` depth compare ($z = 1.0$) and `depth_write = false`. Depth testing against already-rendered opaque terrain rejects occluded sky pixels; GPU early/hierarchical depth optimization may reduce fragment work, subject to hardware implementation.
  - Continuous day/night cycle via `EnvironmentClock` with `day_fraction \in [0.0, 1.0)`, time string formatting, continuous moon phase, and bounded star phase.
  - Explicit celestial coordinate conventions (+Y up, right-handed view transform) with verified semantic anchors (midnight $(0,-1,0)$, sunrise $(+1,0,0)$, noon $(0,+1,0)$, sunset $(-1,0,0)$).
  - Deterministic celestial clock deriving moon direction with an explicit $5.0^\circ$ declination tilt.
  - Continuous moon phase $\in [0, 1)$ driving dynamic 3D sphere terminator shading in WGSL, with 8-phase enum for classification/debug/UI.
  - Twilight smooth cosine bell curve centered on horizon crossing ($|e| \le 0.20$) ensuring $C^1$ continuity and smooth color transitions.
  - Temporally stable 3D angular hash star field on the celestial sphere, invariant under camera translation and rotation, with twinkle animation and daytime/twilight suppression.
  - Unified `EnvironmentState` driving both `SkyUniform` and the existing `LightUniform` (preserving existing terrain lighting pipeline).
  - Single authority for environment clock: default production cycle is $1200.0\,\text{s}$ ($20.0\,\text{min}$); accelerated test cycle is $240.0\,\text{s}$.
  - Constraints respected: **NO HDRI dependencies, NO giant skybox textures, NO expensive volumetric raymarching**. 60 FPS target on integrated GPUs.
  - Verified with 23 regression tests (`tests/sky_environment_tests.rs`), 7-stage validation binary (`src/bin/sky_validation.rs`), and Benchmark 54 (3 CPU profiles).
- **Phase 10.5.x — Developer Debug Console & Camera Tooling (`COMPLETED / VALIDATED`)**:
  - Production-grade developer command console toggled via backquote (`` ` ``) or `F1`.
  - Decoupled developer free camera mode (`camera free` / `camera player`): independent camera instance, 6-DOF flight, speeds in $[0.1, 500.0]\,\text{m/s}$, preserving player world coordinates, velocity, and ground contact without mutation.
  - Single mutable authority for environment time progression in `EnvironmentClock` (`pause`, `resume`, time scale in $(0.0, 1000.0]$, day fraction set).
  - Pure Bounded Command Parser: Unicode scalar iteration, single/double quoting with whitespace normalization, 4096-byte input hard cap.
  - Command Registry with self-documenting metadata (`help`, `help <command>`), decoupled output (`CommandResult::Clear`), read-only player telemetry snapshot.
  - Zero-overhead when closed: exactly 0 vertices, 0 GPU buffer writes, and 0 draw calls when closed.
  - Embedded ASCII font ($128 \times 48$ atlas, 760-byte bitmasks) with deterministic fallback to `?` for non-ASCII/unsupported characters.
  - Verified with 19 focused integration tests (`tests/console_tooling_tests.rs`).
- **Phase 10.6 — Procedural Aurora V1 (`COMPLETED / VALIDATED`)**:
  - Animated multi-band procedural aurora borealis across night skies with latitude modulation.
  - Layered noise evaluation with temporal phase animation.
  - Verified with focused regression tests in `tests/sky_environment_tests.rs`.
- **Phase 10.6.1 — Procedural Aurora V2 (`COMPLETED / VALIDATED`)**:
  - Closed-form trigonometric curtain folds replacing brute-force iteration.
  - Dual-layer altitude modeling and emission color gradients.
  - Verified with 18 regression tests in `tests/sky_environment_tests.rs`.
- **Phase 10.6.1R — Sky Shader Optimization (`COMPLETED / VALIDATED`)**:
  - Loop unrolling, transcendental function factoring, and branching reduction in `src/sky.wgsl`.
  - Achieved a measured 2.08x GPU fragment throughput gain.
- **Phase 10.7 — Integration & Visual Stress (`COMPLETED / VALIDATED`)**:
  - Concurrent verification: heavy terrain impact destruction occurring simultaneously with real-time sky rendering and streaming traversal.
  - Verified via standalone binary `src/bin/stress_10_7.rs` maintaining $\ge 60\text{ FPS}$ under active destruction and dynamic sky.

---

## 4. Master Roadmap (Phases 11 — 25+)

### PHASE 11 — PLAYER ↔ WORLD INTERACTION (`IN PROGRESS`)
- **Phase 11.1 — Interaction Foundation (`COMPLETED / VALIDATED`)**:
  - Deterministic 3D DDA voxel raycast query (`raycast_voxels`) against authoritative `ChunkStore`.
  - Player eye origin integration from `PlayerController::eye_position()`.
  - Configurable interaction reach (`PlayerConfig::interaction_reach`, default 5.0m = 10 voxels) with inclusive boundary condition ($t \le \text{max\_reach}$).
  - Exact voxel hit representation (`VoxelHit`) with coordinate, material, hit point, distance, face, and canonical outward normal (+X, -X, +Y, -Y, +Z, -Z).
  - Continuous Euclidean division for negative coordinate traversal and chunk boundary crossing.
  - Strict residency awareness: non-resident space returns `NonResident` without triggering world generation or disk I/O.
  - Zero heap allocations during raycast traversal ($O(\text{reach} / \text{voxel\_size})$ on stack).
  - Verified with 14 unit tests (`tests/interaction_tests.rs`).
- **Phase 11.2 — Voxel Interaction (`COMPLETED / VALIDATED`)**:
  - Validated voxel removal (`can_remove`) against solid resident targets within reach.
  - Validated adjacent voxel placement (`can_place`) along targeted face normal into resident empty air.
  - Authoritative Player Capsule Overlap Guard (`PlayerCapsuleOverlap`): candidate AABB tested against `player.current_capsule().intersects_aabb(...)`, preventing suffocating or trapping the player for both standing ($1.8\text{m}$) and crouching ($1.2\text{m}$) profiles.
  - Atomic multi-chunk transactions reusing `VoxelEditTransaction` with preflight validation (zero partial writes on failure).
  - Downstream integration: `World::commit_voxel_transaction` reconciles `StructuralSystem` connectivity, physicalizes detached aggregates as `DynamicBody` in `PhysicsWorld`, wakes resting bodies via `handle_static_terrain_mutation`, and marks affected & boundary chunks `MESH_DIRTY` for `ChunkScheduler`.
  - Interaction debounce cooldown (`InteractionCooldown`, default: 0.20s = 5 actions/sec) preventing multi-block destruction per frame.
- **Phase 11.3 — Resource Gathering Primitives (`COMPLETED / VALIDATED`)**:
  - Data-driven `HarvestableComponent` integrated into core block schemas (`content/core/blocks/`).
  - Runtime `ResourceGatheringRegistry` mapping materials and persistent `ResourceId` to `ResourceDefinition`.
  - Deterministic yield evaluation (`calculate_yield`) with zero random variance.
  - Authoritative reach validation ($\le 5.0\text{m}$) and chunk residency checks (`TargetNotResident`).
  - Preflight rejection for air targets (`TargetIsAir`) and unmapped solid blocks (`NotHarvestable`).
  - Atomic voxel removal via `execute_gather_transaction` producing semantic `CollectionResult` on success.
  - Event-driven structural integrity integration: gathering structural supports detaches unanchored voxels into `PhysicsRuntime` as `DynamicBody`.
  - Mesh dirty flag propagation to host and boundary neighbor chunks.
  - Player debounce cooldown rate-limiting (`InteractionCooldown`).
  - Strict architectural firewalls: zero inventory mutation, zero tool dependencies, zero dropped physical items, zero GPU/renderer dependencies.
  - Verified with 27 unit and integration tests (`tests/gathering_tests.rs`).
- **Phase 11.4 — Block Placement & Build Rules (`COMPLETED / VALIDATED`)**:
  - Deterministic 6-face placement proposal generation (`PlacementProposal`) with strict semantic preview firewall (zero visual ghost renderer, zero GPU allocations).
  - Discrete block orientation (`BlockOrientation::Default`, `BlockOrientation::Facing(FaceDirection)`), decoupled from target face.
  - Data-driven build rule definition (`BuildComponent`, `SupportRule`), authoritative in `BlockDefinition` and indexed by `ResourceId` in runtime `BuildRuleRegistry`.
  - Semantic support rules (`AnyAdjacent`, `FloorOnly`, `AttachmentFace`, `None`), distinguishing `SupportNotResident` from `AIR`.
  - Reused player capsule clearance (`player.current_capsule().intersects_aabb(...)`) for standing and crouching postures.
  - Authoritative final commit re-validation (`validate_placement_proposal`) ensuring stale proposals cannot bypass live world changes.
  - Atomic mutation via existing `VoxelEditTransaction` and `World::commit_voxel_transaction()`, preserving structural connectivity, dynamic aggregate extraction, and asynchronous mesh invalidation.
  - Rate-limiting debounce cooldown reuse (`InteractionCooldown`).
  - Verified with 15 focused unit and integration tests (`tests/placement_rules_tests.rs`).
- **Phase 11.5 — Tools & Tool Actions (`COMPLETED / VALIDATED`)**:
  - Semantically distinct `ToolId` architecture (namespaced, zero implicit conversion to `ResourceId`).
  - Content-authoritative tool requirement definition in `HarvestableComponent.required_tool` (`None`, `AnyTool`, `Category`, `Specific`).
  - Runtime `ToolRegistry` with verified core tools (`stone_pickaxe`, `stone_axe`, `stone_shovel`, `generic_tool`).
  - Invariant durability validation: `ToolDefinition.max_durability` is authoritative; invalid state (`current > max`) is rejected without silent repair.
  - Infallible post-commit durability decrement (`saturating_sub(1)` executed only after world CSG transaction commit succeeds).
  - Floating-point validation firewall: strictly finite, non-negative base efficiency and resource multipliers.
  - Effectiveness firewall: `ToolEffectiveness` acts as semantic metadata only; base yield quantity remains strictly unmodified.
  - Backward compatibility: hand gathering preserved for `ToolRequirement::None`.
  - Rate-limiting debounce cooldown integration (`InteractionCooldown`).
  - Verified with 21 focused unit and integration tests (`tests/tool_action_tests.rs`).
- **Phase 11.6 — Generic Interactable World Objects & Feedback (`COMPLETED / VALIDATED`)**:
  - Smallest useful semantic interaction seam for non-voxel-breaking world interactions (switches, levers, doors, examine targets).
  - Semantically distinct `InteractableId` (`namespace:path`, isolated from `ResourceId` and `ToolId`).
  - Content-authoritative `InteractableComponent` in `BlockDefinition`, with derived runtime `InteractableRegistry`.
  - Stale-state validation invariant: `MaterialId` is a consistency/sanity check, not sufficient object identity (`current_id == instance.interactable_id && current_material == instance.expected_material`).
  - Truly read-only query paths (`detect_interactable_target`, `query_interactable_target` taking immutable `&World`), ignoring stale instances with zero registry mutation.
  - Primitive deterministic action set (`Activate`, `Toggle`, `Open`, `Close`, `Examine`) and state set (`Idle`, `Active`, `Open`, `Closed`, `Disabled`).
  - Unambiguous cooldown ownership in `handle_player_generic_interaction()`; cooldown is checked early and triggered strictly post-commit after successful state transition.
  - Purely semantic feedback data (`InteractionFeedback`, `AudioCue`, `VisualCue`, `FeedbackId`) with zero strings, lore copy, UI, or rendering side effects.
  - Mandatory TOCTOU revalidation in `execute_interaction()`: verifies `current_id == proposal.id`, `current_material == proposal.expected_material`, and `current_state == proposal.previous_state` before atomic commit.
  - Verified with 28 unit and integration tests (`tests/interactable_tests.rs`) including all 4 mandatory TOCTOU edge cases.
- **Phase 11.7 — Interaction Audio / Feedback Integration (`PLANNED / NEXT`)**:
  - Audio and visual runtime binding for interaction feedback cues.

### PHASE 12 — ENTITY & CREATURE FOUNDATION (`PLANNED`)
- Entity identity architecture decoupled from voxel data.
- Data-driven `CreatureDefinition`: base stats, health, speed, mass, perception radius.
- Creature classification hierarchy:
  1. **Common Wildlife**: Sheep, deer, wolves, boar, birds.
  2. **Rare Beasts**: Embergator, Frostclaw, Thunderbeast.
  3. **Grand Beasts**: Multi-segment wandering mini-bosses.
  4. **Mega Beasts**: Giant roaming environmental titans.
  5. **Mythical Beasts**: Ancient apex creatures with unique evolutionary traits.
- Deterministic spawning/despawning lifecycles and spatial density caps.
- AI state machine: Idle, Graze, Flee, Alert, Chase, Attack, Stunned, Dead.

### PHASE 13 — COMBAT & ACTION SYSTEM (`PLANNED`)
- Unified action architecture (reusable across player and creatures).
- Primary attacks, secondary attacks, heavy strikes, dodges, and special abilities.
- Continuous swept hitboxes, damage calculations, and directional knockback impulses.
- Physical integration: heavy attacks trigger environmental damage via `ImpactEvent`.

### PHASE 14 — CREATURE ACQUISITION / TAMING / CATCHER (`PLANNED`)
- Creature capture mechanics inspired by Chimeraland's high-stakes taming.
- **20% Health Rule**: Beasts must generally be weakened to $\le 20\%$ health before capture attempts unlock.
- Catcher items: data-driven tiers (Wood $\to$ Iron $\to$ Crystal $\to$ Mythic).
- Catcher durability and level compatibility (low-tier catchers cannot capture Grand/Mythic beasts).
- Probabilistic capture roll with multi-attempt tension and escape behaviors.

### PHASE 15 — EGGS, TEMPORARY CREATURES & PETS (`PLANNED`)
- Successful capture outcomes:
  - **Beast Egg**: Permanent pet potential. Requires incubation and hatching.
  - **Temporary Creature**: Immediate temporary summon for combat assistance or battle sparring.
- Permanent pet management: persistent companion slots, leveling, summon/dismiss lifecycle.
- Pet sparring arena: summoning temporary creatures to fight the player's pet to trigger devour opportunities.

### PHASE 16 — DEVOUR & MODULAR EVOLUTION (`PLANNED`)
- **Core Gameplay Identity**: Evolving pets through biological trait acquisition.
- Devour mechanic: defeated temporary beasts or defeated rivals can be devoured by the player's active pet.
- Anatomical component acquisition:
  - Base body retained (e.g., Deer).
  - Devours Falcon $\to$ gains Falcon Wings (unlocks flight / glide).
  - Devours Wolf $\to$ gains Wolf Head (unlocks bite attacks and howling buffs).
- Genetic compatibility matrix: valid vs conflicting mutations.
- Emergent chimera phenotypes: visually and mechanically distinct modular evolved beasts.

### PHASE 17 — PLAYER TRANSFORMATION (`PLANNED`)
- Human ↔ Beast transformation mechanics.
- Seamless swapping of collision capsule, camera offsets, movement profiles, and ability sets.
- Invariant: Logical player identity remains constant while physical profile shifts.

### PHASE 18 — BLOCKBENCH CREATURE PIPELINE & ANIMATION (`PLANNED`)
- Blockbench model format importer (JSON / GLTF).
- Skeletal hierarchy, bone attachments, and modular socket system (head socket, wing sockets, tail socket).
- Animation state machine: Idle, Walk, Run, Attack, Devour, Roar, Flinch, Die.
- Dynamic mesh composition for hybrid chimera bodies.

### PHASE 19 — INVENTORY, EQUIPMENT & CRAFTING (`PLANNED`)
- Grid inventory, weight/volume restrictions, container storage.
- Data-driven loot tables from defeated creatures:
  - Wolf: Wolf Fur + Meat.
  - Deer: Venison Meat + Antlers.
  - Embergator: Emberscale + Fire Gland.
- Crafting benches, weapons, armor, catchers, and cooking recipes.

### PHASE 20 — MATERIALS, TEXTURING & ADVANCED RENDERING (`PLANNED`)
- Physically-based material properties for voxel terrain and creatures.
- Foliage shader animations (wind sway, player grass parting).
- Particle VFX: blood splatters, dust clouds, impact sparks, elemental breath.
- Eventual hierarchical geometry LOD if streaming distance demands it.

### PHASE 21 — USER INTERFACE & HUD (`PLANNED`)
- Minimalist immersive HUD: Health bar, stamina, hotbar, compass.
- Pet management UI: stats, equipped devoured parts, evolution tree.
- Catcher targeting UI, capture percentage indicators, interaction reticle.
- Inventory, crafting, and creature codex menus.

### PHASE 22 — WORLD SYSTEMS & ADVANCED ECOLOGY (`PLANNED`)
- Living world ecosystem: predator/prey behaviors, herd migration.
- Dynamic environmental weather events: rainstorms, blizzards, sandstorms.
- Day/night gameplay effects: nocturnal predators, moon-phase beast behaviors.

### PHASE 23 — PROGRESSION, SURVIVAL & ECONOMY (`PLANNED`)
- Player level curve, stat allocations, survival meters (hunger, temperature).
- Beast tier progression from novice hunter to Mythic Beast tamer.
- Resource economy and trading outpost systems.

### PHASE 24 — CONTENT EXPANSION & BOSSES (`PLANNED`)
- Population of 50+ unique creature species across biomes.
- Grand and Mythical roaming world bosses.
- Unique evolutionary branches and rare mutation discoveries.

### PHASE 25+ — OPTIMIZATION, POLISH & RELEASE (`PLANNED`)
- Whole-system profiling, cache locality optimization, SIMD math acceleration.
- Audio engine: dynamic soundscapes, creature vocalizations, spatial audio.
- Save/load hardening, modding API documentation, full community QA.

---

## 5. Scope Firewall Rules

To protect architectural integrity, every developer and AI agent must adhere to the **Scope Firewall**:
1. **A phase must never silently absorb future systems**: Do not implement combat in Phase 10; do not implement devour in Phase 12; do not implement inventory in Phase 11.
2. **Interfaces before implementation**: A phase may define a minimal generic contract (e.g., `ImpactEvent` in Phase 10) needed for future integration, but must not implement the future consumers prematurely.
3. **No parallel physics or rendering loops**: New systems must connect to existing `ChunkStore`, `DynamicBody`, and `PhysicsWorld` abstractions rather than inventing ad-hoc physics.
