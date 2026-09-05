use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Vec3};

/// Format vertex GPU untuk rendering micro-voxel
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct VoxelVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
    pub ao: f32, // Nilai Ambient Occlusion [0.0..1.0]
}

impl VoxelVertex {
    pub const fn new(position: [f32; 3], normal: [f32; 3], color: [f32; 3], ao: f32) -> Self {
        Self {
            position,
            normal,
            color,
            ao,
        }
    }

    /// Layout deskriptor buffer untuk pipeline wgpu
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
            0 => Float32x3, // position
            1 => Float32x3, // normal
            2 => Float32x3, // color
            3 => Float32,   // ao
        ];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<VoxelVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBS,
        }
    }
}

/// Struktur data Mesh CPU murni, independen dari resource GPU wgpu.
///
/// INVARIANT 6: CPU Mesh terpisah dari GPU Buffer cache.
#[derive(Default, Debug, Clone)]
pub struct MeshData {
    pub vertices: Vec<VoxelVertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(vert_cap: usize, idx_cap: usize) -> Self {
        Self {
            vertices: Vec::with_capacity(vert_cap),
            indices: Vec::with_capacity(idx_cap),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty() || self.vertices.is_empty()
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    pub fn index_count(&self) -> usize {
        self.indices.len()
    }

    pub fn quad_count(&self) -> usize {
        self.indices.len() / 6
    }
}

/// 6 Arah sisi kubus voxel
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceDirection {
    PosX, // Right  (+X)
    NegX, // Left   (-X)
    PosY, // Top    (+Y)
    NegY, // Bottom (-Y)
    PosZ, // Front  (+Z)
    NegZ, // Back   (-Z)
}

impl FaceDirection {
    pub const ALL: [FaceDirection; 6] = [
        FaceDirection::PosX,
        FaceDirection::NegX,
        FaceDirection::PosY,
        FaceDirection::NegY,
        FaceDirection::PosZ,
        FaceDirection::NegZ,
    ];

    #[inline(always)]
    pub fn normal(&self) -> [f32; 3] {
        match self {
            FaceDirection::PosX => [1.0, 0.0, 0.0],
            FaceDirection::NegX => [-1.0, 0.0, 0.0],
            FaceDirection::PosY => [0.0, 1.0, 0.0],
            FaceDirection::NegY => [0.0, -1.0, 0.0],
            FaceDirection::PosZ => [0.0, 0.0, 1.0],
            FaceDirection::NegZ => [0.0, 0.0, -1.0],
        }
    }

    #[inline(always)]
    pub fn offset(&self) -> (i32, i32, i32) {
        match self {
            FaceDirection::PosX => (1, 0, 0),
            FaceDirection::NegX => (-1, 0, 0),
            FaceDirection::PosY => (0, 1, 0),
            FaceDirection::NegY => (0, -1, 0),
            FaceDirection::PosZ => (0, 0, 1),
            FaceDirection::NegZ => (0, 0, -1),
        }
    }

    #[inline(always)]
    pub fn normal_vec3(&self) -> Vec3 {
        Vec3::from_array(self.normal())
    }

    #[inline(always)]
    pub fn normal_ivec3(&self) -> IVec3 {
        let (x, y, z) = self.offset();
        IVec3::new(x, y, z)
    }
}
