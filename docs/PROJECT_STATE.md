# Omnisia — Authoritative Project State

> **Current Milestone**: Phase 9.12 — Stress / Performance Validation (**COMPLETED / APPROVED**)  
> **Next Milestone**: Phase 10 — World Impact & Atmosphere (**PLANNED / NEXT**)  
> **Verified HEAD Commit**: `4f60bd6`  
> **Branch**: `main`  
> **Source-of-Truth Policy**: Source code and passing automated tests are the absolute source of truth.

---

## 1. Executive Status & Test Metrics

Every count in this document has been directly audited and verified against the live repository at commit `4f60bd6`.

### Quality Gates Status
- **Compiler**: Rust 2021 edition, `cargo build --release` compiles cleanly with zero errors.
- **Formatting**: `cargo fmt --all -- --check` passes cleanly with zero diffs.
- **Linter**: `cargo clippy --all-targets --all-features -- -D warnings` passes cleanly with zero warnings or errors.
- **Test Suite**: `cargo test --all-targets` passes with **660 passed, 0 failed, 0 ignored** across 11 test targets.

### Test Count Breakdown by Target (660 Total Tests)

| Test Target Binary | Scope / Feature Area | Verified Test Count | Status |
|:---|:---|:---:|:---:|
| `tests/physics_9_tests.rs` | RigidBody Physics, Broadphase, Contacts, Solver, Sleeping, Islands, Dynamic-Dynamic, Bridges, Stress (Phase 9.1–9.12) | **415** | PASS |
| `tests/movement_8d_tests.rs` | Kinematic Movement, Bounded Glide, Auto-Step Traversal, Slope Resolution (Phase 8D) | **81** | PASS |
| `tests/integration_8c_tests.rs` | Player ↔ World ↔ DynamicBody Integration, Reintegration Lifecycle (Phase 8C) | **32** | PASS |
| `tests/player_tests.rs` | Kinematic Capsule Controller, Clearance Guard, Jump Edge-Trigger, Swept Collision (Phase 8B) | **30** | PASS |
| `tests/worldgen_tests.rs` | Deterministic Seed, Climate, Hydrology, Caves, Strata, Ores, Biomes (Phase 4–5) | **26** | PASS |
| `tests/physics_tests.rs` | Dynamic Aggregate Baseline, Gravity/AntiGravity, Swept Snapping (Phase 8A) | **23** | PASS |
| `tests/engine_tests.rs` | Canonical Indexing, Ambient Occlusion, Culled & Greedy Meshing, Frustum (Phase 1–3) | **13** | PASS |
| `tests/modding_tests.rs` | Mod Discovery, Manifest Parsing, SemVer, Safe Overrides, ResourceId (Phase 2–2.5) | **11** | PASS |
| `tests/streaming_tests.rs` | ChunkStore Residency, Job Scheduler, Memory Budget, Eviction Lifecycle (Phase 3) | **11** | PASS |
| `tests/structure_tests.rs` | 6-Connected Adjacency, Structural Anchors, Aggregate Extraction (Phase 7) | **11** | PASS |
| `tests/scale_tests.rs` | Metric Coordinate Invariants (1 vx = 0.5m), Scale Ruler, Traversal Residency (Phase 7) | **7** | PASS |
| **Workspace Total** | **All Subsystems** | **660** | **PASS** |

### Verified Validation Binaries (6 Binaries)

| Binary Path | Executable Target | Verification Summary | Status |
|:---|:---|:---|:---:|
| `src/bin/stress_validation.rs` | `cargo run --release --bin stress_validation` | Workloads A–K: Sparse/Dense Scaling, Sleeping, Islands, Stacks, Bridges, 10k Steps, Determinism | **PASS (1.41 s)** |
| `src/bin/physics_validation.rs` | `cargo run --release --bin physics_validation` | Dynamic aggregate lifecycle, AntiGravity floating, unloaded boundary protection | **PASS (5/5 stages)** |
| `src/bin/integration_validation.rs` | `cargo run --release --bin integration_validation` | Structural detachment, DynamicBody interaction, two-phase atomic reintegration | **PASS (6/6 stages)** |
| `src/bin/player_validation.rs` | `cargo run --release --bin player_validation` | Kinematic locomotion, capsule crouching, jump edge-trigger, swept anti-tunneling | **PASS (8/8 stages)** |
| `src/bin/traversal_validation.rs` | `cargo run --release --bin traversal_validation` | Multi-kilometer flight (-1km to +1km), streaming residency audit, memory stability | **PASS (10/10 stages)** |
| `src/bin/benchmarks.rs` | `cargo run --release --bin benchmarks` | Full performance test suite covering 49 isolated benchmarks | **PASS (49/49 runs)** |

---

## 2. Four-State Subsystem Implementation Matrix

Under Omnisia's engineering standard, every subsystem must be explicitly classified into one of four states:
- **`IMPLEMENTED`**: Production implementation exists in the codebase.
- **`VALIDATED`**: Implementation is verified by unit tests, validation binaries, and benchmarks.
- **`PLANNED`**: Scheduled for an upcoming phase; design/requirements defined, production implementation not started.
- **`UNKNOWN / NEEDS VERIFICATION`**: Status requires architectural investigation before proceeding.

| Subsystem / Feature Domain | Architectural State | Location in Repository | Notes / Verification Evidence |
|:---|:---:|:---|:---|
| **Voxel Storage & Indexing** | `VALIDATED` | `src/voxel.rs`, `src/chunk.rs`, `src/storage.rs` | Inlined $O(1)$ indexing, 32³ chunks, 128 KiB memory per chunk. |
| **Meshing Pipeline** | `VALIDATED` | `src/mesh/` (`culled.rs`, `greedy.rs`, `ao.rs`) | Culled & Greedy meshing (29x quad reduction), AO face calculation. |
| **Hierarchical Streaming & Residency** | `VALIDATED` | `src/streaming/` (`store.rs`, `scheduler.rs`, `eviction.rs`) | Authoritative `ChunkStore`, memory budget, thread-pool job queue. |
| **Procedural World Generation** | `VALIDATED` | `src/worldgen/` (biomes, climate, noise, caves, strata, ores) | Deterministic 3D volumetric generation, multi-octave fBm, worm tunnels. |
| **Vegetation & Ecology** | `VALIDATED` | `src/worldgen/vegetation.rs` | Deterministic cross-chunk tree stamping (Oak, Pine, Shrub, Grass). |
| **Modding & Asset Pipeline** | `VALIDATED` | `src/modding/` (`registry.rs`, `manifest.rs`, `resource_id.rs`) | Data-driven block/material registry, namespaced IDs, SemVer dependency solver. |
| **Event-Driven Structural Connectivity** | `VALIDATED` | `src/structure/` (`manager.rs`, `connectivity.rs`, `anchor.rs`) | 6-connected BFS, early anchor exit, detached aggregate extraction. |
| **Metric Scale Invariants** | `VALIDATED` | `src/scale.rs`, `tests/scale_tests.rs` | 1 voxel = 0.5m, human ref = 1.8m (3.6 vx), Euclidean floor coordinates. |
| **Kinematic Player Controller** | `VALIDATED` | `src/player/` (`controller.rs`, `collision.rs`, `config.rs`) | Capsule swept collision, auto-step (0.55m), bounded glide, slope sliding. |
| **DynamicBody Aggregate Runtime** | `VALIDATED` | `src/physics/runtime.rs`, `src/physics/body.rs` | Single ownership, 30 Hz fixed timestep, two-phase atomic reintegration. |
| **RigidBody Physics Core (9.1–9.9)** | `VALIDATED` | `src/physics/` (`world.rs`, `narrowphase.rs`, `solver.rs`, `island.rs`) | Broadphase grid, box-box SAT, sequential impulse solver, islands, sleeping. |
| **Player ↔ RigidBody Bridge (9.10)** | `VALIDATED` | `src/physics/player_bridge.rs` | Kinematic pushing, dynamic carrying, zero solver mass for player. |
| **Structural Aggregate Bridge (9.11)** | `VALIDATED` | `src/physics/aggregate.rs` | 1 aggregate = 1 RigidBody ($M$ colliders), Parallel Axis Theorem inertia. |
| **Physics Stress & Profiling (9.12)** | `VALIDATED` | `src/bin/stress_validation.rs`, `src/physics/world.rs` | Linear scaling up to 10,000 bodies, 45% sleeping CPU reduction, 1.41s suite. |
| **World Impact / Destruction** | `PLANNED` | — (Phase 10.1–10.4) | `ImpactEvent`, crater carving, CSG mutation, structural detachment bridge. |
| **Procedural Sky & Atmosphere** | `PLANNED` | — (Phase 10.5–10.7) | GPU-driven atmospheric gradient, sun/moon cycle, stars, procedural aurora. |
| **Player World Interaction (Tools/Harvest)**| `PLANNED` | — (Phase 11) | Voxel targeting, block placing/breaking, harvesting tools, interaction raycast. |
| **Entity & Creature Foundation** | `PLANNED` | — (Phase 12) | `CreatureDefinition`, stats, health, AI states, animal & beast hierarchy. |
| **Combat & Action System** | `PLANNED` | — (Phase 13) | Attack actions, hitboxes, damage, knockback, dodge, action architecture. |
| **Creature Taming / Catcher** | `PLANNED` | — (Phase 14) | 20% health threshold, catcher durability, capture probability, beast tiers. |
| **Pets, Eggs & Summons** | `PLANNED` | — (Phase 15) | Permanent pet slots, egg incubation, temporary battle summons. |
| **Devour & Modular Evolution** | `PLANNED` | — (Phase 16) | Body-part acquisition, genetic mutations, phenotype composition, hybrid beasts. |
| **Player Transformation** | `PLANNED` | — (Phase 17) | Human ↔ Beast form swapping, profile switching, persistent identity. |
| **Blockbench Creature Pipeline** | `PLANNED` | — (Phase 18) | Skeletal meshes, animations, attachment points, modular body parts. |
| **Inventory & Crafting** | `PLANNED` | — (Phase 19) | Data-driven loot, weapons, armor, recipes, durability, resource economy. |
| **Full LOD Meshing** | `DEFERRED` | — (Phase 20 / Phase 25+) | Deferred until world scale/render distance demands hierarchical geometry. |
| **Volumetric Clouds & Weather Simulation** | `DEFERRED` | — (Phase 22) | Deferred; Phase 10 implements lightweight procedural sky/aurora only. |
| **Multiplayer / Networking** | `DEFERRED` | — (Post-v1.0) | Authoritative single-player engine architecture prioritized first. |

---

## 3. Authoritative vs. Derived / Cache State Boundaries

Omnisia strictly maintains architectural firewalls between simulation substrates:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                      AUTHORITATIVE SIMULATION STATE                         │
├──────────────────────────────────────┬──────────────────────────────────────┤
│ 1. Static Voxel Terrain              │ ChunkStore (Dense 32³ Arrays)        │
│ 2. Dynamic Physical Aggregates       │ DynamicAggregateRecord / DynamicBody │
│ 3. Rigid Body Simulation             │ PhysicsWorld (Bodies, Islands, AABBs)│
│ 4. Player Kinematic Controller       │ PlayerController (Capsule, State)    │
│ 5. Data & Content Registry           │ Registry (Blocks, Materials, Mods)   │
└──────────────────────────────────────┴──────────────────────────────────────┘
                                  │
                                  ▼ [Derivation / Event Triggers]
┌─────────────────────────────────────────────────────────────────────────────┐
│                       DERIVED & CACHED STATE (DISCARDABLE)                  │
├──────────────────────────────────────┬──────────────────────────────────────┤
│ 1. GPU Vertex/Index Buffers          │ MeshCache (Culled/Greedy Meshes)     │
│ 2. Structural Connectivity Graph     │ Local BFS on-demand (No per-tick BFS)│
│ 3. Broadphase Acceleration Proxies   │ Spatial Grid Cells                   │
│ 4. Collision Contact Cache           │ Transient Manifold Contacts          │
│ 5. Frame Render Interpolation        │ Visual Transform Smoothing           │
└──────────────────────────────────────┴──────────────────────────────────────┘
```

---

## 4. Hard Architectural Invariants (25 Rules)

These 25 invariants are binding on all current and future phases. Any pull request or commit that violates them must be rejected:

1. **`ChunkStore` is Authoritative**: All static voxel state lives exclusively in `ChunkStore`. No secondary store or renderer buffer may hold authoritative voxel data.
2. **GPU Mesh State is Derived / Cache State**: GPU vertex/index buffers are transient and reconstructible on demand from `ChunkStore`. The renderer is never authoritative.
3. **Voxel Data is Not Physics State**: Voxels are lattice cells ($0.5\text{m}$ integer grid). They do not have velocities, mass tensors, or forces.
4. **Structural Connectivity is Distinct from Physics**: Connectivity is an topological graph relationship evaluated on voxel events. Physics is a continuous Newtonian impulse simulation.
5. **Detached Aggregates are Represented by `DynamicBody`**: When voxels detach, they are moved out of `ChunkStore` into a `DynamicBody` owning a `DynamicAggregateRecord`.
6. **One Structural Aggregate = One `RigidBody`**: An aggregate of $N$ voxels maps to exactly **one** `RigidBody`. Under no circumstances may 1 voxel become 1 `RigidBody`.
7. **One `RigidBody` May Own Multiple Colliders**: A `RigidBody` owns 1 compound collider or $M \le N$ greedy merged colliders sharing a single body ID.
8. **Player is a Kinematic Character Controller**: The player is governed by kinematic swept capsule locomotion, step traversal, and gravity logic.
9. **Player is NOT a `RigidBody`**: The player has zero mass in the rigid body solver and is never constructed as a `RigidBody`.
10. **Player Must Not Enter `PhysicsWorld.rigid_bodies`**: The player ID must never appear in the rigid body registry.
11. **Player Must Not Enter `PhysicsIsland`**: The player is excluded from island partitioning, island sleeping, and island graph building.
12. **Player ↔ RigidBody Interaction Uses Established Bridge**: Dynamic box pushing and platform carrying must occur strictly via `PlayerRigidBodyBridge`.
13. **Physics Ticks Must Not Perform Structural BFS**: The 30 Hz physics step never scans chunk voxels or executes topological searches.
14. **Structural BFS is Event-Driven**: Connectivity checks are triggered only on `VoxelPlaced`, `VoxelRemoved`, or `VoxelReplaced`.
15. **Runtime Compact IDs are Not Persistent**: Internal sequential indices (`RigidBodyId`, `ColliderId`, `DynamicBodyId`) are runtime-only and must never be serialized as persistent asset keys.
16. **Persistent Resource Identity Must Remain Stable**: Modding and save systems use namespaced string keys (`core:stone`) mapped to stable IDs.
17. **No Synchronous Disk I/O in Hot Paths**: Render frames and 30 Hz physics steps must never trigger synchronous disk reads or writes.
18. **No `wgpu` GPU Buffers in Authoritative Simulation**: Simulation structures (`ChunkStore`, `PhysicsWorld`, `PlayerController`) must have zero dependencies on GPU device handles or rendering types.
19. **Negative Coordinates Use Consistent Euclidean / Floor Semantics**: Chunk coordinates use mathematical floor division (`coord.div_euclid(32)`), preserving continuous spatial hashing across negative boundaries ($x = -1, -32, -33$).
20. **Zero Double-Ownership of Voxels**: Every voxel is at all times either in `STATIC_WORLD`, `DYNAMIC_SIMULATION`, or `REINTEGRATING`. Duplicates are strictly audited and rejected.
21. **Reintegration is Transactional**: Reintegrating an aggregate into `ChunkStore` occurs in two phases (`prepare` $\to$ `commit`). If any target cell is occupied or unloaded, the transaction aborts with zero mutation.
22. **Determinism is an Explicit Design Objective**: Fixed timesteps, deterministic seed contexts, ordered containers (`BTreeMap`), and bitwise reproducible trajectories under identical conditions.
23. **Rendering Must Not Become Authoritative Gameplay State**: Visual effects, camera orientation, and display resolutions cannot dictate physics outcomes or gameplay rules.
24. **Future Gameplay Must Reuse Generic Engine Abstractions**: New systems (combat, impact, abilities) must build upon `ImpactEvent`, `DynamicBody`, and `RigidBody` rather than duplicating parallel physics loops.
25. **Creature Gameplay Identity is Separable from Visual Model**: Creature data, stats, AI, and collision profiles are logical structures that must not be tightly coupled to specific render meshes or Blockbench assets.

---

## 5. Known Limitations & Empirical Characterization

### A. Tall-Stack Solver Compliance Limitation
The current sequential impulse contact solver (10 iterations default) exhibits natural compliance under heavy vertical gravitational loads:
- **Stack Height 5**: Residual compression $= 0.041\,\text{m}$ (stable, settles cleanly).
- **Stack Height 10**: Residual compression $= 0.234\,\text{m}$ (stable, settles cleanly).
- **Stack Height 20**: Residual compression $= 1.045\,\text{m}$ (stable, settles cleanly).
- **Stack Height 50**: Residual compression $= 3.516\,\text{m}$ (unstable compliance / iterative jitter, does not settle).
- **Stack Height 100**: Residual compression $= 37.843\,\text{m}$ (severe compression / collapse limit).

*Architecture Policy*: Do NOT claim the solver is universally stable for arbitrarily tall stacks. Advanced stabilization (warm starting, shock propagation, split impulse, position projection) is intentionally deferred until gameplay requirements demand tall dynamic structures. In Omnisia, natural structures are anchored to terrain rather than simulating 50-tall free stacks.

### B. Determinism Scope
Determinism is verified as:  
*"Bitwise identical across repeated simulation runs under identical execution conditions and platform architectures."*  
Do not claim universal cross-platform or cross-compiler IEEE-754 bitwise parity without hardware abstraction layers.

### C. AABB Edge-Case Taxonomy
Extreme finite-value expansions or degenerate flat boxes may in rare edge cases trigger `InvalidAabb` error classification rather than being silently clipped.

### D. Validation Configuration Hygiene
The historical binary `src/bin/player_validation.rs` tests legacy Phase 8B movement constants (`walk_speed = 5.0 m/s`, `sprint_speed = 9.0 m/s`) via explicit config overrides. Production defaults in `PlayerConfig` remain `walk_speed = 3.0 m/s`, `sprint_speed = 6.0 m/s` (or Phase 8D/9 tuned values). This difference is intentional historical test isolation and must not be altered without updating test specifications.

### E. Allocation & Performance Claims
Benchmark results demonstrate high throughput (e.g., $10,000$ active bodies in $12.65\text{ ms}$, $10,000$ sleeping bodies in $10.61\text{ ms}$). Do not claim "zero heap allocations" in the physics loop unless instrumented with custom tracking allocators; transient vector scratchpads are utilized for manifold caching.

### F. Energy & Momentum Conservation
Kinematic character controller pushing and heuristic velocity clamping intentionally inject/dissipate energy to ensure crisp gameplay controls. Standard physical conservation laws apply to dynamic-dynamic rigid bodies, but bounded numerical stability language must be used whenever the kinematic player is involved.

---

## 6. Intentionally Deferred Systems

The following systems are **intentionally deferred** and must not be prematurely implemented:
1. **Full Hierarchical LOD Meshing**: Deferred until streaming distances and draw call limits require multi-resolution chunk rendering.
2. **Volumetric Cloud Simulation & Atmospheric Raymarching**: Deferred; Phase 10 implements lightweight, high-performance procedural gradients and aurora.
3. **Full Dynamic Weather Simulation**: Deferred to Phase 22 (World Systems); Phase 10 provides the celestial sky foundation only.
4. **Advanced Constraint Solvers (Shock Propagation / Split Impulse)**: Deferred until complex mechanical constraints or tall stacking gameplay is introduced.
5. **Multiplayer Networking & State Replication**: Deferred until the single-player simulation loop, creature systems, and world mutation layers are locked.
6. **Large-Scale Autonomous Pet Ecology**: Deferred to Phase 15/22; active pet limits will be enforced to protect frame budgets.

---

## 7. Source-of-Truth Policy

1. **Source Code is Authoritative for Implementation State**: If a document states a feature exists but the codebase contains no implementation, the source code wins.
2. **Automated Tests are Authoritative for Verified Behavior**: A feature is only verified if automated tests or validation binaries prove it executes correctly.
3. **Benchmarks are Evidence for Tested Workloads**: Benchmark numbers represent measured performance on specific hardware under specific workloads, not universal guarantees.
4. **Architecture Documents Explain Intent**: Architectural docs describe invariants, design rationale, and system relationships.
5. **Roadmaps Explain Future Intent**: Roadmaps outline planned work and must never be cited as evidence of current implementation.
