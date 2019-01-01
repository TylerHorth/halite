use im_rc as im;
use std::iter;

struct Action {
    ship_id: ShipId,
    dir: Direction
}

impl Action {
    pub fn new(ship_id: ShipId, dir: Direction) -> Action {
        Action { ship_id, dir }
    }
}

struct State {
    map: im::HashMap<Position, usize>,
    ships: im::HashMap<ShipId, (Position, usize)>,
    dropoff: Position,
    width: usize,
    height: usize,
    max_halite: usize,
    extract_ratio: usize,
    move_cost_ratio: usize,
}

impl State {
    pub fn from(game: &Game) -> State {
        let mut map = im::HashMap::new();
        for cell in game.map.iter() {
            map.insert(cell.position, cell.halite);
        }

        let mut ships = im::HashMap::new();
        let me = game.players.iter().find(|p| p.id == game.my_id).unwrap();
        for ship_id in &me.ship_ids {
            let ship = &game.ships[ship_id];
            ships.insert(ship_id.clone(), (ship.position, ship.halite));
        }

        let dropoff = me.shipyard.position;

        let width = game.map.width;
        let height = game.map.height;

        let max_halite = game.constants.max_halite;
        let extract_ratio = game.constants.extract_ratio;
        let move_cost_ratio = game.constants.move_cost_ratio;
        
        State {
            map,
            ships,
            dropoff,
            width,
            height,
            max_halite,
            extract_ratio,
            move_cost_ratio,
        }
    }

    fn normalize(&self, position: Position) -> Position {
        let width = self.width as i32;
        let height = self.height as i32;
        let x = ((position.x % width) + width) % width;
        let y = ((position.y % height) + height) % height;
        Position { x, y }
    }

    fn halite(&self, pos: Position) -> usize {
        self.map[&pos]
    }

    fn update_hal(&mut self, pos: Position, hal: usize) {
        self.map.insert(pos, hal).expect(&format!("No cell at pos ({}, {})", pos.x, pos.y));
    }

    fn ship(&self, ship_id: ShipId) -> (Position, usize) {
        self.ships[&ship_id]
    }

    fn update_ship(&mut self, ship_id: ShipId, pos: Position, hal: usize) {
        self.ships.insert(ship_id, (pos, hal)).expect(&format!("No ship with id {}", ship_id.0));
    }

    fn at_dropoff(&self, ship_id: ShipId) -> bool {
        self.ship(ship_id).0 == self.dropoff
    }

    fn move_ship(&mut self, ship_id: ShipId, dir: Direction) {
        assert!(dir != Direction::Still, "Staying still is not a move");

        let ship = self.ship(ship_id);
        let cost = self.halite(ship.0) / self.move_cost_ratio;
        let new_pos = self.normalize(ship.0.directional_offset(dir));
        let new_hal = ship.1.checked_sub(cost).expect("Not enough halite to move");

        self.update_ship(ship_id, new_pos, new_hal);
    }

    #[inline]
    fn div_ceil(num: usize, by: usize) -> usize {
        (num + by - 1) / by
    }

    fn mine_ship(&mut self, ship_id: ShipId) {
        let ship = self.ship(ship_id);
        let pos = ship.0;
        let hal = self.halite(pos);
        let cap = self.max_halite - ship.1;
        let mined = div_ceil(hal, self.extract_ratio).min(cap);

        self.update_hal(pos, hal - mined);
        self.update_ship(ship_id, pos, ship.1 + mined);
    }

    pub fn apply(&self, actions: impl IntoIterator<Item=Action>) -> State {
        let mut state = State {
            map: self.map.clone(),
            ships: self.ships.clone(),
            ..*self
        };

        for action in actions {
            match action.dir {
                Direction::Still => state.mine_ship(action.ship_id),
                dir => state.move_ship(action.ship_id, dir),
            }
        }

        state
    }
}
