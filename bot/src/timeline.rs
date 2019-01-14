use hlt::*;
use std::cell::{RefCell, RefMut};
use state::State;
use std::collections::HashSet;
use std::collections::HashMap;
use std::collections::VecDeque;
use action::{Action, MergedAction};
use pathfinding::directed::astar::astar;
use pathfinding::kuhn_munkres::kuhn_munkres;
use pathfinding::matrix::Matrix;
use cost::Cost;

const MAX_LOOKAHEAD: usize = 30;
const MIN_LOOKAHEAD: usize = 8;
const MIN_DROPOFF_DIST: usize = 16;
const MAX_DROPOFF_DIST: usize = 18;
const KERNEL_SIZE: i32 = 16;
const TARGET_DELTA: i32 = 70;
const SHIP_DIST_RATIO: usize = 4;
const SHIP_DROPOFF_RATIO: usize = 10;
const PATH_TIMEOUT: usize = 8;

pub struct Timeline {
    timeline: RefCell<Vec<State>>,
    unpathed: HashMap<ShipId, Ship>,
    mined: HashMap<Position, usize>,
    target_dropoffs: HashMap<ShipId, (Position, usize)>,
    spawn_action: MergedAction,
    constants: Constants,
    prime: HashSet<Position>,
    can_spawn: bool,
}

impl Timeline {
    pub fn from(
        game: &Game,
        crashed: Vec<Position>,
        paths: &mut HashMap<ShipId, VecDeque<Action>>,
    ) -> Timeline {
        // Prune crashed ships and completed paths
        let me = game.players.iter().find(|p| p.id == game.my_id).unwrap();
        let ship_ids: HashSet<ShipId> = me.ship_ids.iter().cloned().collect();
        paths.retain(|ship_id, path| ship_ids.contains(ship_id) && !path.is_empty());

        // Add each ship to initial state
        let mut state = State::from(&game);
        for ship_id in paths.keys() {
            let ship = &game.ships[ship_id];
            state.add_ship(ship);
        }

        // Transform path into set of actions at each timestep
        let mut actions: Vec<Vec<Action>> = Vec::new();
        for (&ship_id, path) in paths.iter() {
            for (i, &action) in path.iter().enumerate() {
                if i >= actions.len() {
                    actions.push(Vec::new());
                }

                actions[i].push(action);
            }
        }

        // Initialize timeline
        let mut timeline: Vec<State> = vec![state];
        let mut mined: HashMap<Position, usize> = HashMap::new();
        let mut poisoned: HashMap<ShipId, usize> = HashMap::new();
        let mut seen_last: HashSet<ShipId> = HashSet::new(); 
        let mut rm_next: Vec<ShipId> = Vec::new(); 

        // Poison paths that are near a crash
        for &ship_id in paths.keys() {
            let pos = timeline[0].ship(ship_id).0;
            for &crash in &crashed {
                if timeline[0].calculate_distance(pos, crash) <= 6 {
                    poisoned.insert(ship_id, 0);
                }
            }
        }

        let mut building = HashSet::new();

        // Create timeline states from sequence of actions
        for (i, step) in actions.into_iter().enumerate() {
            // Initialize next state
            let mut state = timeline[i].next();

            // Remove ships which completed their path
            for ship_id in rm_next.drain(..) {
                state.rm_ship(ship_id);
            }

            let make_dropoff = step.last().map(|action| action.dropoff).unwrap_or(false);

            let mut seen = HashSet::new();
            for action in step {
                if !poisoned.contains_key(&action.ship_id) {
                    if (i < PATH_TIMEOUT || make_dropoff) && state.can_apply(action) {
                        if action.dropoff {
                            let (pos, _) = state.ship(action.ship_id);
                            building.insert(pos);
                        } else if action.dir == Direction::Still {
                            // Track the latest time a position was mined
                            let (pos, _) = state.ship(action.ship_id);
                            mined.insert(pos, i);
                        }

                        // Apply the action and mark ship seen
                        state.apply(action);
                        seen.insert(action.ship_id);
                    } else {
                        // Poison the path at timestep i if the current action cannot be applied
                        Log::warn(format!("P(s:{},t:{})", action.ship_id.0, i));
                        poisoned.insert(action.ship_id, i);
                    }
                }
            }

            // Ships not in this timestep but in the last should be removed next timestep
            for &ship_id in &seen_last {
                if !seen.contains(&ship_id) {
                    rm_next.push(ship_id);
                }
            }

            seen_last = seen;
            timeline.push(state);
        }

        {
            // Add blank terminal state
            let mut state = timeline.last().unwrap().next();

            state.taken.clear();
            state.ships.clear();

            timeline.push(state);
        }

        // Truncate poisoned paths
        for (ship_id, t) in poisoned {
            if t == 0 {
                paths.remove(&ship_id);
            } else {
                paths.get_mut(&ship_id).unwrap().truncate(t);
            }
        }

        // Ships that are not pathed
        let unpathed = ship_ids.into_iter()
            .filter(|ship_id| !paths.contains_key(ship_id))
            .map(|ship_id| (ship_id, game.ships[&ship_id].clone()))
            .collect();

        // Action to test spawning ships [ship_id = INT_MAX]
        let spawn_action = MergedAction::spawn(me.shipyard.position);
        let constants = game.constants.clone();

        let mut richness: HashMap<Position, usize> = HashMap::new();

        let k = KERNEL_SIZE / 2;
        for cell in game.map.iter() {
            let pos = cell.position;
            let mut sum = 0;
            for i in 0..=KERNEL_SIZE {
                for j in 0..=KERNEL_SIZE {
                    let x = pos.x + i - k;
                    let y = pos.y + j - k;
                    let d = ((i - k).abs() + (j - k).abs()) as usize + 1;
                    
                    sum += game.map.at_position(&Position { x, y }).halite / d;
                }
            }
            richness.insert(pos, sum);
        }

        let mut max = 0;
        for dropoff in timeline[0].dropoffs.iter().chain(building.iter()) {
            max = max.max(richness[dropoff]);
        }

        let num_ships = me.ship_ids.len();
        let cur_rate = max * (num_ships + 6);

        // Create list of (dropoff, t) tuples
        let mut dropoffs = HashMap::new();
        for (i, state) in timeline.iter().enumerate() {
            for &dropoff_pos in &state.dropoffs {
                if !dropoffs.contains_key(&dropoff_pos) {
                    dropoffs.insert(dropoff_pos, i);
                }
            }
        }

        let dropoffs: Vec<_> = dropoffs.into_iter().collect();
        let ship_ids: Vec<_> = me.ship_ids.iter().cloned().collect();

        let mut prime = HashSet::new();
        if ship_ids.len() / SHIP_DROPOFF_RATIO >= dropoffs.len() {
            for (&pos, &hal) in richness.iter() {
                let rate = hal * num_ships / 2 + max * num_ships / 2;
                if rate > cur_rate {
                    let min_dist = game.dropoffs.values()
                        .map(|d| d.position)
                        .chain(game.players.iter().map(|p| p.shipyard.position))
                        .map(|p| game.map.calculate_distance(&pos, &p))
                        .min()
                        .unwrap();

                    let max_dist = game.dropoffs.values()
                        .filter(|d| d.owner == game.my_id)
                        .map(|d| d.position)
                        .chain(std::iter::once(me.shipyard.position))
                        .map(|p| game.map.calculate_distance(&pos, &p))
                        .min()
                        .unwrap();

                    if min_dist >= MIN_DROPOFF_DIST && max_dist <= MAX_DROPOFF_DIST {
                        Log::log(pos, format!("_r{}_", rate), "fuchsia");
                        prime.insert(pos);
                    }
                }
            }
        }

        let can_spawn = building.is_empty() && prime.is_empty();

        let rows = ship_ids.len();
        let columns = dropoffs.len() * ship_ids.len();

        let mut matrix_vec = Vec::with_capacity(rows * columns);
        for &ship_id in &ship_ids {
            let ship_pos = game.ships[&ship_id].position;
            
            for &(pos, t) in &dropoffs {
                let dist = game.map.calculate_distance(&ship_pos, &pos).max(t);
                let dropoff_value = richness[&pos];

                for num_ships in 0..ship_ids.len() {
                    let value = dropoff_value / (dist + num_ships * SHIP_DIST_RATIO + 1);
                    matrix_vec.push(value as i32);
                }
            }
        }

        let matrix = Matrix::from_vec(rows, columns, matrix_vec);
        let matching = kuhn_munkres(&matrix).1;

        let mut target_dropoffs = HashMap::new();
        for (ship_id_index, dropoff_slot_index) in matching.into_iter().enumerate() {
            let ship_id = ship_ids[ship_id_index];
            let ship_pos = game.ships[&ship_id].position;

            let dropoff_index = dropoff_slot_index / ship_ids.len();
            let dropoff_pos_t = dropoffs[dropoff_index];

            Log::msg(dropoff_pos_t.0, format!("_sid{}_", ship_id.0));
            Log::msg(ship_pos, format!("_d({},{},{})_", dropoff_pos_t.0.x, dropoff_pos_t.0.y, dropoff_pos_t.1));

            target_dropoffs.insert(ship_id, dropoff_pos_t);
        }

        Timeline {
            timeline: RefCell::new(timeline),
            unpathed,
            mined,
            target_dropoffs,
            spawn_action,
            constants,
            prime,
            can_spawn,
        }
    }

    /// State at timestep t
    pub fn state(&self, t: usize) -> RefMut<State> {
        let mut timeline = self.timeline.borrow_mut();

        // Add states if necessary
        while t >= timeline.len() {
            let next = timeline.last().unwrap().next();
            timeline.push(next);
        }

        RefMut::map(timeline, |timeline| &mut timeline[t])
    }

    fn path<H, S>(&mut self, initial_action: MergedAction, start: usize, target: i32, inspire: bool, max_lookahead: usize, ignore_mine: bool, heuristic: H, success: S) -> Option<Vec<MergedAction>>
        where S: Fn(bool, bool, bool, bool) -> bool,
              H: Fn(i32, usize) -> Cost {
            let merged: RefCell<HashMap<(Position, usize), MergedAction>> = RefCell::new(HashMap::new());
            let initial_pos = initial_action.pos;
            let target_pos_t = self.target_dropoffs.get(&initial_action.ship_id)
                .cloned()
                .unwrap_or_else(|| (self.state(0).nearest_dropoff(initial_pos), 0));

            merged.borrow_mut().insert((initial_pos, start), initial_action);

            let path = astar(
                &(initial_pos, start, 0),
                |&key| {
                    let local_key = (key.0, key.1);
                    let successors: Vec<MergedAction> = {
                        let parent = &merged.borrow()[&local_key];
                        let state = self.state(key.1 + 1);

                        if key.1 < max_lookahead {
                            let allow_mine = ignore_mine || self.mined.get(&key.0).map(|&t| key.1 > t).unwrap_or(true) || state.dropoffs.contains(&key.0);
                            state.actions(parent, allow_mine, inspire)
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
                    let state = self.state(t);
                    let dist = state.calculate_distance(pos, target_pos_t.0).max(target_pos_t.1);

                    heuristic(hal, dist)
                },
                |&(pos, t, _)| {
                    let state = self.state(t);
                    let turns_remaining = state.turns_remaining();
                    let at_dropoff = state.dropoffs.contains(&pos);
                    let merged = merged.borrow();
                    let action = &merged[&(pos, t)];
                    let hal = action.halite + action.returned;
                    let full_halite = hal as i32 + TARGET_DELTA >= target;
                    // let dist = state.calculate_distance(pos, target_pos_t.0).max(target_pos_t.1);

                    let depth_limit = t >= max_lookahead;
                    let time_limit = t >= turns_remaining;

                    success(depth_limit, time_limit, full_halite, at_dropoff)
                }
            );

            let mut merged = merged.into_inner();
            path.map(|(path, _)| path.into_iter().map(|(pos, t, _)| merged.remove(&(pos, t)).unwrap()).collect())
        }

    pub fn spawn_ship(&mut self) -> bool {
        let spawn_action = self.spawn_action.clone();
        let taken = self.state(1).taken.contains_key(&spawn_action.pos);
        let ship_cost = self.constants.ship_cost;
        let can_afford = self.state(0).halite >= ship_cost;

        let target = self.constants.max_halite as i32;
        let heuristic = |hal, dist| Cost(dist, target + hal);
        let success = |depth_limit, time_limit, full_halite, at_dropoff| {
            depth_limit || ((time_limit || full_halite) && at_dropoff)
        };

        can_afford && !taken && self.can_spawn && self.path(spawn_action, 1, target, false, MIN_LOOKAHEAD, false, heuristic, success).is_some()
    }

    pub fn make_dropoff(
        &mut self,
        paths: &mut HashMap<ShipId, VecDeque<Action>>,
    ) {
        if self.prime.is_empty() {
            return;
        }

        let closest_ship = paths.keys().map(|&ship_id| {
            let state = &self.state(0);
            let (pos, _) = state.ship(ship_id);
            let closest_drop = self.prime.iter().map(|&drop_pos| {
                let dist = state.calculate_distance(pos, drop_pos);
                (drop_pos, dist)
            }).min_by_key(|d| d.1).unwrap();

            (ship_id, closest_drop.0, closest_drop.1)
        }).min_by_key(|cs| cs.2).unwrap();

        let ship_id = closest_ship.0;
        let target = closest_ship.1;

        let (pos, halite) = self.state(0).ship(ship_id);

        let initial_action = MergedAction {
            ship_id,
            pos,
            halite,
            returned: 0,
            inspired: false,
            risk: false,
            mined: im_rc::HashMap::new(),
            cost: 0,
        };

        let start = 0;
        let inspire = true;

        let merged: RefCell<HashMap<(Position, usize), MergedAction>> = RefCell::new(HashMap::new());
        let initial_pos = initial_action.pos;

        merged.borrow_mut().insert((initial_pos, start), initial_action);

        let path = astar(
            &(initial_pos, start, 0),
            |&key| {
                let local_key = (key.0, key.1);
                let successors: Vec<MergedAction> = {
                    let parent = &merged.borrow()[&local_key];
                    let state = self.state(key.1 + 1);

                    if key.1 < MAX_LOOKAHEAD {
                        let allow_mine = self.mined.get(&key.0).map(|&t| key.1 > t).unwrap_or(true) || state.dropoffs.contains(&key.0);
                        state.actions(parent, allow_mine, inspire)
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
            |&(pos, t, _)| {
                let state = self.state(t);
                let dist = state.calculate_distance(pos, target);

                Cost(dist, 0)
            },
            |&(pos, t, _)| {
                let state = self.state(t);
                let merged = merged.borrow();
                let action = &merged[&(pos, t)];

                let has_halite = action.halite + state.halite + state.halite(pos) >= state.constants.dropoff_cost;
                let at_dest = pos == target;

                has_halite && at_dest
            }
        );

        let mut merged = merged.into_inner();
        let path: Option<Vec<MergedAction>> = path.map(|(path, _)| path.into_iter().map(|(pos, t, _)| merged.remove(&(pos, t)).unwrap()).collect());

        if let Some(path) = path {
            paths.remove(&ship_id);

            let mut actions = VecDeque::new();

            for (i, edge) in path.windows(2).enumerate() {
                let prev = &edge[0];
                let next = &edge[1];

                let dir = self.state(0).get_dir(prev.pos, next.pos);
                let action = Action::new(next.ship_id, dir, next.inspired, next.risk, false);

                actions.push_back(action);
                Log::log(next.pos, format!("-ship[{}:t{}:h{}]-", ship_id.0, i, next.halite), "yellow");
            }

            if !actions.is_empty() {
                let mut last = actions[actions.len() - 1].clone();
                last.dir = Direction::Still;
                last.dropoff = true;
                actions.push_back(last);

                paths.insert(ship_id, actions);
            } 
        }
    }

    pub fn path_ship(
        &mut self,
        initial_action: MergedAction,
        paths: &mut HashMap<ShipId, VecDeque<Action>>,
    ) {
        let ship_id = initial_action.ship_id;
        let ship = &self.unpathed.remove(&ship_id).expect(&format!("Ship already pathed {}", ship_id.0));
        let target = self.constants.max_halite as i32;
        let heuristic = |hal, dist| Cost(dist, target + hal);
        let success = |depth_limit, time_limit, full_halite, at_dropoff| {
            depth_limit || ((time_limit || full_halite) && at_dropoff)
        };

        if let Some(path) = self.path(initial_action.clone(), 0, target, true, MAX_LOOKAHEAD, false, heuristic, success)
            .or_else(|| self.path(initial_action.clone(), 0, target, true, MIN_LOOKAHEAD, true, heuristic, success)) {
                let mut actions = VecDeque::new();

                for (i, diff) in path.iter().enumerate() {
                    self.state(i).apply_merged_mut(diff);
                }

                for (i, edge) in path.windows(2).enumerate() {
                    let prev = &edge[0];
                    let next = &edge[1];

                    let dir = self.state(0).get_dir(prev.pos, next.pos);
                    let action = Action::new(next.ship_id, dir, next.inspired, next.risk, false);

                    actions.push_back(action);
                    Log::log(next.pos, format!("-ship[{}:t{}:h{}]-", ship_id.0, i, next.halite), "yellow");
                }

                if !actions.is_empty() {
                    paths.insert(ship_id, actions);
                } 
        } else {
            Log::error(format!("No path found for ship {}", ship_id.0));

            let next_actions = self.state(1).actions(&initial_action, true, true);

            if !next_actions.is_empty() {
                let inspired = self.state(1).inspired.contains(&ship.position);
                let merged_action = if let Some(still_action) = next_actions.iter().find(|a| a.pos == initial_action.pos) {
                    still_action
                } else {
                    next_actions.first().unwrap()
                };

                let dir = self.state(0).get_dir(initial_action.pos, merged_action.pos);
                let action = Action::new(ship_id, dir, inspired, false, false);
                paths.insert(ship_id, VecDeque::from(vec![action]));
                self.state(1).apply_merged_mut(merged_action);
            } else {
                // Consider invalidating the path of the ship moving onto this position recursively
                let inspired = self.state(1).inspired.contains(&ship.position);
                let action = Action::new(ship_id, Direction::Still, inspired, false, false);
                paths.insert(ship_id, VecDeque::from(vec![action]));

                self.state(1).add_ship(ship);
            }
        }
    }

    /// Prioritized actions for each unpathed ship
    pub fn unpathed_actions(&self) -> impl Iterator<Item=MergedAction> {
        // Initial actions for each not-pathed ship and the amount of possible sequential actions
        let mut initial_actions: Vec<(MergedAction, usize)> = self.unpathed.values().map(|ship| {
                let action = MergedAction::new(ship);

                let state = self.state(1);
                let num_turns = state.actions(&action, true, true).len();

                (action, num_turns)
            }).collect();

        // Sort actions by number of possible moves, followed by ship id
        initial_actions.sort_by_key(|(action, num_turns)| (*num_turns, action.ship_id.0));
        initial_actions.into_iter().map(|(action, _)| action)
    }
}
