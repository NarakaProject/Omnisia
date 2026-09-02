use glam::IVec3;

use crate::chunk::{dirty_flags, Chunk};
use crate::coord::CHUNK_SIZE_USIZE;
use crate::material::{MaterialId, MaterialRegistry};
use crate::modding::resource_id::ResourceId;
use crate::voxel::VoxelBlock;

use super::caves::CaveSampler;
use super::features::{FormationSampler, OreSampler, OverhangSampler, UndergroundStrata};
use super::terrain::TerrainProfiler;

/// Struktur cache ID material runtime untuk proses voxelization cepat tanpa alokasi / string parsing
#[derive(Debug, Clone, Copy)]
pub struct ResolvedGenMaterials {
    pub stone: MaterialId,
    pub dirt: MaterialId,
    pub grass: MaterialId,
    pub sand: MaterialId,
    pub water: MaterialId,
    pub snow: MaterialId,
    pub deepslate: MaterialId,
    pub coal_ore: MaterialId,
    pub iron_ore: MaterialId,
    pub gold_ore: MaterialId,
    pub crystal: MaterialId,
}

impl ResolvedGenMaterials {
    /// Resolusi material wajib dari MaterialRegistry aktif.
    /// Gagal secara eksplisit jika ada ResourceId inti yang tidak terdaftar (NO SILENT FALLBACK).
    pub fn resolve(registry: &MaterialRegistry) -> Result<Self, String> {
        let resolve_req = |name: &str| -> Result<MaterialId, String> {
            let res = ResourceId::core(name)
                .map_err(|e| format!("Invalid resource id core:{}: {}", name, e))?;
            registry.resolve_material_id(&res).ok_or_else(|| {
                format!(
                    "Material wajib generasi '{}' tidak ditemukan dalam MaterialRegistry",
                    res
                )
            })
        };

        Ok(Self {
            stone: resolve_req("stone")?,
            dirt: resolve_req("dirt")?,
            grass: resolve_req("grass")?,
            sand: resolve_req("sand")?,
            water: resolve_req("water")?,
            snow: resolve_req("snow")?,
            deepslate: resolve_req("deepslate")?,
            coal_ore: resolve_req("coal_ore")?,
            iron_ore: resolve_req("iron_ore")?,
            gold_ore: resolve_req("gold_ore")?,
            crystal: resolve_req("crystal")?,
        })
    }
}

/// Voxelizer yang mengubah profil medan kontinu dan medan fitur 3D menjadi representasi 32³ micro-voxels
pub struct ChunkVoxelizer;

impl ChunkVoxelizer {
    pub fn voxelize(
        chunk_coord: IVec3,
        profiler: &TerrainProfiler,
        caves: &CaveSampler,
        overhangs: &OverhangSampler,
        ores: &OreSampler,
        formations: &FormationSampler,
        materials: &ResolvedGenMaterials,
    ) -> Chunk {
        let mut chunk = Chunk::new(chunk_coord);
        let mut non_air = 0u16;

        let base_world_x = chunk_coord.x * 32;
        let base_world_y = chunk_coord.y * 32;
        let base_world_z = chunk_coord.z * 32;

        // 1. Evaluasi 2D kolom terrain (32x32 titik)
        let mut column_points = [[None; CHUNK_SIZE_USIZE]; CHUNK_SIZE_USIZE];
        let mut max_surface_y = f32::MIN;

        for (lz, row) in column_points.iter_mut().enumerate() {
            let wz = (base_world_z + lz as i32) as f32;
            for (lx, cell) in row.iter_mut().enumerate() {
                let wx = (base_world_x + lx as i32) as f32;
                let pt = profiler.evaluate(wx, wz);
                if pt.surface_height_y > max_surface_y {
                    max_surface_y = pt.surface_height_y;
                }
                *cell = Some(pt);
            }
        }

        // Fast bounding box rejection: Jika chunk berada jauh di atas ketinggian maksimum terrain + overhang
        if (base_world_y as f32) > max_surface_y + 18.0 {
            return chunk; // 100% Air
        }

        // 2. Voxelization volumetrik 3D ke volume 32x32x32
        for (lz, row) in column_points.iter().enumerate() {
            let world_z = base_world_z + lz as i32;
            for (lx, &cell) in row.iter().enumerate() {
                let world_x = base_world_x + lx as i32;
                let pt = cell.unwrap();
                let surface_floor_y = pt.surface_height_y.floor() as i32;
                let water_floor_y = pt.water_level_y.floor() as i32;

                for ly in 0..CHUNK_SIZE_USIZE {
                    let world_y = base_world_y + ly as i32;
                    let wx = world_x as f32;
                    let wy = world_y as f32;
                    let wz = world_z as f32;

                    // A. Evaluasi Densitas 3D Medan & Overhang
                    let overhang_density =
                        overhangs.sample_density(wx, wy, wz, pt.surface_height_y, pt.biome);
                    let terrain_density = (pt.surface_height_y - wy) + overhang_density;
                    let mut is_solid = terrain_density >= 0.0;

                    let mut formation_mat = None;

                    // B. Formasi Batuan Alami Menonjol di Permukaan
                    if !is_solid && wy > pt.surface_height_y {
                        if let Some(f_mat) = formations.sample_surface_formation(
                            world_x,
                            world_y,
                            world_z,
                            surface_floor_y,
                            pt.biome,
                            materials,
                        ) {
                            is_solid = true;
                            formation_mat = Some(f_mat);
                        }
                    }

                    // C. Pengukiran Gua 3D (Carving to Air)
                    if is_solid
                        && formation_mat.is_none()
                        && caves.is_cave(wx, wy, wz, pt.surface_height_y)
                    {
                        is_solid = false;
                    }

                    // D. Penentuan Material Voxel
                    let mat = if is_solid {
                        if let Some(f_mat) = formation_mat {
                            f_mat
                        } else {
                            // Stratifikasi Bawah Tanah
                            let base_mat = UndergroundStrata::determine_base_material(
                                world_y,
                                surface_floor_y,
                                pt.biome,
                                materials,
                            );

                            // Distribusi Urat & Kantong Bijih Mineral (Ore Replacement pada Batu/Deepslate)
                            if (base_mat == materials.stone || base_mat == materials.deepslate)
                                && world_y < surface_floor_y - 2
                            {
                                if let Some(ore_mat) =
                                    ores.sample_ore(world_x, world_y, world_z, materials)
                                {
                                    ore_mat
                                } else {
                                    base_mat
                                }
                            } else {
                                base_mat
                            }
                        }
                    } else if world_y <= water_floor_y {
                        // Lapisan Air Cair
                        materials.water
                    } else {
                        // Udara Bebas
                        MaterialId::AIR
                    };

                    if mat != MaterialId::AIR {
                        chunk.set_voxel(lx, ly, lz, VoxelBlock::new(mat));
                        non_air += 1;
                    }
                }
            }
        }

        chunk.non_air_count = non_air;
        chunk.dirty_flags = dirty_flags::ALL;
        chunk
    }
}
