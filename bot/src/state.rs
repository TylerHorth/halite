use im_rc as im;
use hlt::*;
use action::{Action, MergedAction};

pub struct State {
    pub map: im::HashMap<Position, usize>,
    pub ships: im::HashMap<ShipId, (Position, usize)>,
    pub taken: im::HashMap<Position, ShipId>,
    pub enemies: im::HashSet<Position>,
    pub inspired: im::HashSet<Position>,
    pub dropoffs: im::HashSet<Position>,
    pub halite: usize,
    pub width: usize,
    pub height: usize,
    pub turn: usize,
    pub start: usize,
    pub constants: Constants,
}

impl State {
    pub fn from(game: &Game) -> State {
        let mut map = im::HashMap::new();
        for cell in game.map.iter() {
            map.insert(cell.position, cell.halite);
        }

        let ships = im::HashMap::new();
        let taken = im::HashMap::new();
        let me = game.players.iter().find(|p| p.id == game.my_id).unwrap();
        let halite = me.halite;

        let mut enemies = im::HashSet::new();
        for ship in game.ships.values() {
            if ship.owner != game.my_id {
                enemies.insert(ship.position);
            }
        }

        let mut inspired = im::HashSet::new();
        for pos in game.map.iter().map(|cell| cell.position) {
            let mut c = 0;
            for &ship_pos in enemies.iter() {
                if game.map.calculate_distance(&pos, &ship_pos) <= game.constants.inspiration_radius {
                    c += 1;
                }

                if c == game.constants.inspiration_ship_count {
                    inspired.insert(pos);
                    break
                }
            }
        }

        let dropoffs: im::HashSet<Position> = std::iter::once(me.shipyard.position)
            .chain(me.dropoff_ids.iter().map(|id| game.dropoffs[id].position))
            .collect();

        let width = game.map.width;
        let height = game.map.height;
        let turn = game.turn_number;
        let start = turn;

        let constants = game.constants.clone();
        
        State {
            map,
            ships,
            taken,
            enemies,
            inspired,
            dropoffs,
            halite,
            width,
            height,
            turn,
            start,
            constants,
        }
    }

    pub fn calculate_distance(&self, source: Position, target: Position) -> usize {
        let normalized_source = self.normalize(source);
        let normalized_target = self.normalize(target);

        let dx = (normalized_source.x - normalized_target.x).abs() as usize;
        let dy = (normalized_source.y - normalized_target.y).abs() as usize;

        let toroidal_dx = dx.min(self.width - dx);
        let toroidal_dy = dy.min(self.height - dy);

        toroidal_dx + toroidal_dy
    }
    
    pub fn nearest_dropoff(&self, pos: Position) -> Position {
        self.dropoffs.iter().cloned().min_by_key(|&d| self.calculate_distance(pos, d)).unwrap().clone()
    }

    pub fn normalize(&self, position: Position) -> Position {
        let width = self.width as i32;
        let height = self.height as i32;
        let x = ((position.x % width) + width) % width;
        let y = ((position.y % height) + height) % height;
        Position { x, y }
    }

    pub fn halite(&self, pos: Position) -> usize {
        self.map[&pos]
    }

    pub fn update_hal(&mut self, pos: Position, hal: usize) {
        self.map.insert(pos, hal).expect(&format!("No cell at pos ({}, {})", pos.x, pos.y));
    }

    pub fn ship(&self, ship_id: ShipId) -> (Position, usize) {
        self.ships[&ship_id]
    }

    pub fn update_ship(&mut self, ship_id: ShipId, pos: Position, hal: usize) {
        let ship = self.ships.get_mut(&ship_id).expect(&format!("No ship with id {}", ship_id.0));
        if self.taken.get(&ship.0) == Some(&ship_id) {
            self.taken.remove(&ship.0);
        }
        self.taken.insert(pos, ship_id);
        *ship = (pos, hal);
    }

    pub fn get_dir(&self, source: Position, destination: Position) -> Direction {
        let normalized_source = self.normalize(source);
        let normalized_destination = self.normalize(destination);

        if normalized_source == normalized_destination {
            return Direction::Still;
        }

        let dx = (normalized_source.x - normalized_destination.x).abs() as usize;
        let dy = (normalized_source.y - normalized_destination.y).abs() as usize;

        let wrapped_dx = self.width - dx;
        let wrapped_dy = self.height - dy;

        if normalized_source.x < normalized_destination.x {
            if dx > wrapped_dx { 
                return Direction::West 
            } else { 
                return Direction::East 
            }
        } else if normalized_source.x > normalized_destination.x {
            if dx < wrapped_dx { 
                return Direction::West 
            } else {
                return Direction::East 
            }
        }

        if normalized_source.y < normalized_destination.y {
            if dy > wrapped_dy { 
                return Direction::North 
            } else {
                return Direction::South 
            }
        } else if normalized_source.y > normalized_destination.y {
            if dy < wrapped_dy {
                return Direction::North 
            } else {
                return Direction::South 
            }
        }

        panic!("This should never happen");
    }

    pub fn move_ship(&mut self, ship_id: ShipId, dir: Direction) {
        assert!(dir != Direction::Still, "Staying still is not a move");

        let ship = self.ship(ship_id);
        let cost = self.halite(ship.0) / self.constants.move_cost_ratio;
        let new_pos = self.normalize(ship.0.directional_offset(dir));
        let after_move = ship.1.checked_sub(cost).expect("Not enough halite to move");

        let new_hal = if self.dropoffs.contains(&new_pos) {
            self.halite += after_move;
            0
        } else {
            after_move
        };

        self.update_ship(ship_id, new_pos, new_hal);
    }

    pub fn mine_ship(&mut self, ship_id: ShipId) {
        let ship = self.ship(ship_id);
        let pos = ship.0;
        let hal = self.halite(pos);
        let cap = self.constants.max_halite - ship.1;
        let mined = div_ceil(hal, self.constants.extract_ratio);
        let mined = if self.inspired.contains(&pos) {
            mined + (mined as f64 * self.constants.inspired_bonus_multiplier) as usize
        } else {
            mined
        };
        let mined = mined.min(cap);

        self.update_hal(pos, hal - mined);
        self.update_ship(ship_id, pos, ship.1 + mined);
    }

    pub fn turns_remaining(&self) -> usize {
        self.constants.max_turns - self.turn
    }

    pub fn apply_merged_mut(&mut self, merged: &MergedAction) {
        if !self.taken.contains_key(&merged.pos) {
            self.taken.insert(merged.pos, merged.ship_id);
        } 
        self.ships.insert(merged.ship_id, (merged.pos, merged.halite));

        self.halite += merged.returned;

        for &(pos, hal) in &merged.mined {
            self.map[&pos] = hal;
        }
    }

    pub fn apply_merged(&self, merged: &MergedAction) -> State {
        let mut state = self.clone();
        state.apply_merged_mut(merged);
        state
    }

    pub fn actions(&self, merged: &MergedAction, allow_mine: bool) -> Vec<MergedAction> {
        let state = self.apply_merged(merged);

        let ship_id = merged.ship_id;
        let position = merged.pos;
        let halite = merged.halite;

        let cost = state.map[&position] / state.constants.move_cost_ratio;
        let mut actions = Vec::new();

        if state.turns_remaining() > 0 {
            if halite >= cost {
                for dir in Direction::get_all_cardinals() {
                    let new_pos = state.normalize(position.directional_offset(dir));
                    let inspired = state.inspired.contains(&new_pos);
                    if !state.enemies.contains(&new_pos) || state.dropoffs.contains(&new_pos) {
                        if state.taken.contains_key(&new_pos) {
                            if state.dropoffs.contains(&new_pos) {
                                let mut action = merged.clone();

                                action.pos = new_pos;
                                action.halite = 0;
                                action.inspired = inspired;
                                action.cost += 2 * state.constants.ship_cost as i32;

                                actions.push(action);
                            }
                        } else {
                            let mut action = merged.clone();

                            let hal_after = action.halite - cost;
                            let new_hal = if state.dropoffs.contains(&new_pos) {
                                action.returned += hal_after;
                                0
                            } else {
                                hal_after
                            };

                            action.pos = new_pos;
                            action.halite = new_hal;
                            action.inspired = inspired;
                            action.cost += cost as i32;

                            actions.push(action);
                        }
                    }                    
                }
            } 

            if allow_mine && state.taken[&position] == ship_id && (!state.enemies.contains(&position) || state.dropoffs.contains(&position)) {
                let mut action = merged.clone();

                let hal = state.halite(position);
                let cap = state.constants.max_halite - halite;

                let mined = div_ceil(hal, state.constants.extract_ratio);
                let mined = if state.inspired.contains(&position) {
                    action.inspired = true;
                    mined + (mined as f64 * state.constants.inspired_bonus_multiplier) as usize
                } else {
                    action.inspired = false;
                    mined
                };
                let mined = mined.min(cap);

                let hal_after = hal - mined;

                action.halite += mined;

                if action.mined.contains_key(&position) {
                    action.mined[&position] = hal_after;
                } else {
                    action.mined.insert(position, hal_after);
                }

                action.cost -= mined as i32;
                actions.push(action);
            }
        }

        actions
    }

    pub fn add_ship(&mut self, ship: &Ship) {
        self.ships.insert(ship.id, (ship.position, ship.halite));
        self.taken.insert(ship.position, ship.id);
    }

    pub fn rm_ship(&mut self, ship_id: ShipId) {
        let (position, _) = self.ships.remove(&ship_id).expect("Cannot remove ship");
        self.taken.remove(&position);
    }

    pub fn next(&self) -> State {
        let mut state = self.clone();
        state.turn += 1;

        match state.turn - state.start {
            1 => {
                for &enemy in &self.enemies {
                    Log::color(enemy, "#770000");
                    for dir in Direction::get_all_cardinals() {
                        let new_pos = self.normalize(enemy.directional_offset(dir));
                        Log::color(new_pos, "#330000");
                        state.enemies.insert(new_pos);
                    }
                }
            },
            // _ => state.enemies.clear()
            _ => {}
        };

        state
    }

    pub fn can_apply(&self, action: Action) -> bool {
        let (pos, hal) = self.ship(action.ship_id);
        let new_pos = self.normalize(pos.directional_offset(action.dir));

        if action.inspired != self.inspired.contains(&new_pos) {
            return false;
        }

        match action.dir {
            Direction::Still => !self.enemies.contains(&pos) || self.dropoffs.contains(&pos),
            _ => {
                let cost = self.map[&pos] / self.constants.move_cost_ratio;
                hal >= cost && !self.enemies.contains(&new_pos)
            }
        }
    }

    pub fn apply(&mut self, action: Action) {
        match action.dir {
            Direction::Still => self.mine_ship(action.ship_id),
            dir => self.move_ship(action.ship_id, dir),
        }
    }
}

impl Clone for State {
    fn clone(&self) -> State {
        State {
            map: self.map.clone(),
            ships: self.ships.clone(),
            taken: self.taken.clone(),
            enemies: self.enemies.clone(),
            inspired: self.inspired.clone(),
            dropoffs: self.dropoffs.clone(),
            ..*self
        }
    }
}

#[inline]
pub fn div_ceil(num: usize, by: usize) -> usize {
    (num + by - 1) / by
}
