use hlt::*;
use im_rc as im;

#[derive(Copy, Clone)]
pub struct Action {
    pub ship_id: ShipId,
    pub dir: Direction,
    pub inspired: bool,
    pub risk: bool,
    pub dropoff: bool,
}

impl Action {
    pub fn new(ship_id: ShipId, dir: Direction, inspired: bool, risk: bool, dropoff: bool) -> Action {
        Action { ship_id, dir, inspired, risk, dropoff}
    }
}

#[derive(Clone)]
pub struct MergedAction {
    pub ship_id: ShipId,
    pub pos: Position,
    pub halite: usize,
    pub returned: usize,
    pub inspired: bool,
    pub risk: bool,
    pub mined: im::HashMap<Position, usize>,
    pub cost: i32,
}

impl MergedAction {
    pub fn new(ship_id: ShipId, pos: Position, halite: usize) -> MergedAction {
        MergedAction {
            ship_id,
            pos,
            halite,
            returned: 0,
            inspired: false,
            risk: false,
            mined: im::HashMap::new(),
            cost: 0,
        }
    }

    pub fn spawn(pos: Position) -> MergedAction {
        MergedAction {
            ship_id: ShipId(std::usize::MAX),
            pos,
            halite: 0,
            returned: 0,
            inspired: false,
            risk: false,
            mined: im::HashMap::new(),
            cost: 0,
        }
    }
}
