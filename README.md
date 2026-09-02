# Omnisia 🌌

[![Rust](https://img.shields.io/badge/Rust-2021_Edition-orange.svg)](https://www.rust-lang.org/)
[![wgpu](https://img.shields.io/badge/wgpu-v24_(Metal)-blue.svg)](https://wgpu.rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Engine API](https://img.shields.io/badge/Engine_API-v0.2.0-green.svg)](#)

**Omnisia** adalah voxel sandbox engine berkinerja tinggi yang dibangun dari nol menggunakan **Rust murni** dan abstraksi grafis **`wgpu`** (Metal backend untuk macOS). 

Dirancang dengan prinsip **Engine-First, Data-Driven, & Deterministic**, memungkinkan modder dan komunitas menambahkan material, blok, dan konten baru secara deklaratif tanpa perlu menyentuh atau mengompilasi ulang kode sumber Rust engine.

---

## 🏛️ Filosofi Arsitektur

Engine ini memisahkan representasi data secara konseptual ke dalam tiga tingkatan:

```text
VOXEL  ──(Data & State Lokal)──>  STRUCTURE  ──(Konektivitas & Agregat)──>  DYNAMIC BODY (Fisika Rigid Body O(1))
```

> **"Voxel adalah data. Structure adalah aggregate. Physics adalah body."**
> **"Engine menyediakan capability. Content mendefinisikan data. Mod menyediakan content."**

* **Micro-Voxel Scale:** Ukuran $0.5 \times 0.5 \times 0.5\text{ meter}$ ($0.125\text{ m}^3$).
* **Authoritative Chunk:** Dimensi $32 \times 32 \times 32$ micro-voxel ($16 \times 16 \times 16\text{ meter}$) dalam flat contiguous array 128 KiB heap.
* **VoxelBlock Compact:** Tepat **4 byte** (`#[repr(C)]`) per voxel dengan verifikasi compile-time invariant.
* **Modding Layer Deterministic:** Pemuatan mod berbasis namespace (`namespace:path`), manifest `mod.toml`, resolusi grafik dependensi (topological sort), dan isolasi error.
* **Dual Meshing Pipeline:** Menyediakan **Culled Face Meshing** dan **Greedy Meshing** berkecepatan tinggi (mereduksi quad poligon hingga **6.93x**).
* **Modern Shading:** Shader WGSL **Half-Lambert** $(\mathbf{N} \cdot \mathbf{L} \times 0.5 + 0.5)^2$ dengan palet pastel datar dan Ambient Occlusion (AO) per-vertex tanpa tekstur piksel kasar 16×16.

---

## 🛠️ Panduan Pembuatan Mod (Data-Driven Modding)

Bagaimana cara membuat blok dan material baru tanpa menyentuh source code Rust?

### 1. Buat Direktori Mod
Buat folder baru di dalam direktori `mods/`, misalnya `mods/my_custom_mod/`:
```text
mods/
└── my_custom_mod/
    ├── mod.toml
    ├── materials/
    │   └── titanium.json
    └── blocks/
        ├── titanium_block.json
        └── heavy_thruster.json
```

### 2. Buat Manifest `mod.toml`
```toml
id = "my_custom_mod"
name = "My Custom Mod"
version = "0.1.0"
engine_api = "0.2"
description = "Mod kustom menambahkan material titanium dan blok pendorong anti-gravitasi"

[author]
name = "Developer Name"

[dependencies]
core = "0.2"
```

### 3. Definisikan Material (`materials/titanium.json`)
```json
{
  "id": "my_custom_mod:titanium",
  "name": "Titanium Alloy",
  "density": 4500.0,
  "shear_strength": 380.0,
  "color": [0.45, 0.48, 0.55],
  "solid": true,
  "transparent": false
}
```

### 4. Definisikan Blok (`blocks/heavy_thruster.json`)
Blok mendukung sistem **Generic Capabilities & Components**:
```json
{
  "id": "my_custom_mod:heavy_thruster",
  "material": "my_custom_mod:titanium",
  "hardness": 60.0,
  "components": {
    "lift_capacity": {
      "capacity_kg": 3500000.0,
      "radius_m": 50.0,
      "power_consumption_w": 65000.0
    }
  },
  "tags": ["machine", "propulsion", "anti_gravity"]
}
```

### 5. Validasi & Jalankan Mod
```bash
# Validasi integritas manifest, dependensi, dan schema JSON:
cargo run --release -- --validate-mods

# Jalankan game:
cargo run --release
```

---

## 📊 Hasil Benchmark (MacBook Pro 2018 Reference)

Dijalankan pada arsitektur Intel Core i7 x86_64 dengan backend Metal:

| No | Pengujian Benchmark | Metrik Pengukuran | Keterangan & Analisis |
|:---|:---|:---|:---|
| 1 | **Chunk Indexing** | **0.32 ns / op** | $10^7$ iterasi dalam 3.24 ms ($O(1)$ inlined) |
| 2 | **Chunk Fill (32k voxels)** | **3.75 µs / chunk** | Pengisian 128 KiB memory throughput ultra-cepat |
| 3 | **Culled Meshing 32³** | **0.645 ms / chunk** | 17,768 Vertices, 4,442 Quads per chunk |
| 4 | **Greedy Meshing 32³** | **1.011 ms / chunk** | 2,564 Vertices, 641 Quads (**6.93x Quad Reduction**) |
| 5 | **AO Calculation** | **16.03 ns / face** | 500,000 sampling sudut dalam 8.01 ms |
| 6 | **100 Chunks Parallel Meshing** | **57.73 ms** | Mengolah 100 chunk serentak (2.88 juta vertex) via Rayon |
| 7 | **1,000 Chunks Synthetic Meshing** | **1.00 s** | Mengolah 1,000 chunk (6.48 juta quad) via Rayon |
| 8 | **Chunk Zstd Compression** | **4.89 ms** | 131,072 bytes $\to$ 1,863 bytes (**70.4x rasio kompresi**) |
| 9 | **Chunk Zstd Decompression** | **7.29 ms** | Rekonstruksi chunk 32k voxel sempurna |
| 10 | **Connectivity BFS Traversal** | **4.44 ms** | Penelusuran 14,759 voxel klaster struktural |
| 11 | **Mod Discovery & Parsing** | **76.70 µs / run** | Discovery deterministik + validasi TOML manifest |
| 12 | **Voxel Hot Path Lookup (MaterialId)** | **1.22 ns / op** | **0 Overhead** runtime index array vs 39.68 ns String Hash |

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

### 2. Menjalankan Mod Validator
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
src/
├── lib.rs                      # Root library re-exports
├── main.rs                     # Winit 0.30 interactive app & CLI arguments
├── material.rs                 # MaterialId (4 bytes invariant) & MaterialRegistry
├── voxel.rs                    # VoxelBlock (4 bytes compact struct)
├── coord.rs                    # Canonical indexing & Euclidean negative math
├── chunk.rs                    # Authoritative Chunk 32³ (128 KiB flat array)
├── camera.rs                   # FPS/Orbital 3D camera & ViewProj uniform
├── renderer.rs                 # wgpu Metal pipeline, depth buffer, mesh cache
├── shader.wgsl                 # Half-Lambert + Pastel palette + Baked AO shader
├── storage.rs                  # RegionStore abstraction & Zstd compression
├── world.rs                    # World sparse runtime & Demo World generator
├── bin/
│   └── benchmarks.rs           # 13 Benchmark suite
├── modding/
│   ├── mod.rs                  # Root modding exports
│   ├── resource_id.rs          # ModId & ResourceId (namespace:path)
│   ├── version.rs              # ENGINE_API_VERSION & semver compatibility
│   ├── manifest.rs             # ModManifest & mod.toml parser
│   ├── definitions.rs          # MaterialDefinition & BlockDefinition JSON schemas
│   ├── registry.rs             # ResourceRegistry<T> & BlockRegistry
│   ├── dependency.rs           # DependencyResolver (Kahn Topological Sort)
│   ├── discovery.rs            # Deterministic ModDiscovery
│   ├── loader.rs               # ModLoader for JSON materials & blocks
│   └── validation.rs           # ValidationReport & CLI tooling
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
