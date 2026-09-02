# Omnisia 🌌

[![Rust](https://img.shields.io/badge/Rust-2021_Edition-orange.svg)](https://www.rust-lang.org/)
[![wgpu](https://img.shields.io/badge/wgpu-v24_(Metal)-blue.svg)](https://wgpu.rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Engine API](https://img.shields.io/badge/Engine_API-v0.2.0-green.svg)](#)

**Omnisia** adalah voxel sandbox engine berkinerja tinggi yang dibangun dari nol menggunakan **Rust murni** dan abstraksi grafis **`wgpu`** (Metal backend untuk macOS). 

Dirancang dengan prinsip **Engine-First, Data-Driven, & Deterministic**, memisahkan secara fisik dan arsitektural antara **Engine Core**, **Core Content**, **Mod Content**, dan **Assets**.

---

## 🏛️ Filosofi & Content Architecture

Engine ini menerapkan pemisahan batas konten secara tegas:

```text
                    ┌──────────────────┐
                    │      ENGINE      │
                    └────────┬─────────┘
                             │
             ┌───────────────┴────────────────┐
             │                                │
             ▼                                ▼
    ┌─────────────────┐             ┌─────────────────┐
    │   CORE CONTENT  │             │   MOD CONTENT   │
    │                 │             │                 │
    │ content/core/   │             │ mods/<id>/      │
    │                 │             │                 │
    │ materials       │             │ materials       │
    │ blocks          │             │ blocks          │
    │ textures        │             │ textures        │
    │ models          │             │ models          │
    └────────┬────────┘             └────────┬────────┘
             │                               │
             └──────────────┬────────────────┘
                            ▼
                  ┌─────────────────────┐
                  │ VALIDATION +        │
                  │ OWNERSHIP CHECK     │
                  └──────────┬──────────┘
                             │
                             ▼
                  ┌─────────────────────┐
                  │ OVERRIDE RESOLVER   │
                  │                     │
                  │ explicit only       │
                  └──────────┬──────────┘
                             │
                             ▼
                  ┌─────────────────────┐
                  │ RESOLVED REGISTRY   │
                  │                     │
                  │ ResourceId →        │
                  │ Definition +        │
                  │ Provenance          │
                  └──────────┬──────────┘
                             │
                ┌────────────┴────────────┐
                ▼                         ▼
        ┌───────────────┐         ┌───────────────┐
        │ Runtime       │         │ AssetResolver │
        │ MaterialId    │         │ AssetId       │
        │ BlockId       │         │               │
        └───────────────┘         └───────────────┘
```

> **"Safety by architecture, bukan safety by convention."**
> Mod bebas mengganti konten bawaan atau menambahkan konten baru, tetapi sistem secara struktural menolak penggantian tidak disengaja (*accidental replacement*).

* **Physical Content Separation:** Konten bawaan game berada di `content/core/`, sedangkan konten mod berada di `mods/<mod_id>/`.
* **Single Source of Truth:** Seluruh definisi material & blok bawaan adalah data JSON murni di `content/core/` (bukan hardcoded di Rust).
* **Reserved Namespace:** Namespace `core:*` hanya boleh didefinisikan oleh Core Content. Mod eksternal dilarang membuat resource baru ber-namespace `core`.
* **Explicit Override System:** Penimpaan konten bawaan wajib dideklarasikan secara eksplisit di `mod.toml` melalui blok `[[overrides]]`.
* **Persistent Identity Preservation:** Jika mod meng-override `core:stone`, identitas persisten dunia/save file tetap `core:stone`, bukan ID replacement.
* **Namespaced Asset Identity:** Asset ID menggunakan format kanonikal `namespace:path` (misal: `core:textures/stone.png`, `example_mod:models/reactor.glb`) yang di-resolve secara aman via `AssetResolver` dengan proteksi path traversal.

---

## 🛠️ Panduan Pembuatan Mod & Explicit Override

### 1. Struktur Folder Mod
Buat folder mod di dalam direktori `mods/`, misalnya `mods/my_custom_mod/`:
```text
mods/
└── my_custom_mod/
    ├── mod.toml
    ├── materials/
    │   ├── titanium.json
    │   └── custom_stone.json
    ├── blocks/
    │   └── heavy_thruster.json
    ├── textures/
    │   └── thruster.png
    └── models/
        └── thruster.glb
```

### 2. Manifest `mod.toml` dengan Explicit Override
```toml
id = "my_custom_mod"
name = "My Custom Mod"
version = "0.1.0"
engine_api = "0.2"
description = "Mod kustom menambahkan material titanium dan meng-override batu bawaan"

[author]
name = "Developer Name"

[dependencies]
core = "0.2"

# Deklarasi Explicit Override (Mengganti core:stone dengan custom_stone milik mod ini)
[[overrides]]
target = "core:stone"
replacement = "my_custom_mod:custom_stone"
```

### 3. Definisikan Material (`materials/custom_stone.json`)
```json
{
  "id": "my_custom_mod:custom_stone",
  "name": "Heavy Granite Stone",
  "density": 3100.0,
  "shear_strength": 18.0,
  "color": [0.48, 0.50, 0.54],
  "solid": true,
  "transparent": false
}
```

### 4. Validasi & Jalankan Game
```bash
# Validasi integritas Core Content, Mod, Namespace, & Overrides:
cargo run --release -- --validate-mods

# Jalankan game:
cargo run --release
```

---

## 📊 Hasil Benchmark (MacBook Pro 2018 Reference)

Dijalankan pada arsitektur Intel Core i7 x86_64 dengan backend Metal:

| No | Pengujian Benchmark | Metrik Pengukuran | Keterangan & Analisis |
|:---|:---|:---|:---|
| 1 | **Chunk Indexing** | **0.25 ns / op** | $10^7$ iterasi dalam 2.47 ms ($O(1)$ inlined) |
| 2 | **Chunk Fill (32k voxels)** | **3.54 µs / chunk** | 128 KiB memory throughput ultra-cepat |
| 3 | **Culled Meshing 32³** | **0.321 ms / chunk** | 17,768 Vertices, 4,442 Quads per chunk |
| 4 | **Greedy Meshing 32³** | **0.667 ms / chunk** | 2,564 Vertices, 641 Quads (**6.93x Quad Reduction**) |
| 5 | **AO Calculation** | **13.77 ns / face** | 500,000 sampling sudut dalam 6.88 ms |
| 6 | **100 Chunks Parallel Meshing** | **54.31 ms** | Mengolah 100 chunk serentak (2.88 juta vertex) via Rayon |
| 7 | **1,000 Chunks Synthetic Meshing** | **745.34 ms** | Mengolah 1,000 chunk (6.48 juta quad) via Rayon |
| 8 | **Chunk Zstd Compression** | **5.54 ms** | 131,072 bytes $\to$ 1,863 bytes (**70.4x rasio kompresi**) |
| 9 | **Chunk Zstd Decompression** | **8.48 ms** | Rekonstruksi chunk 32k voxel sempurna |
| 10 | **Connectivity BFS Traversal** | **5.13 ms** | Penelusuran 14,759 voxel klaster struktural |
| 11 | **Mod Discovery & Parsing** | **79.95 µs / run** | Discovery deterministik + validasi TOML manifest |
| 12 | **Voxel Hot Path Lookup (MaterialId)** | **1.23 ns / op** | **0 Overhead** runtime index array vs 34.77 ns String Hash |

---

## 🚀 Menjalankan Engine & Tooling

### 1. Menjalankan Demo World Interaktif
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

### 3. Menjalankan Test Suite
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
├── main.rs                     # Winit 0.30 interactive app & CLI arguments
├── material.rs                 # MaterialId (2 bytes) & MaterialRegistry
├── voxel.rs                    # VoxelBlock (4 bytes compact struct)
├── coord.rs                    # Canonical indexing & Euclidean negative math
├── chunk.rs                    # Authoritative Chunk 32³ (128 KiB flat array)
├── camera.rs                   # FPS/Orbital 3D camera & ViewProj uniform
├── renderer.rs                 # wgpu Metal pipeline, depth buffer, mesh cache
├── shader.wgsl                 # Half-Lambert + Pastel palette + Baked AO shader
├── storage.rs                  # RegionStore abstraction & Zstd compression
├── world.rs                    # World runtime (consumes ResolvedContent)
├── bin/
│   └── benchmarks.rs           # 13 Benchmark suite
├── modding/
│   ├── mod.rs                  # Root modding exports
│   ├── asset.rs                # AssetId (namespaced) & AssetResolver (path traversal safe)
│   ├── resource_id.rs          # ModId & ResourceId (namespace:path)
│   ├── version.rs              # ENGINE_API_VERSION & semver compatibility
│   ├── manifest.rs             # ModManifest, OverrideDeclaration, & mod.toml parser
│   ├── definitions.rs          # MaterialDefinition & BlockDefinition JSON schemas
│   ├── registry.rs             # ResourceRegistry<T>, BlockRegistry, & Provenance
│   ├── dependency.rs           # DependencyResolver (Kahn Topological Sort)
│   ├── discovery.rs            # Deterministic ModDiscovery
│   ├── loader.rs               # ModLoader for Core Content, Mod Content, & Overrides
│   ├── runtime.rs              # ContentRuntime & ResolvedContent orchestration
│   └── validation.rs           # ValidationReport & CLI tooling (--validate-mods)
└── mesh/
    ├── mod.rs                  # Mesher module exports
    ├── types.rs                # VoxelVertex (GPU) & MeshData (CPU)
    ├── ao.rs                   # Vertex Ambient Occlusion calculation
    ├── culled.rs               # Culled Face Mesher
    └── greedy.rs               # High-performance Greedy Mesher
```

---

## 📜 Lisensi
Dilisensikan di bawah [MIT License](LICENSE).
