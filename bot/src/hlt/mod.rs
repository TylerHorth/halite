#[allow(dead_code)]
pub mod command;
pub use self::command::*;
#[allow(dead_code)]
pub mod constants;
pub use self::constants::*;
#[allow(dead_code)]
pub mod direction;
pub use self::direction::*;
#[allow(dead_code)]
pub mod dropoff;
pub use self::dropoff::*;
#[allow(dead_code)]
pub mod entity;
pub use self::entity::*;
#[allow(dead_code)]
pub mod game;
pub use self::game::*;
#[allow(dead_code)]
pub mod game_map;
pub use self::game_map::*;
#[allow(dead_code)]
pub mod log;
pub use self::log::*;
#[allow(dead_code)]
pub mod map_cell;
pub use self::map_cell::*;
#[allow(dead_code)]
pub mod map_cell_iterator;
pub use self::map_cell_iterator::*;
#[allow(dead_code)]
pub mod navi;
pub use self::navi::*;
#[allow(dead_code)]
pub mod player;
pub use self::player::*;
#[allow(dead_code)]
pub mod position;
pub use self::position::*;
#[allow(dead_code)]
pub mod ship;
pub use self::ship::*;
#[allow(dead_code)]
pub mod shipyard;
pub use self::shipyard::*;

#[allow(dead_code)]
mod input;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct PlayerId(pub usize);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct DropoffId(pub usize);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ShipId(pub usize);
