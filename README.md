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

### Invariant & Prinsip Utama:
1. **4-State Rendering Model & Frustum Culling:** Pipeline rendering memisahkan 4 status independen: `CPU Resident` $\to$ `GPU Mesh Resident` $\to$ `Render-Distance Eligible` $\to$ `Frustum Visible` $\to$ `Draw Submission`.
2. **Zero Heap Allocation Frustum Culling:** Ekstraksi 6 bidang frustum presisi standar Metal NDC $[0, 1]$ dengan $p$-vertex testing $O(1)$ (throughput > 200 juta tes/detik).
3. **Canonical Vegetation Ownership & Multi-Chunk Stamping:** Setiap pohon/semak memiliki koordinat anchor dunia kanonikal independen urutan loading chunk ($A \to B == B \to A$).
4. **Replaceable Voxel Policy:** Vegetasi hanya menimpa udara atau dedaunan, tidak pernah menimpa batuan, bijih mineral, kristal, atau air.
5. **Volumetric 3D Caves & Overhangs:** Gua 3D berongga non-kolumnar (*cheese caverns & elongated worm tunnels*) dan overhang tebing sejati.
6. **Underground Stratification:** Pembagian lapisan geologi bertingkat (*Topsoil $\to$ Subsoil $\to$ Stone $\to$ Deepslate* pada $y < -32$).
7. **Deterministic Ore Distribution:** Sebaran urat/kantong bijih mineral (*Coal, Iron, Gold, Crystal*) yang hanya menggantikan batuan padat.
8. **Hardened Stale Async Identity:** Menggunakan tuple identitas `ChunkCoord + LifecycleGeneration + Revision` untuk mencegah race condition.
9. **Persistence Precedence:** Chunk yang telah tersimpan di disk (`RegionStore`) atau dimutasi oleh pemain **selalu menang** atas generator prosedural.

---

## 📊 Hasil Benchmark (MacBook Pro 2018 Reference)

Dijalankan pada arsitektur Intel Core i7 x86_64 dengan backend Metal dalam mode `release`:

| No | Pengujian Benchmark | Metrik Pengukuran | Keterangan & Analisis |
|:---|:---|:---|:---|
| 1 | **Chunk Indexing** | **0.26 ns / op** | Inlined $O(1)$ canonical index |
| 2 | **Chunk Fill (32k voxels)** | **3.42 µs / chunk** | 128 KiB memory throughput |
| 3 | **Culled Meshing 32³** | **0.320 ms / chunk** | 16,896 Vertices, 4,224 Quads per chunk |
| 4 | **Greedy Meshing 32³** | **0.611 ms / chunk** | 580 Vertices, 145 Quads (**29.13x Quad Reduction**) |
| 5 | **AO Calculation** | **14.28 ns / face** | 500,000 sampling sudut AO |
| 6 | **100 Chunks Procedural Meshing** | **111.86 ms** | Mengolah 100 chunk prosedural dengan Rayon |
| 7 | **Chunk Palette Zstd Compress** | **1.46 ms** | 131,072 bytes $\to$ 1,307 bytes (**100.3x rasio kompresi**) |
| 8 | **Chunk Palette Zstd Decompress** | **688.67 µs** | Rekonstruksi chunk 32k voxel sempurna (< 1 ms) |
| 9 | **Noise 3D fBm Sampling** | **133.63 ns / sample** | $10^6$ sampling volumetrik 3D bebas alokasi |
| 10 | **3D Cave & Worm Tunnel Sampling** | **254.87 ns / point** | 100,000 titik evaluasi rongga gua 3D |
| 11 | **3D Overhang & Feature Eval** | **39.00 ns / point** | 100,000 titik evaluasi densitas tebing |
| 12 | **Phase 6 Procedural Chunk Gen** | **7.141 ms / chunk** | Generasi 32³ micro-voxels dengan 3D caves & vegetasi kanonikal |
| 13 | **100 Chunks Parallel Gen (Rayon)** | **95.24 ms total** | **~0.95 ms/chunk amortized** |
| 14 | **Frustum Culling Intersection** | **4.86 ns / chunk** | **> 200M tests/sec, 0 alokasi heap** |

---

## 🚀 Menjalankan Engine

### 1. Menjalankan Engine Utama
```bash
cargo run --release
```

Kontrol Kamera:
- `W`, `A`, `S`, `D`: Gerak horizontal (relatif arah hadap)
- `Space`: Terbang naik (+Y)
- `Shift`: Terbang turun (-Y)
- `Klik Kanan + Gerak Mouse`: Rotasi kamera (*First-Person Free Look*)

### 2. Menjalankan Mod Validation CLI
```bash
cargo run --release -- --validate-mods
```

### 3. Menjalankan Benchmark Suite
```bash
cargo run --release --bin benchmarks
```

### 4. Menjalankan Seluruh Unit Test Suite (61 Tests)
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
