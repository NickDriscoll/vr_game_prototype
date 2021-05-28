use strum::EnumCount;

#[derive(Copy, Clone, Debug, Hash, EnumCount, PartialEq, Eq)]

pub enum Gadget {
    Shotgun,
    StickyHand,
    WaterCannon
}

impl Gadget {
    //I hate Rust
    pub fn from_usize(i: usize) -> Self {
        match i {
            0 => { Gadget::Shotgun }
            1 => { Gadget::StickyHand }
            2 => { Gadget::WaterCannon }
            _ => { panic!("{} is out of range", i); }
        }
    }
}