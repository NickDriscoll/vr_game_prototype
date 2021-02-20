use std::collections::HashMap;
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
    ToggleFullScreen,
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

pub struct Configuration {
    pub int_options: HashMap<&'static str, u32>
}

impl Configuration {
    pub const WINDOWED_WIDTH: &'static str = "windowed_width";
    pub const WINDOWED_HEIGHT: &'static str = "windowed_height";
}

pub fn get_window_size(config: &Configuration) -> glm::TVec2<u32> {
    match (config.int_options.get(Configuration::WINDOWED_WIDTH), config.int_options.get(Configuration::WINDOWED_HEIGHT)) {
        (Some(width), Some(height)) => { glm::vec2(*width, *height) }
        _ => { 
            println!("Window width or height not found in config file");
            glm::vec2(1280, 720)
        }
    }
}