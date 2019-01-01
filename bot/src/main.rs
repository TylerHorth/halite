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
use pathfinding::directed::dijkstra::dijkstra_all;
use pathfinding::directed::astar::astar;
use pathfinding::num_traits::identities::Zero;

mod hlt;

struct Action {
    ship_id: ShipId,
    dir: Direction
}

impl Action {
    pub fn new(ship_id: ShipId, dir: Direction) -> Action {
        Action { ship_id, dir }
    }
}

struct MergedAction {
    ships: im::HashMap<ShipId, (Position, usize)>,
}

struct Node {
    action: Action,
    value: i32,
    count: i32,
    state: State,
}

impl Eq for Node {}

impl Ord for Node {
    fn cmp(&self, other: &Node) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Node) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Node) -> bool {
        self.value == other.value
    }
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
    dropoffs: im::HashSet<Position>,
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
        let new_hal = ship.1.checked_sub(cost).expect("Not enough halite to move");

        self.update_ship(ship_id, new_pos, new_hal);

        cost as i32
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

            actions.push(Action::new(ship_id, Direction::Still))
        }

        actions
    }

    pub fn with_ship(&self, ship: &Ship) -> State {
        let mut state = self.clone();

        state.ships.insert(ship.id, (ship.position, ship.halite));
        state.taken.insert(ship.position, ship.id);

        state
    }

    pub fn apply(&self, action: Action) -> (State, i32) {
        let mut state = self.clone();
        state.turn += 1;

        let value = match action.dir {
            Direction::Still => state.mine_ship(action.ship_id),
            dir => state.move_ship(action.ship_id, dir),
        };

        (state, value)
    }

    pub fn apply_all(&self, actions: impl IntoIterator<Item=Action>) -> (State, i32) {
        let mut state = self.clone();
        state.turn += 1;

        let mut total = 0;
        for action in actions {
            let value = match action.dir {
                Direction::Still => state.mine_ship(action.ship_id),
                dir => state.move_ship(action.ship_id, dir),
            };

            total += value;
        }

        (state, total)
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
    let min_space = 100;
    let kernel_size = 8;
    let max_lookahead = 50;

    // Colors
    let red = "#FF0000";
    let purple = "#9d00ff";
    let orange = "#ff8800";
    let green = "#00ff48";
    let teal = "#42f4ee";
    let yellow = "#FFFF00";

    let total_halite: usize = game.map.iter().map(|cell| cell.halite).sum();

    let mut terminal = false;
    let mut returning: HashMap<ShipId, Position> = HashMap::new();
    let mut mining: HashMap<Position, (ShipId, usize)> = HashMap::new();

    Game::ready("downside");

    loop {
        game.update_frame();
        navi.update_frame(&game);

        let me = game.players.iter().find(|p| p.id == game.my_id).unwrap();
        let map = &game.map;

        let mut command_queue: Vec<Command> = Vec::new();

        let dropoffs: Vec<Position> = std::iter::once(me.shipyard.position)
            .chain(me.dropoff_ids.iter().map(|id| game.dropoffs[id].position))
            .collect();

        let nearest_dropoff = |pos: Position| dropoffs.iter().min_by_key(|d| map.calculate_distance(&pos, d)).unwrap().clone();

        let halite_remaining: usize = game.map.iter().map(|cell| cell.halite).sum();
        // let turn_limit = game.constants.max_turns * 2 / 3;
        let turn_limit = 2; //game.constants.max_turns * 2 / 3;
        
        let early_game = halite_remaining > total_halite / 2 && game.turn_number < turn_limit;

        let mut used_halite = 0usize;

        // terminal = terminal || me.ship_ids
        //     .iter()
        //     .cloned()
        //     .any(|ship_id| {
        //         let ship = &game.ships[&ship_id];
        //         let dist = map.calculate_distance(&ship.position, &nearest_dropoff(ship.position));
        //         let turns_remaining = game.constants.max_turns - game.turn_number;
        //
        //         dist + 7 > turns_remaining
        //     });

        if terminal {
            Log::log(me.shipyard.position, "_terminal_", red);
            mining.clear();
            returning.clear();
            navi.terminal = true;

            for ship_id in me.ship_ids.iter().cloned() {
                let ship = &game.ships[&ship_id];
                returning.insert(ship_id, nearest_dropoff(ship.position));
            }
        } else {
            // Remove ships that have reached their target and finished mining/depositing
            returning.retain(|ship_id, pos| {
                game.ships
                    .get(ship_id)
                    .map(|ship| &ship.position != pos)
                    .unwrap_or_default()
            });

            mining.retain(|_, (ship_id, count)| {
                game.ships
                    .get(ship_id)
                    .map(|ship| *count > 0 && ship.halite < game.constants.max_halite)
                    .unwrap_or_default()
            });
            
            // Paths (cost) from all cells back to nearest dropoff
            let paths_dropoff = dropoffs.iter().map(|dropoff| {
                dijkstra_all(dropoff, |pos| {
                    let dist = map.calculate_distance(pos, dropoff);
                    pos.get_surrounding_cardinals()
                        .into_iter()
                        .filter(move |p| map.calculate_distance(p, dropoff) > dist)
                        .map(|p| (map.normalize(&p), map.at_position(&p).halite / game.constants.move_cost_ratio))
                })
            }).fold(HashMap::new(), |mut res: HashMap<Position, usize>, cur| {
                for (pos, (_, cost)) in cur {
                    res.entry(pos)
                        .and_modify(|c| *c = (*c).min(cost))
                        .or_insert(cost);
                }
                res
            });

            let mut richness: HashMap<Position, usize> = HashMap::new();
            let mut r_count = 0;

            // Calculate value of neighbourhood arround each cell
            for origin in map.iter().map(|cell| cell.position) {
                let mut q = VecDeque::new();
                q.push_back((origin, 1));

                let mut seen = HashSet::new();
                seen.insert(origin);

                let mut sum = 0usize;
                while let Some((pos, dist)) = q.pop_front() {
                    // sum += map.at_position(&pos).halite / game.constants.extract_ratio;
                    sum += map.at_position(&pos).halite;
                    
                    let dist = dist + 1;
                    if dist < kernel_size {
                        for next in pos.get_surrounding_cardinals() {
                            let next = map.normalize(&next);
                            if !seen.contains(&next) {
                                q.push_back((next, dist));
                                seen.insert(next);
                            }
                        }
                    }
                }

                r_count = seen.len();

                richness.insert(origin, sum);
                // DEBUG: Log::msg(origin, format!("_r{}_", sum));
            }

            let prime_ter: HashSet<Position> = if early_game {
                let passive_halite = halite_remaining / map.width / map.height * 7 / 8;
                let dropoff_richness: usize = dropoffs.iter().map(|d| richness[d]).sum();
                let avg_halite_before = dropoff_richness / dropoffs.len() / r_count / 8 + passive_halite;
                let turns_remaining = game.constants.max_turns - game.turn_number;
                let num_ships = me.ship_ids.len();

                richness.iter()
                    .filter(|(&pos, &sum)| {
                        let dist = map.calculate_distance(&nearest_dropoff(pos), &pos);
                        let cost_home = paths_dropoff.get(&pos).unwrap_or(&0);
                        let avg_halite_after = (dropoff_richness + sum) / (dropoffs.len() + 1) / r_count / 8 + passive_halite;
                        avg_halite_after * num_ships * turns_remaining > avg_halite_before * (num_ships + 6) * turns_remaining - (dist * avg_halite_before + cost_home) * (num_ships + 6) / dropoffs.len()
                    })
                    .map(|(&pos, _)| {
                        // DEBUG: Log::color(pos, yellow);
                        pos
                    })
                    .collect()
            } else {
                HashSet::new()
            };

            if !prime_ter.is_empty() { 
                used_halite += game.constants.dropoff_cost;   
            }

            let mut converted: Option<ShipId> = None;
            for ship_id in &me.ship_ids {
                let ship = &game.ships[&ship_id];
                if (converted.is_none() && map.at_entity(ship).structure == Structure::None && map.at_entity(ship).halite + ship.halite + me.halite >= game.constants.dropoff_cost) && prime_ter.contains(&ship.position) {
                    used_halite += game.constants.dropoff_cost - map.at_entity(ship).halite - ship.halite;
                    
                    returning.remove(&ship_id);
                    mining.retain(|_, (id, _)| id != ship_id);

                    command_queue.push(ship.make_dropoff());
                    converted = Some(*ship_id);
                }
            }

            let ships_mining: HashSet<ShipId> = mining.values().map(|s| s.0).collect();
            let ship_ids: Vec<ShipId> = me.ship_ids.iter().filter(|id| !ships_mining.contains(id) && !returning.contains_key(id) && Some(id.clone()) != converted.as_ref()).cloned().collect();
            for ship_id in ship_ids {
                let ship = &game.ships[&ship_id];
                let cargo_space = game.constants.max_halite - ship.halite;

                if cargo_space < min_space {
                    returning.insert(ship_id, nearest_dropoff(ship.position));
                    continue
                }

                let paths_ship = dijkstra_all(&ship.position, |pos| {
                    let cost = map.at_position(pos).halite / game.constants.move_cost_ratio;
                    let dist = map.calculate_distance(pos, &ship.position);
                    pos.get_surrounding_cardinals()
                        .into_iter()
                        .filter(move |p| map.calculate_distance(p, &ship.position) > dist)
                        .map(move |p| (map.normalize(&p), cost))
                });

                let mut candidates: Vec<_> = paths_dropoff.iter().map(|(&pos, &cost_home)| {
                    let cost_to = paths_ship.get(&pos).map(|p| p.1).unwrap_or(0);
                    let dist_to = map.calculate_distance(&ship.position, &pos) + 1;
                    let dist_home = map.calculate_distance(&pos, &nearest_dropoff(pos)) + 1;

                    // let halite = map.at_position(&pos).halite / game.constants.extract_ratio;
                    let halite = map.at_position(&pos).halite;

                    let value = (halite as i32 - cost_to as i32).min(cargo_space as i32) / dist_to as i32;
                    let value_exp = (richness[&pos] as i32 / r_count as i32).min(cargo_space as i32 - value) / (dist_to as i32 + kernel_size as i32);
                    let value_home = (halite as i32 - cost_home as i32) / (dist_to as i32 + dist_home as i32);

                    // let rate = value + value_exp;
                    let rate = value + (value_exp * cargo_space as i32 / 1000 + value_home * ship.halite as i32 / 1000);
                    // let rate =  if cargo_space > richness[&pos] {
                    //     value + value_exp
                    // } else {
                    //     value + value_home
                    // };

                    (pos, 6, rate)
                }).collect();

                candidates.sort_unstable_by(|a, b| a.2.cmp(&b.2).reverse());

                if let Some(best) = candidates.iter().find(|(p, _, _)| !mining.contains_key(p)) {
                    mining.insert(best.0, (ship_id, best.1));
                } else {
                    Log::warn("Could not find target");
                } 
            }

            // Move towards target (or mine if already on it)
            for (position, (ship_id, count)) in mining.iter_mut() {
                continue;
                // paint target
                Log::log(position.clone(), format!("_t{:?}_", ship_id.0), teal);

                let ship = &game.ships[ship_id];
                let cell = map.at_entity(ship);

                // If we are at mining location
                if &ship.position == position {
                    Log::log(ship.position, format!("_m{}_", count), purple);
                    command_queue.push(ship.stay_still());
                    *count -= 1;

                // Cant afford to move
                } else if cell.halite / game.constants.move_cost_ratio > ship.halite {
                    Log::log(ship.position, "_fuel_", orange);
                    command_queue.push(ship.stay_still());

                // We have enough halite to move
                } else {
                    navi.naive_navigate(ship, position, map);
                }
            }

            {
                let start_time = SystemTime::now();
                for &ship_id in me.ship_ids.iter() {
                    let mut states = RefCell::new(HashMap::new());

                    let ship = &game.ships[&ship_id];
                    let start = (ship.position, 0, 0);
                    let local_key = (ship.position, 0);
                    states.borrow_mut().insert(local_key, (State::from(&game).with_ship(&ship), 0));

                    let path = astar(
                        &start,
                        |&key| {
                            let local_key = (key.0, key.1);
                            let (successors, value): (Vec<_>, i32) = {
                                let (state, value) = &states.borrow()[&local_key];
                                let mut actions = state.actions(ship_id);

                                (actions.into_iter().map(|action| state.apply(action)).collect(), *value)
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
                        |&(pos, t, _)| {
                            let state = &states.borrow()[&(pos, t)].0;
                            let (pos, hal) = state.ship(ship_id);
                            let dist = state.calculate_distance(pos, state.nearest_dropoff(pos));

                            Cost(dist, state.max_halite as i32 - hal as i32)
                        },
                        |&(pos, t, _)| {
                            let state = &states.borrow()[&(pos, t)].0;
                            let (pos, hal) = state.ship(ship_id);
                            let turn_limit = max_lookahead.min(state.turns_remaining());
                            t >= turn_limit || (hal > 950 && dropoffs.contains(&pos))
                        }
                    );

                    let duration = SystemTime::now().duration_since(start_time).expect("Time goes forwards");
                    Log::info(format!("Time: {:?}", duration));

                    if let Some((path, _)) = path {
                        if let Some(&(pos, t, _)) = path.get(1) {
                            let state = &states.borrow()[&(pos, t)].0;
                            command_queue.push(Command::move_ship(ship_id, state.get_dir(ship.position, pos)));
                        }
                        for (pos, t, hal) in path {
                            Log::log(pos, format!("-ship[{}:t{}:h{}]-", ship_id.0, t, hal), yellow);
                        }
                    } else {
                        Log::warn(format!("No path found for ship {}", ship_id.0));
                    }
                }

                // for (&target, &(ship_id, _)) in mining.iter() {
                //     let mut states = HashMap::new();
                //
                //     let ship = &game.ships[&ship_id];
                //     let start = (ship.position, 0);
                //     states.insert(start, State::from(&game).with_ship(&ship));
                //
                //     let path = astar(
                //         &start,
                //         |&key| {
                //             let successors: Vec<_> = {
                //                 let state = &states[&key];
                //                 let mut actions = state.actions(ship_id);
                //                 // if actions.is_empty() {
                //                     actions.push(Action { ship_id, dir: Direction::Still });
                //                 // }
                //
                //                 actions.into_iter().map(|action| state.apply(action)).collect()
                //             };
                //
                //             let res: Vec<_> = successors.into_iter()
                //                 .map(|(state, value)| {
                //                     let key = (state.ship(ship_id).0, key.1 + 1);
                //                     states.insert(key, state);
                //                     (key, Cost(1, value))
                //                 }).collect();
                //
                //             res
                //         },
                //         |key| Cost(map.calculate_distance(&key.0, &target), 0),
                //         |&(pos, _)| pos == target
                //     );
                //
                //     if let Some((path, _)) = path {
                //         for (pos, t) in path {
                //             Log::log(pos, format!("-ship[{}:t{}]-", ship_id.0, t), yellow);
                //         }
                //     } else {
                //         Log::warn(format!("No path found from ship {} to ({}, {})", ship_id.0, target.x, target.y));
                //     }
                // }
            }
        }

        for (ship_id, position) in returning.iter() {
            continue;
            // paint target
            Log::log(position.clone(), format!("_t{:?}_", ship_id.0), teal);

            let ship = &game.ships[ship_id];
            let cell = map.at_entity(ship);

            if &ship.position == position {
                Log::warn("Already at shipyard, should have set a new target by this point");

            // Cant afford to move
            } else if cell.halite / game.constants.move_cost_ratio > ship.halite {
                Log::log(ship.position, "_fuel_", orange);
                command_queue.push(ship.stay_still());

            // We have enough halite to move
            } else {
                navi.naive_navigate(ship, position, map);
            }
        }

        for (ship_id, dir) in navi.collect_moves() {
            continue;
            Log::log(game.ships[&ship_id].position, format!("_{}_", dir.get_char_encoding()), green);
            command_queue.push(Command::move_ship(ship_id, dir));
        }

        if early_game && 
            me.halite >= game.constants.ship_cost + used_halite &&
                navi.get(&me.shipyard.position).map_or(true, |ship| game.ships[&ship].owner != game.my_id)
        {
            command_queue.push(me.shipyard.spawn());
        }

        game.end_turn(&command_queue);
    }
}
