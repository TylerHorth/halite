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
const MIN_LOOKAHEAD: usize = 20;
const MIN_DROPOFF_DIST: usize = 16;
const MAX_DROPOFF_DIST: usize = 22;
const KERNEL_SIZE: i32 = 30;
const TARGET_DELTA: i32 = 70;
const SHIP_DIST_RATIO: usize = 4;
const SHIP_DROPOFF_RATIO: usize = 15;
const PATH_TIMEOUT: usize = 16;
const SCALE_FACTOR: f64 = SHIP_DIST_RATIO as f64 * 20.0;

fn sig(total: usize, f: usize, scale: f64) -> usize {
    let factor = -1.0 / (1.0 + (4.0 * (1.0 - f as f64 / scale)).exp()) + 1.0;
    let res = total as f64 * factor;
    res as usize
}

pub struct Timeline {
    timeline: RefCell<Vec<State>>,
    unpathed: Vec<(MergedAction, usize)>,
    mined: HashMap<Position, usize>,
    target_dropoffs: HashMap<ShipId, (Position, usize)>,
    spawn_action: MergedAction,
    constants: Constants,
    nav: Navi,
    prime: Option<Position>,
    save: usize,
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

        let mut nav = Navi::new(game.map.width, game.map.height);
        nav.update_frame(game);

        // Add each ship to initial state
        let mut state = State::from(&game);
        for ship_id in paths.keys() {
            let ship = &game.ships[ship_id];
            state.add_ship(ship);
        }

        // Transform path into set of actions at each timestep
        let mut actions: Vec<Vec<Action>> = Vec::new();
        for (&_ship_id, path) in paths.iter() {
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
        let mut save = 0;

        // Create timeline states from sequence of actions
        for (i, step) in actions.into_iter().enumerate() {
            // Initialize next state
            let mut state = timeline[i].next();

            // Remove ships which completed their path
            for ship_id in rm_next.drain(..) {
                state.rm_ship(ship_id);
            }

            let mut seen = HashSet::new();
            for action in step {
                if !poisoned.contains_key(&action.ship_id) {
                    let make_dropoff = paths[&action.ship_id].back().map(|a| a.dropoff).unwrap_or(false);
                    let complete = paths[&action.ship_id].len() <= PATH_TIMEOUT + PATH_TIMEOUT / 2;

                    if (i < PATH_TIMEOUT || make_dropoff || complete) && state.can_apply(action) {
                        let (pos, _) = state.ship(action.ship_id);
                        let halite_before = state.halite;

                        // Apply the action and mark ship seen
                        state.apply(action);
                        seen.insert(action.ship_id);

                        if action.dropoff {
                            save += halite_before - state.halite;
                            building.insert(pos);
                        } else if action.dir == Direction::Still {
                            // Track the latest time a position was mined
                            mined.insert(pos, i);
                        }
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

        for ship_id in &me.ship_ids {
            let ship = &game.ships[ship_id];
            timeline[0].add_ship(ship);
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

        let mut unpathed = Vec::new();
        for ship_id in ship_ids {
            if !paths.contains_key(&ship_id) {
                let ship = &game.ships[&ship_id];
                let action = MergedAction::new(ship_id, ship.position, ship.halite);
                unpathed.push((action, 0));
            } else {
                let len = paths[&ship_id].len();
                if len <= PATH_TIMEOUT / 2 {
                    if timeline[len].ships.contains_key(&ship_id) {
                        let (pos, hal) = timeline[len].ship(ship_id);
                        let action = MergedAction::new(ship_id, pos, hal);
                        unpathed.push((action, len));
                    } else {
                        Log::info(format!("ShipDel:{}", ship_id.0));
                    }
                }
            }
        }

        unpathed.sort_by_key(|u| u.1);

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

        let mut max_prime: Option<(Position, usize)> = None;
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
                        max_prime = max_prime.map(|prev| if rate > prev.1 { (pos, rate) } else { prev }).or(Some((pos, rate)));
                        Log::log(pos, format!("_r{}_", rate), "fuchsia");
                    }
                }
            }
        }

        let prime = max_prime.map(|m| m.0);

        if prime.is_some() {
            save += constants.dropoff_cost;
        }

        let rows = ship_ids.len();
        let columns = dropoffs.len() * ship_ids.len();

        let mut matrix_vec = Vec::with_capacity(rows * columns);
        for &ship_id in &ship_ids {
            let ship_pos = game.ships[&ship_id].position;
            
            for &(pos, t) in &dropoffs {
                let dist = game.map.calculate_distance(&ship_pos, &pos).max(t);
                let dropoff_value = richness[&pos];

                for num_ships in 0..ship_ids.len() {
                    let value = sig(dropoff_value, dist + num_ships * SHIP_DIST_RATIO, SCALE_FACTOR);
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
            nav,
            prime,
            save,
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

    pub fn target_pos_t(&self, ship_id: ShipId, pos: Position) -> (Position, usize) {
        let state = self.state(0);
        let nearest = (state.nearest_dropoff(pos), 0);
        if state.end_game() {
            nearest
        } else {
            self.target_dropoffs.get(&ship_id).cloned().unwrap_or(nearest)
        }
    }

    fn path(
        &mut self,
        initial_action: MergedAction,
        start: usize,
        target: (Position, usize),
        max_lookahead: usize,
    ) -> Option<Vec<MergedAction>> {
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

                        if key.1 < max_lookahead {
                            let already_mined = self.mined.get(&key.0).map(|&t| key.1 <= t).unwrap_or(false);
                            state.actions(parent, already_mined)
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
                    let dist = state.calculate_distance(pos, target.0).max(target.1);

                    Cost(dist, state.constants.max_halite as i32 + hal)
                },
                |&(pos, t, _)| {
                    let merged = merged.borrow();
                    let action = &merged[&(pos, t)];

                    let state = self.state(t).apply_merged(action);

                    let turns_remaining = state.turns_remaining();
                    let at_target = pos == target.0 && t >= target.1;
                    let hal = action.halite + action.returned;
                    let full_halite = hal as i32 + TARGET_DELTA >= state.constants.max_halite as i32;

                    let depth_limit = t >= max_lookahead;
                    let time_limit = t >= turns_remaining;

                    depth_limit || ((time_limit || full_halite) && at_target)
                }
            );

        let mut merged = merged.into_inner();
        path.map(|(path, _)| path.into_iter().map(|(pos, t, _)| merged.remove(&(pos, t)).unwrap()).collect())
    }

    pub fn spawn_ship(&mut self) -> bool {
        let spawn_action = self.spawn_action.clone();
        let taken = self.state(1).taken.contains_key(&spawn_action.pos);
        let can_afford = self.state(0).halite >= self.constants.ship_cost + self.save;
        let target = (spawn_action.pos, 0);

        can_afford && !taken && self.path(spawn_action, 1, target, MIN_LOOKAHEAD).is_some()
    }

    fn earliest_build(&self, pos: Position) -> usize {
        let max_ship = self.constants.max_halite - TARGET_DELTA as usize;
        for (i, state) in self.timeline.borrow().iter().enumerate() {
            if state.halite + state.halite(pos) + max_ship >= self.constants.dropoff_cost {
                return i;
            }
        }

        return MAX_LOOKAHEAD + 1;
    }

    pub fn make_dropoff(&mut self, paths: &mut HashMap<ShipId, VecDeque<Action>>) {
        if self.unpathed.len() == 0 {
            return;
        }

        let target = match self.prime {
            Some(p) => p,
            _ => return,
        };
        let t = self.earliest_build(target);

        let action_index = self.unpathed.iter()
            .enumerate()
            .min_by_key(|(_, (action, start))| {
                let dist = self.state(0).calculate_distance(action.pos, target);
                let tdist = if start > &t {
                    start - t
                } else { 0 };

                dist + tdist
            }).map(|ia| ia.0).expect("Could not find ship to build dropoff with");

        let (initial_action, start) = self.unpathed.remove(action_index);
        let ship_id = initial_action.ship_id;

        if let Some(path) = self.path(initial_action, start, (target, 0), MAX_LOOKAHEAD) {
            if path.is_empty() {
                return;
            }

            let (i, end) = path.iter().enumerate().last().unwrap();

            if end.pos != target {
                return;
            }

            let (total_halite, ship_halite, tile_halite) = {
                let state = self.state(start + i);
                (state.halite, end.halite, state.halite(target))
            };

            let can_afford = start + i >= t && total_halite + ship_halite + tile_halite >= self.constants.dropoff_cost;
            if !can_afford {
                return;
            }

            self.save = self.constants.dropoff_cost - ship_halite - tile_halite;

            for (i, diff) in path.iter().enumerate() {
                self.state(start + i).apply_merged_mut(diff);
            }

            {
                let dropoff_state = &mut self.state(start + i + 1);
                dropoff_state.apply_merged_mut(end);
                dropoff_state.make_dropoff(ship_id);
            }

            let mut actions = VecDeque::new();

            for (i, edge) in path.windows(2).enumerate() {
                let prev = &edge[0];
                let next = &edge[1];

                let dir = self.state(0).get_dir(prev.pos, next.pos);
                let action = Action::new(next.ship_id, dir, next.inspired, next.risk, false);

                actions.push_back(action);
                Log::log(next.pos, format!("-ship[{}:t{}:h{}]-", ship_id.0, start + i, next.halite), "yellow");
            }

            assert!(!actions.is_empty());

            let mut last = actions[actions.len() - 1].clone();
            last.dir = Direction::Still;
            last.dropoff = true;
            actions.push_back(last);
            Log::info(format!("_md{}_", last.ship_id.0));

            if !paths.contains_key(&ship_id) {
                paths.insert(ship_id, actions);
            } else {
                let path = paths.get_mut(&ship_id).unwrap();
                assert_eq!(path.len(), start);
                path.extend(actions);
            }
        }
    }

    pub fn path_ships(&mut self, paths: &mut HashMap<ShipId, VecDeque<Action>>) -> Vec<Command> {
        let mut command_queue = Vec::new();

        let unpathed: Vec<_> = self.unpathed.drain(..).collect();
        for (action, start) in unpathed {
            self.path_ship(action, start, paths);
        }

        let ships = self.state(0).ships.clone();
        for &(ship_id, (pos, hal)) in &ships {
            if !paths.contains_key(&ship_id) {
                let target = self.target_dropoffs[&ship_id].0;
                let can_move = hal >= self.state(0).halite(pos) / self.constants.move_cost_ratio;
                if can_move {
                    if pos == target {
                        self.nav.move_away(ship_id, pos);
                    } else {
                        self.nav.naive_navigate(ship_id, pos, target);
                    }
                }
            }
        }

        let mut expected = HashMap::new();
        for (&ship_id, path) in paths.iter_mut() {
            let action = path.pop_front().expect("Empty path");
            let pos = self.state(0).ship(ship_id).0;
            if action.dropoff {
                command_queue.push(Command::transform_ship_into_dropoff_site(ship_id))
            } else {
                expected.insert(ship_id, Command::move_ship(ship_id, action.dir));
                self.nav.nav(ship_id, pos, action.dir);
            }
        }

        let end_game = self.state(0).end_game();
        self.nav.terminal = end_game;

        for (ship_id, dir) in self.nav.collect_moves() {
            let command = Command::move_ship(ship_id, dir);
            if let Some(expected) = expected.remove(&ship_id) {
                if expected != command {
                    Log::warn(format!("Blocked:{}", ship_id.0));
                    paths.remove(&ship_id);
                }
            }

            command_queue.push(command);
        }

        command_queue
    }

    fn path_ship(
        &mut self,
        initial_action: MergedAction,
        start: usize,
        paths: &mut HashMap<ShipId, VecDeque<Action>>,
    ) {
        let ship_id = initial_action.ship_id;
        let ship_pos = initial_action.pos;
        let target = self.target_pos_t(ship_id, ship_pos);

        if let Some(path) = self.path(initial_action.clone(), start, target, MAX_LOOKAHEAD) {
            let mut actions = VecDeque::new();

            for (i, diff) in path.iter().enumerate() {
                self.state(i + start).apply_merged_mut(diff);
            }

            for (i, edge) in path.windows(2).enumerate() {
                let prev = &edge[0];
                let next = &edge[1];

                let dir = self.state(0).get_dir(prev.pos, next.pos);
                let action = Action::new(next.ship_id, dir, next.inspired, next.risk, false);

                actions.push_back(action);
                Log::log(next.pos, format!("-ship[{}:t{}:h{}]-", ship_id.0, i + start, next.halite), "yellow");
            }

            if !actions.is_empty() {
                if !paths.contains_key(&ship_id) {
                    paths.insert(ship_id, actions);
                } else {
                    let path = paths.get_mut(&ship_id).unwrap();
                    assert_eq!(path.len(), start);
                    path.extend(actions);
                }
            } 
        } else {
            Log::error(format!("No path found for ship {}", ship_id.0));
        }
    }
}
