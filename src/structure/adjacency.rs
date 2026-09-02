use glam::IVec3;

/// 6 arah ketetanggaan ortogonal (face-connected adjacency): ±X, ±Y, ±Z.
///
/// INVARIANT: Konektivitas struktural Omnisia mengadopsi 6-connectivity murni.
/// Dua voxel hanya terhubung secara struktural jika saling bersentuhan pada sisi muka kubus.
/// Sentuhan pada rusuk (edge) atau sudut (corner) secara diagonal TIDAK dianggap tersambung.
pub const ADJACENCY_OFFSETS_6: [IVec3; 6] = [
    IVec3::new(1, 0, 0),  // +X (East)
    IVec3::new(-1, 0, 0), // -X (West)
    IVec3::new(0, 1, 0),  // +Y (Up)
    IVec3::new(0, -1, 0), // -Y (Down)
    IVec3::new(0, 0, 1),  // +Z (South)
    IVec3::new(0, 0, -1), // -Z (North)
];

/// Mengecek apakah dua koordinat voxel dunia bersentuhan langsung secara 6-connected face adjacency
#[inline(always)]
pub fn is_face_adjacent(a: IVec3, b: IVec3) -> bool {
    let diff = (a - b).abs();
    (diff.x + diff.y + diff.z == 1) && diff.max_element() == 1
}
