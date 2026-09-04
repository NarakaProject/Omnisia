# Omnisia — Master Roadmap & Project Vision

> **Vision Statement**: *"Revive Chimeraland with a voxel soul."*  
> **Core Substrate**: Procedural, structural, and rigid-body voxel world.  
> **Core Gameplay Identity**: Creature ecosystem, taming, acquisition, pet companions, and modular devour/evolution.  
> **Current Completed Phase**: **Phase 10.5 — Procedural Sky & Atmosphere Foundation**  
> **Next Active Phase**: **Phase 10.6 — Procedural Aurora** (`PLANNED / NEXT`)

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

## 3. Active Milestone: Phase 10 — World Impact & Atmosphere

> **Status**: `IN PROGRESS` (Track A: Phase 10.1 `VALIDATED`, Phase 10.2 `VALIDATED`, Phase 10.3 `VALIDATED`, Phase 10.4 `VALIDATED`; Track B: Phase 10.5 `VALIDATED`, Phase 10.6 `NEXT`)  
> **Objective**: Connect the physical simulation substrate to destruction events (Track A) and atmospheric visuals (Track B).  
> **Scope Firewall**: Phase 10 must **NOT** implement creature AI, taming, devour, inventory, or full weather simulation.

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
  10.2 Terrain Mutation / CSG (VALIDATED)                 10.6 Procedural Aurora (NEXT)
  10.3 Impact -> Structure -> Physics (VALIDATED)         10.7 Integration & Visual Stress
  10.4 CSG Hardening & Revert (VALIDATED)                        │
         │                                                       │
         └───────────────────────────┬───────────────────────────┘
                                     ▼
                      10.7 Final Phase 10 Validation
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
  - Constraints respected: **NO HDRI dependencies, NO giant skybox textures, NO expensive volumetric raymarching**. 60 FPS target on integrated GPUs.
  - Verified with 23 regression tests (`tests/sky_environment_tests.rs`), 7-stage validation binary (`src/bin/sky_validation.rs`), and Benchmark 54 (3 CPU profiles).
- **Phase 10.6 — Procedural Aurora (`PLANNED / NEXT`)**:
  - Animated multi-band procedural aurora borealis across night skies.
  - Altitude and parallax illusion using layered noise functions.
  - Visually stunning yet computationally lightweight.
- **Phase 10.7 — Integration & Visual Stress**:
  - Concurrent verification: heavy terrain impact destruction occurring simultaneously with real-time sky rendering and streaming traversal.
  - Frame-rate stability audit ($\ge 60\text{ FPS}$).

---

## 4. Future Master Roadmap (Phases 11 — 25+)

The following phases outline the planned path toward full gameplay release. None of these systems are implemented currently.

### PHASE 11 — PLAYER ↔ WORLD INTERACTION (`PLANNED`)
- Voxel crosshair targeting and raycasting against static and dynamic voxels.
- Block harvesting, breaking, and placement tools.
- Resource gathering mechanics from natural voxels (wood, stone, crystals).
- Interactable world objects and placement ghost previews.

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
