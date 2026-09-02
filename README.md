# Omnisia 🌌

[![Rust](https://img.shields.io/badge/Rust-2021_Edition-orange.svg)](https://www.rust-lang.org/)
[![wgpu](https://img.shields.io/badge/wgpu-v24_(Metal)-blue.svg)](https://wgpu.rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Engine API](https://img.shields.io/badge/Engine_API-v0.2.0-green.svg)](#)

**Omnisia** adalah voxel sandbox engine berkinerja tinggi yang dibangun dari nol menggunakan **Rust murni** dan abstraksi grafis **`wgpu`** (Metal backend untuk macOS). 

Dirancang dengan prinsip **Engine-First, Data-Driven, Deterministic, 3D Volumetric Procedural Generation, & Scalable Hierarchical Streaming**, memisahkan secara tegas antara **Authoritative Near World (Full-Resolution Voxels)** dan **Derived Far World (Hierarchical LOD / Distant Horizons Boundary)**.

---

## 🏛️ Arsitektur Generasi Dunia Volumetrik 3D (Phase 5)

Engine ini menerapkan arsitektur generasi dunia prosedural berbasis medan multi-skala dan densitas volumetrik 3D:

```text
                        WORLD SEED
                            │
                            ▼
                       Seed Context
                            │
        ┌───────────────────┼───────────────────┐
        ▼                   ▼                   ▼
 Continentalness       Temperature           Moisture
        │                   │                   │
        └───────────────────┼───────────────────┘
                            ▼
                       Biome Field
                            │
                 ┌──────────┴──────────┐
                 ▼                     ▼
              Erosion                Peaks
                 │                     │
                 └──────────┬──────────┘
                            ▼
                  Height Field H(x,z)
                            │
                            ▼
                     Hydrology Layer
                     (Rivers & Lakes)
                            │
                            ▼
                  Terrain 3D Densities
               (Overhangs & Cliffs D(x,y,z))
                            │
                            ▼
                     3D Cave Carving
             (Worm Tunnels & Cheese Caverns)
                            │
                            ▼
                  Underground Strata
            (Topsoil -> Subsoil -> Stone -> Deepslate)
                            │
                            ▼
                 Ore & Resource Placement
            (Coal, Iron, Gold, Lumina Crystals)
                            │
                            ▼
                    Natural Formations
                 (Surface Rock Boulders)
                            │
                            ▼
                     Chunk Voxelizer
                            │
                            ▼
                 32³ Authoritative Chunk
```

### Invariant & Prinsip Utama:
1. **Volumetric 3D Caves & Overhangs:** Gua 3D berongga non-kolumnar (*cheese caverns & elongated worm tunnels*) dan overhang tebing sejati ($\text{Air} \to \text{Solid} \to \text{Air} \to \text{Solid}$).
2. **Underground Stratification:** Pembagian lapisan geologi bertingkat (*Topsoil $\to$ Subsoil $\to$ Stone $\to$ Deepslate* pada $y < -32$).
3. **Deterministic Ore Distribution:** Sebaran urat/kantong bijih mineral (*Coal, Iron, Gold, Crystal*) yang hanya menggantikan batuan padat dan tidak pernah muncul di udara atau air.
4. **Deterministic & Seed-Based:** Formula murni `(WorldSeed, GeneratorVersion, WorldGenConfig, WorldCoord) -> Exact Chunk`. Bebas dari ketergantungan urutan thread atau urutan loading chunk.
5. **Seamless Across Chunk Boundaries:** Kontinuitas matematis penuh pada perbatasan antar-chunk tanpa diskontinuitas buatan (*zero seams*) pada sumbu X, Y, Z maupun koordinat negatif.
6. **Hardened Stale Async Identity:** Menggunakan tuple identitas `ChunkCoord + LifecycleGeneration + Revision` untuk mencegah race condition dan stale job execution setelah eviksi/resurrection chunk.
7. **Persistence Precedence:** Chunk yang telah tersimpan di disk (`RegionStore`) atau dimutasi oleh pemain **selalu menang** atas generator prosedural.
8. **Explicit Missing-Content Handling:** Menolak silent fallback ke `core:air` jika `ResourceId` tidak ditemukan di registry untuk mencegah *silent data loss*.

---

## 📊 Hasil Benchmark (MacBook Pro 2018 Reference)

Dijalankan pada arsitektur Intel Core i7 x86_64 dengan backend Metal:

| No | Pengujian Benchmark | Metrik Pengukuran | Keterangan & Analisis |
|:---|:---|:---|:---|
| 1 | **Chunk Indexing** | **0.25 ns / op** | Inlined $O(1)$ canonical index |
| 2 | **Chunk Fill (32k voxels)** | **3.46 µs / chunk** | 128 KiB memory throughput |
| 3 | **Culled Meshing 32³** | **0.305 ms / chunk** | 16,896 Vertices, 4,224 Quads per chunk |
| 4 | **Greedy Meshing 32³** | **0.729 ms / chunk** | 580 Vertices, 145 Quads (**29.13x Quad Reduction**) |
| 5 | **AO Calculation** | **15.76 ns / face** | 500,000 sampling sudut AO |
| 6 | **100 Chunks Procedural Meshing** | **32.40 ms** | Mengolah 100 chunk prosedural serentak (1.73M vertex) via Rayon |
| 7 | **Chunk Palette Zstd Compress** | **1.72 ms** | 131,072 bytes $\to$ 1,307 bytes (**100.3x rasio kompresi**) |
| 8 | **Chunk Palette Zstd Decompress** | **683.14 µs** | Rekonstruksi chunk 32k voxel sempurna (< 1 ms) |
| 9 | **Noise 3D fBm Sampling** | **128.38 ns / sample** | $10^6$ sampling volumetrik 3D bebas alokasi |
| 10 | **3D Cave & Worm Tunnel Sampling** | **222.02 ns / point** | 100,000 titik evaluasi rongga gua 3D |
| 11 | **3D Overhang & Feature Eval** | **43.47 ns / point** | 100,000 titik evaluasi densitas tebing |
| 12 | **Phase 5 Procedural Chunk Gen** | **7.802 ms / chunk** | Generasi 32³ micro-voxels dengan fitur 3D lengkap (0 alokasi heap di hot loop) |
| 13 | **100 Chunks Parallel Generation** | **103.47 ms** | **~966.5 chunks/detik** (Rayon parallel throughput) |
| 14 | **Voxel Hot Path Lookup** | **1.23 ns / op** | Zero-overhead runtime index array |
| 15 | **Mod Discovery & Parsing** | **79.84 µs / run** | Discovery deterministik + validasi TOML manifest |

---

## 🚀 Menjalankan Engine & Tooling

### 1. Menjalankan Pembangkitan Dunia Prosedural Interaktif
```bash
cargo run --release
```

**Kontrol Kamera:**
* `W`, `A`, `S`, `D`: Gerak horizontal (Fly / FPS mode)
* `Space`: Terbang naik (+Y)
* `Left Shift`: Terbang turun (-Y)
* **Klik Kanan / Kiri + Drag Mouse**: Rotasi orientasi pandangan (Yaw & Pitch)

### 2. Menjalankan Content & Mod Validator
```bash
cargo run --release -- --validate-mods
```

### 3. Menjalankan Test Suite (52 Unit Tests)
```bash
cargo test
```

### 4. Menjalankan Benchmark Suite
```bash
cargo run --release --bin benchmarks
```

---

## 📂 Struktur Modul Engine

```text
content/
└── core/                       # Authoritative Built-in Core Content
    ├── materials/              # stone, dirt, grass, sand, water, snow, deepslate, coal_ore, iron_ore, gold_ore, crystal, dll.
    ├── blocks/                 # stone_block, dirt_block, grass_block, water_block, snow_block, deepslate_block, dll.
    ├── textures/
    ├── models/
    ├── sounds/
    └── structures/

mods/
└── example_mod/                # External Mod Content & Overrides
    ├── mod.toml                # Manifest & [[overrides]]
    ├── materials/              # steel.json, reinforced_concrete.json, reinforced_stone.json
    ├── blocks/                 # steel_block.json, reactor_core.json
    ├── textures/
    ├── models/
    ├── sounds/
    └── structures/

src/
├── lib.rs                      # Root library re-exports
├── main.rs                     # Winit 0.30 interactive streaming app & CLI
├── material.rs                 # MaterialId (2 bytes) & MaterialRegistry
├── voxel.rs                    # VoxelBlock (4 bytes compact struct)
├── coord.rs                    # Canonical indexing & Euclidean negative math
├── chunk.rs                    # Authoritative Chunk 32³ (128 KiB flat array, revision counter)
├── camera.rs                   # FPS/Orbital 3D camera & ViewProj uniform
├── renderer.rs                 # wgpu Metal pipeline, depth buffer, mesh cache
├── shader.wgsl                 # Half-Lambert + Pastel palette + Baked AO shader
├── storage.rs                  # RegionStore abstraction, palette serialization, Zstd
├── world.rs                    # World façade (drives streaming, eviction, & meshing)
├── bin/
│   └── benchmarks.rs           # 18 Benchmark suite
├── worldgen/                   # Procedural World Generation Subsystem (Phase 4 & 5)
│   ├── mod.rs
│   ├── seed.rs                 # WorldSeed (u64 & SplitMix64 string hash), GeneratorVersion, SeedContext
│   ├── config.rs               # WorldGenConfig & WorldIdentity
│   ├── noise.rs                # Deterministic 2D/3D Gradient noise, fBm, & Ridged noise
│   ├── climate.rs              # Continentalness, Temperature, Moisture, Erosion, Peaks/Valleys
│   ├── biome.rs                # BiomeType & BiomeClassifier
│   ├── hydrology.rs            # 2D continuous river curve & lake basins
│   ├── terrain.rs              # Continuous height profiling H(x, z)
│   ├── caves.rs                # 3D Cave Sampler (Elongated worm tunnels & cheese caverns)
│   ├── features.rs             # Overhangs, Underground Strata, Ore distribution, Formations
│   ├── voxelizer.rs            # ChunkVoxelizer (32³ 3D volumetric voxelization)
│   └── pipeline.rs             # ProceduralWorldGenerator (implements ChunkGenerator)
├── streaming/                  # World Streaming Subsystem
│   ├── mod.rs
│   ├── residency.rs            # Lifecycle StateMachine (Residency, Persistence, Mesh)
│   ├── memory.rs               # MemoryBudget & MemoryUsage accounting
│   ├── eviction.rs             # Safe eviction policy (dirty protection)
│   ├── jobs.rs                 # JobPriority, ChunkJobRequest, ChunkJobResult
│   ├── generator.rs            # ChunkGenerator trait
│   ├── store.rs                # ChunkStore (resident chunks & lifecycle generation tracking)
│   └── scheduler.rs            # ChunkScheduler (bounded channels, priority escalation, stale protect)
├── lod/                        # Distant Horizons / Voxy Architectural Boundary
│   └── mod.rs                  # DistantRepresentation trait & HierarchicalLodStore (derived)
├── modding/
│   ├── mod.rs
│   ├── asset.rs                # AssetId & AssetResolver
│   ├── definitions.rs
│   ├── dependency.rs
│   ├── discovery.rs
│   ├── loader.rs
│   ├── manifest.rs
│   ├── registry.rs
│   ├── resource_id.rs
│   ├── runtime.rs
│   └── validation.rs
└── mesh/
    ├── mod.rs
    ├── types.rs
    ├── ao.rs
    ├── culled.rs
    └── greedy.rs
```

---

## 📜 Lisensi
Dilisensikan di bawah [MIT License](LICENSE).
