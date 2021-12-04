use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::io::prelude::*;
use crate::*;

#[derive(PartialEq, Eq)]
pub enum ClickAction {
    Select,
    CreateTotoro,
    CreatePointLight,
    DeleteObject,
    MovePlayerSpawn,
    MoveSelectedTotoro,
    MovePointLight,
    ToggleGrabbableTriangle
}

impl Default for ClickAction {
    fn default() -> Self { ClickAction::Select }
}

pub struct Mouse {
    pub clicked: bool,
    pub was_clicked: bool,
    pub screen_space_pos: glm::TVec2<f32>
}

pub struct Camera {
    pub position: glm::TVec3<f32>,
    pub last_position: glm::TVec3<f32>,
    pub view_space_velocity: glm::TVec3<f32>,
    pub orientation: glm::TVec2<f32>,
    pub speed: f32,
    pub radius: f32,
    pub aspect_ratio: f32,
	pub fov_radians: f32,
    pub is_collidable: bool,
    pub using_mouselook: bool,    
	pub view_from_world: glm::TMat4<f32>,
    pub clipping_from_view: glm::TMat4<f32>,
    pub clipping_from_world: glm::TMat4<f32>,
    pub world_from_clipping: glm::TMat4<f32>,
	pub world_from_view: glm::TMat4<f32>,
    pub clipping_from_screen: glm::TMat4<f32>
}

impl Camera {
    pub fn update_view(&mut self, view_from_world: glm::TMat4<f32>, window_size: glm::TVec2<u32>) {
		let clipping_from_world = self.clipping_from_view * view_from_world;
        let world_from_clipping = glm::affine_inverse(clipping_from_world);
		let world_from_view = glm::affine_inverse(view_from_world);
        let clipping_from_screen = clip_from_screen(window_size);
		
		self.view_from_world = view_from_world;
		self.clipping_from_world = clipping_from_world;
		self.world_from_clipping = world_from_clipping;
		self.world_from_view = world_from_view;
		self.clipping_from_screen = clipping_from_screen;
	}
}

#[derive(PartialEq, Eq)]
enum TokenType {
    Int,
    Float,
    String
}

pub struct Configuration {
    pub int_options: HashMap<String, u32>,
    pub float_options: HashMap<String, f32>,
    pub string_options: HashMap<String, String>
}

impl Configuration {
    pub const WINDOWED_WIDTH: &'static str = "windowed_width";
    pub const WINDOWED_HEIGHT: &'static str = "windowed_height";
    const INTS: [&'static str; 2] = [Self::WINDOWED_WIDTH, Self::WINDOWED_HEIGHT];

    pub const BGM_VOLUME: &'static str = "bgm_volume";
    const FLOATS: [&'static str; 1] = [Self::BGM_VOLUME];

    pub const LEVEL_NAME: &'static str = "level_name";
    pub const MUSIC_NAME: &'static str = "default_music";
    const STRS: [&'static str; 2] = [Self::LEVEL_NAME, Self::MUSIC_NAME];

    pub const CONFIG_FILEPATH: &'static str = "settings.cfg";

    pub fn from_file(filepath: &str) -> Option<Self> {
        let mut int_options = HashMap::with_capacity(Self::INTS.len());
        let mut float_options = HashMap::with_capacity(Self::FLOATS.len());
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

                            //Insert into hashmap based on token type
                            match token_type {
                                TokenType::Int => {
                                    let int = u32::from_str_radix(tokens[2], 10).unwrap();
                                    int_options.insert(String::from(tokens[0]), int);
                                }
                                TokenType::Float => {
                                    let f = tokens[2].parse::<f32>().unwrap();
                                    float_options.insert(String::from(tokens[0]), f);
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
                float_options,
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

    pub fn get_window_size(&self) -> glm::TVec2<u32> {
        match (self.int_options.get(Configuration::WINDOWED_WIDTH), self.int_options.get(Configuration::WINDOWED_HEIGHT)) {
            (Some(width), Some(height)) => { glm::vec2(*width, *height) }
            _ => { 
                println!("Window width or height not found in config file");
                glm::vec2(1280, 720)
            }
        }
    }
}

pub struct DebugSphere {
    pub position: glm::TVec3<f32>,
    pub color: glm::TVec4<f32>,
    pub radius: f32,
    pub highlighted: bool
}

pub struct EntityList<T> {
    pub entities: OptionVec<T>,
    pub selected_idx: Option<usize>
}

impl<T> EntityList<T> {
    pub fn new() -> Self {
        EntityList {
            entities: OptionVec::new(),
            selected_idx: None
        }
    }

    pub fn clear(&mut self) {
        self.entities.clear();
        self.selected_idx = None;
    }

    pub fn delete(&mut self, idx: usize) {
        self.entities.delete(idx);
        if let Some(i) = self.selected_idx {
            if i == idx {
                self.selected_idx = None;
            }
        }
    }

    pub fn count(&self) -> usize { self.entities.count() }

    pub fn get_mut_element(&mut self, idx: usize) -> Option<&mut T> { self.entities.get_mut_element(idx) }

    pub fn insert(&mut self, item: T) -> usize { self.entities.insert(item) }

    pub fn len(&self) -> usize { self.entities.len() }

    pub fn with_capacity(capacity: usize) -> Self {
        EntityList {
            entities: OptionVec::with_capacity(capacity),
            selected_idx: None
        }
    }
}