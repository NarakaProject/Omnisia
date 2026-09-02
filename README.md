# Omnisia 🌌

[![Rust](https://img.shields.io/badge/Rust-2021_Edition-orange.svg)](https://www.rust-lang.org/)
[![wgpu](https://img.shields.io/badge/wgpu-v24_(Metal)-blue.svg)](https://wgpu.rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Engine API](https://img.shields.io/badge/Engine_API-v0.2.0-green.svg)](#)

**Omnisia** adalah voxel sandbox engine berkinerja tinggi yang dibangun dari nol menggunakan **Rust murni** dan abstraksi grafis **`wgpu`** (Metal backend untuk macOS). 

Dirancang dengan prinsip **Engine-First, Data-Driven, Deterministic, Continuous Procedural Generation, & Scalable Hierarchical Streaming**, memisahkan secara tegas antara **Authoritative Near World (Full-Resolution Voxels)** dan **Derived Far World (Hierarchical LOD / Distant Horizons Boundary)**.

---

## 🏛️ Arsitektur Pembangkitan Dunia Prosedural (Phase 4)

Engine ini menerapkan arsitektur generasi dunia prosedural berbasis medan multi-skala:

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
                       Height Field
                            │
                            ▼
                     Hydrology Layer
                     (Rivers & Lakes)
                            │
                            ▼
                     Terrain Profile
                            │
                 ┌──────────┴──────────┐
                 ▼                     ▼
              Surface              Subsurface
             Materials             Materials
                 │                     │
                 └──────────┬──────────┘
                            ▼
                     Chunk Voxelizer
                            │
                            ▼
                 32³ Authoritative Chunk
```

### Invariant & Prinsip Utama:
1. **Deterministic & Seed-Based:** Formula murni `(WorldSeed, GeneratorVersion, WorldGenConfig, WorldCoord) -> Exact Chunk`. Bebas dari ketergantungan urutan thread atau urutan loading chunk.
2. **Seamless Across Chunk Boundaries:** Kontinuitas matematis penuh pada perbatasan antar-chunk tanpa diskontinuitas buatan (*zero seams*) pada sumbu X, Z, maupun koordinat negatif.
3. **Hardened Stale Async Identity:** Menggunakan tuple identitas `ChunkCoord + LifecycleGeneration + Revision` untuk mencegah race condition dan stale job execution setelah eviksi/resurrection chunk.
4. **Coherent Hydrology & Continuous Rivers:** Jaringan sungai 2D kontinu yang mengukir lembah secara mulus menuju batas permukaan air laut (*sea level*) melintasi batas chunk.
5. **Persistence Precedence:** Chunk yang telah tersimpan di disk (`RegionStore`) atau dimutasi oleh pemain **selalu menang** atas generator prosedural.
6. **Explicit Missing-Content Handling:** Menolak silent fallback ke `core:air` jika `ResourceId` tidak ditemukan di registry untuk mencegah *silent data loss*.

---

## 📊 Hasil Benchmark (MacBook Pro 2018 Reference)

Dijalankan pada arsitektur Intel Core i7 x86_64 dengan backend Metal:

| No | Pengujian Benchmark | Metrik Pengukuran | Keterangan & Analisis |
|:---|:---|:---|:---|
| 1 | **Chunk Indexing** | **0.37 ns / op** | $10^7$ iterasi dalam 3.69 ms ($O(1)$ inlined) |
| 2 | **Chunk Fill (32k voxels)** | **3.92 µs / chunk** | 128 KiB memory throughput ultra-cepat |
| 3 | **Culled Meshing 32³** | **0.469 ms / chunk** | 16,896 Vertices, 4,224 Quads per chunk |
| 4 | **Greedy Meshing 32³** | **0.885 ms / chunk** | 288 Vertices, 72 Quads (**58.67x Quad Reduction**) |
| 5 | **AO Calculation** | **21.66 ns / face** | 500,000 sampling sudut dalam 10.83 ms |
| 6 | **100 Chunks Procedural Meshing** | **44.66 ms** | Mengolah 100 chunk prosedural serentak (1.72M vertex) via Rayon |
| 7 | **Chunk Palette Zstd Compress** | **1.96 ms** | 131,072 bytes $\to$ 624 bytes (**210.1x rasio kompresi**) |
| 8 | **Chunk Palette Zstd Decompress** | **850.93 µs** | Rekonstruksi chunk 32k voxel sempurna (< 1 ms) |
| 9 | **Noise 2D fBm Sampling** | **123.90 ns / sample** | $10^6$ sampling kontinu deterministik bebas alokasi |
| 10 | **Terrain Profile Evaluation** | **706.52 ns / point** | 100,000 titik evaluasi profil medan, iklim, dan sungai |
| 11 | **Procedural Chunk Generation** | **0.889 ms / chunk** | **1,124.6 chunks/detik** (Single-core voxelization) |
| 12 | **100 Chunks Parallel Generation** | **18.18 ms** | **5,500 chunks/detik** (Rayon parallel throughput) |
| 13 | **Voxel Hot Path Lookup (MaterialId)** | **1.47 ns / op** | **0 Overhead** runtime index array vs string hash |
| 14 | **Mod Discovery & Parsing** | **120.30 µs / run** | Discovery deterministik + validasi TOML manifest |

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

### 3. Menjalankan Test Suite (45 Unit Tests)
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
    ├── materials/              # stone, dirt, grass, sand, water, snow, metal_frame, dll.
    ├── blocks/                 # stone_block, dirt_block, grass_block, water_block, snow_block, dll.
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
│   └── benchmarks.rs           # 17 Benchmark suite
├── worldgen/                   # Procedural World Generation Subsystem (Phase 4)
│   ├── mod.rs
│   ├── seed.rs                 # WorldSeed (u64 & SplitMix64 string hash), GeneratorVersion, SeedContext
│   ├── config.rs               # WorldGenConfig & WorldIdentity
│   ├── noise.rs                # Deterministic Gradient noise, fBm, & Ridged noise
│   ├── climate.rs              # Continentalness, Temperature, Moisture, Erosion, Peaks/Valleys
│   ├── biome.rs                # BiomeType & BiomeClassifier
│   ├── hydrology.rs            # 2D continuous river curve & lake basins
│   ├── terrain.rs              # Continuous height profiling H(x, z)
│   ├── voxelizer.rs            # ChunkVoxelizer (32³ voxelization & material assignment)
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
