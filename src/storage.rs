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

/// Format biner serialisasi chunk menggunakan Local Palette Table berbasis stable ResourceId.
///
/// INVARIANT: Runtime MaterialId TIDAK BOLEH menjadi persistent identity.
/// Save file menyimpan ResourceId stabil sehingga kompatibel terhadap perubahan mod load order.
#[derive(Serialize, Deserialize)]
pub struct ChunkSerializedPayload {
    pub position: [i32; 3],
    pub non_air_count: u16,
    pub revision: u64,
    pub palette: Vec<ResourceId>,
    pub palette_indices: Vec<u16>,
}

/// Serialisasi chunk ke byte stream terkompresi Zstandard menggunakan local ResourceId palette
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
                .unwrap_or_else(|| ResourceId::core("air").unwrap());
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

/// Dekompresi Zstandard dan deserialisasi kembali ke `Chunk` dengan resolusi ResourceId $\to$ runtime MaterialId
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
    let resolved_palette: Vec<MaterialId> = payload
        .palette
        .iter()
        .map(|res_id| {
            registry
                .resolve_material_id(res_id)
                .unwrap_or(MaterialId::AIR) // Fallback aman jika mod telah di-uninstall
        })
        .collect();

    let mut voxels_raw = Vec::with_capacity(CHUNK_VOLUME);
    for &palette_idx in &payload.palette_indices {
        let mat_id = resolved_palette
            .get(palette_idx as usize)
            .copied()
            .unwrap_or(MaterialId::AIR);
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
        dirty_flags: crate::chunk::dirty_flags::ALL,
        revision: payload.revision,
    })
}

/// Trait abstraksi RegionStore untuk mendukung swap backend persistensi secara thread-safe
pub trait RegionStore: Send + Sync {
    fn save_chunk(&self, chunk: &Chunk, registry: &MaterialRegistry) -> Result<(), std::io::Error>;
    fn load_chunk(
        &self,
        position: IVec3,
        registry: &MaterialRegistry,
    ) -> Result<Option<Chunk>, std::io::Error>;
    fn has_chunk(&self, position: IVec3) -> bool;
}

/// Implementasi in-memory RegionStore terkompresi Zstd thread-safe
pub struct MemoryCompressedRegionStore {
    storage: RwLock<HashMap<IVec3, Vec<u8>>>,
}

impl Default for MemoryCompressedRegionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryCompressedRegionStore {
    pub fn new() -> Self {
        Self {
            storage: RwLock::new(HashMap::new()),
        }
    }

    pub fn total_compressed_bytes(&self) -> usize {
        let guard = self.storage.read().unwrap();
        guard.values().map(|v| v.len()).sum()
    }
}

impl RegionStore for MemoryCompressedRegionStore {
    fn save_chunk(&self, chunk: &Chunk, registry: &MaterialRegistry) -> Result<(), std::io::Error> {
        let compressed = serialize_and_compress_chunk(chunk, registry)?;
        let mut guard = self.storage.write().unwrap();
        guard.insert(chunk.position, compressed);
        Ok(())
    }

    fn load_chunk(
        &self,
        position: IVec3,
        registry: &MaterialRegistry,
    ) -> Result<Option<Chunk>, std::io::Error> {
        let guard = self.storage.read().unwrap();
        if let Some(compressed) = guard.get(&position) {
            let chunk = decompress_and_deserialize_chunk(compressed, registry)?;
            Ok(Some(chunk))
        } else {
            Ok(None)
        }
    }

    fn has_chunk(&self, position: IVec3) -> bool {
        let guard = self.storage.read().unwrap();
        guard.contains_key(&position)
    }
}

/// Implementasi FileRegionStore untuk persistensi filesystem berbasis direktori terkompresi
pub struct FileRegionStore {
    root_dir: PathBuf,
}

impl FileRegionStore {
    pub fn new<P: AsRef<Path>>(root_dir: P) -> Result<Self, std::io::Error> {
        let path = root_dir.as_ref().to_path_buf();
        fs::create_dir_all(&path)?;
        Ok(Self { root_dir: path })
    }

    fn chunk_path(&self, pos: IVec3) -> PathBuf {
        self.root_dir
            .join(format!("chunk_{}_{}_{}.omc", pos.x, pos.y, pos.z))
    }
}

impl RegionStore for FileRegionStore {
    fn save_chunk(&self, chunk: &Chunk, registry: &MaterialRegistry) -> Result<(), std::io::Error> {
        let compressed = serialize_and_compress_chunk(chunk, registry)?;
        let target_path = self.chunk_path(chunk.position);
        let temp_path = target_path.with_extension("tmp");

        // Penulisan atomik: tulis ke .tmp terlebih dahulu kemudian rename
        let mut file = File::create(&temp_path)?;
        file.write_all(&compressed)?;
        file.sync_all()?;
        fs::rename(&temp_path, &target_path)?;

        Ok(())
    }

    fn load_chunk(
        &self,
        position: IVec3,
        registry: &MaterialRegistry,
    ) -> Result<Option<Chunk>, std::io::Error> {
        let target_path = self.chunk_path(position);
        if !target_path.exists() {
            return Ok(None);
        }

        let mut file = File::open(&target_path)?;
        let mut compressed_data = Vec::new();
        file.read_to_end(&mut compressed_data)?;

        let chunk = decompress_and_deserialize_chunk(&compressed_data, registry)?;
        Ok(Some(chunk))
    }

    fn has_chunk(&self, position: IVec3) -> bool {
        self.chunk_path(position).exists()
    }
}
