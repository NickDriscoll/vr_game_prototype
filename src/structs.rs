#[derive(Clone, Copy)]
pub enum Command {
    Quit,
    ToggleMenu(usize, usize),
    ToggleAllMenus,
    ToggleNormalVis,
    ToggleComplexNormals,
    ToggleWireframe,
    ToggleOutline,
    ToggleHMDPov,
    ResetPlayerPosition
}