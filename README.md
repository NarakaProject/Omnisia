# Omnisia 🌌

[![Rust](https://img.shields.io/badge/Rust-2021_Edition-orange.svg)](https://www.rust-lang.org/)
[![wgpu](https://img.shields.io/badge/wgpu-v24_(Metal)-blue.svg)](https://wgpu.rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Engine API](https://img.shields.io/badge/Engine_API-v0.2.0-green.svg)](#)

**Omnisia** adalah voxel sandbox engine berkinerja tinggi yang dibangun dari nol menggunakan **Rust murni** dan abstraksi grafis **`wgpu`** (Metal backend untuk macOS). 

Dirancang dengan prinsip **Engine-First, Data-Driven, Deterministic, 3D Volumetric Procedural Generation, Scalable Hierarchical Streaming, & Zero Frame Degradation Rendering**, memisahkan secara tegas antara **Authoritative Near World (Full-Resolution Voxels)** dan **Derived Far World (Hierarchical LOD / Distant Horizons Boundary)**.

---

## 🌲 Arsitektur Generasi Dunia Volumetrik & Vegetasi (Phase 6)

Engine ini menerapkan arsitektur generasi dunia prosedural berbasis medan multi-skala, densitas volumetrik 3D, dan vegetasi kanonikal multi-chunk:

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
            Canonical Vegetation Stamping
             (Oak, Pine, Desert Shrub, Grass)
                            │
                            ▼
                 32³ Authoritative Chunk
```

## 🏛️ Arsitektur Structural Connectivity & Validasi Skala Metrik (Phase 7)

Engine mengintegrasikan sistem topologi struktural berbasis event (*event-driven structural connectivity*) dan pengujian skala metrik dunia nyata:

```text
               World::set_voxel_world(world_voxel, block)
                                   │
                                   ▼
                   Authoritative Chunk Mutation
                                   │
                                   ▼
                       StructuralEvent Emission
               (VoxelPlaced / VoxelRemoved / VoxelReplaced)
                                   │
                                   ▼
                   StructuralSystem::process_event
                                   │
         ┌─────────────────────────┴─────────────────────────┐
         ▼                                                   ▼
6-Connected Adjacency                               Data-Driven Anchors
(Face-touching ±X, ±Y, ±Z)                   (BlockComponents::structural_anchor)
         │                                                   │
         └─────────────────────────┬─────────────────────────┘
                                   ▼
                    Localized Connectivity Traversal
                       (Early-exit on first anchor)
                                   │
                 ┌─────────────────┴─────────────────┐
                 ▼                                   ▼
      Connected to Anchor                  Unconnected to Any Anchor
(No action, structure stable)                        │
                                                     ▼
                                          Detached Aggregate Extraction
                                      (Voxel transfer: Chunk -> Aggregate)
                                      (No double ownership, data-only firewall)
```

### Invariant & Fitur Utama Phase 7:
1. **Event-Driven Structural Connectivity:** Mutasi voxel melalui `World::set_voxel_world` secara langsung memicu evaluasi konektivitas tetangga. Tidak pernah ada full-world BFS per frame.
2. **Data-Driven Anchor Policy:** Menggunakan `BlockComponents::structural_anchor` (`stone_block.json` & `deepslate_block.json`). Engine tidak meng-hardcode nama material atau koordinat Y; mod dapat mendaftarkan anchor kustom tanpa mengubah kode engine.
3. **Model Ketetanggaan 6-Arah (6-Connected):** Dua voxel hanya terhubung jika bersentuhan pada sisi muka kubus ($\pm X, \pm Y, \pm Z$). Sentuhan diagonal (rusuk atau sudut) ditolak.
4. **Unloaded Chunk Guard:** Menemukan chunk di luar `ChunkStore` tidak pernah diasumsikan sebagai udara (`AIR`) atau lepas (`Detached`), melainkan `PendingUnloadedNeighbor`.
5. **Search Budget Guard:** Jika batas alokasi pencarian tercapai sebelum menemukan anchor, status adalah `IndeterminateBudgetExceeded` (bukan lepas).
6. **Integritas Detached Aggregate:** Gugusan lepas diekstraksi ke dalam `DetachedAggregate` dengan koordinat relatif, bounding box, dan material utuh. Voxel dipindahkan dari chunk otoritatif (`set_voxel_world(AIR)`) untuk menjamin ketiadaan kepemilikan ganda (*no double ownership*).
7. **Developer Free-Flight Camera ($m/s$):** Pergerakan kamera menggunakan kecepatan fisik meter per detik ($m/s$) yang invarian terhadap frame-rate dengan 4 preset: `[1] Slow (5 m/s)`, `[2] Normal (20 m/s)`, `[3] Fast (100 m/s)`, dan `[4] Extreme (500 m/s)`.
8. **Scale Ruler & Referensi Manusia:** Penggaris metrik standar ($1\text{m}, 2\text{m}, 5\text{m}, 10\text{m}, 25\text{m}, 50\text{m}, 100\text{m}$) dan referensi manusia $\approx 1.8\text{m}$ ($3.6\text{ voxel}$) untuk memverifikasi dimensi fisik vegetasi dan kontur medan.
9. **Audit Streaming & Validasi Multi-Kilometer:** Traversal multi-kilometer ($100\text{m}, 250\text{m}, 500\text{m}, 1\text{km}$, koordinat negatif, dan kembali ke origin) membuktikan stabilitas FPS ($109\text{–}195\text{ FPS}$) dan konsumsi memori stabil ($\le 85\text{ MB}$).

## ⚡ Arsitektur Dynamic Aggregate Runtime & AntiGravity (Phase 8A)

Engine mengintegrasikan runtime simulasi dinamis untuk gugusan voxel yang terlepas (*detached aggregates*) dengan model kepemilikan tunggal yang otoritatif (*single authoritative owner*):

```text
┌───────────────────────────────────────┐
│              STATIC WORLD             │
│        (ChunkStore / Near World)      │
└───────────────────┬───────────────────┘
                    │
                    │ 1. Structural Break
                    ▼
┌───────────────────────────────────────┐
│           DetachedAggregate           │
│        (Topology + Voxel Move)        │
└───────────────────┬───────────────────┘
                    │
                    │ 2. Atomic Ownership Transfer (Prepare -> Commit)
                    ▼
┌───────────────────────────────────────┐
│              DynamicBody              │
│  - BTreeMap deterministik             │
│  - 30 Hz Fixed-Timestep Accumulator   │
│  - Gravity / AntiGravity (scale = 0)  │
│  - Swept Vertical Collision Guard     │
│  - Unloaded Chunk Barrier (Unknown!=0)│
│  - Grid Integer Voxel Snapping        │
└───────────────────┬───────────────────┘
                    │
                    │ 3. Two-Phase Reintegration (Prepare -> Validate -> Commit)
                    ▼
┌───────────────────────────────────────┐
│              STATIC WORLD             │
│   - Restored at Exact Integer Lattice │
│   - MESH_DIRTY & SAVE_DIRTY Marked    │
│   - Dynamic Body Released (Zero Leak) │
└───────────────────┬───────────────────┘
```

---

## 📊 Hasil Benchmark Resmi Engine (Phase 8A)

Benchmark dijalankan secara presisi pada perangkat target **MacBook Pro 2018 (Intel Core i7 x86_64, macOS Metal)**:

| # | Skenario Benchmark | Hasil / Throughput | Catatan Arsitektur |
|:---|:---|:---|:---|
| 1 | **Chunk Indexing** | **0.26 ns / op** | Inlined $O(1)$ canonical index |
| 2 | **Chunk Fill (32k voxels)** | **3.37 µs / chunk** | 128 KiB memory throughput |
| 3 | **Culled Meshing 32³** | **0.334 ms / chunk** | 16,896 Vertices, 4,224 Quads per chunk |
| 4 | **Greedy Meshing 32³** | **0.819 ms / chunk** | 580 Vertices, 145 Quads (**29.13x Quad Reduction**) |
| 5 | **AO Calculation** | **12.97 ns / face** | 500,000 sampling sudut AO |
| 6 | **100 Chunks Procedural Meshing** | **114.30 ms** | Mengolah 100 chunk prosedural dengan Rayon |
| 7 | **Chunk Palette Zstd Compress** | **2.07 ms** | 131,072 bytes $\to$ 1,307 bytes (**100.3x rasio kompresi**) |
| 8 | **Chunk Palette Zstd Decompress** | **899.78 µs** | Rekonstruksi chunk 32k voxel sempurna (< 1 ms) |
| 9 | **Noise 3D fBm Sampling** | **125.97 ns / sample** | $10^6$ sampling volumetrik 3D bebas alokasi |
| 10 | **3D Cave & Worm Tunnel Sampling** | **218.30 ns / point** | 100,000 titik evaluasi rongga gua 3D |
| 11 | **3D Overhang & Feature Eval** | **37.97 ns / point** | 100,000 titik evaluasi densitas tebing |
| 12 | **Phase 6 Procedural Chunk Gen** | **7.237 ms / chunk** | Generasi 32³ micro-voxels dengan 3D caves & vegetasi kanonikal |
| 13 | **100 Chunks Parallel Gen (Rayon)** | **100.55 ms total** | **~1.00 ms/chunk amortized** |
| 14 | **Frustum Culling Intersection** | **10.54 ns / chunk** | **> 94M tests/sec, 0 alokasi heap** |
| 15 | **Localized Structural Connectivity** | **10.69 µs / check** | **Rata-rata 15.0 voxel terpindai, early-exit anchor** |
| 16 | **Detached Aggregate Extraction** | **0.26 µs / op** | **3.82M extractions/sec (125 voxels)** |
| 17 | **DynamicBody 30 Hz Physics Tick** | **10.09 µs / tick** | **99,088 ticks/sec (100 badan dinamis, swept collision)** |
| 18 | **Two-Phase Dynamic Reintegration** | **4.70 µs / op** | **212,557 ops/sec (Prepare + Validate + Commit)** |

---

## 🚀 Menjalankan Engine

### 1. Menjalankan Engine Utama
```bash
cargo run --release
```

Kontrol Kamera (Developer Free-Flight):
- `W`, `A`, `S`, `D`: Gerak horizontal (relatif arah hadap)
- `Space`: Terbang naik (+Y)
- `Shift`: Terbang turun (-Y)
- `1`, `2`, `3`, `4`: Preset kecepatan ($5\text{ m/s}$, $20\text{ m/s}$, $100\text{ m/s}$, $500\text{ m/s}$)
- `Klik Kanan + Gerak Mouse`: Rotasi kamera (*First-Person Free Look*)

### 2. Menjalankan Scale Validation Report CLI
```bash
cargo run --release -- --scale-validation
```

### 3. Menjalankan Mod Validation CLI
```bash
cargo run --release -- --validate-mods
```

### 4. Menjalankan Real-World Traversal Validation (100m, 250m, 500m, 1km)
```bash
cargo run --release --bin traversal_validation
```

### 5. Menjalankan Dynamic Aggregate Runtime Validation (Phase 8A)
```bash
cargo run --release --bin physics_validation
```

### 6. Menjalankan Benchmark Suite Lengkap
```bash
cargo run --release --bin benchmarks
```

### 7. Menjalankan Seluruh Unit Test Suite (102 Tests)
```bash
cargo test
```

---

## 📂 Struktur Mod & Content Registry

```text
content/
└── core/
    ├── materials/       # stone, dirt, grass, wood_oak, leaves_oak, wood_pine, leaves_pine, shrub, dll.
    └── blocks/          # Definisi blok struktural dengan resource ID 'core:<name>'
mods/
└── example_mod/         # Contoh mod eksternal pihak ketiga dengan explicit overrides
docs/
└── reports/             # Laporan teknis mendalam setiap fase pengembangan
```

---

## 📜 Lisensi
Proyek ini dilisensikan di bawah lisensi MIT. Lihat [LICENSE](LICENSE) untuk detail.
