# Omnisia 🌌

[![Rust](https://img.shields.io/badge/Rust-2021_Edition-orange.svg)](https://www.rust-lang.org/)
[![wgpu](https://img.shields.io/badge/wgpu-v24_(Metal)-blue.svg)](https://wgpu.rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**Omnisia** adalah voxel sandbox engine berkinerja tinggi yang dibangun dari nol menggunakan **Rust murni** dan abstraksi grafis **`wgpu`** (Metal backend untuk macOS). 

Dirancang khusus dengan prinsip arsitektur sistem modular untuk mensimulasikan dunia voxel berskala masif, fisika anti-gravitasi, integritas struktural dinamis, dan ekstraksi rigid body pulau jatuh (*dynamic island impact*).

---

## 🏛️ Filosofi Arsitektur

Engine ini memisahkan representasi data secara konseptual ke dalam tiga tingkat:

```text
VOXEL  ──(Data & State Lokal)──>  STRUCTURE  ──(Konektivitas & Agregat)──>  DYNAMIC BODY (Fisika Rigid Body O(1))
```

> **"Voxel adalah data. Structure adalah aggregate. Physics adalah body."**

* **Micro-Voxel Scale:** Ukuran $0.5 \times 0.5 \times 0.5\text{ meter}$ ($0.125\text{ m}^3$).
* **Authoritative Chunk:** Dimensi $32 \times 32 \times 32$ micro-voxel ($16 \times 16 \times 16\text{ meter}$) dalam flat contiguous array 128 KiB heap.
* **VoxelBlock Compact:** Tepat **4 byte** (`#[repr(C)]`) per voxel dengan verifikasi compile-time invariant.
* **Modern Shading:** Shader WGSL **Half-Lambert** $(\mathbf{N} \cdot \mathbf{L} \times 0.5 + 0.5)^2$ dengan palet pastel datar dan Ambient Occlusion (AO) per-vertex termodulasi tanpa tekstur piksel kasar 16×16.
* **Dual Meshing Pipeline:** Menyediakan **Culled Face Meshing** dan **Greedy Meshing** berkecepatan tinggi (mereduksi quad poligon hingga **6.93x**).

---

## 📊 Hasil Benchmark (MacBook Pro 2018 Reference)

Dijalankan pada arsitektur Intel Core i7 x86_64 dengan GPU Metal:

| No | Pengujian Benchmark | Metrik Pengukuran | Keterangan |
|:---|:---|:---|:---|
| 1 | **Chunk Indexing** | **0.25 ns / op** | $10^7$ iterasi dalam 2.47 ms ($O(1)$ SIMD friendly) |
| 2 | **Chunk Fill (32k voxels)** | **2.99 µs / chunk** | Pengisian 128 KiB memory throughput ultra-cepat |
| 3 | **Culled Meshing 32³** | **0.309 ms / chunk** | 17,768 Vertices, 4,442 Quads per chunk bukit |
| 4 | **Greedy Meshing 32³** | **0.708 ms / chunk** | 2,564 Vertices, 641 Quads (**6.93x Quad Reduction**) |
| 5 | **AO Calculation** | **17.51 ns / face** | 500,000 sampling kalkulasi sudut dalam 8.75 ms |
| 6 | **100 Chunks Parallel Meshing** | **42.96 ms** | Mengolah 100 chunk serentak (2.88 juta vertex) via Rayon |
| 7 | **1,000 Chunks Synthetic Meshing** | **355.82 ms** | Mengolah 1,000 chunk (6.48 juta quad) via Rayon |
| 8 | **Chunk Zstd Compression** | **4.97 ms** | 131,072 bytes $\to$ 1,863 bytes (**70.4x rasio kompresi**) |
| 9 | **Chunk Zstd Decompression** | **6.95 ms** | Rekonstruksi chunk 32k voxel sempurna |
| 10 | **Connectivity BFS Traversal** | **4.05 ms** | Penelusuran 14,759 voxel klaster struktural terhubung |

---

## 🚀 Menjalankan Engine

### Prasyarat
* [Rust toolchain](https://rustup.rs/) (versi 1.80+ direkomendasikan)
* GPU dengan dukungan Metal (macOS), Vulkan (Linux/Windows), atau DirectX 12.

### 1. Menjalankan Demo World Interaktif
```bash
cargo run --release
```

**Kontrol Kamera:**
* `W`, `A`, `S`, `D`: Gerak horizontal (Fly / FPS mode)
* `Space`: Terbang naik (+Y)
* `Left Shift`: Terbang turun (-Y)
* **Klik Kanan / Kiri + Drag Mouse**: Rotasi orientasi pandangan (Yaw & Pitch)

### 2. Menjalankan Test Suite
```bash
cargo test
```

### 3. Menjalankan Benchmark Suite
```bash
cargo run --release --bin benchmarks
```

---

## 📂 Struktur Modul

```text
src/
├── lib.rs                      # Root library re-exports
├── main.rs                     # Winit 0.30 interactive app & 60 FPS loop
├── material.rs                 # MaterialId & MaterialRegistry data-driven
├── voxel.rs                    # VoxelBlock (4 bytes invariant) & structural flags
├── coord.rs                    # Canonical indexing & Euclidean negative math
├── chunk.rs                    # Authoritative Chunk 32³ (128 KiB flat array)
├── camera.rs                   # FPS/Orbital 3D camera & ViewProj uniform
├── renderer.rs                 # wgpu Metal pipeline, depth buffer, mesh cache
├── shader.wgsl                 # Half-Lambert + Pastel palette + Baked AO shader
├── storage.rs                  # RegionStore abstraction & Zstd compression
├── world.rs                    # World sparse runtime & Demo World generator
├── bin/
│   └── benchmarks.rs           # 11 Benchmark suite
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
