#[macro_use]
extern crate lazy_static;
extern crate pathfinding;
extern crate im_rc;

use im_rc as im;
use std::cell::RefCell;
use std::time::SystemTime;
use std::time::Duration;
use hlt::*;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use pathfinding::directed::astar::astar;
use pathfinding::num_traits::identities::Zero;

mod hlt;

#[derive(Copy, Clone)]
struct Action {
    ship_id: ShipId,
    dir: Direction,
    inspired: bool,
}

impl Action {
    pub fn new(ship_id: ShipId, dir: Direction, inspired: bool) -> Action {
        Action { ship_id, dir, inspired }
    }
}

#[derive(Clone)]
struct MergedAction {
    ship_id: ShipId,
    pos: Position,
    halite: usize,
    returned: usize,
    inspired: bool,
    mined: im::HashMap<Position, usize>,
    cost: i32,
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Copy)]
struct Cost(usize, i32);

impl std::ops::Add for Cost {
    type Output = Cost;

    fn add(self, rhs: Cost) -> Cost {
        Cost(self.0 + rhs.0, self.1 + rhs.1)
    }
}

impl Zero for Cost {
    fn zero() -> Cost {
        Cost(0, 0)
    }

    fn is_zero(&self) -> bool {
        self.0 == 0 && self.1 == 0
    }
}

struct State {
    map: im::HashMap<Position, usize>,
    ships: im::HashMap<ShipId, (Position, usize)>,
    taken: im::HashMap<Position, ShipId>,
    enemies: im::HashSet<Position>,
    inspired: im::HashSet<Position>,
    dropoffs: im::HashSet<Position>,
    halite: usize,
    width: usize,
    height: usize,
    turn: usize,
    start: usize,
    constants: Constants,
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

    fn calculate_distance(&self, source: Position, target: Position) -> usize {
        let normalized_source = self.normalize(source);
        let normalized_target = self.normalize(target);

        let dx = (normalized_source.x - normalized_target.x).abs() as usize;
        let dy = (normalized_source.y - normalized_target.y).abs() as usize;

        let toroidal_dx = dx.min(self.width - dx);
        let toroidal_dy = dy.min(self.height - dy);

        toroidal_dx + toroidal_dy
    }
    
    fn nearest_dropoff(&self, pos: Position) -> Position {
        self.dropoffs.iter().cloned().min_by_key(|&d| self.calculate_distance(pos, d)).unwrap().clone()
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

    fn move_ship(&mut self, ship_id: ShipId, dir: Direction) {
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

    fn mine_ship(&mut self, ship_id: ShipId) {
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
fn div_ceil(num: usize, by: usize) -> usize {
    (num + by - 1) / by
}

fn main() {
    let mut game = Game::new();
    let mut navi = Navi::new(game.map.width, game.map.height);

    // Constants
    let max_lookahead = 40;

    // Colors
    // let red = "#FF0000";
    // let purple = "#9d00ff";
    // let orange = "#ff8800";
    // let green = "#00ff48";
    // let teal = "#42f4ee";
    let yellow = "#FFFF00";

    let total_halite: usize = game.map.iter().map(|cell| cell.halite).sum();

    // State
    let mut paths: HashMap<ShipId, VecDeque<(Direction, bool)>> = HashMap::new();
    let mut runtime = Duration::default();
    let mut max = (Duration::default(), 0);

    Game::ready("downside");

    loop {
        let start_time = SystemTime::now();

        game.update_frame();
        navi.update_frame(&game);

        let me = game.players.iter().find(|p| p.id == game.my_id).unwrap();

        let mut command_queue: Vec<Command> = Vec::new();

        let halite_remaining: usize = game.map.iter().map(|cell| cell.halite).sum();
        let turn_limit = game.constants.max_turns * 2 / 3;
        let early_game = halite_remaining > total_halite / 2 && game.turn_number < turn_limit;

        let ship_ids: HashSet<ShipId> = me.ship_ids.iter().cloned().collect();
        paths.retain(|ship_id, path| ship_ids.contains(ship_id) && !path.is_empty());

        let mut state = State::from(&game);
        for ship_id in paths.keys() {
            let ship = &game.ships[ship_id];
            state.add_ship(ship);
        }

        let mut actions: Vec<Vec<Action>> = Vec::new();
        for (&ship_id, dirs) in &paths {
            for (i, &(dir, inspired)) in dirs.iter().enumerate() {
                if i >= actions.len() {
                    actions.push(Vec::new());
                }

                actions[i].push(Action::new(ship_id, dir, inspired));
            }
        }

        let timeline: RefCell<Vec<State>> = RefCell::new(vec![state]);
        let mut poisoned: HashMap<ShipId, usize> = HashMap::new();
        let mut mined: HashMap<Position, usize> = HashMap::new();

        let mut seen_last: HashSet<ShipId> = HashSet::new(); 
        let mut rm_next: Vec<ShipId> = Vec::new(); 
        for (i, step) in actions.into_iter().enumerate() {
            let mut timeline = timeline.borrow_mut();
            let mut state = timeline[i].next();

            for ship_id in rm_next.drain(..) {
                state.rm_ship(ship_id);
            }

            let mut seen = HashSet::new();
            for action in step {
                if !poisoned.contains_key(&action.ship_id) {
                    if state.can_apply(action) {
                        state.apply(action);
                        seen.insert(action.ship_id);

                        if action.dir == Direction::Still {
                            let (pos, _) = state.ship(action.ship_id);
                            mined.insert(pos, i);
                        }
                    } else {
                        Log::warn(format!("P(s:{},t:{})", action.ship_id.0, i));
                        poisoned.insert(action.ship_id, i);
                    }
                }
            }

            for &ship_id in &seen_last {
                if !seen.contains(&ship_id) {
                    rm_next.push(ship_id);
                }
            }

            seen_last = seen;
            timeline.push(state);
        }

        {
            let mut timeline = timeline.borrow_mut();
            let mut state = timeline.last().unwrap().next();

            state.taken.clear();
            state.ships.clear();

            timeline.push(state);
        }

        for (ship_id, t) in poisoned {
            if t == 0 {
                paths.remove(&ship_id);
            } else {
                paths.get_mut(&ship_id).unwrap().truncate(t);
            }
        }

        let mut initial_actions: Vec<(MergedAction, ShipId, usize)> = ship_ids
            .into_iter()
            .filter(|ship_id| !paths.contains_key(ship_id))
            .map(|ship_id| {
                let ship = &game.ships[&ship_id];
                let action = MergedAction {
                    ship_id,
                    pos: ship.position,
                    halite: ship.halite,
                    returned: 0,
                    inspired: false,
                    mined: im::HashMap::new(),
                    cost: 0,
                };

                let mut timeline = timeline.borrow_mut();
                if timeline.len() == 1 {
                    let next = timeline.last().unwrap().next();
                    timeline.push(next);
                }

                let state = &timeline[1];
                let num_turns = state.actions(&action, true).len();

                (action, ship_id, num_turns)
            }).collect();

        initial_actions.sort_by_key(|(_, ship_id, num_turns)| (*num_turns, ship_id.0));

        for (action, ship_id, _) in initial_actions {
            if paths.contains_key(&ship_id) {
                continue;
            }

            Log::info(format!("s:{},", ship_id.0));

            let ship = &game.ships[&ship_id];
            let target = (game.constants.max_halite - ship.halite) as i32;

            let merged: RefCell<HashMap<(Position, usize), MergedAction>> = RefCell::new(HashMap::new());

            merged.borrow_mut().insert((ship.position, 0), action);

            let path = astar(
                &(ship.position, 0, 0),
                |&key| {
                    let mut timeline = timeline.borrow_mut();

                    let local_key = (key.0, key.1);
                    let successors: Vec<MergedAction> = {
                        let parent = &merged.borrow()[&local_key];
                        if key.1 + 1 == timeline.len() {
                            let next = timeline.last().unwrap().next();
                            timeline.push(next);
                        }

                        let state = &timeline[key.1 + 1];

                        if key.1 < max_lookahead {
                            let allow_mine = mined.get(&key.0).map(|&t| key.1 > t).unwrap_or(true) || state.dropoffs.contains(&key.0);
                            state.actions(parent, allow_mine)
                        } else {
                            Vec::new()
                        }
                    };

                    let res: Vec<_> = successors.into_iter()
                        .filter_map(|action| {
                            let mut merged = merged.borrow_mut();
                            let parent_cost = merged[&local_key].cost;

                            let key = (action.pos, key.1 + 1, action.cost);
                            let local_key = (key.0, key.1);
                            let marginal_cost = action.cost - parent_cost;

                            if merged.contains_key(&local_key) {
                                let prev = merged.get_mut(&local_key).unwrap();
                                if action.cost < prev.cost {
                                    *prev = action;
                                    Some((key, Cost(1, marginal_cost)))
                                } else {
                                    None
                                }
                            } else {
                                merged.insert(local_key, action);
                                Some((key, Cost(1, marginal_cost)))
                            }
                        }).collect();

                    res
                },
                |&(pos, t, hal)| {
                    let state = &timeline.borrow()[t];
                    let dist = state.calculate_distance(pos, state.nearest_dropoff(pos));

                    Cost(dist, target + hal)
                },
                |&(pos, t, hal)| {
                    let state = &timeline.borrow()[t];
                    let turns_remaining = state.turns_remaining();
                    let at_dropoff = state.dropoffs.contains(&pos);
                    let full_halite = target + hal < 50;
                    let dist = state.calculate_distance(pos, state.nearest_dropoff(pos));

                    t + dist >= max_lookahead || ((t >= turns_remaining || full_halite) && at_dropoff)
                }
            );

            if let Some((path, _)) = path {
                let mut dir_path = VecDeque::new();
                let mut timeline = timeline.borrow_mut();
                let merged = merged.borrow();

                for &(pos, t, _) in &path {
                    let diff = &merged[&(pos, t)];
                    timeline[t].apply_merged_mut(diff);
                }

                for edge in path.windows(2) {
                    let prev = edge[0].0;
                    let (next, t, hal) = edge[1];

                    let dir = timeline[0].get_dir(prev, next);

                    dir_path.push_back((dir, merged[&(next, t)].inspired));
                    Log::log(next, format!("-ship[{}:t{}:h{}]-", ship_id.0, t, hal), yellow);
                }

                if !dir_path.is_empty() {
                    paths.insert(ship_id, dir_path);
                } 
            } else {
                Log::error(format!("No path found for ship {}", ship_id.0));
                paths.insert(ship_id, VecDeque::from(vec![(Direction::Still, timeline.borrow()[0].inspired.contains(&ship.position))]));
                timeline.borrow_mut().get_mut(1).map(|state| state.add_ship(ship));
            }
        }

        for (&ship_id, path) in paths.iter_mut() {
            let dir = path.pop_front().expect("Empty path").0;
            command_queue.push(Command::move_ship(ship_id, dir));
        }

        let can_afford_ship = me.halite >= game.constants.ship_cost;
        let is_safe = timeline.borrow().get(1).map(|state| !state.taken.contains_key(&me.shipyard.position)).unwrap_or(true);
        if early_game && can_afford_ship && is_safe {
            command_queue.push(me.shipyard.spawn());
        }

        let duration = SystemTime::now().duration_since(start_time).expect("Time goes forwards");

        runtime += duration;
        max = max.max((duration, game.turn_number));

        let mean = runtime / game.turn_number as u32;

        Log::info(format!("Time: {:?}, mean: {:?}, max: {:?}, total: {:?}, look-ahead: {}", duration, mean, max, runtime, max_lookahead));

        game.end_turn(&command_queue);
    }
}
