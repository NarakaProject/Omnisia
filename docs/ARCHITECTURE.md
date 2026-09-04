# Omnisia — System Architecture & Source of Truth

> **Current Milestone**: Phase 10.5.x — Developer Debug Console & Camera Tooling (`COMPLETED / VALIDATED`)  
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
│  • DynamicAggregateRecord (authoritative dynamic owner in PhysicsWorld)     │
│  • DynamicBody: Synchronized snapshot view                                  │
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
│  • ImpactBridge: Whole-Impact Phase A Atomicity & Phase B Impulse Response   │
│  • PlayerRigidBodyBridge: Kinematic velocity transfer & box pushing         │
│  • StructuralAggregateBridge: Aggregate -> DynamicBody -> RigidBody         │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                     DERIVED & DISCARDABLE RENDER LAYERS                     │
├─────────────────────────────────────────────────────────────────────────────┤
│  • MeshCache: Culled & Greedy 32³ GPU vertex/index buffers (Metal / Vulkan) │
│  • EnvironmentState: Derived visual environment model & GPU uniforms       │
│  • SkyUniform & LightUniform: Harmonized GPU celestial buffers              │
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

### 2.8 Impact → Structure → Physics Integration (`src/impact/bridge.rs`, Phase 10.3)
- **Two-Phase Bridge Pattern**: Coordinates between voxel destruction, structural detachment, and rigid body simulation.
- **Whole-Impact Atomicity**: Phase A extracts detached aggregates across all structural events produced by CSG in a single batch, preventing intermediate structural fragmentation.
- **Single Authoritative Dynamic Owner**: Extracted aggregates are recorded in `DynamicAggregateRecord` and physicalized as single `RigidBody` instances with greedy compound colliders.
- **Blast Impulse Coupling**: Phase B applies outward impulses inversely proportional to distance from the impact epicenter directly to rigid body linear and angular velocities.

### 2.9 CSG Hardening, Boundary Invalidation & Transactional Revert (`src/csg/`, Phase 10.4)
- **Authoritative Boundary Mutation**: Arbitrary multi-chunk mutations (Add, Remove, Replace) preserve total voxel conservation and mathematical coordinate continuity across negative chunk space via Euclidean floor division (`div_euclid`) and positive modulo (`rem_euclid(32)`).
- **Symmetric 6-Face & Corner Invalidation**: Voxel boundary edits propagate `MESH_DIRTY` to all resident adjacent faces ($\pm X, \pm Y, \pm Z$), edges, and corners.
- **Unloaded Neighbor Protection (`UNLOADED != AIR`)**: Target edits require resident chunks or fail validation. Neighbor mesh invalidation touches resident neighbors only; unloaded chunks are strictly left untouched with zero implicit chunk allocation, zero disk I/O, and zero phantom dirty flags.
- **Transactional Pre-State Capture & Revert**: Captures exact chunk pre-states (`ChunkPreState`: `chunk_coord`, `dirty_flags`, `revision`, `non_air_count`) before the first mutation. Explicit `revert(&mut ChunkStore)` performs a full residency preflight check before applying inverse voxel deltas and restoring exact chunk metadata, leaving zero partial mutations on failure.
- **Deterministic Ordering & Preservation**: Canonical `(x, y, z)` spatial ordering is maintained for deltas and structural events, while `LastWriteWins` strictly respects transaction insertion order without spatial reordering. CSG produces zero downstream physics or structural BFS side effects.

### 2.10 Procedural Sky & Celestial Environment (`src/environment/`, `src/sky.wgsl`, Phase 10.5)
- **Derived Visual Environment Model**: `EnvironmentState` is strictly a derived visual model driven by simulation/application time progression. It derives both `SkyUniform` (sky pass) and `LightUniform` (opaque terrain lighting). It maintains zero authority over `ChunkStore`, `StructuralSystem`, `PhysicsRuntime`, `DynamicBody`, CSG, persistence, or terrain simulation.
- **Single Mutable Authority for Time Progression**:
  - `EnvironmentClock` is the sole mutable authority for environment time progression, paused/running state, time scale, and day fraction.
  - `EnvironmentState` delegates all time mutations (`pause()`, `resume()`, `is_paused()`, `set_time_scale()`, `set_day_fraction()`, `advance()`) directly to its internal `EnvironmentClock` instance, guaranteeing that zero duplicate mutable time state exists.
  - Pausing environment time freezes celestial motion and day/night progression without affecting the gameplay/kinematic character physics simulation loop.
  - Time scale is strictly bounded to finite values in $(0.0, 1000.0]$.
  - Day length authority: Production default is $1200.0\,\text{s}$ ($20.0\,\text{minutes}$ per celestial cycle, defined in `EnvironmentClock::default()`). Accelerated test configurations explicitly instantiate $240.0\,\text{s}$ cycles (`EnvironmentClock::new(240.0, 0.0)`) for fast test turnaround.
- **Preserved Terrain Lighting Authority**: `LightUniform { sun_direction, sun_color, ambient_color }` remains the single authoritative lighting contract for terrain chunk shaders via `Renderer::update_light()`. `EnvironmentState` harmonizes celestial solar angles and color temperatures with `LightUniform`, ensuring unified environmental lighting without duplicating terrain shader pipelines or introducing competing lighting systems.
- **Celestial Coordinate Conventions & Semantic Anchors**:
  - Coordinate frame: $+Y = \text{world up}$, right-handed view transform (`Mat4::look_at_rh`, `Mat4::perspective_rh`, `wgpu` NDC depth $[0, 1]$).
  - Explicit canonical solar anchors:
    $$\text{day\_fraction } 0.00 \implies \text{midnight} \implies \text{sun } \approx (0, -1, 0)$$
    $$\text{day\_fraction } 0.25 \implies \text{sunrise} \implies \text{sun } \approx (+1, 0, 0)$$
    $$\text{day\_fraction } 0.50 \implies \text{noon} \implies \text{sun } \approx (0, +1, 0)$$
    $$\text{day\_fraction } 0.75 \implies \text{sunset} \implies \text{sun } \approx (-1, 0, 0)$$
- **Deterministic Moon Orbit & Continuous Phase**:
  - Moon direction is derived from the same celestial clock with an explicit, bounded $5.0^\circ$ orbital declination tilt ($0.0872665\,\text{rad}$ rotation around $+Z$), resolving opposition and declination deterministically.
  - The canonical rendering authority is continuous `moon_phase \in [0.0, 1.0)`. In `sky.wgsl`, the moon disc reconstructs 3D spherical surface normals to shade illuminated vs unshadowed lunar regions continuously without step quantization. The 8-phase enum (`MoonPhase`) is restricted to classification, debug, and UI.
- **Twilight Smooth Cosine Bell Curve**:
  - Twilight transition factor $T \in [0, 1]$ is evaluated as a cosine bell curve:
    $$T(e) = \cos^2\left(\frac{|e|}{0.20} \cdot \frac{\pi}{2}\right) \quad \text{for } |e| \le 0.20, \quad 0 \text{ elsewhere}$$
    where $e = \text{sun\_direction.y}$ is the solar elevation. This guarantees $C^1$ continuity with zero derivatives at horizon crossing ($e = 0$) and transition boundaries ($|e| = 0.20$), eliminating visual pops and color derivative discontinuities.
- **Camera Translation Invariance**:
  - Sky view unprojection matrix is constructed via `Camera::build_sky_view_projection_matrix(aspect)` with position forced to $\vec{0}$. This mathematically isolates camera rotation from world-space translation, guaranteeing that celestial bodies and atmospheric gradients remain at optical infinity with zero translation parallax or floating-point jitter across multi-kilometer movements.
- **Temporally Stable Procedural Stars & Bounded Shader Time**:
  - Procedural stars use a deterministic 3D angular hash on the unit celestial sphere: fixed angular positions, rotation-stable, translation-invariant, and texture-free.
  - Star twinkling is modulated via a smooth periodic wave function without altering star spatial coordinates or hash cell assignments.
  - Shader time inputs are strictly bounded (`day_fraction \in [0, 1)`, star animation time in $[0, 60.0)\,\text{s}$) to prevent long-run IEEE-754 precision loss during extended gameplay sessions.
- **Renderer Integration & Depth Rejection Semantics**:
  - The sky is rendered after opaque voxel geometry in the existing primary render pass via a single fullscreen triangle at depth $1.0$ (`depth_compare: LessEqual`, `depth_write_enabled: false`).
  - *Depth Rejection Specification*: The sky is depth-tested against already-rendered opaque terrain so pixels whose depth is already less than $1.0$ are rejected by the depth test. GPU early/hierarchical depth optimization may reduce fragment work, but exact fragment execution behavior is implementation-dependent. Universal zero-overdraw or hardware-specific performance guarantees are explicitly disclaimed.

### 2.11 Developer Debug Console & Camera Tooling (`src/console/`, `src/console.wgsl`, Phase 10.5.x)
- **Dedicated Non-Gameplay Developer Infrastructure**:
  - Implemented as developer tooling to inspect, validate, and iterate on visual environments (particularly Phase 10.6 Procedural Aurora) without touching gameplay code, mutating player physics, or awaiting real-time day/night cycles.
  - Strict scope firewall: zero cheats, godmode, noclip player mutation, item spawning, world block editing, or general UI dependencies.
- **Zero-Cost When Closed**:
  - When the console is closed, `Renderer::render(...)` generates exactly 0 console vertices, performs 0 GPU vertex buffer writes, binds 0 console pipelines/textures, and issues 0 draw calls. Memory and frame-time overhead during gameplay is completely eliminated.
- **Decoupled Developer Free Camera (`CameraMode::Developer`)**:
  - Managed via `DeveloperCameraContext`, which maintains a dedicated `Camera` instance distinct from the player camera.
  - Switching to developer free camera preserves the player's physical coordinates, velocity, and ground state without mutation.
  - Player input is suppressed when the developer camera is active or when the console is open, preventing stuck keys or unintended movement.
  - WASD, Space, Shift, and mouse look control the developer camera using existing camera projection and look math. Speed is configurable in $[0.1, 500.0]\,\text{m/s}$ (default: $20.0\,\text{m/s}$).
  - Toggling back to `CameraMode::Player` seamlessly restores player camera control with zero disruption to player trajectory or collision state.
- **Bounded Command Parser & UTF-8 Safety**:
  - Pure function `parse_command(input: &str) -> Result<ParsedCommand, ParseError>` operating over Unicode scalar values.
  - Enforces a hard input limit of 4096 bytes (`MAX_CONSOLE_INPUT_BYTES`).
  - Normalizes leading, trailing, and duplicate whitespace outside of quotes.
  - Supports both single (`'`) and double (`"`) quotes for multi-token arguments, with unclosed quote detection.
  - Cursor navigation, insertion, and backspace in `ConsoleState` index strictly along UTF-8 character boundaries (`char_indices()`) to eliminate byte-boundary panics.
- **Command Registry & Execution Context**:
  - Reflection-free dynamic registry (`CommandRegistry`) mapping canonical command names to implementations of the `ConsoleCommand` trait.
  - Self-documenting: commands provide structured name, syntax, description, and usage metadata. The built-in `help` and `help <command>` inspect this metadata dynamically.
  - Execution context (`DeveloperExecutionContext`) provides:
    - Mutable access to `EnvironmentState` (which delegates time mutations to `EnvironmentClock`).
    - Mutable access to `DeveloperCameraContext`.
    - Read-only snapshot of the player (`&KinematicCharacterController`), exposing position, yaw, pitch, and velocity for diagnostics while firewalled against mutation.
    - Read-only diagnostics (resident chunks, render stats, frame timing).
  - Built-in commands: `help`, `clear` (via decoupled `CommandResult::Clear`), `time` (`get`, `pause`, `resume`, `scale`, `set`), `camera` (`free`, `player`, `speed`, `position`, `rotation`, `status`), `env` (`status`, `moon`), `status`.
- **Embedded ASCII Font & Texture Atlas**:
  - Embedded 760-byte 8x8 font bitmasks covering ASCII printable characters $32..126$ arranged in a $128 \times 48$ RGBA atlas ($24.5\,\text{KB}$ uncompressed GPU texture).
  - Deterministic fallback: unsupported or non-ASCII Unicode characters render as `?` (ASCII 63), preventing glyph missing errors or crashes.
  - WGSL shader `console.wgsl` maps pixel coordinates to NDC $[-1, 1]$ and renders character glyph quads and solid semi-transparent background panels (`ALPHA_BLENDING`, `depth_compare: Always`, `depth_write_enabled: false`).

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
