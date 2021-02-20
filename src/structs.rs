use crate::collision::LineSegment;

#[derive(Clone, Copy)]
pub enum Command {
    Quit,
    ToggleMenu(usize, usize),
    ToggleAllMenus,
    ToggleNormalVis,
    ToggleComplexNormals,
    ToggleWireframe,
    ToggleHMDPov,
    ResetPlayerPosition
}

#[derive(PartialEq, Eq)]
pub enum MoveState {
    Grounded,
    Falling,
    Sliding
}

pub struct Player {
    pub tracking_position: glm::TVec3<f32>,
    pub tracking_velocity: glm::TVec3<f32>,
    pub tracked_segment: LineSegment,
    pub last_tracked_segment: LineSegment,
    pub movement_state: MoveState,
    pub radius: f32,
    pub jumps_remaining: usize
}

impl Player {
    pub const MAX_JUMPS: usize = 2;
}

pub fn set_player_falling(player: &mut Player) {
    player.jumps_remaining -= 1;
    player.movement_state = MoveState::Falling;    
}