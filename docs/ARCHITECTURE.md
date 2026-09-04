# Omnisia — System Architecture & Source of Truth

> **Current Milestone**: Phase 9.12 — Stress / Performance Validation (`4f60bd6`)  
> **Core Architectural Paradigm**: Engine-First, Data-Driven, Deterministic Voxel Simulation with Authoritative Firewalls.

---

## 1. High-Level Architectural Overview

Omnisia separates simulation into distinct, non-overlapping architectural layers. Authoritative state lives strictly in designated memory models, while rendering and visual caches remain strictly derived and discardable.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                       AUTHORITATIVE SIMULATION LAYERS                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  [ LAYER 1: VOXEL TERRAIN SUBSTRATE ]                                       │
│  • ChunkStore: Sparse spatial map of 32³ dense voxel arrays                 │
│  • Single authoritative owner for static terrain                            │
│  • 1 voxel = 0.5m lattice cell (Metric unit standard)                       │
│                                                                             │
│  [ LAYER 2: STRUCTURAL CONNECTIVITY SUBSTRATE ]                             │
│  • StructuralSystem: Event-driven 6-connected adjacency graph               │
│  • Data-driven anchors: BlockComponents::structural_anchor                  │
│  • DetachedAggregate extraction (Zero per-tick BFS scans)                   │
│                                                                             │
│  [ LAYER 3: DYNAMIC AGGREGATE RUNTIME ]                                     │
│  • DynamicBody / DynamicAggregateRecord: Authoritative owner of detached vx │
│  • Mutual exclusivity: STATIC_WORLD <-> DYNAMIC_SIMULATION <-> REINTEGRATING│
│  • Two-phase transactional reintegration (prepare -> commit)                │
│                                                                             │
│  [ LAYER 4: RIGIDBODY PHYSICS ENGINE ]                                      │
│  • PhysicsWorld: Broadphase grid, box-box SAT narrowphase                   │
│  • Sequential impulse solver (normal, friction, restitution)                │
│  • Union-Find island partitioning & kinetic energy sleeping                 │
│  • 1 structural aggregate = 1 RigidBody owning M colliders                  │
│                                                                             │
│  [ LAYER 5: KINEMATIC CHARACTER CONTROLLER ]                                │
│  • PlayerController: Kinematic swept capsule locomotion                    │
│  • Stand clearance guard, auto-step (0.55m), bounded glide                 │
│  • Zero mass in physics solver; NEVER in rigid_bodies; NEVER in islands     │
│                                                                             │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           INTER-SUBSTRATE BRIDGES                           │
├─────────────────────────────────────────────────────────────────────────────┤
│  • PlayerRigidBodyBridge: Kinematic velocity transfer & box pushing         │
│  • StructuralAggregateBridge: Aggregate -> DynamicBody -> RigidBody         │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                     DERIVED & DISCARDABLE RENDER LAYERS                     │
├─────────────────────────────────────────────────────────────────────────────┤
│  • MeshCache: Culled & Greedy 32³ GPU vertex/index buffers (Metal / Vulkan) │
│  • Camera: First-person & developer free-flight perspective matrices        │
│  • Frustum Culling: Sub-microsecond bounding box visibility queries         │
│  • Renderers have ZERO authority over physics, collisions, or voxel data    │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Core Subsystems & Invariants

### 2.1 Authoritative Voxel Storage (`src/storage.rs`, `src/chunk.rs`)
- Chunks are uniform cubes of $32 \times 32 \times 32 = 32,768$ voxels.
- Voxels are compact 32-bit descriptors referencing data-driven materials in the registry.
- Voxel coordinates map to world metric space via $1\text{ voxel} = 0.5\text{m}$.
- **Euclidean / Floor Coordinate Rule**: Chunk indexing in negative space uses mathematical floor division:
  ```rust
  let chunk_coord = IVec3::new(
      world_x.div_euclid(32),
      world_y.div_euclid(32),
      world_z.div_euclid(32),
  );
  ```
  This guarantees seamless spatial continuity across negative boundaries ($x = -1, -32, -33$).

### 2.2 Event-Driven Structural Connectivity (`src/structure/`)
- Structural integrity is evaluated purely in response to voxel mutations: `VoxelPlaced`, `VoxelRemoved`, `VoxelReplaced`.
- **6-Connected Adjacency**: Voxels connect only along shared face planes ($\pm X, \pm Y, \pm Z$). Diagonal touches are not load-bearing.
- **Anchor Semantics**: Anchors are defined in mod data (`BlockComponents::structural_anchor`).
- **Early-Exit Traversal**: The localized BFS stops as soon as any valid anchor is reached (averaging 15 voxels checked).
- Under no circumstances does the physics loop or render loop perform chunk-wide BFS scans.

### 2.3 DynamicBody & Structural Reintegration (`src/physics/runtime.rs`, `src/physics/aggregate.rs`)
- When unanchored voxels detach, they are extracted into a `DetachedAggregate` and cleared from `ChunkStore`.
- They become a `DynamicBody` with a single authoritative `RigidBody`.
- **Two-Phase Reintegration**:
  1. `prepare_aggregate_reintegration`: Validates that target voxel cells in `ChunkStore` are loaded and empty (air). Evaluates orientation against the 24 proper rotations of the cubic octahedral group $O$.
  2. `commit_aggregate_reintegration`: Writes restored voxels into `ChunkStore`, invalidates mesh caches (`MESH_DIRTY`), and destroys the dynamic body.
  3. If conflict is detected, the transaction aborts with zero mutation.

### 2.4 RigidBody Physics Core (`src/physics/`)
- **Broadphase**: Uniform spatial hash grid. Spatial proxies synchronize AABBs every step.
- **Narrowphase**: SAT (Separating Axis Theorem) box-box collision detection producing contact manifolds with face-centered support vectors.
- **Contact Solver**: 10-iteration sequential impulse solver resolving non-penetration and Coulomb friction constraints.
- **Islands & Sleeping**: Touching dynamic bodies form connected islands via Union-Find. Islands where all bodies exhibit kinetic energy below the sleep threshold for $\ge 0.5\text{s}$ transition atomically to sleep. Static anchors act as absolute barriers to wake propagation.
- **Body Collider Indexing**: Indexed via `body_colliders: BTreeMap<RigidBodyId, Vec<ColliderId>>` ensuring $O(M)$ lookups without full-registry scans.

### 2.5 Kinematic Character Controller (`src/player/`)
- Player is a kinematic capsule (height $1.8\text{m}$, radius $0.3\text{m}$, crouch $1.2\text{m}$).
- Locomotion is evaluated at a fixed 30 Hz simulation frequency decoupled from display frame rates.
- **Clearance Guard**: Player cannot stand up if vertical overhead clearance $< 1.8\text{m}$.
- **Auto-Step Traversal**: Obstacles $\le 0.55\text{m}$ are smoothly climbed without vertical snagging.
- **Bounded Glide**: Glide activates only during descent ($v_y \le 0$) from a sprint-jump takeoff.
- **Hard Player Invariant**: The player is never a `RigidBody`, has zero mass in the solver, and is never registered in `PhysicsWorld.rigid_bodies` or `PhysicsIsland`.

### 2.6 Impact Event Foundation (`src/impact/`, Phase 10.1)
- **Generic Event Abstraction**: `ImpactEvent` describes position, direction, surface normal, magnitude, and radius without gameplay or explosion-specific assumptions.
- **Physical Distinction**: `ImpactMagnitude` strictly separates energy (Joules, scalar work capability) and impulse (Newton-seconds, momentum transfer).
- **Bounded Affected Volume**: `AffectedVolume` answers *which spatial region is affected* without answering *which voxels to destroy*. Uses authoritative Euclidean floor division (`div_euclid`) for negative coordinates.
- **Deterministic Pipeline**: `DeterministicImpactPipeline` processes event batches in canonical total order (`Ord`), independent of input submission order.
- **Strict Immutability Boundary**: Impact queries are 100% observational and never mutate `ChunkStore`, `PhysicsWorld`, or `PlayerController`.

### 2.7 Terrain Mutation & CSG Foundation (`src/csg/`, Phase 10.2)
- **Separation of Proposal vs Mutation**: `VoxelEdit` and `VoxelEditTransaction` are proposals. Authoritative voxel mutation occurs strictly inside `VoxelEditTransaction::commit()` against `ChunkStore`.
- **Atomicity Guarantee & Proof**: `validate(&ChunkStore)` executes first with pure read-only inspection. Because target chunks are proven resident and local coordinates are bounded `0..32` by `rem_euclid(32)`, the subsequent array writes are provably infallible. Rollback snapshots are maintained for fail-safe atomicity.
- **Deterministic Ordering**: Transactions reject conflicting duplicate edits targeting the same voxel and process deltas in canonical `(x, y, z)` spatial order, ensuring identical outputs regardless of edit submission order.
- **Bounded Crater Geometry**: `CraterGenerator` evaluates candidate voxels within a sphere using Euclidean division. Work scales strictly with the affected volume $O(r^3)$ rather than the world size.
- **Material-Aware Policy**: `MaterialDestructionPolicy` designates indestructible materials (such as `AG_CORE_CASING`) that are bypassed by crater carving without introducing speculative physics or damage values.
- **Invalidation Signals without Simulation Coupling**: Committed edits mark in-memory `dirty_flags::MESH_DIRTY` (including border neighbor propagation) and emit `StructuralEvent` notifications. Crucially, CSG does **NOT** run structural BFS, does **NOT** detach aggregates, does **NOT** spawn `DynamicBody` or `RigidBody` instances, and does **NOT** perform disk I/O.

---

## 3. The 25 Architectural Invariants

The 25 hard invariants documented in [docs/PROJECT_STATE.md](file:///Users/mymac/Documents/Coding%20Work/Omnisia/docs/PROJECT_STATE.md) represent non-negotiable architectural boundaries:
1. `ChunkStore` is Authoritative.
2. GPU Mesh State is Derived / Cache State.
3. Voxel Data is Not Physics State.
4. Structural Connectivity is Distinct from Physics.
5. Detached Aggregates are Represented by `DynamicBody`.
6. One Structural Aggregate = One `RigidBody`.
7. One `RigidBody` May Own Multiple Colliders.
8. Player is a Kinematic Character Controller.
9. Player is NOT a `RigidBody`.
10. Player Must Not Enter `PhysicsWorld.rigid_bodies`.
11. Player Must Not Enter `PhysicsIsland`.
12. Player ↔ RigidBody Interaction Uses Established Bridge.
13. Physics Ticks Must Not Perform Structural BFS.
14. Structural BFS is Event-Driven.
15. Runtime Compact IDs are Not Persistent.
16. Persistent Resource Identity Must Remain Stable.
17. No Synchronous Disk I/O in Hot Paths.
18. No `wgpu` GPU Buffers in Authoritative Simulation.
19. Negative Coordinates Use Consistent Euclidean / Floor Semantics.
20. Zero Double-Ownership of Voxels.
21. Reintegration is Transactional.
22. Determinism is an Explicit Design Objective.
23. Rendering Must Not Become Authoritative Gameplay State.
24. Future Gameplay Must Reuse Generic Engine Abstractions.
25. Creature Gameplay Identity is Separable from Visual Model.

---

## 4. Source of Truth Hierarchy

When resolving technical ambiguity, developers and AI agents must follow this explicit precedence order:

$$\text{Active Source Code} \succ \text{Passing Automated Tests} \succ \text{PROJECT\_STATE.md} \succ \text{ROADMAP.md} \succ \text{Design Documents}$$

1. **Source Code**: Represents what is currently compiled and executed.
2. **Automated Tests**: Represents what is provably functioning and regression-guarded.
3. **`docs/PROJECT_STATE.md`**: Authoritative record of metrics, completed phases, and invariants.
4. **`docs/ROADMAP.md`**: Guide for planned future phases and architectural constraints.
5. **Historical Reports**: Contextual narrative for how past phases were implemented.
