use ozy::structs::OptionVec;
use ozy::collision::{LineSegment, Terrain};
use strum::EnumCount;
use ozy::collision::*;
use xr::Posef;
use crate::traits::SphereCollider;
use crate::routines::*;

#[derive(PartialEq, Eq)]
pub enum MoveState {
    Grounded,
    Falling
}

pub struct WorldState {
    pub player: Player,
    pub totoros: OptionVec<Totoro>,
    pub selected_totoro: Option<usize>,
    pub collision: StaticCollision,
    pub opaque_terrain_indices: Vec<usize>,     //Indices of the terrain's graphics data in a RenderEntities array
    pub transparent_terrain_indices: Vec<usize>,     //Indices of the terrain's graphics data in a RenderEntities array
    pub skybox_strings: Vec<String>,
    pub level_name: String,
    pub active_skybox_index: usize,
    pub delta_timescale: f32
}

pub struct StaticCollision {
    pub terrain: Terrain,
    pub grabbable_flags: Vec<bool>
}

impl StaticCollision {
    pub fn new(terrain: Terrain) -> Self {
        let grabbable_flags = vec![false; terrain.face_normals.len()];
        StaticCollision {
            terrain,
            grabbable_flags
        }
    }
}

pub struct Player {
    pub tracking_position: glm::TVec3<f32>,
    pub tracking_velocity: glm::TVec3<f32>,
    pub spawn_position: glm::TVec3<f32>,
    pub tracked_segment: LineSegment,
    pub last_tracked_segment: LineSegment,
    pub movement_state: MoveState,
    pub stick_data: Option<StickData>,
    pub jumps_remaining: usize,
    pub was_holding_jump: bool
}

impl Player {
    pub const MAX_JUMPS: usize = 1;
    pub const RADIUS: f32 = 0.15;

    pub fn new(pos: glm::TVec3<f32>, spawn_position: glm::TVec3<f32>) -> Self {
        Player {
            tracking_position: pos,
            tracking_velocity: glm::zero(),
            spawn_position,
            tracked_segment: LineSegment::zero(),
            last_tracked_segment: LineSegment::zero(),
            movement_state: MoveState::Falling,
            stick_data: None,
            jumps_remaining: Player::MAX_JUMPS,
            was_holding_jump: false
        }
    }
}

pub fn ground_player(player: &mut Player, max_energy: &mut f32) {    
    player.tracking_velocity = glm::zero();
    player.jumps_remaining = Player::MAX_JUMPS;
    *max_energy = Gadget::MAX_ENERGY;
}

pub fn set_player_falling(player: &mut Player) {
    player.jumps_remaining -= 1;
    player.movement_state = MoveState::Falling;
}

pub fn reset_player_position(player: &mut Player) {    
    player.tracking_position = player.spawn_position;
    player.tracking_velocity = glm::zero();
    player.tracked_segment = LineSegment::zero();
    player.last_tracked_segment = LineSegment::zero();
    player.jumps_remaining = Player::MAX_JUMPS;
    player.movement_state = MoveState::Falling;
}

/*
pub struct CaptureBall {
    pub position: glm::TVec3<f32>,
    pub velocity: glm::TVec3<f32>
}

impl CaptureBall {
    pub const RADIUS: f32 = 1.0;
}
*/

#[derive(Debug)]
pub enum TotoroState {
    Relaxed,
    Meandering,
    Startled,
    PrePanicking,
    Panicking,
    StartDying,
    Dying,
    BrainDead
}

pub struct Totoro {
    pub position: glm::TVec3<f32>,
    pub velocity: glm::TVec3<f32>,
    pub scale: f32,
    pub health: f32,
    pub home: glm::TVec3<f32>,
    pub forward: glm::TVec3<f32>,
    pub desired_forward: glm::TVec3<f32>,
    pub state: TotoroState,
    pub state_timer: f32,
    pub relax_duration: f32,
    pub drown_sfx_id: Option<usize>,
    pub saw_player_last: f32,
}

impl Totoro {
    pub const MAX_HEALTH: f32 = 100.0;

    pub fn new(position: glm::TVec3<f32>, creation_time: f32) -> Self {
        //Generate random orientation and scale
        let forward = glm::normalize(&glm::vec3(rand::random::<f32>() * 2.0 - 1.0, ranged_randomf32(-1.0, 1.0), 0.0));
        let scale = ranged_randomf32(0.5, 2.0);
        
        Totoro {
            position,
            velocity: glm::zero(),
            scale,
            health: Self::MAX_HEALTH,
            home: position,
            forward,
            desired_forward: forward,
            state_timer: creation_time,
            state: TotoroState::Relaxed,
            relax_duration: 2.0,
            saw_player_last: 0.0,
            drown_sfx_id: None
        }
    }
}

pub fn delete_object<T>(objects: &mut OptionVec<T>, selected: &mut Option<usize>, idx: usize) {
    objects.delete(idx);
    if let Some(i) = selected {
        if *i == idx {
            *selected = None;
        }
    }
}

impl SphereCollider for Totoro {
    fn sphere(&self) -> Sphere {        
        let radius = self.scale * 0.65;
        Sphere {
            radius,
            focus: self.position + glm::vec3(0.0, 0.0, radius)
        }
    }
}

pub struct Gadget {
    pub energy_remaining: f32,
    pub pose: Posef,
    pub entity_index: usize,
    pub current_type: GadgetType
}

impl Gadget {
    pub const MAX_ENERGY: f32 = 100.0;
}

#[derive(Copy, Clone, Debug, Hash, EnumCount, PartialEq, Eq)]
pub enum GadgetType {
    Net,
    StickyHand,
    WaterCannon
}

impl GadgetType {
    //I hate Rust
    pub fn from_usize(i: usize) -> Self {
        match i {
            0 => { GadgetType::Net }
            1 => { GadgetType::StickyHand }
            2 => { GadgetType::WaterCannon }
            _ => { panic!("{} is out of range", i); }
        }
    }
}

#[derive(Debug)]
pub enum StickData {
    Left(glm::TVec3<f32>),
    Right(glm::TVec3<f32>)
}