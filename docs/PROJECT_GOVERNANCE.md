# Omnisia — Engineering Governance & Project Operating Model

> **Status**: Active Governance Specification  
> **Applies to**: All contributors, developers, and AI coding agents operating on `NarakaProject/Omnisia`  
> **Effective Milestone**: Post-Phase 10.7 (Phase 0 Audit Mainline)

---

## 1. Project Naming & Terminology Standard

The authoritative name of this project is:

$$\textbf{Omnisia}$$

- **Permitted**: `Omnisia`
- **Strictly Prohibited**: `OmniSia`, `Omni-Sia`, `Omnisia Engine` (unless explicitly referring to the engine component as "the Omnisia engine").
- All documentation, commit messages, issue titles, project boards, and architectural reports must use **Omnisia** consistently.

---

## 2. Source-of-Truth Hierarchy

When evaluating the state of features, metrics, architecture, or roadmap progression, the following deterministic hierarchy is legally binding. Higher tiers strictly supersede lower tiers:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 1. LIVE REPOSITORY SOURCE CODE (Authoritative Ground Truth)                 │
│    src/, tests/, content/, Cargo.toml, Cargo.lock                           │
├─────────────────────────────────────────────────────────────────────────────┤
│ 2. EXECUTABLE QUALITY GATES (Behavioral Ground Truth)                       │
│    cargo test --all-targets (859 passing tests)                             │
│    cargo run --release --bin <validation_binary> (8 validation binaries)   │
│    cargo clippy --all-targets --all-features -- -D warnings                 │
├─────────────────────────────────────────────────────────────────────────────┤
│ 3. GIT HISTORY & COMMITS (Chronological Audit Record)                       │
│    git log, commit SHAs, tags, git blame                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│ 4. HISTORICAL PHASE COMPLETION REPORTS (Immutable Milestones)               │
│    docs/reports/PHASE_*.md                                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│ 5. LIVING ENGINEERING DOCUMENTATION (Synchronized Specifications)           │
│    README.md, docs/PROJECT_STATE.md, docs/ROADMAP.md, docs/ARCHITECTURE.md  │
├─────────────────────────────────────────────────────────────────────────────┤
│ 6. GITHUB PROJECTS DASHBOARD & ISSUES (Operational Tracking)               │
│    https://github.com/users/NarakaProject/projects/2                        │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Hierarchy Rules
1. **Source Code Wins Over Documentation**: If a document claims a feature is implemented but no production code exists in `src/`, the feature is `PLANNED` or `UNKNOWN`, never `IMPLEMENTED`.
2. **Automated Tests Win Over Claims**: A feature cannot be declared `VALIDATED` unless automated tests, validation binaries, or benchmarks prove its behavior in CI/local runs.
3. **Historical Reports are Immutable**: Completed phase reports in `docs/reports/` are permanent historical records. They must not be rewritten retroactively to match future architectural changes.
4. **Documentation Must Reflect Reality**: Living documents (`README.md`, `docs/PROJECT_STATE.md`, `docs/ROADMAP.md`) must be updated at the conclusion of every phase to reflect the verified state of Tier 1 and Tier 2.
5. **GitHub Project Reflects Documentation**: The remote GitHub Project board is an operational dashboard synchronized from the repository state.

---

## 3. Four-State Classification Model

Every feature, subsystem, and milestone in Omnisia must be explicitly classified into one of four states:

| Classification | Meaning & Criteria | Required Evidence |
|:---|:---|:---|
| **`IMPLEMENTED`** | Production code exists in `src/` fulfilling the requirements. | Module path in `src/`, clean compilation. |
| **`VALIDATED`** | Implementation is verified against regressions, stress, and edge cases. | Automated tests in `tests/`, passing validation binaries, benchmarks. |
| **`PLANNED`** | Scheduled on the roadmap; architecture/requirements defined; zero production code started. | Roadmap entry in `docs/ROADMAP.md`, open GitHub issue. |
| **`UNKNOWN / NEEDS VERIFICATION`** | Code status or contract is uncertain and requires audit before proceeding. | Flagged in `PROJECT_STATE.md` with investigation items. |

*Rule*: Never declare a subsystem `VALIDATED` merely because code compiles or a placeholder/TODO exists.

---

## 4. Standard Phase Lifecycle Workflow (SOP)

Every development phase from Phase 11 onward must strictly adhere to the following 7-step lifecycle:

```
┌───────┐     ┌───────────┐     ┌──────────┐     ┌────────┐
│ PLAN  │ ──> │ IMPLEMENT │ ──> │ VALIDATE │ ──> │ REPORT │
└───────┘     └───────────┘     └──────────┘     └────────┘
                                                      │
┌────────────┐     ┌───────────────────┐     ┌────────┴───────┐
│ NEXT PHASE │ <── │ SYNC GITHUB PROJ  │ <── │ RECONCILE DOCS │
└────────────┘     └───────────────────┘     └────────────────┘
```

1. **PLAN**:
   - Define exact requirements, architectural invariants, interfaces, and scope firewalls.
   - Verify all prerequisite phases are `VALIDATED`.
   - Update the corresponding GitHub issue and move its status on the GitHub Project to `In Progress`.
2. **IMPLEMENT**:
   - Write clean, modular, production-ready Rust code in `src/`.
   - Strictly honor existing invariants (e.g., zero double-ownership, kinematic player isolation, metric coordinate scale).
3. **VALIDATE**:
   - Implement comprehensive regression tests in `tests/`.
   - If introducing heavy computational or visual logic, create/update standalone validation binaries and benchmarks in `src/bin/`.
   - Run quality gates: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --all-targets`.
4. **REPORT**:
   - Write an authoritative milestone report in `docs/reports/PHASE_<ID>_<NAME>.md`.
   - Include: executive summary, architectural design, verification evidence, benchmark results, invariants check, and files modified.
5. **RECONCILE DOCS**:
   - Update `README.md` (badges, quick answers, completed phases list, test counts).
   - Update `docs/PROJECT_STATE.md` (milestone header, test table, validation binaries, subsystem matrix).
   - Update `docs/ROADMAP.md` (milestone header, completed track status, next active phase).
6. **SYNC GITHUB PROJECT**:
   - Close the completed phase milestone issue on GitHub with commit SHA, test count, and report link.
   - Update the GitHub Project item status to `Done`.
   - Populate the `Evidence` field on the GitHub Project board with verified metrics.
7. **NEXT PHASE**:
   - Advance to the next phase on the roadmap. Never start Phase $N+1$ before Phase $N$ is fully reconciled and verified.

---

## 5. Scope Firewalls & Architectural Invariants

- **Scope Firewalls**: No phase may silently absorb features assigned to future phases. (e.g., Phase 11 must not implement creature definitions or taming; Phase 12 must not implement combat hitboxes).
- **Core Architectural Invariants**:
  - `ChunkStore` is the sole authoritative store for static terrain voxels.
  - 1 structural aggregate = 1 `RigidBody` owning $M$ colliders (never 1 voxel = 1 body).
  - Player controller is strictly Kinematic (never a `RigidBody`, never in `PhysicsIsland` or solver).
  - Voxel ownership is conserved (zero double-ownership across static, dynamic, or reintegrating states).
  - No per-tick structural BFS scans (structural graph checks are strictly event-driven).
  - No GPU or disk I/O in physics or game loops.
  - Coordinate system uses Euclidean floor division (`div_euclid(32)`) across all negative coordinate boundaries.

---

## 6. GitHub Project Operation

- **Official Board URL**: [https://github.com/users/NarakaProject/projects/2](https://github.com/users/NarakaProject/projects/2)
- **Board Title**: **Omnisia — Development Roadmap**
- **Standard Columns / Statuses**:
  - `Todo`: Scheduled roadmap phases.
  - `In Progress`: Currently active phase.
  - `Done`: Validated phases locked against regression with empirical evidence.
- **Item Fields**:
  - `Phase`: Standardized identifier (`Phase 1` through `Phase 25+`).
  - `Area`: Subsystem domain (`Core Engine`, `Simulation`, `Gameplay`, `Visuals`, `Content`, etc.).
  - `Target`: Primary milestone deliverable description.
  - `Status`: `Todo` | `In Progress` | `Done`.
  - `Evidence`: Test counts, binary results, benchmark numbers, commit SHAs.
