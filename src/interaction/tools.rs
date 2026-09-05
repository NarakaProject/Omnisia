use glam::{IVec3, Vec3};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::gathering::calculate_yield;
use super::mutation::execute_interaction_transaction;
use super::raycast::raycast_player_interaction;
use super::types::{
    CollectionResult, InteractionCooldown, ResourceDefinition, ToolCategory, ToolDefinition,
    ToolEffectiveness, ToolError, ToolGatheringResult, ToolId, ToolRequirement, ToolState,
    VoxelHit, VoxelRaycastResult,
};
use crate::coord::world_voxel_to_chunk_and_local;
use crate::csg::edit::VoxelEdit;
use crate::csg::transaction::VoxelEditTransaction;
use crate::modding::resource_id::ResourceId;
use crate::player::PlayerController;
use crate::world::World;

/// Registri runtime turunan untuk definisi alat (Phase 11.5).
///
/// Menyediakan pencarian O(1) berdasarkan `ToolId`.
/// INVARIANT GUARDRAIL 12: Merupakan derived runtime cache, BUKAN database konten otoritatif independen.
#[derive(Debug, Clone, Default)]
pub struct ToolRegistry {
    by_id: HashMap<ToolId, ToolDefinition>,
}

impl ToolRegistry {
    /// Membuat registri alat baru yang kosong
    pub fn new() -> Self {
        Self {
            by_id: HashMap::new(),
        }
    }

    /// Mendaftarkan definisi alat baru secara eksplisit.
    /// Memvalidasi floating-point efektivitas alat sebelum diterima (Guardrail 13).
    pub fn register(&mut self, def: ToolDefinition) -> Result<(), ToolError> {
        def.validate().map_err(ToolError::InvalidToolDefinition)?;
        self.by_id.insert(def.tool_id.clone(), def);
        Ok(())
    }

    /// Mengambil referensi definisi alat berdasarkan `ToolId` secara O(1)
    #[inline(always)]
    pub fn get(&self, id: &ToolId) -> Option<&ToolDefinition> {
        self.by_id.get(id)
    }

    /// Mengecek apakah suatu `ToolId` terdaftar dalam registri
    #[inline(always)]
    pub fn contains(&self, id: &ToolId) -> bool {
        self.by_id.contains_key(id)
    }

    /// Jumlah definisi alat yang terdaftar
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Apakah registri kosong
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Iterasi seluruh pasangan `(&ToolId, &ToolDefinition)`
    pub fn iter(&self) -> impl Iterator<Item = (&ToolId, &ToolDefinition)> {
        self.by_id.iter()
    }

    /// Memuat definisi alat dari direktori JSON (`tools/*.json`) jika ada
    pub fn load_from_dir<P: AsRef<Path>>(&mut self, dir: P) -> Result<usize, String> {
        let p = dir.as_ref();
        if !p.exists() || !p.is_dir() {
            return Ok(0);
        }

        let mut count = 0;
        let entries = fs::read_dir(p).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
                let def: ToolDefinition =
                    serde_json::from_str(&content).map_err(|e| e.to_string())?;
                self.register(def).map_err(|e| e.to_string())?;
                count += 1;
            }
        }
        Ok(count)
    }

    /// Inisialisasi fixture baseline minimal untuk verifikasi engine (Guardrail 5).
    /// HANYA alat minimal yang diperlukan untuk pengujian semantik engine, BUKAN sistem balancing gameplay.
    pub fn default_core_tools() -> Self {
        let mut registry = Self::new();

        // 1. Stone Pickaxe (Category: Pickaxe, max durability: 100)
        let pickaxe_id = ToolId::core("stone_pickaxe").expect("valid core id");
        let mut pickaxe_eff = ToolEffectiveness::default();
        if let Ok(iron_res) = ResourceId::core("iron_ore") {
            let _ = pickaxe_eff.resource_multipliers.insert(iron_res, 1.0);
        }
        if let Ok(stone_res) = ResourceId::core("stone") {
            let _ = pickaxe_eff.resource_multipliers.insert(stone_res, 1.0);
        }
        let pickaxe_def = ToolDefinition {
            tool_id: pickaxe_id,
            category: ToolCategory::Pickaxe,
            max_durability: 100,
            effectiveness: pickaxe_eff,
        };
        let _ = registry.register(pickaxe_def);

        // 2. Stone Axe (Category: Axe, max durability: 100)
        let axe_id = ToolId::core("stone_axe").expect("valid core id");
        let mut axe_eff = ToolEffectiveness::default();
        if let Ok(wood_res) = ResourceId::core("wood") {
            let _ = axe_eff.resource_multipliers.insert(wood_res, 1.0);
        }
        let axe_def = ToolDefinition {
            tool_id: axe_id,
            category: ToolCategory::Axe,
            max_durability: 100,
            effectiveness: axe_eff,
        };
        let _ = registry.register(axe_def);

        // 3. Stone Shovel (Category: Shovel, max durability: 100)
        let shovel_id = ToolId::core("stone_shovel").expect("valid core id");
        let mut shovel_eff = ToolEffectiveness::default();
        if let Ok(dirt_res) = ResourceId::core("dirt") {
            let _ = shovel_eff.resource_multipliers.insert(dirt_res, 1.0);
        }
        let shovel_def = ToolDefinition {
            tool_id: shovel_id,
            category: ToolCategory::Shovel,
            max_durability: 100,
            effectiveness: shovel_eff,
        };
        let _ = registry.register(shovel_def);

        // 4. Generic Tool (Category: Generic, max durability: 50)
        let generic_id = ToolId::core("generic_tool").expect("valid core id");
        let generic_def = ToolDefinition {
            tool_id: generic_id,
            category: ToolCategory::Generic,
            max_durability: 50,
            effectiveness: ToolEffectiveness::default(),
        };
        let _ = registry.register(generic_def);

        registry
    }
}

/// Menyelesaikan definisi alat aktif terhadap registri alat dunia (Phase 11.5).
///
/// INVARIANT GUARDRAIL 3: Memvalidasi bahwa `current_durability` pada `ToolState`
/// tidak melebihi `max_durability` otoritatif dari `ToolDefinition`.
pub fn resolve_tool<'a>(
    registry: &'a ToolRegistry,
    tool_state: Option<&ToolState>,
) -> Result<Option<&'a ToolDefinition>, ToolError> {
    match tool_state {
        None => Ok(None),
        Some(state) => {
            let def = registry
                .get(&state.tool_id)
                .ok_or_else(|| ToolError::UnknownTool {
                    tool_id: state.tool_id.clone(),
                })?;

            // Validasi invariant durabilitas (Guardrail 3)
            if state.current_durability > def.max_durability {
                return Err(ToolError::InvalidToolState {
                    reason: format!(
                        "Tool '{}' current durability ({}) exceeds definition max durability ({})",
                        state.tool_id, state.current_durability, def.max_durability
                    ),
                });
            }

            Ok(Some(def))
        }
    }
}

/// Memvalidasi kesesuaian alat terhadap kebutuhan pemanenan resource (Phase 11.5).
///
/// INVARIANT GUARDRAIL 8: Alat yang rusak (durabilitas == 0) TIDAK PERNAH memenuhi
/// kebutuhan `AnyTool`, `Category`, atau `Specific`.
pub fn validate_tool_requirement(
    req: &ToolRequirement,
    tool: Option<(&ToolState, &ToolDefinition)>,
) -> Result<(), ToolError> {
    match req {
        ToolRequirement::None => {
            // Jika ada alat yang dipasok namun alat tersebut rusak, tolak aksi
            if let Some((state, _)) = tool {
                if state.is_broken() {
                    return Err(ToolError::ToolBroken {
                        tool_id: state.tool_id.clone(),
                    });
                }
            }
            Ok(())
        }
        ToolRequirement::AnyTool => match tool {
            None => Err(ToolError::NoTool {
                required: ToolRequirement::AnyTool,
            }),
            Some((state, _)) if state.is_broken() => Err(ToolError::ToolBroken {
                tool_id: state.tool_id.clone(),
            }),
            Some(_) => Ok(()),
        },
        ToolRequirement::Category(expected_cat) => match tool {
            None => Err(ToolError::NoTool {
                required: ToolRequirement::Category(*expected_cat),
            }),
            Some((state, _)) if state.is_broken() => Err(ToolError::ToolBroken {
                tool_id: state.tool_id.clone(),
            }),
            Some((_, def)) if def.category != *expected_cat => Err(ToolError::WrongToolCategory {
                expected: *expected_cat,
                actual: def.category,
            }),
            Some(_) => Ok(()),
        },
        ToolRequirement::Specific(expected_id) => match tool {
            None => Err(ToolError::NoTool {
                required: ToolRequirement::Specific(expected_id.clone()),
            }),
            Some((state, _)) if state.is_broken() => Err(ToolError::ToolBroken {
                tool_id: state.tool_id.clone(),
            }),
            Some((state, _)) if state.tool_id != *expected_id => Err(ToolError::WrongTool {
                expected: expected_id.clone(),
                actual: state.tool_id.clone(),
            }),
            Some(_) => Ok(()),
        },
    }
}

/// Menghitung nilai efektivitas deterministik alat terhadap resource (Phase 11.5).
///
/// INVARIANT GUARDRAIL 4: Efektivitas adalah metadata semantik dan TIDAK mengubah kuantitas hasil panen.
#[inline(always)]
pub fn calculate_tool_effectiveness(
    tool_def: Option<&ToolDefinition>,
    resource_id: &ResourceId,
) -> f32 {
    match tool_def {
        Some(def) => def.effectiveness.calculate_effectiveness(resource_id),
        None => 1.0,
    }
}

/// Jalur validasi otoritatif tunggal untuk aksi pemanenan menggunakan alat (Phase 11.5).
///
/// INVARIANT GUARDRAIL 8: Seluruh titik masuk publik konvergen ke fungsi validasi ini.
/// Tidak ada implementasi paralel yang dapat berselisih.
pub fn validate_tool_gather_internal(
    world: &World,
    hit: &VoxelHit,
    max_reach: f32,
    tool_state: Option<&ToolState>,
) -> Result<(ResourceDefinition, f32, VoxelEditTransaction), ToolError> {
    // 1. Validasi reach
    if hit.distance > max_reach {
        return Err(ToolError::ExceedsReach {
            distance: hit.distance,
            max_reach,
        });
    }

    // 2. Validasi residency chunk target
    let (chunk_coord, _) = world_voxel_to_chunk_and_local(hit.voxel_coord);
    if !world.store.is_chunk_resident(&chunk_coord) {
        return Err(ToolError::TargetNotResident {
            coord: hit.voxel_coord,
        });
    }

    // 3. Validasi isi voxel target (bukan udara)
    let block = world.store.get_voxel_world_checked(hit.voxel_coord).ok_or(
        ToolError::TargetNotResident {
            coord: hit.voxel_coord,
        },
    )?;

    if block.is_air() {
        return Err(ToolError::TargetIsAir {
            coord: hit.voxel_coord,
        });
    }

    // 4. Resolusi resource dan validasi harvestable
    let res_def = world
        .resources
        .get_by_material(block.material())
        .cloned()
        .ok_or_else(|| ToolError::NotHarvestable {
            coord: hit.voxel_coord,
            material: block.material(),
            block_id: None,
        })?;

    if !res_def.harvestable {
        return Err(ToolError::NotHarvestable {
            coord: hit.voxel_coord,
            material: block.material(),
            block_id: res_def.source_block.clone(),
        });
    }

    // 5. Resolusi alat aktif dan validasi invariant durabilitas (Guardrail 3)
    let tool_def = resolve_tool(&world.tools, tool_state)?;

    // 6. Validasi kebutuhan alat (Guardrail 2 & 8)
    let tool_pair = tool_state.zip(tool_def);
    validate_tool_requirement(&res_def.required_tool, tool_pair)?;

    // 7. Hitung efektivitas deterministik (Guardrail 4)
    let effectiveness = calculate_tool_effectiveness(tool_def, &res_def.resource_id);

    // 8. Bangun transaksi CSG penghapusan voxel
    let mut transaction = VoxelEditTransaction::new();
    transaction.add_edit(VoxelEdit::remove(hit.voxel_coord));

    // 9. Preflight validation transaksi CSG
    transaction.validate(&world.store)?;

    Ok((res_def, effectiveness, transaction))
}

/// Memvalidasi kelayakan pemanenan dengan alat secara read-only (Phase 11.5).
pub fn can_gather_with_tool(
    world: &World,
    hit: &VoxelHit,
    max_reach: f32,
    tool_state: Option<&ToolState>,
) -> Result<(ResourceDefinition, f32, VoxelEdit), ToolError> {
    let (res_def, eff, _) = validate_tool_gather_internal(world, hit, max_reach, tool_state)?;
    Ok((res_def, eff, VoxelEdit::remove(hit.voxel_coord)))
}

/// Melakukan validasi menyeluruh terhadap aksi gathering pemain dengan alat dan membangun transaksi CSG.
pub fn validate_tool_gather_action(
    world: &World,
    player: &PlayerController,
    look_direction: Vec3,
    tool_state: Option<&ToolState>,
) -> Result<(ResourceDefinition, f32, VoxelEditTransaction), ToolError> {
    let result = raycast_player_interaction(&world.store, player, look_direction);
    let hit = match result {
        VoxelRaycastResult::Hit(h) => h,
        VoxelRaycastResult::NonResident { voxel_coord, .. } => {
            return Err(ToolError::TargetNotResident { coord: voxel_coord });
        }
        VoxelRaycastResult::Miss => return Err(ToolError::NoTargetHit),
    };

    validate_tool_gather_internal(world, &hit, player.config.interaction_reach, tool_state)
}

/// Mengeksekusi transaksi gathering dengan alat secara atomik:
/// - Mengomit mutasi penghapusan voxel ke dunia melalui pipeline CSG/struktural/fisika.
/// - HANYA jika komit berhasil, mengonsumsi 1 unit durabilitas pada `active_tool`.
///
/// INVARIANT GUARDRAIL 6 & 9: Jika komit gagal, TIDAK ADA mutasi dunia, TIDAK ADA durabilitas terkonsumsi.
pub fn execute_tool_gather_transaction(
    world: &mut World,
    target_coord: IVec3,
    resource_def: &ResourceDefinition,
    effectiveness: f32,
    active_tool: Option<&mut ToolState>,
    transaction: &VoxelEditTransaction,
) -> Result<ToolGatheringResult, ToolError> {
    let quantity = calculate_yield(resource_def);

    // 1. Eksekusi mutasi dunia via pipeline interaksi otoritatif (Phase 11.2)
    let mutation = execute_interaction_transaction(world, transaction)?;

    // 2. Dekremen durabilitas HANYA pasca-komit yang sukses (Guardrail 6 & 9)
    let (tool_id, durability_consumed, remaining_durability) = match active_tool {
        Some(tool) => {
            let id = tool.tool_id.clone();
            tool.consume_durability();
            (Some(id), 1, Some(tool.current_durability))
        }
        None => (None, 0, None),
    };

    let collection = CollectionResult {
        source_coord: target_coord,
        resource_id: resource_def.resource_id.clone(),
        quantity,
    };

    Ok(ToolGatheringResult {
        collection,
        mutation,
        tool_id,
        effectiveness,
        durability_consumed,
        remaining_durability,
    })
}

/// Menangani alur gathering pemain menggunakan alat dari input look_direction hingga mutasi dunia,
/// konsumsi durabilitas, dan hasil panen dengan cooldown debounce terintegrasi.
///
/// INVARIANT GUARDRAIL 7: Cooldown debounce direset HANYA setelah aksi sukses dieksekusi.
pub fn handle_player_tool_gather(
    world: &mut World,
    player: &PlayerController,
    look_direction: Vec3,
    active_tool: Option<&mut ToolState>,
    cooldown: &mut InteractionCooldown,
) -> Result<ToolGatheringResult, ToolError> {
    // 1. Periksa cooldown debounce (Guardrail 7)
    if !cooldown.can_act() {
        return Err(ToolError::CooldownActive {
            remaining: cooldown.timer,
        });
    }

    // 2. Validasi aksi gathering dan bangun transaksi CSG melalui jalur otoritatif tunggal
    let (resource_def, effectiveness, transaction) =
        validate_tool_gather_action(world, player, look_direction, active_tool.as_deref())?;

    let target_coord = transaction
        .edits()
        .first()
        .map(|e| e.position)
        .ok_or(ToolError::NoTargetHit)?;

    // 3. Eksekusi transaksi secara atomik dan konsumsi durabilitas pasca-komit
    let result = execute_tool_gather_transaction(
        world,
        target_coord,
        &resource_def,
        effectiveness,
        active_tool,
        &transaction,
    )?;

    // 4. Picu timer cooldown HANYA setelah aksi sukses
    cooldown.trigger();

    Ok(result)
}
