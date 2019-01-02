#[macro_use]
extern crate lazy_static;
extern crate pathfinding;
extern crate im_rc;

use im_rc as im;
use std::iter;
use std::cell::RefCell;
use std::time::SystemTime;
use hlt::*;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::collections::BinaryHeap;
use pathfinding::directed::astar::astar;
use pathfinding::num_traits::identities::Zero;

mod hlt;

#[derive(Copy, Clone)]
struct Action {
    ship_id: ShipId,
    dir: Direction
}

impl Action {
    pub fn new(ship_id: ShipId, dir: Direction) -> Action {
        Action { ship_id, dir }
    }
}

#[derive(Clone)]
struct MergedAction {
    ship_id: ShipId,
    pos: Position,
    halite: usize,
    mined: im::HashMap<Position, usize>
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

// Ship halite not tracked (not important)
struct Diff {
    map: Vec<(Position, i32)>,
    ships: Vec<(ShipId, Position, Position)>,
    dropoffs: Vec<Position>,
    halite: i32,
    turn: i32,
}

struct State {
    map: im::HashMap<Position, usize>,
    ships: im::HashMap<ShipId, (Position, usize)>,
    taken: im::HashMap<Position, ShipId>,
    dropoffs: im::HashSet<Position>,
    halite: usize,
    width: usize,
    height: usize,
    turn: usize,
    max_turns: usize,
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

        let ships = im::HashMap::new();
        let taken = im::HashMap::new();
        let me = game.players.iter().find(|p| p.id == game.my_id).unwrap();
        let halite = me.halite;

        let dropoffs: im::HashSet<Position> = std::iter::once(me.shipyard.position)
            .chain(me.dropoff_ids.iter().map(|id| game.dropoffs[id].position))
            .collect();

        let width = game.map.width;
        let height = game.map.height;
        let turn = game.turn_number;

        let max_turns = game.constants.max_turns;
        let max_halite = game.constants.max_halite;
        let extract_ratio = game.constants.extract_ratio;
        let move_cost_ratio = game.constants.move_cost_ratio;
        
        State {
            map,
            ships,
            taken,
            dropoffs,
            halite,
            width,
            height,
            turn,
            max_turns,
            max_halite,
            extract_ratio,
            move_cost_ratio,
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
        self.taken.remove(&ship.0);
        self.taken.insert(pos, ship_id);
        *ship = (pos, hal);
    }

    fn at_dropoff(&self, ship_id: ShipId) -> bool {
        self.dropoffs.contains(&self.ship(ship_id).0)
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

    fn move_ship(&mut self, ship_id: ShipId, dir: Direction) -> i32 {
        assert!(dir != Direction::Still, "Staying still is not a move");

        let ship = self.ship(ship_id);
        let cost = self.halite(ship.0) / self.move_cost_ratio;
        let new_pos = self.normalize(ship.0.directional_offset(dir));
        let after_move = ship.1.checked_sub(cost).expect("Not enough halite to move");

        let (new_hal, val) = if self.dropoffs.contains(&new_pos) {
            self.halite += after_move;
            (0, cost as i32)
        } else {
            (after_move, cost as i32)
        };

        self.update_ship(ship_id, new_pos, new_hal);

        val
    }

    fn mine_ship(&mut self, ship_id: ShipId) -> i32 {
        let ship = self.ship(ship_id);
        let pos = ship.0;
        let hal = self.halite(pos);
        let cap = self.max_halite - ship.1;
        let mined = div_ceil(hal, self.extract_ratio).min(cap);

        self.update_hal(pos, hal - mined);
        self.update_ship(ship_id, pos, ship.1 + mined);

        mined as i32 * -1
    }

    pub fn turns_remaining(&self) -> usize {
        self.max_turns - self.turn
    }

    pub fn actions(&self, ship_id: ShipId) -> Vec<Action> {
        let (position, halite) = self.ships[&ship_id];

        let cost = self.map[&position] / self.move_cost_ratio;
        let mut actions = Vec::new();

        if self.turns_remaining() > 0 {
            if halite >= cost {
                Direction::get_all_cardinals()
                    .into_iter()
                    .filter_map(|dir| {
                        let new_pos = self.normalize(position.directional_offset(dir));
                        if !self.taken.contains_key(&new_pos) {
                            Some(Action::new(ship_id, dir))
                        } else { 
                            None 
                        }
                    }).for_each(|action| actions.push(action));
            } 

            if self.taken[&position] == ship_id {
                actions.push(Action::new(ship_id, Direction::Still))
            }
        }

        actions
    }

    pub fn with_ship(&self, ship: &Ship) -> State {
        let mut state = self.clone();

        state.ships.insert(ship.id, (ship.position, ship.halite));
        state.taken.insert(ship.position, ship.id);

        state
    }

    pub fn add_ship(&mut self, ship: &Ship) {
        self.ships.insert(ship.id, (ship.position, ship.halite));
        self.taken.insert(ship.position, ship.id);
    }

    pub fn rm_ship(&mut self, ship_id: ShipId) {
        let (position, _) = self.ships.remove(&ship_id).expect("Cannot remove ship");
        self.taken.remove(&position);
    }

    pub fn diff(&self, other: &State) -> Diff {
        let mut map = Vec::new();
        for &(pos, val) in self.map.iter() {
            let diff = other.halite(pos) as i32 - val as i32;
            if diff != 0 {
                map.push((pos, diff));
            }
        }

        let mut ships = Vec::new();
        for &(ship_id, (pos, _)) in self.ships.iter() {
            let other_pos = other.ship(ship_id).0;
            if other_pos != pos {
                ships.push((ship_id, pos, other_pos));
            }
        }

        let mut dropoffs = Vec::new();
        for &dropoff_pos in other.dropoffs.iter() {
            if !self.dropoffs.contains(&dropoff_pos) {
                dropoffs.push(dropoff_pos);
            }
        }

        let halite = other.halite as i32 - self.halite as i32;
        let turn = other.turn as i32 - self.turn as i32;

        Diff {
            map: map,
            ships: ships,
            dropoffs: dropoffs,
            halite: halite,
            turn: turn,
        }
    }

    pub fn merge(&self, diff: &Diff) -> State {
        let mut state = self.clone();

        for &(pos, hal) in &diff.map {
            if hal > 0 {
                state.map[&pos] += hal as usize;
            } else {
                state.map[&pos] -= -hal as usize;
            }
        }

        for &(ship_id, old_pos, new_pos) in &diff.ships {
            state.ships[&ship_id].0 = new_pos;
            state.taken.remove(&old_pos);
            state.taken.insert(new_pos, ship_id);
        }

        for &dropoff_pos in &diff.dropoffs {
            state.dropoffs.insert(dropoff_pos);
        }

        if diff.halite > 0 {
            state.halite += diff.halite as usize;
        } else {
            state.halite -= -diff.halite as usize;
        }

        if diff.turn > 0 {
            state.turn += diff.turn as usize;
        } else {
            state.turn -= -diff.turn as usize;
        }

        state
    }

    pub fn next(&self) -> State {
        let mut state = self.clone();
        state.turn += 1;

        state
    }

    pub fn can_apply(&self, action: Action) -> bool {
        match action.dir {
            Direction::Still => true,
            _ => {
                let (pos, hal) = self.ship(action.ship_id);
                let cost = self.map[&pos] / self.move_cost_ratio;
                hal >= cost
            }
        }
    }

    pub fn apply(&self, action: Action) -> (State, i32) {
        let mut state = self.next();

        let value = state.apply_mut(action);

        (state, value)
    }

    pub fn apply_mut(&mut self, action: Action) -> i32 {
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
    let max_lookahead = 100;

    // Colors
    let red = "#FF0000";
    let purple = "#9d00ff";
    let orange = "#ff8800";
    let green = "#00ff48";
    let teal = "#42f4ee";
    let yellow = "#FFFF00";

    let total_halite: usize = game.map.iter().map(|cell| cell.halite).sum();

    // State
    let mut paths: HashMap<ShipId, VecDeque<Direction>> = HashMap::new();

    Game::ready("downside");

    loop {
        let start_time = SystemTime::now();

        game.update_frame();
        navi.update_frame(&game);

        let me = game.players.iter().find(|p| p.id == game.my_id).unwrap();

        let mut command_queue: Vec<Command> = Vec::new();

        let halite_remaining: usize = game.map.iter().map(|cell| cell.halite).sum();
        // let turn_limit = game.constants.max_turns * 2 / 3;
        let turn_limit = 2; //game.constants.max_turns * 2 / 3;
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
            for (i, &dir) in dirs.iter().enumerate() {
                if i >= actions.len() {
                    actions.push(Vec::new());
                }

                actions[i].push(Action::new(ship_id, dir));
            }
        }

        let mut timeline: Vec<State> = vec![state];
        let mut diffs: Vec<Diff> = Vec::new();
        let mut poisoned: HashMap<ShipId, usize> = HashMap::new();
        let mut mined: HashMap<Position, usize> = HashMap::new();

        for (i, step) in actions.into_iter().enumerate() {
            let state = timeline[i].next();
            timeline.push(state);

            let state = timeline.last_mut().unwrap();
            for action in step {
                if !poisoned.contains_key(&action.ship_id) {
                    if state.can_apply(action) {
                        state.apply_mut(action);

                        if action.dir == Direction::Still {
                            let (pos, _) = state.ship(action.ship_id);
                            mined.insert(pos, i);
                        }
                    } else {
                        poisoned.insert(action.ship_id, i);
                    }
                }
            }
        }

        for (ship_id, t) in poisoned {
            if t == 0 {
                paths.remove(&ship_id);
            } else {
                paths.get_mut(&ship_id).unwrap().truncate(t);
            }
        }

        for edge in timeline.windows(2) {
            let prev = &edge[0];
            let next = &edge[1];

            let diff = prev.diff(next);
            diffs.push(diff);
        }

        for &ship_id in ship_ids.iter() {
            if paths.contains_key(&ship_id) {
                continue;
            }

            let mut states = RefCell::new(HashMap::new());

            let ship = &game.ships[&ship_id];
            let target = (game.constants.max_halite - ship.halite) as i32;
            let start = (ship.position, 0, 0);
            let local_key = (ship.position, 0);
            let state = timeline.first().unwrap().with_ship(&ship);

            states.borrow_mut().insert(local_key, (state, 0));

            let path = astar(
                &start,
                |&key| {
                    let local_key = (key.0, key.1);
                    let (successors, value): (Vec<_>, i32) = {
                        let (state, value) = &states.borrow()[&local_key];

                        let diff = &diffs[key.1];
                        let state = state.merge(&diff);

                        let actions = if key.1 < max_lookahead {
                            state.actions(ship_id)
                                .into_iter()
                                .filter(|action| {
                                    action.dir != Direction::Still || mined.get(&key.0).map(|&t| key.1 > t).unwrap_or(true)
                                })
                                .map(|action| state.apply(action))
                                .collect()
                        } else {
                            Vec::new()
                        };

                        (actions, *value)
                    };

                    let res: Vec<_> = successors.into_iter()
                        .filter_map(|(state, cost)| {
                            let new_val = value + cost;
                            let key = (state.ship(ship_id).0, key.1 + 1, new_val);
                            let local_key = (key.0, key.1);
                            if states.borrow().contains_key(&local_key) {
                                let mut old_state = states.borrow_mut();
                                let prev = old_state.get_mut(&local_key).unwrap();
                                if new_val < prev.1 {
                                    *prev = (state, new_val);
                                    Some((key, Cost(1, cost)))
                                } else {
                                    None
                                }
                            } else {
                                states.borrow_mut().insert(local_key, (state, new_val));
                                Some((key, Cost(1, cost)))
                            }
                        }).collect();

                    res
                },
                |&(pos, t, hal)| {
                    let state = &states.borrow()[&(pos, t)].0;
                    let dist = state.calculate_distance(pos, state.nearest_dropoff(pos));

                    Cost(dist, target + hal)
                },
                |&(pos, t, hal)| {
                    let state = &states.borrow()[&(pos, t)].0;
                    let turn_limit = max_lookahead.min(state.turns_remaining());
                    (t >= turn_limit || target + hal < 50) && state.dropoffs.contains(&pos)
                }
            );

            let mut states = states.into_inner();
            if let Some((path, _)) = path {
                let mut dir_path = VecDeque::new();

                let state = states.remove(&(path[0].0, 0)).unwrap().0;
                timeline[0] = state;

                for (i, edge) in path.windows(2).enumerate() {
                    let prev = edge[0];
                    let next = edge[1];

                    let prev_state = &timeline[i];
                    let state = states.remove(&(next.0, next.1)).unwrap().0;
                    let diff = prev_state.diff(&state);

                    // How is timeline extended ?
                    timeline[i + 1] 
                }

                // let mut prev = path[0].0;
                // for &(pos, t, hal) in path[1..].iter() {
                //     let state = &states.borrow()[&(pos, t)].0;
                //     let dir = state.get_dir(prev, pos);
                //     prev = pos;
                //
                //     dir_path.push_back(dir);
                //     Log::log(pos, format!("-ship[{}:t{}:h{}]-", ship_id.0, t, hal), yellow);
                // }

                if !dir_path.is_empty() {
                    paths.insert(ship_id, dir_path);
                }
            } else {
                Log::warn(format!("No path found for ship {}", ship_id.0));
            }
        }

        for (&ship_id, path) in paths.iter_mut() {
            let dir = path.pop_front().expect("Empty path");
            command_queue.push(Command::move_ship(ship_id, dir));
        }

        if early_game && 
            me.halite >= game.constants.ship_cost &&
                navi.get(&me.shipyard.position).map_or(true, |ship| game.ships[&ship].owner != game.my_id)
        {
            command_queue.push(me.shipyard.spawn());
        }

        let duration = SystemTime::now().duration_since(start_time).expect("Time goes forwards");
        Log::info(format!("Time: {:?}", duration));

        game.end_turn(&command_queue);
    }
}
