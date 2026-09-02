# Omnisia 🌌

[![Rust](https://img.shields.io/badge/Rust-2021_Edition-orange.svg)](https://www.rust-lang.org/)
[![wgpu](https://img.shields.io/badge/wgpu-v24_(Metal)-blue.svg)](https://wgpu.rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Engine API](https://img.shields.io/badge/Engine_API-v0.2.0-green.svg)](#)

**Omnisia** adalah voxel sandbox engine berkinerja tinggi yang dibangun dari nol menggunakan **Rust murni** dan abstraksi grafis **`wgpu`** (Metal backend untuk macOS). 

Dirancang dengan prinsip **Engine-First, Data-Driven, Deterministic, & Scalable Hierarchical Streaming**, memisahkan secara tegas antara **Authoritative Near World (Full-Resolution Voxels)** dan **Derived Far World (Hierarchical LOD / Distant Horizons Boundary)**.

---

## 🏛️ Filosofi & Streaming Architecture (Phase 3)

Engine ini menerapkan arsitektur streaming hirarkis:

```text
                         WORLD
                           │
                           ▼
                    Chunk Scheduler
                           │
             ┌─────────────┼─────────────┐
             │             │             │
             ▼             ▼             ▼
          Load/IO      Generation      Save
             │             │             │
             └─────────────┼─────────────┘
                           ▼
                      Chunk Store
                           │
                     Resident Chunks
                           │
             ┌─────────────┴─────────────┐
             ▼                           ▼
        Meshing Jobs                Future Systems
             │
             ▼
        GPU Chunk Mesh


              Authoritative World
                     │
                     ▼
              Full Resolution
                32³ Chunks
                     │
                     ▼
              Future LOD Builder
                     │
                     ▼
           Distant Representation
```

> **"Near world = full-resolution voxel truth."**
> **"Far world = hierarchical derived representation."**

* **Chunk ≠ LOD Invariant:** `Chunk` tetap berukuran murni $32 \times 32 \times 32$ micro-voxel ($16 \times 16 \times 16$ meter, 128 KiB memory contiguous). Data LOD jauh tidak pernah mencemari struct `Chunk`.
* **Zero Main-Thread Blocking:** Seluruh operasi I/O disk, kompresi/dekompresi Zstd, generasi prosedural, dan meshing CPU berjalan pada background worker pool (`crossbeam_channel`). Main thread hanya menangani input, camera uniform, integrasi scheduler, dan upload GPU.
* **Deterministic Priority Scheduling:** Priority queue dengan penanganan berurutan: `Critical` $\to$ `High` $\to$ `Normal` $\to$ `Low` $\to$ `VeryLow`, dengan tie-breaking deterministik berdasarkan jarak, usia request, dan koordinat chunk.
* **Request Coalescing & Cancellation:** Mencegah redundansi permintaan job untuk koordinat yang sama dan membatalkan job yang keluar dari radius pandang secara kooperatif saat kamera bergerak cepat.
* **Stale Job Protection:** Pelacakan mutasi berbasis `revision` memastikan hasil async worker yang terlambat tidak dapat menimpa mutasi voxel terbaru (*no stale overwrites*).
* **Safe Eviction with Dirty Protection:** Chunk dengan status `SAVE_DIRTY` wajib disimpan ke disk terlebih dahulu sebelum dievict dari memori. Jika proses simpan gagal, chunk tetap resident.
* **Stable ResourceId Persistence via Palette Compression:** Persistensi ke disk menggunakan **Local Palette Table** berbasis stable `ResourceId` (`Vec<ResourceId>` + voxel palette indices) dikompresi Zstd level 3 (mencapai rasio kompresi hingga **120.9x**). Runtime `MaterialId` tidak pernah disimpan ke disk.

---

## 📊 Hasil Benchmark (MacBook Pro 2018 Reference)

Dijalankan pada arsitektur Intel Core i7 x86_64 dengan backend Metal:

| No | Pengujian Benchmark | Metrik Pengukuran | Keterangan & Analisis |
|:---|:---|:---|:---|
| 1 | **Chunk Indexing** | **0.23 ns / op** | $10^7$ iterasi dalam 2.34 ms ($O(1)$ inlined) |
| 2 | **Chunk Fill (32k voxels)** | **3.37 µs / chunk** | 128 KiB memory throughput ultra-cepat |
| 3 | **Culled Meshing 32³** | **0.319 ms / chunk** | 17,768 Vertices, 4,442 Quads per chunk |
| 4 | **Greedy Meshing 32³** | **0.820 ms / chunk** | 2,564 Vertices, 641 Quads (**6.93x Quad Reduction**) |
| 5 | **AO Calculation** | **19.84 ns / face** | 500,000 sampling sudut dalam 9.92 ms |
| 6 | **100 Chunks Parallel Meshing** | **89.33 ms** | Mengolah 100 chunk serentak (2.88 juta vertex) via Rayon |
| 7 | **1,000 Chunks Synthetic Meshing** | **1.02 s** | Mengolah 1,000 chunk (6.48 juta quad) via Rayon |
| 8 | **Chunk Palette Zstd Compress** | **2.09 ms** | 131,072 bytes $\to$ 1,084 bytes (**120.9x rasio kompresi**) |
| 9 | **Chunk Palette Zstd Decompress** | **885.49 µs** | Rekonstruksi chunk 32k voxel sempurna (< 1 ms) |
| 10 | **Connectivity BFS Traversal** | **10.77 ms** | Penelusuran 14,759 voxel klaster struktural |
| 11 | **Mod Discovery & Parsing** | **114.25 µs / run** | Discovery deterministik + validasi TOML manifest |
| 12 | **Voxel Hot Path Lookup (MaterialId)** | **1.36 ns / op** | **0 Overhead** runtime index array vs 34.77 ns String Hash |
| 13 | **Scheduler Queue Throughput** | **263.23 ns / req** | 10,000 request prioritization & insertion dalam 2.63 ms |
| 14 | **Streaming Simulation (1,000 Chunks)**| **106.43 ms** | 1,000 chunk streaming + memory budget management |

---

## 🚀 Menjalankan Engine & Tooling

### 1. Menjalankan Demo World Streaming Interaktif
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

### 3. Menjalankan Test Suite (30 Unit Tests)
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
    ├── materials/              # JSON definitions: stone, dirt, grass, metal_frame, dll.
    ├── blocks/                 # JSON definitions: stone_block, ag_core_casing_block, dll.
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
│   └── benchmarks.rs           # 15 Benchmark suite
├── streaming/                  # World Streaming Subsystem
│   ├── mod.rs
│   ├── residency.rs            # Lifecycle StateMachine (Residency, Persistence, Mesh)
│   ├── memory.rs               # MemoryBudget & MemoryUsage accounting
│   ├── eviction.rs             # Safe eviction policy (dirty protection)
│   ├── jobs.rs                 # JobPriority, ChunkJobRequest, ChunkJobResult
│   ├── generator.rs            # Deterministic ChunkGenerator & DemoChunkGenerator
│   ├── store.rs                # ChunkStore (resident chunks & in-flight sets)
│   └── scheduler.rs            # ChunkScheduler (priority queue, workers, coalescing, stale protect)
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
