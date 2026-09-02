use glam::Vec3;

/// Bentuk geometri kapsul kinematik tegak lurus (Upright Capsule Collider).
///
/// KONVENSI KANONIKAL KOORDINAT:
/// - `base`: Titik dasar telapak kaki pemain ($y_{\text{feet}}$).
///   Jika pemain berdiri di atas permukaan balok $y$, maka $y_{\text{feet}} = (y + 1) \times 0.5\text{m}$.
/// - `radius`: Radius bola bawah, silinder, dan bola atas dalam meter.
/// - `height`: Tinggi total kapsul dari telapak kaki ke puncak kepala dalam meter.
/// - Pusat belahan bola bawah: $P_0 = \text{base} + (0, \text{radius}, 0)$.
/// - Pusat belahan bola atas: $P_1 = \text{base} + (0, \text{height} - \text{radius}, 0)$.
/// - Panjang segmen garis tengah tulang punggung kapsul: $\text{height} - 2 \times \text{radius}$.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Capsule {
    /// Posisi telapak kaki (y_feet) dalam meter
    pub base: Vec3,
    /// Radius kapsul dalam meter
    pub radius: f32,
    /// Tinggi total kapsul dalam meter
    pub height: f32,
}

impl Capsule {
    /// Membuat kapsul tegak lurus dengan titik dasar telapak kaki (feet) tertentu
    #[inline(always)]
    pub fn new(base: Vec3, radius: f32, height: f32) -> Self {
        debug_assert!(
            height >= 2.0 * radius,
            "Tinggi kapsul ({}) harus setidaknya 2x radius ({})",
            height,
            radius
        );
        Self {
            base,
            radius,
            height,
        }
    }

    /// Pusat belahan bola bawah kapsul
    #[inline(always)]
    pub fn lower_sphere_center(&self) -> Vec3 {
        self.base + Vec3::new(0.0, self.radius, 0.0)
    }

    /// Pusat belahan bola atas kapsul
    #[inline(always)]
    pub fn upper_sphere_center(&self) -> Vec3 {
        self.base + Vec3::new(0.0, self.height - self.radius, 0.0)
    }

    /// Panjang segmen garis vertikal antara pusat bola bawah dan pusat bola atas
    #[inline(always)]
    pub fn segment_length(&self) -> f32 {
        (self.height - 2.0 * self.radius).max(0.0)
    }

    /// Titik puncak teratas kapsul (ubun-ubun kepala)
    #[inline(always)]
    pub fn top(&self) -> Vec3 {
        self.base + Vec3::new(0.0, self.height, 0.0)
    }

    /// BROAD-PHASE AABB: Bounding box AABB terluar pembungkus kapsul.
    /// HANYA digunakan untuk mengeliminasi kandidat balok voxel yang tidak mungkin beririsan.
    /// BUKAN bentuk tabrakan kapsul itu sendiri (No Fake AABB Collision)!
    #[inline(always)]
    pub fn aabb(&self) -> (Vec3, Vec3) {
        let min = Vec3::new(
            self.base.x - self.radius,
            self.base.y,
            self.base.z - self.radius,
        );
        let max = Vec3::new(
            self.base.x + self.radius,
            self.base.y + self.height,
            self.base.z + self.radius,
        );
        (min, max)
    }

    /// BROAD-PHASE SWEPT AABB: Bounding box yang melingkupi kapsul di posisi awal dan posisi target.
    #[inline(always)]
    pub fn swept_aabb(&self, delta: Vec3) -> (Vec3, Vec3) {
        let (cur_min, cur_max) = self.aabb();
        let target_min = cur_min + delta;
        let target_max = cur_max + delta;

        let min = cur_min.min(target_min);
        let max = cur_max.max(target_max);
        (min, max)
    }

    /// NARROW-PHASE: Menghitung jarak kuadrat geometris sejati antara segmen vertikal kapsul dan AABB balok voxel.
    ///
    /// IMPLEMENTASI TERTUTUP (Closed-Form):
    /// Karena kapsul selalu tegak lurus, koordinat X dan Z di sepanjang segmen garis vertikal adalah konstan ($c_x, c_z$).
    /// - Komponen horizontal: Jarak ke interval $[B_{\min}.x, B_{\max}.x]$ dan $[B_{\min}.z, B_{\max}.z]$.
    /// - Komponen vertikal: Jarak terdekat antara interval segmen kapsul $[y_0, y_1]$ dan interval balok $[B_{\min}.y, B_{\max}.y]$.
    ///
    /// Total jarak kuadrat: $\Delta x^2 + \Delta y^2 + \Delta z^2$.
    #[inline(always)]
    pub fn distance_sq_to_aabb(&self, aabb_min: Vec3, aabb_max: Vec3) -> f32 {
        let cx = self.base.x;
        let cz = self.base.z;

        // 1. Deviasi sumbu horizontal X
        let dx = if cx < aabb_min.x {
            aabb_min.x - cx
        } else if cx > aabb_max.x {
            cx - aabb_max.x
        } else {
            0.0
        };

        // 2. Deviasi sumbu horizontal Z
        let dz = if cz < aabb_min.z {
            aabb_min.z - cz
        } else if cz > aabb_max.z {
            cz - aabb_max.z
        } else {
            0.0
        };

        // 3. Deviasi sumbu vertikal Y antara segmen [y_lower_center, y_upper_center] dan [aabb_min.y, aabb_max.y]
        let y_lower = self.base.y + self.radius;
        let y_upper = self.base.y + self.height - self.radius;

        let dy = if y_upper < aabb_min.y {
            aabb_min.y - y_upper
        } else if y_lower > aabb_max.y {
            y_lower - aabb_max.y
        } else {
            0.0
        };

        dx * dx + dy * dy + dz * dz
    }

    /// NARROW-PHASE: Menguji irisan geometris sejati antara kapsul dan balok AABB voxel.
    /// Mengembalikan `true` jika dan hanya jika jarak terdekat antara segmen kapsul dan AABB $\le \text{radius}$.
    #[inline(always)]
    pub fn intersects_aabb(&self, aabb_min: Vec3, aabb_max: Vec3) -> bool {
        let dist_sq = self.distance_sq_to_aabb(aabb_min, aabb_max);
        dist_sq <= (self.radius * self.radius)
    }
}
