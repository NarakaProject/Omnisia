use glam::IVec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::chunk::Chunk;
use crate::coord::CHUNK_VOLUME;
use crate::material::{MaterialId, MaterialRegistry};
use crate::modding::resource_id::ResourceId;
use crate::voxel::VoxelBlock;

/// Format data chunk terkompresi yang disimpan ke disk menggunakan Local Palette Table
#[derive(Serialize, Deserialize)]
pub struct ChunkSerializedPayload {
    pub position: [i32; 3],
    pub non_air_count: u16,
    pub revision: u64,
    pub palette: Vec<ResourceId>,
    pub palette_indices: Vec<u16>,
}

/// Serialisasi `Chunk` ke format byte stream terkompresi Zstandard menggunakan Local Palette Table berbasis `ResourceId`
pub fn serialize_and_compress_chunk(
    chunk: &Chunk,
    registry: &MaterialRegistry,
) -> Result<Vec<u8>, std::io::Error> {
    let mut palette: Vec<ResourceId> = Vec::new();
    let mut palette_map: HashMap<MaterialId, u16> = HashMap::new();
    let mut palette_indices = Vec::with_capacity(CHUNK_VOLUME);

    // Pastikan core:air berada di indeks palet 0 jika mungkin
    let air_res_id =
        ResourceId::core("air").unwrap_or_else(|_| ResourceId::parse("core:air").unwrap());
    palette.push(air_res_id);
    palette_map.insert(MaterialId::AIR, 0);

    for block in chunk.voxels.iter() {
        let mat_id = block.material();
        let idx = if let Some(&p_idx) = palette_map.get(&mat_id) {
            p_idx
        } else {
            let res_id = registry
                .resolve_resource_id(mat_id)
                .cloned()
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "Gagal menyelesaikan ResourceId untuk MaterialId {:?}",
                            mat_id
                        ),
                    )
                })?;
            let new_idx = palette.len() as u16;
            palette.push(res_id);
            palette_map.insert(mat_id, new_idx);
            new_idx
        };
        palette_indices.push(idx);
    }

    let payload = ChunkSerializedPayload {
        position: [chunk.position.x, chunk.position.y, chunk.position.z],
        non_air_count: chunk.non_air_count,
        revision: chunk.revision,
        palette,
        palette_indices,
    };

    let raw_bytes = serde_json::to_vec(&payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    // Kompresi Zstd level 3 (kecepatan & rasio optimal untuk streaming real-time)
    let mut compressed_bytes = Vec::new();
    let mut encoder = zstd::stream::Encoder::new(&mut compressed_bytes, 3)?;
    encoder.write_all(&raw_bytes)?;
    encoder.finish()?;

    Ok(compressed_bytes)
}

/// Dekompresi Zstandard dan deserialisasi kembali ke `Chunk` dengan resolusi ResourceId $\to$ runtime MaterialId.
///
/// INVARIANT: Menolak silent fallback ke Air jika ada ResourceId yang tidak terdaftar di MaterialRegistry.
pub fn decompress_and_deserialize_chunk(
    compressed_data: &[u8],
    registry: &MaterialRegistry,
) -> Result<Chunk, std::io::Error> {
    let mut decoder = zstd::stream::Decoder::new(compressed_data)?;
    let mut decompressed_bytes = Vec::new();
    decoder.read_to_end(&mut decompressed_bytes)?;

    let payload: ChunkSerializedPayload = serde_json::from_slice(&decompressed_bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    if payload.palette_indices.len() != CHUNK_VOLUME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "Ukuran voxel payload tidak valid: {}",
                payload.palette_indices.len()
            ),
        ));
    }

    // Resolusi seluruh ResourceId di palette ke MaterialId runtime sesi ini
    let mut resolved_palette = Vec::with_capacity(payload.palette.len());
    for res_id in &payload.palette {
        let mat_id = registry.resolve_material_id(res_id).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "Missing ResourceId dalam persistent chunk data: '{}'. Operasi ditolak untuk mencegah silent data loss.",
                    res_id
                ),
            )
        })?;
        resolved_palette.push(mat_id);
    }

    let mut voxels_raw = Vec::with_capacity(CHUNK_VOLUME);
    for &palette_idx in &payload.palette_indices {
        let mat_id = resolved_palette
            .get(palette_idx as usize)
            .copied()
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Palette index out of bounds: {}", palette_idx),
                )
            })?;
        voxels_raw.push(VoxelBlock::new(mat_id));
    }

    let voxels_box: Box<[VoxelBlock; CHUNK_VOLUME]> =
        voxels_raw.into_boxed_slice().try_into().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Gagal konversi ke Box array",
            )
        })?;

    Ok(Chunk {
        position: IVec3::new(
            payload.position[0],
            payload.position[1],
            payload.position[2],
        ),
        voxels: voxels_box,
        non_air_count: payload.non_air_count,
        dirty_flags: 0,
        revision: payload.revision,
    })
}

/// Trait abstraksi penyimpanan region/disk dunia yang asynchronous-ready dan thread-safe
pub trait RegionStore: Send + Sync {
    fn load_chunk(
        &self,
        coord: IVec3,
        registry: &MaterialRegistry,
    ) -> Result<Option<Chunk>, std::io::Error>;

    fn save_chunk(&self, chunk: &Chunk, registry: &MaterialRegistry) -> Result<(), std::io::Error>;

    fn has_chunk(&self, coord: IVec3) -> bool;

    fn delete_chunk(&self, coord: IVec3) -> Result<(), std::io::Error>;
}

/// Penyimpanan Memory terkompresi Zstd (sangat cepat untuk unit testing dan sandbox transient)
pub struct MemoryCompressedRegionStore {
    data: RwLock<HashMap<IVec3, Vec<u8>>>,
}

impl Default for MemoryCompressedRegionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryCompressedRegionStore {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }
}

impl RegionStore for MemoryCompressedRegionStore {
    fn load_chunk(
        &self,
        coord: IVec3,
        registry: &MaterialRegistry,
    ) -> Result<Option<Chunk>, std::io::Error> {
        let read_guard = self.data.read().unwrap();
        if let Some(bytes) = read_guard.get(&coord) {
            let chunk = decompress_and_deserialize_chunk(bytes, registry)?;
            Ok(Some(chunk))
        } else {
            Ok(None)
        }
    }

    fn save_chunk(&self, chunk: &Chunk, registry: &MaterialRegistry) -> Result<(), std::io::Error> {
        let bytes = serialize_and_compress_chunk(chunk, registry)?;
        let mut write_guard = self.data.write().unwrap();
        write_guard.insert(chunk.position, bytes);
        Ok(())
    }

    fn has_chunk(&self, coord: IVec3) -> bool {
        self.data.read().unwrap().contains_key(&coord)
    }

    fn delete_chunk(&self, coord: IVec3) -> Result<(), std::io::Error> {
        self.data.write().unwrap().remove(&coord);
        Ok(())
    }
}

/// Penyimpanan file berbasis direktori chunk terkompresi Zstd dengan atomic write
pub struct FileRegionStore {
    base_dir: PathBuf,
}

impl FileRegionStore {
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self, std::io::Error> {
        let path = base_dir.as_ref().to_path_buf();
        fs::create_dir_all(&path)?;
        Ok(Self { base_dir: path })
    }

    fn chunk_path(&self, coord: IVec3) -> PathBuf {
        self.base_dir
            .join(format!("c_{}_{}_{}.chk", coord.x, coord.y, coord.z))
    }
}

impl RegionStore for FileRegionStore {
    fn load_chunk(
        &self,
        coord: IVec3,
        registry: &MaterialRegistry,
    ) -> Result<Option<Chunk>, std::io::Error> {
        let path = self.chunk_path(coord);
        if !path.exists() {
            return Ok(None);
        }

        let mut file = File::open(path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let chunk = decompress_and_deserialize_chunk(&buffer, registry)?;
        Ok(Some(chunk))
    }

    fn save_chunk(&self, chunk: &Chunk, registry: &MaterialRegistry) -> Result<(), std::io::Error> {
        let path = self.chunk_path(chunk.position);
        let temp_path = self.base_dir.join(format!(
            "c_{}_{}_{}.tmp",
            chunk.position.x, chunk.position.y, chunk.position.z
        ));

        let compressed = serialize_and_compress_chunk(chunk, registry)?;
        {
            let mut file = File::create(&temp_path)?;
            file.write_all(&compressed)?;
            file.sync_all()?;
        }

        // Atomic rename
        fs::rename(temp_path, path)?;
        Ok(())
    }

    fn has_chunk(&self, coord: IVec3) -> bool {
        self.chunk_path(coord).exists()
    }

    fn delete_chunk(&self, coord: IVec3) -> Result<(), std::io::Error> {
        let path = self.chunk_path(coord);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}
