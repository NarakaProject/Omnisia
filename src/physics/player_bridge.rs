use glam::{Quat, Vec2, Vec3};
use std::collections::BTreeSet;

use super::aabb::Aabb;
use super::broadphase::RigidBodyId;
use super::collider::{Collider, ColliderId};
use super::narrowphase;
use super::shape::{Capsule as ShapeCapsule, Shape};
use super::transform::Transform;
use super::world::PhysicsWorld;

use crate::player::collision::{
    check_ground_support, GroundContactResult, GROUND_PENETRATION_TOLERANCE,
};
use crate::player::controller::PlayerController;
use crate::player::state::{AirborneOrigin, MovementState};
use crate::streaming::store::ChunkStore;

/// Konfigurasi parameter interaksi antara PlayerController dan PhysicsWorld (Phase 9.10).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerBridgeConfig {
    /// Massa efektif pemain dalam kg yang digunakan untuk menghitung transfer impuls ke badan dinamis (default: 75.0 kg).
    /// CATATAN ARSITEKTURAL: Pemain TIDAK PERNAH menjadi badan kaku dinamis dan tidak memiliki massa di solver fisika.
    pub effective_player_mass: f32,
    /// Koefisien pengali gaya dorong pemain terhadap badan dinamis (default: 1.0).
    pub push_coefficient: f32,
    /// Ambang batas kecepatan relatif minimum untuk membangunkan badan dinamis yang sedang tidur (default: 0.05 m/s).
    pub wake_velocity_threshold: f32,
    /// Ambang batas penetrasi minimum untuk membangunkan badan dinamis yang sedang tidur (default: 0.001 m).
    pub wake_penetration_threshold: f32,
    /// Pengali impuls reaksi vertikal ke bawah saat melompat dari tumpuan dinamis (default: 1.0).
    pub jump_reaction_scale: f32,
    /// Apakah pemain terbawa oleh kecepatan linier dan sudut tumpuan dinamis bergerak (default: true).
    pub support_carry: bool,
    /// Apakah interaksi dorongan (push) terhadap badan dinamis aktif (default: true).
    pub dynamic_push: bool,
}

impl Default for PlayerBridgeConfig {
    fn default() -> Self {
        Self {
            effective_player_mass: 75.0,
            push_coefficient: 1.0,
            wake_velocity_threshold: 0.05,
            wake_penetration_threshold: 0.001,
            jump_reaction_scale: 1.0,
            support_carry: true,
            dynamic_push: true,
        }
    }
}

/// Hasil ringkasan langkah interaksi jembatan Player ↔ RigidBody per tick simulasi.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerBridgeStepResult {
    /// ID badan kaku yang menjadi tumpuan grounded pemain saat ini (jika ada).
    pub grounded_on_rigidbody: Option<RigidBodyId>,
    /// Jumlah badan dinamis yang terdorong atau menerima transfer impuls pada tick ini.
    pub bodies_pushed: usize,
    /// Jumlah badan kaku yang dibangunkan dari status tidur akibat gangguan pemain.
    pub bodies_woken: usize,
    /// Kecepatan permukaan tumpuan yang diterapkan ke pemain (m/s).
    pub support_velocity: Vec3,
    /// Apakah impuls reaksi lompatan telah diterapkan pada tumpuan dinamis.
    pub jump_reaction_applied: bool,
}

/// Jembatan formal interaksi antara PlayerController (Kinematik) dan PhysicsWorld (Badan Kaku).
#[derive(Debug, Clone, Default)]
pub struct PlayerRigidBodyBridge {
    pub config: PlayerBridgeConfig,
    /// ID badan kaku yang menjadi tumpuan pemain pada tick sebelumnya.
    pub last_support_body: Option<RigidBodyId>,
    /// Titik kontak tumpuan pada ruang dunia pada tick sebelumnya.
    pub last_support_point: Option<Vec3>,
    /// Kecepatan permukaan tumpuan saat ini (m/s).
    pub support_surface_velocity: Vec3,
    /// Badan-badan dinamis yang terdorong pada tick ini (deterministik BTreeSet).
    pub last_pushed_bodies: BTreeSet<RigidBodyId>,
}

impl PlayerRigidBodyBridge {
    /// Membuat instance baru jembatan dengan konfigurasi tertentu.
    pub fn new(config: PlayerBridgeConfig) -> Self {
        Self {
            config,
            last_support_body: None,
            last_support_point: None,
            support_surface_velocity: Vec3::ZERO,
            last_pushed_bodies: BTreeSet::new(),
        }
    }

    /// Menjalankan satu langkah koordinasi interaksi antara PlayerController dan PhysicsWorld.
    pub fn step(
        &mut self,
        player: &mut PlayerController,
        world: &mut PhysicsWorld,
        store: Option<&ChunkStore>,
        dt: f32,
        camera_yaw_deg: f32,
    ) -> PlayerBridgeStepResult {
        let mut result = PlayerBridgeStepResult::default();
        self.last_pushed_bodies.clear();

        // --------------------------------------------------------------------
        // TAHAP 1: SUPPORT CARRY (TUMPUAN BERGERAK / MOVING PLATFORM)
        // --------------------------------------------------------------------
        if self.config.support_carry {
            if let Some(support_id) = self.last_support_body {
                if let Some(body) = world.get_rigid_body(support_id) {
                    let contact_pt = self
                        .last_support_point
                        .unwrap_or(player.state.position);
                    let r = contact_pt - body.position();
                    let v_surf = body.linear_velocity() + body.angular_velocity().cross(r);

                    if v_surf.is_finite() && v_surf.length_squared() > 1e-6 {
                        player.state.position += v_surf * dt;
                        self.support_surface_velocity = v_surf;
                        result.support_velocity = v_surf;
                    } else {
                        self.support_surface_velocity = Vec3::ZERO;
                    }
                } else {
                    self.last_support_body = None;
                    self.last_support_point = None;
                    self.support_surface_velocity = Vec3::ZERO;
                }
            } else {
                self.support_surface_velocity = Vec3::ZERO;
            }
        }

        // --------------------------------------------------------------------
        // TAHAP 2: REAKSI LOMPATAN DARI TUMPUAN DINAMIS
        // --------------------------------------------------------------------
        if player.state.jump_requested && player.state.grounded {
            let support_vel = self.support_surface_velocity;

            if let Some(support_id) = self.last_support_body {
                if let Some(body) = world.get_rigid_body_mut(support_id) {
                    if body.is_dynamic() {
                        let contact_pt = self
                            .last_support_point
                            .unwrap_or(player.state.position);

                        let was_sleeping = body.is_sleeping();

                        // Terapkan impuls reaksi ke bawah pada badan dinamis
                        let body_mass = body.mass_properties().mass;
                        let max_body_reaction = body_mass * 3.0;
                        let player_jump_reaction = self.config.effective_player_mass * 4.0;
                        let reaction_mag = player_jump_reaction.min(max_body_reaction).max(0.0)
                            * self.config.jump_reaction_scale;

                        let impulse_down = Vec3::new(0.0, -reaction_mag, 0.0);
                        let _ = body.apply_impulse_at_point(impulse_down, contact_pt);

                        if was_sleeping {
                            let woken = world.wake_body_and_island(support_id);
                            result.bodies_woken += woken.max(1);
                        }

                        result.jump_reaction_applied = true;
                    }
                }
            }

            // Eksekusi lompatan pada controller pemain
            let _ = player.try_execute_jump();

            // Kecepatan permukaan tumpuan berkontribusi ke momentum lompatan pemain di udara
            if support_vel.length_squared() > 1e-4 {
                player.state.velocity.x += support_vel.x;
                player.state.velocity.z += support_vel.z;
                if support_vel.y > 0.0 {
                    player.state.velocity.y += support_vel.y;
                }
            }

            player.state.grounded = false;
            self.last_support_body = None;
            self.last_support_point = None;
        }

        let was_grounded_before_step = player.state.grounded;

        // --------------------------------------------------------------------
        // TAHAP 3: LANGKAH SIMULASI KINEMATIK PEMAIN
        // --------------------------------------------------------------------
        if let Some(st) = store {
            player.step_simulation(dt, st, camera_yaw_deg);
        } else {
            self.step_player_kinematic_minimal(player, dt, camera_yaw_deg);
        }

        // --------------------------------------------------------------------
        // TAHAP 4: INTERAKSI DORONGAN (PLAYER -> DYNAMIC) & DEPENETRASI
        // --------------------------------------------------------------------
        let push_res = self.resolve_player_rigidbody_contacts(player, world, dt);
        result.bodies_pushed = push_res.bodies_pushed;
        result.bodies_woken += push_res.bodies_woken;

        // --------------------------------------------------------------------
        // TAHAP 5: EVALUASI TUMPUAN TANAH (GROUND DETECTION STATIC + RIGIDBODY)
        // --------------------------------------------------------------------
        let ground = self.check_ground(player.state.position, player, store, world);

        if ground.grounded && player.state.velocity.y <= 0.0 {
            player.state.grounded = true;
            player.state.gliding = false;
            player.state.airborne_origin = AirborneOrigin::None;
            player.state.movement_state = MovementState::Grounded;
            player.state.ground_normal = ground.ground_normal;
            player.state.ground_distance = ground.ground_distance;
            player.state.velocity.y = 0.0;

            if let Some(stable_feet) = ground.stable_feet_y {
                player.state.position.y = stable_feet;
            }

            if let Some(body_id) = self.last_support_body {
                result.grounded_on_rigidbody = Some(body_id);

                let move_speed =
                    Vec2::new(player.state.velocity.x, player.state.velocity.z).length();
                if move_speed > self.config.wake_velocity_threshold {
                    let woken = world.wake_body_and_island(body_id);
                    result.bodies_woken += woken;
                }
            }
        } else {
            if was_grounded_before_step && !ground.grounded {
                if player.state.airborne_origin == AirborneOrigin::None {
                    player.state.airborne_origin = AirborneOrigin::FellFromEdge;
                }
                player.state.movement_state = MovementState::Falling;
                if player.state.velocity.y >= 0.0 {
                    player.state.velocity.y += player.config.gravity * dt;
                }
            }
            player.state.grounded = false;
            player.state.ground_distance = ground.ground_distance;
            self.last_support_body = None;
            self.last_support_point = None;
        }

        result
    }

    /// Evaluasi kontak tumpuan tanah terhadap ChunkStore statis dan seluruh badan di PhysicsWorld.
    pub fn check_ground(
        &mut self,
        feet_pos: Vec3,
        player: &PlayerController,
        store: Option<&ChunkStore>,
        world: &PhysicsWorld,
    ) -> GroundContactResult {
        let radius = player.config.capsule_radius;
        let epsilon = player.config.ground_contact_epsilon;
        let penetration_tol = GROUND_PENETRATION_TOLERANCE;

        // 1. Cek tumpuan statis dari ChunkStore jika tersedia
        let static_ground = if let Some(st) = store {
            check_ground_support(feet_pos, radius, epsilon, st)
        } else {
            GroundContactResult::default()
        };

        // 2. Cek tumpuan terhadap seluruh RigidBody di PhysicsWorld
        let min_bound = Vec3::new(
            feet_pos.x - radius,
            feet_pos.y - epsilon,
            feet_pos.z - radius,
        );
        let max_bound = Vec3::new(
            feet_pos.x + radius,
            feet_pos.y + radius + penetration_tol,
            feet_pos.z + radius,
        );
        let footprint_aabb = Aabb::from_min_max(min_bound, max_bound)
            .unwrap_or_else(|_| Aabb::from_min_max(feet_pos, feet_pos).unwrap());

        let candidate_body_ids = world.broadphase.query_aabb(&footprint_aabb);

        struct RigidCandidate {
            body_id: RigidBodyId,
            surface_y: f32,
            stable_feet_y: f32,
            vertical_dist: f32,
            abs_vertical_dist: f32,
            horiz_dist_sq: f32,
            contact_pt: Vec3,
        }

        let mut best_rigid: Option<RigidCandidate> = None;

        for body_id in candidate_body_ids {
            let body = match world.get_rigid_body(body_id) {
                Some(b) => b,
                None => continue,
            };

            let body_transform = body.transform();

            for (_, collider) in world
                .colliders
                .iter()
                .filter(|(_, c)| c.rigid_body_id() == body_id)
            {
                let world_trans = body_transform.mul_transform(collider.local_transform());

                match collider.shape() {
                    Shape::Box(box_shape) => {
                        let h = box_shape.half_extents();
                        let n_world = world_trans.rotation * Vec3::Y;
                        if n_world.dot(Vec3::Y) < 0.7 {
                            continue;
                        }

                        let rot_inv = world_trans.rotation.conjugate();
                        let p_loc = rot_inv * (feet_pos - world_trans.position);

                        let closest_loc =
                            Vec3::new(p_loc.x.clamp(-h.x, h.x), h.y, p_loc.z.clamp(-h.z, h.z));
                        let p_contact = world_trans.transform_point(closest_loc);

                        let dx = feet_pos.x - p_contact.x;
                        let dz = feet_pos.z - p_contact.z;
                        let horiz_dist_sq = dx * dx + dz * dz;

                        if horiz_dist_sq > (radius * radius) {
                            continue;
                        }

                        let y_offset = (radius * radius - horiz_dist_sq).max(0.0).sqrt();
                        let capsule_bottom = (feet_pos.y + radius) - y_offset;
                        let vertical_dist = capsule_bottom - p_contact.y;

                        if vertical_dist >= -penetration_tol && vertical_dist <= epsilon {
                            let stable_feet_y = p_contact.y - (radius - y_offset);
                            let abs_v = vertical_dist.abs();

                            let cand = RigidCandidate {
                                body_id,
                                surface_y: p_contact.y,
                                stable_feet_y,
                                vertical_dist,
                                abs_vertical_dist: abs_v,
                                horiz_dist_sq,
                                contact_pt: p_contact,
                            };

                            let is_better = match &best_rigid {
                                None => true,
                                Some(prev) => {
                                    if (cand.abs_vertical_dist - prev.abs_vertical_dist).abs()
                                        > 1e-4
                                    {
                                        cand.abs_vertical_dist < prev.abs_vertical_dist
                                    } else {
                                        cand.horiz_dist_sq < prev.horiz_dist_sq
                                    }
                                }
                            };

                            if is_better {
                                best_rigid = Some(cand);
                            }
                        }
                    }
                    Shape::Sphere(sphere) => {
                        let center = world_trans.position;
                        let r_sph = sphere.radius();
                        let top_y = center.y + r_sph;

                        let dx = feet_pos.x - center.x;
                        let dz = feet_pos.z - center.z;
                        let horiz_dist_sq = dx * dx + dz * dz;

                        if horiz_dist_sq <= ((r_sph + radius) * (r_sph + radius)) {
                            let p_contact = Vec3::new(center.x, top_y, center.z);
                            let vertical_dist = feet_pos.y - top_y;

                            if vertical_dist >= -penetration_tol && vertical_dist <= epsilon {
                                let cand = RigidCandidate {
                                    body_id,
                                    surface_y: top_y,
                                    stable_feet_y: top_y,
                                    vertical_dist,
                                    abs_vertical_dist: vertical_dist.abs(),
                                    horiz_dist_sq,
                                    contact_pt: p_contact,
                                };

                                let is_better = match &best_rigid {
                                    None => true,
                                    Some(prev) => cand.abs_vertical_dist < prev.abs_vertical_dist,
                                };
                                if is_better {
                                    best_rigid = Some(cand);
                                }
                            }
                        }
                    }
                    Shape::Capsule(capsule) => {
                        let top_y =
                            world_trans.position.y + capsule.half_height() + capsule.radius();
                        let dx = feet_pos.x - world_trans.position.x;
                        let dz = feet_pos.z - world_trans.position.z;
                        let horiz_dist_sq = dx * dx + dz * dz;

                        if horiz_dist_sq
                            <= ((capsule.radius() + radius) * (capsule.radius() + radius))
                        {
                            let p_contact =
                                Vec3::new(world_trans.position.x, top_y, world_trans.position.z);
                            let vertical_dist = feet_pos.y - top_y;

                            if vertical_dist >= -penetration_tol && vertical_dist <= epsilon {
                                let cand = RigidCandidate {
                                    body_id,
                                    surface_y: top_y,
                                    stable_feet_y: top_y,
                                    vertical_dist,
                                    abs_vertical_dist: vertical_dist.abs(),
                                    horiz_dist_sq,
                                    contact_pt: p_contact,
                                };
                                let is_better = match &best_rigid {
                                    None => true,
                                    Some(prev) => cand.abs_vertical_dist < prev.abs_vertical_dist,
                                };
                                if is_better {
                                    best_rigid = Some(cand);
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. Rekonsiliasi antara tumpuan statis dan tumpuan badan kaku:
        if let Some(rigid) = best_rigid {
            if static_ground.grounded {
                let static_y = static_ground.stable_feet_y.unwrap_or(f32::NEG_INFINITY);
                if rigid.stable_feet_y >= static_y - 1e-4 {
                    self.last_support_body = Some(rigid.body_id);
                    self.last_support_point = Some(rigid.contact_pt);
                    GroundContactResult {
                        grounded: true,
                        ground_normal: Vec3::Y,
                        ground_distance: rigid.vertical_dist.max(0.0),
                        support_voxel: None,
                        ground_y_surface: Some(rigid.surface_y),
                        stable_feet_y: Some(rigid.stable_feet_y),
                    }
                } else {
                    self.last_support_body = None;
                    self.last_support_point = None;
                    static_ground
                }
            } else {
                self.last_support_body = Some(rigid.body_id);
                self.last_support_point = Some(rigid.contact_pt);
                GroundContactResult {
                    grounded: true,
                    ground_normal: Vec3::Y,
                    ground_distance: rigid.vertical_dist.max(0.0),
                    support_voxel: None,
                    ground_y_surface: Some(rigid.surface_y),
                    stable_feet_y: Some(rigid.stable_feet_y),
                }
            }
        } else if static_ground.grounded {
            self.last_support_body = None;
            self.last_support_point = None;
            static_ground
        } else {
            self.last_support_body = None;
            self.last_support_point = None;
            GroundContactResult::default()
        }
    }

    /// Menyelesaikan kontak tabrakan antara kapsul pemain dan collider di PhysicsWorld.
    fn resolve_player_rigidbody_contacts(
        &mut self,
        player: &mut PlayerController,
        world: &mut PhysicsWorld,
        dt: f32,
    ) -> PushResult {
        let mut result = PushResult::default();

        let p_height = player.current_capsule().height;
        let p_radius = player.config.capsule_radius;
        let p_half_h = (p_height - 2.0 * p_radius).max(0.0) * 0.5;

        let p_shape = match ShapeCapsule::new(p_radius, p_half_h) {
            Ok(c) => Shape::Capsule(c),
            Err(_) => return result,
        };

        let p_center = player.state.position + Vec3::new(0.0, p_height * 0.5, 0.0);
        let p_transform = match Transform::new(p_center, Quat::IDENTITY) {
            Ok(t) => t,
            Err(_) => return result,
        };

        let p_collider = Collider::new(
            ColliderId(u64::MAX),
            RigidBodyId(u64::MAX),
            p_shape,
            Transform::IDENTITY,
        );

        let p_aabb = match p_collider.compute_world_aabb(&p_transform) {
            Ok(a) => a,
            Err(_) => return result,
        };

        // Perluas AABB dengan displacement pergerakan pemain
        let swept_min = p_aabb.min.min(p_aabb.min + player.state.velocity * dt);
        let swept_max = p_aabb.max.max(p_aabb.max + player.state.velocity * dt);
        let swept_aabb = Aabb::from_min_max(swept_min, swept_max).unwrap_or(p_aabb);

        let candidate_body_ids = world.broadphase.query_aabb(&swept_aabb);

        let mut impulses_to_apply: Vec<(RigidBodyId, Vec3, Vec3, bool)> = Vec::new();

        for body_id in candidate_body_ids {
            let body = match world.rigid_bodies.get(&body_id) {
                Some(b) => b,
                None => continue,
            };

            let body_trans = body.transform();
            let is_dynamic = body.is_dynamic();
            let is_sleeping = body.is_sleeping();

            for (_, collider) in world
                .colliders
                .iter()
                .filter(|(_, c)| c.rigid_body_id() == body_id)
            {
                let collider_trans = body_trans.mul_transform(collider.local_transform());

                if let Ok(Some(contact)) =
                    narrowphase::collide(&p_collider, &p_transform, collider, &collider_trans)
                {
                    let normal = contact.normal;
                    let penetration = contact.penetration;
                    let point = contact.point;

                    // Abaikan kontak tumpuan vertikal di bawah kaki
                    if normal.dot(Vec3::Y) < -0.7 {
                        continue;
                    }

                    if is_dynamic && self.config.dynamic_push {
                        let r = point - body.position();
                        let v_body_surf = body.linear_velocity() + body.angular_velocity().cross(r);
                        let v_rel = (v_body_surf - player.state.velocity).dot(normal);

                        // Pemain bergerak menuju badan atau terjadi penetrasi geometris
                        if v_rel < 0.0 || penetration > 1e-4 {
                            let inv_mass = body.mass_properties().inverse_mass;
                            let inv_inertia = body.world_inverse_inertia();
                            let r_cross_n = r.cross(normal);
                            let k = inv_mass + r_cross_n.dot(inv_inertia * r_cross_n);

                            if k > 1e-6 {
                                let max_push_impulse = self.config.effective_player_mass * 10.0;
                                let push_speed = (-v_rel).max(penetration * 5.0).max(0.1);
                                let raw_impulse = push_speed / k * self.config.push_coefficient;
                                let j_n = raw_impulse.clamp(0.0, max_push_impulse);

                                let impulse_vec = j_n * normal;
                                let should_wake = is_sleeping;

                                impulses_to_apply.push((body_id, impulse_vec, point, should_wake));
                            }
                        }

                        // Depenetrasi kinematik pemain jika menabrak badan dinamis
                        if normal.dot(Vec3::Y).abs() < 0.7 && penetration > 1e-4 {
                            player.state.position -= normal * penetration;
                            let v_into = player.state.velocity.dot(normal);
                            if v_into > 0.0 {
                                player.state.velocity -= normal * v_into;
                            }
                        }
                    } else {
                        // Rintangan Statis atau Kinematik
                        if penetration > 1e-4 {
                            player.state.position -= normal * penetration;
                            let v_into = player.state.velocity.dot(normal);
                            if v_into > 0.0 {
                                player.state.velocity -= normal * v_into;
                            }
                        }
                    }
                }
            }
        }

        // Terapkan seluruh impuls yang telah dihitung secara aman ke registri mutabel
        for (body_id, impulse_vec, point, should_wake) in impulses_to_apply {
            if let Some(b_mut) = world.rigid_bodies.get_mut(&body_id) {
                let _ = b_mut.apply_impulse_at_point(impulse_vec, point);
            }
            if should_wake {
                let woken = world.wake_body_and_island(body_id);
                result.bodies_woken += woken.max(1);
            }
            self.last_pushed_bodies.insert(body_id);
            result.bodies_pushed += 1;
        }

        result
    }

    /// Langkah simulasi pergerakan pemain mandiri saat ChunkStore tidak disediakan.
    fn step_player_kinematic_minimal(
        &self,
        player: &mut PlayerController,
        dt: f32,
        camera_yaw_deg: f32,
    ) {
        // Sinkronkan status jongkok dari input
        if player.input.crouch {
            player.state.crouching = true;
            player.state.forced_crouch = false;
        } else {
            player.state.crouching = false;
            player.state.forced_crouch = false;
        }

        player.update_movement_states();
        player.try_execute_jump();

        let move_intent = player.compute_horizontal_intent(camera_yaw_deg);
        let target_speed = player.current_target_speed();

        if move_intent.length_squared() > 1e-4 {
            player.state.velocity.x = move_intent.x * target_speed;
            player.state.velocity.z = move_intent.z * target_speed;
        } else if player.state.grounded {
            // Berhenti di tanah jika tidak ada input
            player.state.velocity.x = 0.0;
            player.state.velocity.z = 0.0;
        }

        if !player.state.grounded {
            if player.state.airborne_origin == AirborneOrigin::None {
                player.state.airborne_origin = AirborneOrigin::FellFromEdge;
            }

            let eff_gravity = if player.state.gliding {
                player.config.gravity * player.config.glide_gravity_multiplier
            } else {
                player.config.gravity
            };
            player.state.velocity.y += eff_gravity * dt;

            if player.state.velocity.y < 0.0 && !player.state.gliding {
                player.state.movement_state = MovementState::Falling;
            }

            if player.state.gliding {
                player.state.velocity.y = player
                    .state
                    .velocity
                    .y
                    .max(-player.config.glide_max_downward_speed);
            }
        }

        player.state.position += player.state.velocity * dt;
    }
}

#[derive(Default)]
struct PushResult {
    bodies_pushed: usize,
    bodies_woken: usize,
}
