use glam::IVec3;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

use crate::chunk::Chunk;
use crate::coord::CHUNK_VOLUME;
use crate::voxel::VoxelBlock;

/// Format biner serialisasi chunk terkompresi
#[derive(Serialize, Deserialize)]
pub struct ChunkSerializedPayload {
    pub position: [i32; 3],
    pub non_air_count: u16,
    pub voxels: Vec<VoxelBlock>,
}

/// Serialisasi chunk ke byte stream terkompresi Zstandard
pub fn serialize_and_compress_chunk(chunk: &Chunk) -> Result<Vec<u8>, std::io::Error> {
    let payload = ChunkSerializedPayload {
        position: [chunk.position.x, chunk.position.y, chunk.position.z],
        non_air_count: chunk.non_air_count,
        voxels: chunk.voxels.to_vec(),
    };

    let raw_bytes = serde_json::to_vec(&payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    // Kompresi Zstd level 3 (keseimbangan kecepatan kompresi & rasio ukuran)
    let mut compressed_bytes = Vec::new();
    let mut encoder = zstd::stream::Encoder::new(&mut compressed_bytes, 3)?;
    encoder.write_all(&raw_bytes)?;
    encoder.finish()?;

    Ok(compressed_bytes)
}

/// Dekompresi Zstandard dan deserialisasi kembali ke `Chunk`
pub fn decompress_and_deserialize_chunk(compressed_data: &[u8]) -> Result<Chunk, std::io::Error> {
    let mut decoder = zstd::stream::Decoder::new(compressed_data)?;
    let mut decompressed_bytes = Vec::new();
    decoder.read_to_end(&mut decompressed_bytes)?;

    let payload: ChunkSerializedPayload = serde_json::from_slice(&decompressed_bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    if payload.voxels.len() != CHUNK_VOLUME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Ukuran voxel payload tidak valid: {}", payload.voxels.len()),
        ));
    }

    let voxels_box: Box<[VoxelBlock; CHUNK_VOLUME]> =
        payload.voxels.into_boxed_slice().try_into().map_err(|_| {
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
    })
}

/// Trait abstraksi RegionStore untuk mendukung swap backend persistensi
pub trait RegionStore {
    fn save_chunk(&mut self, chunk: &Chunk) -> Result<(), std::io::Error>;
    fn load_chunk(&mut self, position: IVec3) -> Result<Option<Chunk>, std::io::Error>;
    fn has_chunk(&self, position: IVec3) -> bool;
}

/// Implementasi in-memory RegionStore terkompresi Zstd
pub struct MemoryCompressedRegionStore {
    storage: std::collections::HashMap<IVec3, Vec<u8>>,
}

impl Default for MemoryCompressedRegionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryCompressedRegionStore {
    pub fn new() -> Self {
        Self {
            storage: std::collections::HashMap::new(),
        }
    }

    pub fn total_compressed_bytes(&self) -> usize {
        self.storage.values().map(|v| v.len()).sum()
    }
}

impl RegionStore for MemoryCompressedRegionStore {
    fn save_chunk(&mut self, chunk: &Chunk) -> Result<(), std::io::Error> {
        let compressed = serialize_and_compress_chunk(chunk)?;
        self.storage.insert(chunk.position, compressed);
        Ok(())
    }

    fn load_chunk(&mut self, position: IVec3) -> Result<Option<Chunk>, std::io::Error> {
        if let Some(compressed) = self.storage.get(&position) {
            let chunk = decompress_and_deserialize_chunk(compressed)?;
            Ok(Some(chunk))
        } else {
            Ok(None)
        }
    }

    fn has_chunk(&self, position: IVec3) -> bool {
        self.storage.contains_key(&position)
    }
}
