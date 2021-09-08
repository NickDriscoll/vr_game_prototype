use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::io::prelude::*;
use crate::*;

#[derive(PartialEq, Eq)]
pub enum ClickAction {
    Default,
    CreateTotoro,
    CreatePointLight,
    DeleteObject,
    MovePlayerSpawn,
    MoveSelectedTotoro,
    MovePointLight
}

impl Default for ClickAction {
    fn default() -> Self { ClickAction::Default }
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
    pub is_colliding: bool,
    pub using_mouselook: bool,
    pub screen_state: ScreenState
}

impl Camera {
    
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
    pub const MUSIC_NAME: &'static str = "default_music";
    const STRS: [&'static str; 2] = [Self::LEVEL_NAME, Self::MUSIC_NAME];

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

                            //Insert into hashmap based on token type
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
                for label in &
                Self::STRS {
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