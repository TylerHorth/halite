use hlt::*;
use im_rc as im;

#[derive(Copy, Clone)]
pub struct Action {
    pub ship_id: ShipId,
    pub dir: Direction,
    pub inspired: bool,
}

impl Action {
    pub fn new(ship_id: ShipId, dir: Direction, inspired: bool) -> Action {
        Action { ship_id, dir, inspired }
    }
}

#[derive(Clone)]
pub struct MergedAction {
    pub ship_id: ShipId,
    pub pos: Position,
    pub halite: usize,
    pub returned: usize,
    pub inspired: bool,
    pub mined: im::HashMap<Position, usize>,
    pub cost: i32,
}

impl MergedAction {
    pub fn new(ship: &Ship) -> MergedAction {
        MergedAction {
            ship_id: ship.id,
            pos: ship.position,
            halite: ship.halite,
            returned: 0,
            inspired: false,
            mined: im::HashMap::new(),
            cost: 0,
        }
    }
}
