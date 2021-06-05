use strum::EnumCount;
use xr::Posef;

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
    Shotgun,
    StickyHand,
    WaterCannon
}

impl GadgetType {
    //I hate Rust
    pub fn from_usize(i: usize) -> Self {
        match i {
            0 => { GadgetType::Shotgun }
            1 => { GadgetType::StickyHand }
            2 => { GadgetType::WaterCannon }
            _ => { panic!("{} is out of range", i); }
        }
    }
}