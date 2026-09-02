# LAPORAN TEKNIS PHASE 6 — VEGETATION, NATURAL STRUCTURES & PERFORMANCE STABILIZATION GATE

**Omnisia Voxel Engine Architecture**  
**Repository**: `NarakaProject/Omnisia`  
**Target Platform**: MacBook Pro 2018 (Intel Core i7 x86_64, macOS Metal Backend)  
**Status**: **SELESAI (100% Passed & Terverifikasi)**  

---

## 1. EKSEKUTIF SUMMARY & TUJUAN ARSITEKTURAL

Phase 6 dibagi menjadi dua gerbang (*gates*) berurutan yang sangat ketat:
1. **PART A — Performance Stabilization Gate**: Menyelesaikan regresi penurunan frame-rate (FPS drop) saat kamera bergerak menjauh dari area origin/spawn, memisahkan *residency* dari *rendering*, mengimplementasikan *chunk-level frustum culling* nol-alokasi berbasis standar Metal NDC, menegakkan disiplin upload GPU (*upload queue rate-limiting*), dan mengintegrasikan *Greedy Meshing* ke pipeline background worker production.
2. **PART B — Vegetation & Natural Structures**: Mengintegrasikan sistem vegetasi kanonikal prosedural (*Oak Trees*, *Pine Trees*, *Desert Shrubs*, *Tall Grass*) dengan kepemilikan *world-space* deterministik (*canonical anchor ownership*), *multi-chunk footprint stamping* yang independen dari urutan pemuatan chunk ($A \to B == B \to A$), serta penegakan ketat kebijakan *replaceable voxel*.

---

## 2. PART A: PERFORMANCE STABILIZATION GATE

### 2.1. Model Rendering 4-Status (4-State Rendering Model)
Untuk mencegah akumulasi tak berbatas dan beban render yang membesar seiring perpindahan kamera, pipeline rendering dibagi secara ortogonal:

```text
CPU Resident (ChunkStore, retain_radius = 6)
    ↓
GPU Mesh Resident (Renderer::chunk_meshes)
    ↓
Render-Distance Eligible (|dx| <= 4, |dz| <= 4, |dy| <= 2)
    ↓
Frustum Visible (Intersects 6 Camera Planes)
    ↓
Draw Submission (Index Buffer Draw Calls)
```

* **CPU Resident**: Chunk yang berada dalam memori CPU (`ChunkStore`).
* **GPU Mesh Resident**: VBO/IBO yang telah di-upload ke GPU VRAM. Disinkronkan dengan `renderer.retain_only(&active_set)` sehingga saat chunk diev適 dari CPU, GPU mesh langsung dibersihkan.
* **Render-Distance Eligible**: Filter jarak horizontal dan vertikal terhadap posisi kamera chunk.
* **Frustum Visible**: Chunk AABB berpotongan dengan 6 bidang frustum kamera.
* **Draw Submission**: Aksi submit perintah render ke GPU (bukan state persisten).

### 2.2. Frustum Culling Nol-Alokasi (Zero Heap Allocation)
* Menggunakan konvensi wgpu / Metal NDC depth $[0, 1]$:
  * **Left**: $r_3 + r_0$
  * **Right**: $r_3 - r_0$
  * **Bottom**: $r_3 + r_1$
  * **Top**: $r_3 - r_1$
  * **Near**: $r_2$
  * **Far**: $r_3 - r_2$
* Normalisasi bidang $\frac{1}{\sqrt{a^2 + b^2 + c^2}}$ untuk pengujian jarak metrik yang eksak.
* Pengujian AABB menggunakan $p$-vertex testing ($O(1)$, 0 alokasi heap): throughput tercatat **4.86 ns per chunk** (> 200 juta tes per detik).
* Perhitungan batas dunia chunk $16.0\text{m}$ presisi mendukung kuadran koordinat positif maupun negatif ($cx = -1, -2$).

### 2.3. GPU Upload Discipline & Greedy Meshing Production
* **Upload Queue Rate-Limiting**: Upload mesh ke GPU dibatasi maksimum 32 chunk per frame dan diurutkan berdasarkan jarak terdekat ke kamera (`sort_by_key`). Menghilangkan transient micro-stutter saat traversal cepat.
* **Production Greedy Meshing**: Worker background pool (`crossbeam_channel`) menghasilkan mesh optimal dengan reduksi quad **29.13x lipat** dibandingkan culled meshing dasar, secara drastis memotong konsumsi index buffer GPU dan draw call overhead.

---

## 3. PART B: VEGETATION & NATURAL STRUCTURES

### 3.1. Definisi Satuan Koordinat Eksak
* **Ukuran Voxel**: $0.5\text{m} \times 0.5\text{m} \times 0.5\text{m}$
* **Ukuran Chunk**: $32 \times 32 \times 32$ voxel = $16.0\text{m} \times 16.0\text{m} \times 16.0\text{m}$
* **Konversi Koordinat**:
  $$\text{Voxel } vx \implies \text{Chunk } cx = vx.\text{div\_euclid}(32), \quad \text{Lokal } lx = vx.\text{rem\_euclid}(32)$$
* **Batas Negatif Eksak**:
  * $wx = -1 \implies cx = -1, lx = 31$
  * $wx = -32 \implies cx = -1, lx = 0$
  * $wx = -33 \implies cx = -2, lx = 31$

### 3.2. Kepemilikan Kanonikal & Multi-Chunk Stamping ($A \to B == B \to A$)
1. **Grid Sel Anchor**: Dunia dibagi menjadi sel kanonikal $8 \times 8$ voxel.
2. **Anchor Deterministic**: Setiap sel $(cell\_x, cell\_z)$ menghitung posisi anchor $(ax, az)$ via `hash3d(cell_x, 0, cell_z, seed)`.
3. **Evaluasi Profil Medan**: Menghitung elevasi permukaan $ay = \lfloor \text{surface\_height\_y}(ax, az) \rfloor$.
4. **Validasi Lingkungan**:
   * $ay \le \text{sea\_level} \implies$ **Ditolak** (Mencegah pohon darat tumbuh di laut/danau/sungai).
   * $\text{caves.is\_cave}(ax, ay, az, surface\_y) == \text{true} \implies$ **Ditolak** (Mencegah pohon melayang di dalam rongga gua).
   * Anchor harus tanah solid (Grass, Dirt, Sand, Snow).
5. **Multi-Chunk Stamping**:
   * Untuk chunk target $C$, dilakukan pemindaian seluruh sel kandidat dengan radius batas $R_{\text{max}} = 4$ voxel ($[C.x \cdot 32 - 4 \dots (C.x+1) \cdot 32 + 3]$).
   * Hanya voxel pohon yang jatuh di dalam rentang koordinat lokal $C$ yang diaplikasikan ke `chunk.set_voxel(lx, ly, lz)`.
   * **Hasil**: Chunk $A$ dan chunk $B$ di sebelahnya yang terpotong pohon yang sama akan menghasilkan blok identik tanpa perlu saling memuat atau membaca tetangga.

### 3.3. Spesies Vegetasi & Ekologi Biome
* **Oak Trees** (`core:wood_oak`, `core:leaves_oak`): Batang silinder tinggi 4–6 voxel, mahkota daun sferis radius 2 voxel. Dominan di biome `Forest` (65%) dan `Plains` (15%).
* **Pine Trees** (`core:wood_pine`, `core:leaves_pine`): Batang tinggi 6–9 voxel, mahkota daun kerucut bertingkat (*conical stepped*). Dominan di `Hills` (30%), `Mountains` (35%), dan `SnowPeaks` (20%).
* **Desert Shrubs** (`core:shrub`): Semak belukar 1 blok di atas pasir gurun (`Desert` 25%).
* **Tall Grass** (`core:tall_grass`): Rumput tinggi 1 blok di atas padang rumput (`Forest` 20%, `Plains` 45%, `Hills` 20%).

### 3.4. Kebijakan Replaceable Voxel & Persistence Precedence
* Vegetasi hanya boleh menggantikan `MaterialId::AIR`, daun, atau tanaman kecil.
* **Dilarang keras** menimpa: `stone`, `deepslate`, `coal_ore`, `iron_ore`, `gold_ore`, `crystal`, atau `water`.
* **Persistence Precedence**: Mutasi voxel oleh pemain (`SAVE_DIRTY` yang tersimpan di `RegionStore`) selalu menang atas status prosedural awal vegetasi.

---

## 4. MATRIKS REGRESI & BENCHMARK PERFORMANCE

Diuji pada: **MacBook Pro 2018 (Intel Core i7 x86_64, macOS Metal Backend)** dalam mode `release`.

| Komponen Pengujian | Phase 5 Baseline | Phase 6 Aktual | Status / Catatan |
| :--- | :--- | :--- | :--- |
| **Chunk Indexing** | 0.25 ns/op | **0.26 ns/op** | $O(1)$ zero overhead |
| **Chunk Fill (32k voxels)** | 3.52 µs/chunk | **3.42 µs/chunk** | Flat 128 KiB |
| **Culled Meshing 32³** | 0.312 ms/chunk | **0.320 ms/chunk** | 4,224 Quads |
| **Greedy Meshing 32³** | 0.635 ms/chunk | **0.611 ms/chunk** | **145 Quads (29.13x Reduksi)** |
| **AO Calculation** | 15.98 ns/face | **14.28 ns/face** | 3-way neighbor lookup |
| **Zstd Palette Compression** | 1.708 ms (100.3x) | **1.465 ms (100.3x)** | 131 KiB $\to$ 1,307 bytes |
| **Zstd Palette Decompression** | 741.4 µs | **688.6 µs** | 100% Bit-exact |
| **3D fBm Noise (1M samples)** | 131.05 ns/sample | **133.63 ns/sample** | 3 octaves |
| **3D Cave Sampling** | 227.20 ns/point | **254.87 ns/point** | Double 3D ridged |
| **Procedural Chunk Gen** | 6.756 ms/chunk | **7.141 ms/chunk** | 3D Features + Vegetasi |
| **100 Chunks Parallel Gen (Rayon)** | 155.78 ms | **95.24 ms** | **~0.95 ms/chunk amortized** |
| **Frustum Culling Intersection** | *N/A (Baru)* | **4.86 ns/chunk** | **> 200M tests/sec, 0 alokasi** |

---

## 5. VERIFIKASI TEST SUITE

Seluruh 61 unit test lulus tanpa kegagalan:

1. **`tests/engine_tests.rs` (13/13 Passed)**:
   * `test_invariant_voxel_block_size` (4 bytes Pod/Zeroable)
   * `test_invariant_chunk_size_and_volume` (32³, 128 KiB flat)
   * `test_canonical_indexing_roundtrip`
   * `test_negative_coordinates_correctness`
   * `test_chunk_mutation_and_non_air_count`
   * `test_material_registry`
   * `test_ao_calculation`
   * `test_culled_mesher_single_voxel`
   * `test_greedy_mesher_optimization`
   * `test_greedy_mesher_chunk_boundary_continuity`
   * `test_frustum_extraction_and_culling`
   * `test_frustum_negative_coordinates_and_zero_allocation`
   * `test_zstd_chunk_persistence_roundtrip`

2. **`tests/modding_tests.rs` (11/11 Passed)**:
   * `test_asset_id_parsing_and_format`
   * `test_asset_id_path_traversal_rejection`
   * `test_mod_declaring_reserved_core_namespace_rejected`
   * `test_explicit_override_success_and_persistent_identity`
   * `test_asset_resolver_resolution_and_containment`
   * `test_override_conflict_detection`
   * `test_missing_core_directory_fails_explicitly`
   * `test_valid_manifest_parsing_with_overrides`
   * `test_manifest_invalid_override_rules`
   * `test_core_content_loading_from_disk`
   * `test_example_mod_end_to_end_with_override`

3. **`tests/streaming_tests.rs` (11/11 Passed)**:
   * `test_chunk_lifecycle_state_transitions`
   * `test_memory_budget_enforcement`
   * `test_scheduler_deterministic_priority_and_tie_break`
   * `test_duplicate_request_coalescing_and_priority_escalation`
   * `test_dirty_chunk_mutation_during_save_race`
   * `test_stale_job_result_rejection`
   * `test_distant_lod_contract_rebuildable`
   * `test_lifecycle_generation_stale_rejection_after_eviction_and_resurrection`
   * `test_missing_resource_id_in_palette_fails_explicitly`
   * `test_save_load_stable_resource_id_palette_roundtrip`
   * `test_negative_coordinates_streaming_and_storage`

4. **`tests/worldgen_tests.rs` (26/26 Passed)**:
   * `test_seed_determinism`
   * `test_different_seeds_produce_different_terrain`
   * `test_chunk_loading_order_independence`
   * `test_border_continuity_across_chunks`
   * `test_negative_coordinates_worldgen_continuity`
   * `test_negative_chunk_y_deep_subsurface`
   * `test_sea_level_consistency_in_world_coordinates`
   * `test_biome_classification_determinism`
   * `test_river_continuity_across_boundaries`
   * `test_3d_cave_determinism_and_topology`
   * `test_cave_boundary_continuity_xyz`
   * `test_overhang_topology_non_columnar`
   * `test_underground_layers_stratification`
   * `test_ore_distribution_invariants`
   * `test_natural_formations_voxel_presence`
   * `test_vegetation_determinism`
   * `test_vegetation_loading_order_independence`
   * `test_vegetation_chunk_boundary_crossing`
   * `test_vegetation_biome_compatibility`
   * `test_vegetation_negative_coordinates_boundary_math`
   * `test_vegetation_replaceable_voxel_policy`
   * `test_persistence_precedence_and_mutation_preservation`
   * `test_generator_version_identity`
   * `test_generator_does_not_depend_on_neighbor_residency`
   * `test_deterministic_golden_snapshot`
   * `test_missing_generation_material_fails_explicitly`

---

## 6. KESIMPULAN & INVARIAN ENGINE

Phase 6 berhasil dieksekusi dengan kepatuhan penuh terhadap prinsip **NO REGRESSION**:
1. Seluruh 61 unit test lulus 100%.
2. Kode 100% terformat rapi sesuai standar `rustfmt` dan 0 warning `cargo clippy`.
3. Validasi runtime content & mod via `cargo run --release -- --validate-mods` mendeteksi 27 core materials dan 22 core blocks.
4. Engine siap melangkah ke fase berikutnya dengan performa stabil, tanpa kebocoran mesh GPU, dan dunia prosedural yang hidup dengan vegetasi alami.
