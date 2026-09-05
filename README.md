# Omnisia 🌌

[![Rust](https://img.shields.io/badge/Rust-2021_Edition-orange.svg)](https://www.rust-lang.org/)
[![wgpu](https://img.shields.io/badge/wgpu-v24_(Metal%20%2F%20Vulkan%20%2F%20DX12)-blue.svg)](https://wgpu.rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Phase](https://img.shields.io/badge/Phase-11.2_Validated-brightgreen.svg)](#completed-phases)
[![Tests](https://img.shields.io/badge/Tests-891_Passing-brightgreen.svg)](#test-suite--validation-evidence)

> **Vision**: *"Revive Chimeraland with a voxel soul."*  
> Omnisia is a high-performance voxel sandbox engine and game built from scratch in pure Rust and `wgpu`. It merges continuous procedural world generation, structural connectivity, and rigid-body physical simulation with a deep creature ecosystem, taming, pet raising, and modular chimera devour/evolution.

---

## ⚡ Current Reality & Quick Answers

For any developer or AI coding agent entering this repository for the first time:

| Question | Answer / Verified Fact |
|:---|:---|
| **What is Omnisia?** | A voxel sandbox game engine combining a procedural voxel substrate with modular creature and evolution gameplay. |
| **What is the long-term vision?** | Revive the modular creature capture, chimera evolution, and open living world of *Chimeraland* on an authoritative, physically reactive voxel substrate. |
| **What is the current gameplay reality?** | **Early physics & locomotion substrate with developer tooling & interaction mutation.** The player can walk, sprint, crouch, jump, auto-step, glide, push dynamic rigid bodies, query targeted voxels via deterministic 3D DDA raycasting, destroy and place voxels with player capsule overlap protection, and use the Developer Debug Console (`` ` ``) / Free Camera (`camera free`). There are **no creatures, combat, taming, inventory, crafting, or devour systems yet**. |
| **What Phase is complete?** | **Phase 11.2 — Voxel Interaction** (Validated with 891 total passing workspace tests across 19 test targets, 8 validation binaries, 54 benchmarks). |
| **What is the next Phase?** | **Phase 11.3 — Resource & Gathering Primitives** (Resource identity, gathering, harvesting yields, collection events). |
| **What systems are authoritative?** | `ChunkStore` (static terrain), `DynamicBody` (detached voxels), `PhysicsWorld` (rigid bodies), `PlayerController` (kinematic locomotion), `EnvironmentClock` (environment time), Data Registry (modding definitions). |
| **What systems are derived / cache state?** | `EnvironmentState` (derived visual environment model), `DeveloperCameraContext` (observational free camera mode), `SkyUniform` & `LightUniform` (GPU uniform buffers), `MeshCache` (GPU mesh buffers), spatial broadphase grid, collision contact manifolds, transient structural BFS results. |
| **What are the top architectural invariants?** | (1) Player is Kinematic, NEVER a `RigidBody`, NEVER in islands. (2) 1 structural aggregate = 1 `RigidBody` owning $M$ colliders (never 1 voxel = 1 body). (3) Zero double-ownership of voxels. (4) Single authority for environment time in `EnvironmentClock`. (5) Developer camera does NOT mutate player position or physics. |
| **What are the known limitations?** | Sequential impulse solver shows compliance under tall vertical stacks ($H \ge 20$ boxes). Warm starting and shock propagation are deferred. |
| **What has been deliberately deferred?** | Full LOD meshing, volumetric clouds, dynamic weather simulation, multiplayer networking, advanced constraint solvers. |
| **Where should a new contributor start?** | Read this README, then inspect [docs/PROJECT_STATE.md](docs/PROJECT_STATE.md), [docs/ROADMAP.md](docs/ROADMAP.md), and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). |

---

## 📋 Documentation Principles & Four-State Model

Every subsystem in Omnisia is classified into one of four explicit states:
- **`IMPLEMENTED`**: Production implementation exists in the codebase.
- **`VALIDATED`**: Implementation is verified by automated tests, validation binaries, and benchmarks.
- **`PLANNED`**: Scheduled for an upcoming phase; requirements defined, production implementation not started.
- **`UNKNOWN / NEEDS VERIFICATION`**: Status requires architectural investigation before proceeding.

*Never claim a feature is implemented merely because a struct, interface, or TODO comment exists.*

---

## 🏗️ Completed Phases (Phases 1 — 10.7)

All phases below are **`VALIDATED`** and locked against regression:

```text
PHASE 1   FOUNDATION                 Rust 2021 + wgpu + 32³ chunks + culled & greedy meshing
PHASE 2   MODDING / DATA DEFINITION  Data-driven JSON block & material registries
PHASE 2.5 CONTENT & ASSET IDENTITY   ResourceIdentifier (namespace:path) + SemVer overrides
PHASE 3   WORLD STREAMING            Authoritative ChunkStore + Rayon jobs + LRU eviction
PHASE 4   WORLDGEN FOUNDATION        Deterministic seed + climate + biomes + hydrology
PHASE 5   VOXEL FEATURES             3D caves, worm tunnels, overhangs, cliffs, ores
PHASE 6   VEGETATION & ECOLOGY       Canonical cross-chunk tree stamping (Oak, Pine, Shrub)
PHASE 7   STRUCTURAL CONNECTIVITY    6-connected adjacency + anchor policy + detached aggregates
PHASE 8A  DYNAMIC AGGREGATE RUNTIME  DynamicBody + 30 Hz fixed timestep + two-phase reintegration
PHASE 8B  PLAYER CONTROLLER          Kinematic capsule + stand clearance + swept anti-tunneling
PHASE 8C  INTEGRATION LAYER          Player <-> DynamicBody interaction + ownership conservation
PHASE 8D  MOVEMENT HARDENING         Auto-step (0.55m) + bounded glide + slope sliding
PHASE 9   RIGIDBODY PHYSICS          Broadphase + SAT contacts + impulse solver + sleeping
          (9.1 to 9.12)              Islands + dynamic-dynamic + bridges + stress validation
PHASE 10.1 IMPACT FOUNDATION          Generic ImpactEvent + bounded volume query + pipeline
PHASE 10.2 CSG & TERRAIN MUTATION    VoxelEditTransaction + atomic commit + crater generator + invalidation
PHASE 10.3 IMPACT -> STRUCTURE -> PHYSICS ImpactBridge + Phase A atomicity + Phase B impulse + 34 tests
PHASE 10.4 CSG / DESTRUCTION HARDENING  Pre-state snapshotting + revert() + negative coords + 6-face invalidation + 45 tests
PHASE 10.5 SKY & ATMOSPHERE FOUNDATION  Procedural GPU sky + continuous day/night + celestial anchors + 3D stars + 23 tests
PHASE 10.5.x DEVELOPER CONSOLE & CAMERA Developer debug console + free camera + single clock authority + 17 tests
PHASE 10.6 PROCEDURAL AURORA V1         Multi-band animated procedural aurora borealis + 18 tests
PHASE 10.6.1 PROCEDURAL AURORA V2       Mathematical curtain folds + dual-layer altitude + 18 tests
PHASE 10.6.1R SKY SHADER OPTIMIZATION   Loop unrolling + transcendental factoring + 2.08x GPU speedup
PHASE 10.7 STRESS INTEGRATION           Concurrent terrain destruction + real-time sky at 60+ FPS + stress binary
PHASE 11.1 INTERACTION FOUNDATION       Deterministic 3D DDA voxel raycast + eye origin + 14 tests
PHASE 11.2 VOXEL INTERACTION            Voxel removal + placement + capsule overlap guard + CSG + 18 tests
```

### Authoritative Player Locomotion Semantics (Phase 8D / 9.10)
- `W` = Walk ($3.0\text{ m/s}$ production default).
- `Shift + W` = Sprint ($6.0\text{ m/s}$ production default).
- `Space` while grounded = Normal Jump ($6.0\text{ m/s}$ upward impulse).
- `Shift + W + Space` while grounded = **Sprint-Jump**, which can transition into **Glide** during descent.
- **Running off a cliff = Fall, NOT Glide** (`AirborneOrigin::FellFromEdge` is immutable).
- **Standing still + `Shift + Space` = Normal Jump, NOT Glide**.
- Normal Jump does not become Glide merely because `Shift` is pressed later.
- Airborne origin is authoritative for determining whether Glide is eligible.

---

## 🎯 Completed Milestone: Phase 10 & Next Milestone: Phase 11

### Phase 10 — World Impact & Atmosphere (`COMPLETED / VALIDATED`)
Phase 10 successfully connected the completed physical simulation substrate to destruction events and atmospheric aesthetics:
- **Track A: World Impact & CSG Destruction**
  - **10.1 Impact Foundation (`VALIDATED`)**: Generic `ImpactEvent` representation, `ImpactSource`, `ImpactMagnitude`, bounded `AffectedVolume` queries, and `DeterministicImpactPipeline`.
  - **10.2 Terrain Mutation / CSG Foundation (`VALIDATED`)**: Transactional spherical and directional crater carving in `ChunkStore`.
  - **10.3 Impact $\to$ Structure $\to$ Physics (`VALIDATED`)**: Terrain blast $\to$ structural detachment $\to$ `DynamicBody` $\to$ `RigidBody` outward impulse $\to$ settling $\to$ reintegration.
  - **10.4 Destruction Hardening (`VALIDATED`)**: Multi-chunk boundary craters, negative coordinates, persistence interaction, transactional revert.
- **Track B: Sky & Atmosphere**
  - **10.5 Sky & Atmosphere Foundation (`VALIDATED`)**: Lightweight procedural GPU sky shader, continuous day/night cycle, celestial anchors (sun, moon with $5^\circ$ declination and continuous phase), twilight cosine bell curve, temporally stable 3D stars, unified `SkyUniform` and `LightUniform` harmonization. (*No HDRIs, no giant skyboxes, no expensive volumetric raymarching*).
  - **10.5.x Developer Debug Console & Camera Tooling (`VALIDATED`)**: Production-grade keyboard developer debug console (`` ` `` / `F1`), decoupled 6-DOF developer free camera (`camera free` / `camera player`), single environment clock mutable authority, bounded UTF-8 parser, dynamic self-documenting command registry, zero-overhead closed rendering overlay, 17 tests.
  - **10.6 Procedural Aurora V1 (`VALIDATED`)**: Multi-band animated procedural aurora borealis across night skies with latitude modulation.
  - **10.6.1 Procedural Aurora V2 (`VALIDATED`)**: Closed-form trigonometric curtain folds, dual-layer altitude, and 18 regression tests.
  - **10.6.1R Sky Shader Optimization (`VALIDATED`)**: Loop unrolling, transcendental function factoring, 2.08x GPU fragment throughput gain.
  - **10.7 Integration & Visual Stress (`VALIDATED`)**: Concurrent terrain destruction during real-time sky rendering at $\ge 60\text{ FPS}$ verified via `stress_10_7.rs`.

### Phase 11 — Player ↔ World Interaction (`IN PROGRESS`)
- **Phase 11.1 — Interaction Foundation (`VALIDATED`)**: Deterministic 3D DDA voxel raycast query (`raycast_voxels`) against authoritative `ChunkStore`, player eye origin integration (`player.eye_position()`), configurable interaction reach (`PlayerConfig.interaction_reach`, default 5.0m), 6 canonical face normals (+X, -X, +Y, -Y, +Z, -Z), Euclidean negative coordinate support, strict residency awareness (zero silent chunk loading/generation), 14 tests.
- **Phase 11.2 — Voxel Interaction (`VALIDATED`)**: Validated voxel removal (`can_remove`), adjacent placement (`can_place`), player capsule overlap guard (`PlayerCapsuleOverlap`), atomic multi-chunk CSG transaction, structural detachment handoff into `DynamicBody` in `PhysicsWorld`, remesh invalidation, rate-limiting debounce cooldown, 18 tests.
- **Phase 11.3 — Resource & Gathering Primitives (`PLANNED / NEXT`)**: Resource identity, gathering, harvesting yields, collection events.

---

## 🗺️ Master Future Roadmap (Phases 11 — 25+)

The high-level path from physical substrate to full creature gameplay:
- **Phase 11 — Player ↔ World Interaction**: Voxel crosshair targeting, mining, placing, harvesting tools (`PLANNED`).
- **Phase 12 — Entity & Creature Foundation**: `CreatureDefinition`, health, AI states, Common Wildlife $\to$ Grand Beasts $\to$ Mega Beasts $\to$ Mythical Beasts (`PLANNED`).
- **Phase 13 — Combat & Action System**: Unified action architecture, attacks, hitboxes, damage, knockback (`PLANNED`).
- **Phase 14 — Creature Acquisition / Taming / Catcher**: 20% health rule, catcher durability, capture probability (`PLANNED`).
- **Phase 15 — Eggs, Temporary Creatures & Pets**: Incubation, permanent companions, temporary sparring summons (`PLANNED`).
- **Phase 16 — Devour & Modular Evolution**: Core chimera progression; defeating beasts to acquire wings, heads, horns, and tails for hybrid evolution (`PLANNED`).
- **Phase 17 — Player Transformation**: Human ↔ Beast form swapping with persistent identity (`PLANNED`).
- **Phase 18 — Blockbench Creature Pipeline & Animation**: Skeletal animation, attachment sockets (`PLANNED`).
- **Phase 19 — Inventory, Equipment & Crafting**: Data-driven loot, weapons, armor, crafting recipes (`PLANNED`).
- **Phase 20 — Materials, Texturing & Advanced Rendering**: Foliage shaders, VFX, eventual LOD where justified (`PLANNED`).
- **Phase 21 — UI / HUD / UX**: Minimalist immersive HUD, pet management, evolution tree UI (`PLANNED`).
- **Phase 22 — World Systems & Ecology**: Weather systems, predator/prey herds, day/night ecology (`PLANNED`).
- **Phase 23 — Progression, Survival & Economy**: Leveling, survival meters, trading outposts (`PLANNED`).
- **Phase 24 — Content Expansion & World Bosses**: 50+ species, apex roaming titans (`PLANNED`).
- **Phase 25+ — Optimization, Polish & Release**: Profiling, audio engine, modding API, release preparation (`PLANNED`).

*For full details on each future phase and the modular devour vision, see [docs/ROADMAP.md](docs/ROADMAP.md).*

---

## 🛡️ Hard Architectural Invariants

Any modification violating these core rules will be rejected:
1. **`ChunkStore` is Authoritative**: Static terrain lives only in `ChunkStore`. Renderers have zero authority.
2. **One Aggregate = One RigidBody**: $N$ detached voxels physicalize into **1** `RigidBody` owning $M$ colliders. Never 1 voxel = 1 body.
3. **Player is a Kinematic Controller**: Player is **never** a `RigidBody`, has zero mass in solver, and is **never** in `rigid_bodies` or `PhysicsIsland`.
4. **Player ↔ RigidBody Bridge**: Interactions occur strictly through `PlayerRigidBodyBridge` (kinematic push & dynamic carry).
5. **No Per-Tick Structural BFS**: Structural connectivity is strictly event-driven (`VoxelPlaced`, `VoxelRemoved`).
6. **Zero Double-Ownership**: Every voxel exists in exactly one state: `STATIC_WORLD`, `DYNAMIC_SIMULATION`, or `REINTEGRATING`.
7. **Transactional Reintegration**: Settling restores voxels to `ChunkStore` in two phases (`prepare` $\to$ `commit`). Target conflicts abort cleanly.
8. **Euclidean Floor Coordinates**: Spatial indexing uses continuous floor division (`coord.div_euclid(32)`) for negative coordinates.
9. **No GPU/Disk I/O in Simulation Loops**: Physics ticks and render loops are strictly memory-resident without blocking I/O.
10. **Scope Firewall**: Phases must not silently absorb future scope (e.g., Phase 10 must not implement creature AI or taming).

*For the complete list of 25 invariants, see [docs/PROJECT_STATE.md](docs/PROJECT_STATE.md).*

---

## ⚠️ Known Limitations & Deferred Work

- **Tall-Stack Solver Compliance**: The sequential impulse solver exhibits compression under tall vertical stacks ($H = 5$: $0.041\text{m}$, $H = 20$: $1.045\text{m}$, $H = 50+$: unstable compliance). Advanced constraint solvers (shock propagation, split impulse) are deferred until tall dynamic structures are required by gameplay.
- **Determinism Scope**: Trajectories are verified bitwise identical across repeated runs under identical hardware and execution environments.
- **Validation Hygiene**: `src/bin/player_validation.rs` tests legacy Phase 8B movement speeds ($5.0\text{ m/s}$ walk, $9.0\text{ m/s}$ sprint) via explicit config overrides. Production defaults remain $3.0\text{ m/s}$ walk, $6.0\text{ m/s}$ sprint.
- **Deferred Systems**: Hierarchical geometry LOD, volumetric clouds, dynamic weather simulation, and multiplayer networking are intentionally deferred until prerequisite phases are complete.

---

## 🧪 Test Suite & Validation Evidence

Verified across all subsystems:
- **Workspace Test Suite**: **859 / 859 tests passing** (`cargo test --all-targets`).
  - `tests/physics_9_tests.rs`: **415 tests** (RigidBody, contacts, islands, sleeping, dynamic-dynamic, stress).
  - `tests/movement_8d_tests.rs`: **81 tests** (Auto-step, slope sliding, bounded glide).
  - `tests/sky_environment_tests.rs`: **57 tests** (Phase 10.5 celestial anchors, twilight curve, 3D stars; Phase 10.6 & 10.6.1 procedural aurora curtains & altitude).
  - `tests/csg_hardening_tests.rs`: **45 tests** (Arbitrary Add/Remove/Replace, cross-chunk boundaries, negative coordinates, structural consistency, persistence/revision contracts, deterministic replay, 6-face invalidation, transactional revert).
  - `tests/impact_physics_integration_tests.rs`: **34 tests** (Whole-impact Phase A atomicity, Phase B impulse response, single dynamic ownership, 2-phase reintegration).
  - `tests/integration_8c_tests.rs`: **32 tests** (Player-world integration, structural breaks).
  - `tests/player_tests.rs`: **30 tests** (Kinematic capsule, clearance guard, swept anti-tunneling).
  - `tests/csg_tests.rs`: **27 tests** (Add/Remove/Replace, atomic commit & failure rollback, duplicate rejection, cross-chunk atomicity, negative coords, crater generator, material-aware policy, invalidation).
  - `tests/worldgen_tests.rs`: **26 tests** (Deterministic seed, climate, hydrology, caves, strata, ores, biomes).
  - `tests/physics_tests.rs`: **23 tests** (Dynamic aggregate baseline, gravity/antigravity, swept snapping).
  - `tests/console_tooling_tests.rs`: **19 tests** (Command registration, parsing, execution, clock mutation, free camera, history buffer).
  - `tests/impact_tests.rs`: **17 tests** (ImpactEvent construction, sources, Euclidean volume queries, deterministic pipeline, replay).
  - `tests/interaction_tests.rs`: **14 tests** (Deterministic 3D DDA raycast, player eye origin, reach boundary, 6 face normals, negative coordinates, chunk boundaries, residency awareness).
  - `tests/engine_tests.rs`: **13 tests** (Canonical indexing, ambient occlusion, culled & greedy meshing, frustum).
  - `tests/modding_tests.rs`: **11 tests** (Mod discovery, manifest parsing, SemVer, safe overrides, ResourceId).
  - `tests/streaming_tests.rs`: **11 tests** (ChunkStore residency, job scheduler, memory budget, eviction lifecycle).
  - `tests/structure_tests.rs`: **11 tests** (6-connected adjacency, structural anchors, aggregate extraction).
  - `tests/scale_tests.rs`: **7 tests** (Metric coordinate invariants, scale ruler, traversal residency).
- **Validation Binaries (8 Binaries)**:
  - `cargo run --release --bin sky_validation` $\to$ **PASS** (7-stage deterministic celestial validation in $0.06\text{ ms}$).
  - `cargo run --release --bin stress_10_7` $\to$ **PASS** (Concurrent terrain destruction + real-time sky at $60+\text{ FPS}$).
  - `cargo run --release --bin stress_validation` $\to$ **PASS** (11 workloads + 10k steps + determinism in $1.30\text{ s}$).
  - `cargo run --release --bin physics_validation` $\to$ **PASS** (5/5 stages).
  - `cargo run --release --bin integration_validation` $\to$ **PASS** (6/6 stages).
  - `cargo run --release --bin player_validation` $\to$ **PASS** (8/8 stages).
  - `cargo run --release --bin traversal_validation` $\to$ **PASS** (10/10 stages, 1km traversal, memory $\le 85\text{ MB}$).
  - `cargo run --release --bin benchmarks` $\to$ **PASS** (54 benchmarks including BM 54 for Celestial & Sky Rendering CPU Prep).
- **Benchmark Suite**: **54 benchmarks** in `src/bin/benchmarks.rs` (including Benchmark 54 for Celestial & Sky Rendering CPU Prep).

---

## 🚀 Running the Engine & Validation Suite

### 1. Run the Main Game / Diagnostics
```bash
cargo run --release
```
**Controls:**
- `F3` or `P`: Toggle between **Player Mode** (Kinematic Capsule) and **Free-Flight Mode** (Developer Diagnostic).
- **Player Mode**:
  - `W`, `A`, `S`, `D`: Walk ($3.0\text{ m/s}$).
  - `Shift + W`: Sprint ($6.0\text{ m/s}$).
  - `Space`: Jump ($6.0\text{ m/s}$, edge-triggered).
  - `Shift + W + Space`: Sprint-Jump $\to$ hold `Shift` during descent to **Glide**.
  - `C` or `Ctrl`: Crouch ($1.6\text{ m/s}$, height shrinks to $1.2\text{m}$).
  - `Right Click + Mouse Move`: First-Person Camera Look.
- **Free-Flight Mode**:
  - `W`, `A`, `S`, `D`: Fly horizontally.
  - `Space` / `Shift`: Fly up / down.
  - `1`, `2`, `3`, `4`: Speed presets ($5\text{ m/s}$, $20\text{ m/s}$, $100\text{ m/s}$, $500\text{ m/s}$).

### 2. Run Validation Binaries
```bash
# Phase 10.5 Sky & Celestial Environment Validation
cargo run --release --bin sky_validation

# Phase 10.7 Integration & Visual Stress Validation
cargo run --release --bin stress_10_7

# Phase 9.12 RigidBody Stress & Performance Validation
cargo run --release --bin stress_validation

# Phase 8B Player Controller Validation
cargo run --release --bin player_validation

# Phase 8C Integration Layer Validation
cargo run --release --bin integration_validation

# Phase 8A Dynamic Aggregate Validation
cargo run --release --bin physics_validation

# Phase 7 Real-World Multi-Kilometer Traversal Validation
cargo run --release --bin traversal_validation

# Full 54-Benchmark Suite
cargo run --release --bin benchmarks
```

### 3. Run Quality Gates
```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
```

---

## 📂 Project Structure & Documentation Index

```text
Omnisia/
├── content/core/        # Data-driven JSON block and material schemas
├── docs/                # Authoritative engineering documentation
│   ├── PROJECT_STATE.md # Verified metrics, subsystem status matrix, 28 invariants
│   ├── ROADMAP.md       # Full roadmap (Phases 1-25+), devour mechanics, Phase 10 spec
│   ├── ARCHITECTURE.md  # Simulation substrates, authority boundaries, bridges
│   ├── PROJECT_GOVERNANCE.md # Source-of-truth hierarchy, phase workflow, audit rules
│   └── reports/         # Historical phase completion reports (Phases 4-10.7)
├── mods/                # External modding test suites and asset overrides
├── src/                 # Authoritative engine source code
│   ├── bin/             # 8 standalone validation and benchmark binaries
│   ├── environment/     # Celestial clock, sun/moon orbits, SkyUniform, Light harmonization
│   ├── mesh/            # Culled and Greedy 32³ mesh generation
│   ├── modding/         # ResourceIdentifier, manifest loader, registry
│   ├── physics/         # RigidBody, SAT narrowphase, solver, islands, bridges
│   ├── player/          # Kinematic capsule controller, auto-step, glide
│   ├── streaming/       # ChunkStore, scheduler, eviction, memory budget
│   ├── structure/       # 6-connected localized BFS, anchor components
│   └── worldgen/        # Deterministic 3D volumetric noise, biomes, vegetation
└── tests/               # 17 regression test targets (859 automated tests)
```

---

## 📜 License
Omnisia is licensed under the [MIT License](LICENSE).

