use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::io::prelude::*;
use ozy::collision::*;
use crate::gadget::Gadget;

#[derive(PartialEq, Eq)]
pub enum MoveState {
    Grounded,
    Falling
}

#[derive(PartialEq, Eq)]
pub enum ClickAction {
    None,
    SelectingTotoro,
    SpawningTotoro
}

impl Default for ClickAction {
    fn default() -> Self { ClickAction::None }
}

pub struct MouseState {
    
    /*
    let mut mouselook_enabled = false;
    let mut mouse_clicked = false;
    */
}

pub struct Camera {
    pub position: glm::TVec3<f32>,
    pub last_position: glm::TVec3<f32>,
    pub collision_radius: f32

    //Camera state
    /*
    let mut mouselook_enabled = false;
    let mut mouse_clicked = false;
    let mut was_mouse_clicked = false;
    let mut camera_position = glm::vec3(0.0, -8.0, 5.5);
    let mut last_camera_position = camera_position;
    let mut camera_input: glm::TVec3<f32> = glm::zero();             //This is a unit vector in view space that represents the input camera movement vector
    let mut camera_orientation = glm::vec2(0.0, -glm::half_pi::<f32>() * 0.6);
    let mut camera_speed = 5.0;
    let camera_hit_sphere_radius = 0.5;
    let mut camera_collision = true;
    */
}

pub struct Player {
    pub tracking_position: glm::TVec3<f32>,
    pub tracking_velocity: glm::TVec3<f32>,
    pub tracked_segment: LineSegment,
    pub last_tracked_segment: LineSegment,
    pub movement_state: MoveState,
    pub radius: f32,
    pub jumps_remaining: usize,
    pub was_holding_jump: bool
}

impl Player {
    pub const MAX_JUMPS: usize = 2;
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

//But what _is_ a Totoro?
pub struct Totoro {
    pub position: glm::TVec3<f32>,
    pub home: glm::TVec3<f32>,
    pub rotation: f32,              //This is a rotation about the world-space z-axis, where 0 is positive y and rotation is counterclockwise. In radians
    pub desired_rotation: f32,
    pub state: TotoroState,
    pub state_timer: f32
}

impl Totoro {
    pub fn new(position: glm::TVec3<f32>, creation_time: f32) -> Self {
        let rotation = rand::random::<f32>() * glm::two_pi::<f32>();
        Totoro {
            position,
            home: position,
            rotation,
            desired_rotation: rotation,
            state_timer: creation_time,
            state: TotoroState::Relaxed
        }
    }
}

pub enum TotoroState {
    Relaxed,
    Meandering
}

#[derive(PartialEq, Eq)]
enum TokenType {
    Int,
    Float,
    String
}

pub struct Configuration {
    pub int_options: HashMap<String, u32>,
    pub string_options: HashMap<String, String>
}

impl Configuration {
    pub const WINDOWED_WIDTH: &'static str = "windowed_width";
    pub const WINDOWED_HEIGHT: &'static str = "windowed_height";
    const INTS: [&'static str; 2] = [Self::WINDOWED_WIDTH, Self::WINDOWED_HEIGHT];

    pub const LEVEL_NAME: &'static str = "level_name";
    const STRS: [&'static str; 1] = [Self::LEVEL_NAME];

    pub const CONFIG_FILEPATH: &'static str = "settings.cfg";

    pub fn from_file(filepath: &str) -> Option<Self> {
        let mut int_options = HashMap::with_capacity(Self::INTS.len());
        let mut string_options = HashMap::with_capacity(Self::STRS.len());

        match File::open(filepath) {
            Ok(file) => {
                let reader = BufReader::new(file);
                for line in reader.lines() {
                    match line {
                        Ok(s) => {
                            //Ignore blank or commented lines
                            if s.is_empty() {
                                continue;
                            }

                            let mut tokens = Vec::new();
                            for token in s.split_whitespace() {
                                tokens.push(token);
                            }

                            //Continue if this is a comment line
                            if tokens[0].chars().next().unwrap() == '#' {
                                continue;
                            }

                            if tokens.len() != 3 {
                                println!("{} is malformed", filepath);
                                return None;
                            }

                            let mut token_type = TokenType::Int;
                            for ch in tokens[2].chars() {
                                if ch < '0' || ch > '9' {
                                    if ch == '.' {
                                        if token_type == TokenType::Float {
                                            token_type = TokenType::String;
                                            break;
                                        }
                                        token_type = TokenType::Float;
                                    } else {
                                        token_type = TokenType::String;
                                        break;
                                    }
                                }
                            }

                            match token_type {
                                TokenType::Int => {
                                    let int = u32::from_str_radix(tokens[2], 10).unwrap();
                                    int_options.insert(String::from(tokens[0]), int);
                                }
                                TokenType::Float => {

                                }
                                TokenType::String => {
                                    string_options.insert(String::from(tokens[0]), String::from(tokens[2]));
                                }
                            }
                        }
                        Err(e) => {
                            println!("Couldn't read config lines: {}", e);
                            return None;    
                        }
                    }
                }
            }
            Err(e) => {
                println!("Couldn't open config file: {}", e);
                return None;
            }
        }

        Some(
            Configuration {
                int_options,
                string_options
            }
        )
    }

    pub fn to_file(&self, filepath: &str) {
        match File::create(filepath) {
            Ok(mut file) => {
                //Write int options
                for label in &Self::INTS {
                    let string = format!("{} = {}\n", label, self.int_options.get(*label).unwrap());
                    if let Err(e) = file.write(string.as_bytes()) {
                        println!("Error writing configuration file: {}", e);
                        return;
                    }
                }
    
                //Write string options
                for label in &Self::STRS {
                    let string = format!("{} = {}\n", label, self.string_options.get(*label).unwrap());
                    if let Err(e) = file.write(string.as_bytes()) {
                        println!("Error writing configuration file: {}", e);
                        return;
                    }
                }
            }
            Err(e) => {
                println!("Error opening {}: {}", filepath, e);
            }
        }
    }
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