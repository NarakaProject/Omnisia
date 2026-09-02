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

---

## 📊 Hasil Benchmark (MacBook Pro 2018 Reference)

Dijalankan pada arsitektur Intel Core i7 x86_64 dengan backend Metal dalam mode `release`:

| No | Pengujian Benchmark | Metrik Pengukuran | Keterangan & Analisis |
|:---|:---|:---|:---|
| 1 | **Chunk Indexing** | **0.24 ns / op** | Inlined $O(1)$ canonical index |
| 2 | **Chunk Fill (32k voxels)** | **3.73 µs / chunk** | 128 KiB memory throughput |
| 3 | **Culled Meshing 32³** | **0.373 ms / chunk** | 16,896 Vertices, 4,224 Quads per chunk |
| 4 | **Greedy Meshing 32³** | **0.694 ms / chunk** | 580 Vertices, 145 Quads (**29.13x Quad Reduction**) |
| 5 | **AO Calculation** | **13.97 ns / face** | 500,000 sampling sudut AO |
| 6 | **100 Chunks Procedural Meshing** | **128.02 ms** | Mengolah 100 chunk prosedural dengan Rayon |
| 7 | **Chunk Palette Zstd Compress** | **1.54 ms** | 131,072 bytes $\to$ 1,307 bytes (**100.3x rasio kompresi**) |
| 8 | **Chunk Palette Zstd Decompress** | **753.39 µs** | Rekonstruksi chunk 32k voxel sempurna (< 1 ms) |
| 9 | **Noise 3D fBm Sampling** | **143.05 ns / sample** | $10^6$ sampling volumetrik 3D bebas alokasi |
| 10 | **3D Cave & Worm Tunnel Sampling** | **268.89 ns / point** | 100,000 titik evaluasi rongga gua 3D |
| 11 | **3D Overhang & Feature Eval** | **45.13 ns / point** | 100,000 titik evaluasi densitas tebing |
| 12 | **Phase 6 Procedural Chunk Gen** | **7.275 ms / chunk** | Generasi 32³ micro-voxels dengan 3D caves & vegetasi kanonikal |
| 13 | **100 Chunks Parallel Gen (Rayon)** | **107.77 ms total** | **~1.07 ms/chunk amortized** |
| 14 | **Frustum Culling Intersection** | **4.94 ns / chunk** | **> 200M tests/sec, 0 alokasi heap** |
| 15 | **Localized Structural Connectivity** | **9.95 µs / check** | **Rata-rata 15.0 voxel terpindai, early-exit anchor** |
| 16 | **Detached Aggregate Extraction** | **0.21 µs / op** | **4.73M extractions/sec (125 voxels)** |

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

### 5. Menjalankan Benchmark Suite Lengkap
```bash
cargo run --release --bin benchmarks
```

### 6. Menjalankan Seluruh Unit Test Suite (79 Tests)
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
