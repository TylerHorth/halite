use hlt::*;
use std::cell::{RefCell, RefMut};
use state::State;
use std::collections::HashSet;
use std::collections::HashMap;
use std::collections::VecDeque;
use action::{Action, MergedAction};
use pathfinding::directed::astar::astar;
use cost::Cost;

const MAX_LOOKAHEAD: usize = 40;
const MIN_LOOKAHEAD: usize = 8;
const TARGET_DELTA: i32 = 50;

pub struct Timeline {
    timeline: RefCell<Vec<State>>,
    unpathed: HashMap<ShipId, Ship>,
    mined: HashMap<Position, usize>,
    spawn_action: MergedAction,
    constants: Constants,
}

impl Timeline {
    pub fn from(game: &Game, crashed: Vec<Position>, paths: &mut HashMap<ShipId, VecDeque<Action>>) -> Timeline {
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

                actions[i].push(Action::new(ship_id, action.dir, action.inspired, action.risk));
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
                    if state.can_apply(action) {
                        // Apply the action and mark ship seen
                        state.apply(action);
                        seen.insert(action.ship_id);

                        if action.dir == Direction::Still {
                            // Track the latest time a position was mined
                            let (pos, _) = state.ship(action.ship_id);
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

        Timeline {
            timeline: RefCell::new(timeline),
            unpathed,
            mined,
            spawn_action,
            constants,
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

    fn path<H, S>(&mut self, initial_action: MergedAction, start: usize, target: i32, inspire: bool, max_lookahead: usize, heuristic: H, success: S) -> Option<Vec<MergedAction>>
        where S: Fn(bool, bool, bool, bool) -> bool,
              H: Fn(i32, usize) -> Cost {
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
                |&(pos, t, hal)| {
                    let state = self.state(t);
                    let dist = state.calculate_distance(pos, state.nearest_dropoff(pos));

                    heuristic(hal, dist)
                },
                |&(pos, t, hal)| {
                    let state = self.state(t);
                    let turns_remaining = state.turns_remaining();
                    let at_dropoff = state.dropoffs.contains(&pos);
                    let full_halite = target + hal < TARGET_DELTA;
                    let dist = state.calculate_distance(pos, state.nearest_dropoff(pos));

                    let depth_limit = t + dist >= max_lookahead;
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

        can_afford && !taken && self.path(spawn_action, 1, target, false, MIN_LOOKAHEAD, heuristic, success).is_some()
    }

    pub fn path_ship(
        &mut self,
        initial_action: MergedAction,
        paths: &mut HashMap<ShipId, VecDeque<Action>>,
    ) {
        let ship_id = initial_action.ship_id;
        let ship = &self.unpathed.remove(&ship_id).expect(&format!("Ship already pathed {}", ship_id.0));
        let target = (self.constants.max_halite - ship.halite) as i32;
        let heuristic = |hal, dist| Cost(dist, target + hal);
        let success = |depth_limit, time_limit, full_halite, at_dropoff| {
            depth_limit || ((time_limit || full_halite) && at_dropoff)
        };

        if let Some(path) = self.path(initial_action, 0, target, true, MAX_LOOKAHEAD, heuristic, success) {
            let mut actions = VecDeque::new();

            for (i, diff) in path.iter().enumerate() {
                self.state(i).apply_merged_mut(diff);
            }

            for (i, edge) in path.windows(2).enumerate() {
                let prev = &edge[0];
                let next = &edge[1];

                let dir = self.state(0).get_dir(prev.pos, next.pos);
                let action = Action::new(next.ship_id, dir, next.inspired, next.risk);

                actions.push_back(action);
                Log::log(next.pos, format!("-ship[{}:t{}:h{}]-", ship_id.0, i, next.halite), "yellow");
            }

            if !actions.is_empty() {
                paths.insert(ship_id, actions);
            } 
        } else {
            Log::error(format!("No path found for ship {}", ship_id.0));

            let inspired = self.state(0).inspired.contains(&ship.position);
            let action = Action::new(ship_id, Direction::Still, inspired, false);
            paths.insert(ship_id, VecDeque::from(vec![action]));

            self.state(1).add_ship(ship);
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
