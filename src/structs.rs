pub struct Sphere {
    pub rotation_multiplier: f32,
    pub hover_multiplier: f32
}

impl Sphere {
    pub fn new(rotation_multiplier: f32, hover_multiplier: f32) -> Self {
        Sphere {
            rotation_multiplier,
            hover_multiplier
        }
    }
}

#[derive(Clone, Copy)]
pub enum Command {
    Quit,
    ToggleMenu(usize, usize),
    ToggleNormalVis,
    ToggleComplexNormals,
    ToggleWireframe,
    ToggleOutline
}