use bimap::BiMap;
use hlt::colors::*;
use hlt::direction::Direction;
use hlt::dropoff::Dropoff;
use hlt::game::Game;
use hlt::game_map::GameMap;
use hlt::log::Log;
use hlt::position::Position;
use hlt::ship::Ship;
use hlt::ShipId;
use hlt::constants::Constants;
use std::collections::HashMap;
use std::collections::HashSet;
use std::iter::FromIterator;

enum Command {
    Harvest(ShipId, usize),
    Move(ShipId, Direction, usize),
}

impl Command {
    pub fn value(&self) -> i32 {
        match self {
            &Command::Harvest(.., v) => v as i32,
            &Command::Move(.., c) => -(c as i32),
        }
    }
}

struct State {
    width: usize,
    height: usize,
    constants: Constants,
    cells: Vec<Vec<usize>>,
    ships: BiMap<Position, ShipId>,
    cargo: HashMap<ShipId, usize>,
    dests: HashSet<Position>,
}

impl State {
    pub fn from(game: &Game) -> State {
        let me = &game.players[game.my_id.0];
        let map = &game.map;

        let width = map.width;
        let height = map.height;
        let constants = game.constants.clone();
        let cells = map.cells
            .iter()
            .map(|column| column.iter().map(|cell| cell.halite).collect())
            .collect();

        let ships = BiMap::from_iter(
            me.ship_ids
                .iter()
                .map(|id| (game.ships[id].position, id.clone())),
        );

        let cargo = me.ship_ids
            .iter()
            .map(|id| (id.clone(), game.ships[id].halite))
            .collect();

        let mut dests: HashSet<_> = me.dropoff_ids
            .iter()
            .map(|id| game.dropoffs[id].position)
            .collect();

        dests.insert(me.shipyard.position);

        State {
            width,
            height,
            constants,
            cells,
            ships,
            cargo,
            dests,
        }
    }

    fn normalize(&self, position: Position) -> Position {
        let width = self.width as i32;
        let height = self.height as i32;
        let x = ((position.x % width) + width) % width;
        let y = ((position.y % height) + height) % height;
        Position { x, y }
    }

    fn distance(a: Position, b: Position) -> i32 {
        (a.x - b.x).abs() + (a.y - b.y).abs()
    }

    #[inline]
    fn div(num: usize, by: usize) -> usize {
        (num + by - 1) / by
    }

    fn cell(&self, position: Position) -> usize {
        self.cells[position.y as usize][position.x as usize]
    }

    fn cell_mut(&mut self, position: Position) -> &mut usize {
        &mut self.cells[position.y as usize][position.x as usize]
    }

    /// Creates a command, does not validate if it is possible to execute
    pub fn command(&self, ship_id: ShipId, direction: Direction) -> Command {
        if direction == Direction::Still {
            let position = self.ships.get_by_right(&ship_id).unwrap();
            let space = self.constants.max_halite - self.cargo[&ship_id];
            let amount = Self::div(self.cell(*position), self.constants.extract_ratio);

            Command::Harvest(ship_id, amount.min(space))
        } else {
            let old_pos = *self.ships.get_by_right(&ship_id).unwrap();
            let cost = self.cell(old_pos) / self.constants.move_cost_ratio;

            Command::Move(ship_id, direction, cost)
        }
    }

    pub fn can_apply(&self, cmd: &Command) -> bool {
        match cmd {
            &Command::Move(ship_id, _, cost) => cost <= self.cargo[&ship_id],
            _ => true,
        }
    }

    pub fn apply(&mut self, cmd: &Command) {
        match cmd {
            &Command::Harvest(ship_id, amount) => {
                *self.cargo.get_mut(&ship_id).unwrap() += amount;
                let pos = self.ships.get_by_right(&ship_id).unwrap().clone();
                *self.cell_mut(pos) -= amount;
            }
            &Command::Move(ship_id, dir, cost) => {
                *self.cargo.get_mut(&ship_id).unwrap() -= cost;
                let old_pos = self.ships.get_by_right(&ship_id).unwrap().clone();
                let new_pos = self.normalize(old_pos.directional_offset(dir));
                self.ships.insert(new_pos, ship_id);
            }
        }
    }

    pub fn unapply(&mut self, cmd: &Command) {
        match cmd {
            &Command::Harvest(ship_id, amount) => {
                *self.cargo.get_mut(&ship_id).unwrap() -= amount;
                let pos = self.ships.get_by_right(&ship_id).unwrap().clone();
                *self.cell_mut(pos) += amount;
            }
            &Command::Move(ship_id, dir, cost) => {
                *self.cargo.get_mut(&ship_id).unwrap() += cost;
                let new_pos = self.ships.get_by_right(&ship_id).unwrap().clone();
                let old_pos = self.normalize(new_pos.directional_offset(dir.invert_direction()));
                self.ships.insert(old_pos, ship_id);
            }
        }
    }

    pub fn heuristic(&self, cmd: &Command) -> i32 {
        match cmd {
            &Command::Move(ship_id, dir, _) => {
                let old_pos = self.ships.get_by_right(&ship_id).unwrap().clone();
                let new_pos = self.normalize(old_pos.directional_offset(dir));

                let dist = |pos| self.dests
                    .iter()
                    .map(|&dest| State::distance(pos, dest))
                    .min()
                    .unwrap();

                let old_dist = dist(old_pos);
                let new_dist = dist(new_pos);

                let cargo = self.cargo[&ship_id];
                if cargo > 900 {
                    old_dist - new_dist
                } else {
                    new_dist - old_dist
                }
            }
            &Command::Harvest(..) => -25
        }
    }

    // Only really makes sense to apply full set of commands at once...{{{
    // But we want to take advantage of the property:
    // > value(a + b + c) ~= value(a) + value(b) + value(c)
    // in order to speed up computation...
    //
    // We can likewise think of a value of INT_MIN as an invalid move
    // > valid(a + b + c) != valid(a) + valid(b) + valid(c), clearly
    // therefore the privous property for value doesn't hold
    //
    // valid(a + b + c) is likely to be true. In the case that it isn't,
    // how do we speed up the process of finding the next best move set?
    //
    // > Reminder, (a, b, c) were chosen since they were the best moves
    // for each of the three ships, irespective of the others.
    // i.e. based on a heuristic which considers the other ships current
    //      position, but doesn't know their future position. 
    //      (Maybe simply encourages ships to keep their distance, but 
    //      this would potentially discourage swapping positions).
    //
    // This question is equivalent to: if the base ordering by the 
    // heuristic orders by the sum of the individuals, how do we efficiently
    // select the next best candidate if the true cost turns out to be bad?
    //
    // Three cases this should work for:
    // 1) Heuristic suggests an invalid move. Simple case, value(a) sufficient
    // 2) Move is valueable independenly, but not with the group. 
    //    i.e. Causes a collision
    // 3) Move is generally not too good. i.e. blocks another ship from returning
    //
    // Problem statement:
    //   Assuming we have a function heuristic(a) where a is a move for 1 ship
    //   Find an algorithm which produces a ordered set of moves (a, b, ...)
    //
    //   Using the heuristic we can get [a1, a2, ...], [b1, b2, ...], ...
    //   From this, we want (a, b, ...), (a, b, ...) ordered by value + heuristic
    //
    //   heuristic(a, b, ...) is equal to heuristic(a) + heuristic(b) + ...
    //   So (a1, b1, ...) will be ideal if value is similar 
    //
    //   (Value being defined as halite mined - cost of bad moves,
    //   crashing, invalid moves, etc)
    //
    //  

    //  --- Pause
    // Let's just start out with one ship...
    // Then we can move onto multiple ships, pathing one at a time...
    // Finally we'll move onto collaborative pathing, using a product graph
    //
    //


    // pub fn apply(&mut self, command: Command) {
    // }
    //
    // pub fn unapply(&mut self, command: Command) {
    //
    // }}}}
}

const ALL_DIRS: [Direction; 5] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
    Direction::Still,
];

pub struct Navi {
    state: State,
    ship_id: Option<ShipId>,
}

impl Navi {
    pub fn from(game: &Game) -> Navi {
        let state = State::from(game);
        let ship_id = state.ships.right_values().cloned().next();

        Navi {
            state,
            ship_id,
        }
    }

    fn neighbours(&self) -> Vec<(Command, i32, i32)> {
        let ship_id = self.ship_id.unwrap();
        let mut cmds: Vec<_> = ALL_DIRS
            .iter()
            .map(|dir| self.state.command(ship_id, *dir))
            .filter(|cmd| self.state.can_apply(&cmd))
            .map(|cmd| {
                let v = cmd.value();
                let h = v + self.state.heuristic(&cmd);
                (cmd, v, h)
            })
            .collect();

        cmds.sort_unstable_by_key(|(.., h)| *h);

        cmds
    }

    // A node should be the state of the world{{{
    //
    // Successors returns a vec of vecs of commands
    // > Each vec corresponding to the set of commands each ship can execute, unfiltered
    // commands mutate the state, and should be reversable
    //
    // A path is then a vec of vecs of commands
    // Each turn, we compute a path and apply the first vec of commands
    //
    // Given a state, we can compute the value of applying a command
    // Value is different from a heuristic
    // Value is absolute, a heuristic is speculative
    //
    // Why have both concepts? A heuristic lets you order which paths visit first
    // Whereas cost is the soure of truth
    //
    // Ex.  Minimize cost. Costs to burn fuel. Profits to harvest. But ships have a
    //      maximum capacity.
    //
    //      - Harvest   -> +value
    //      - Move      -> -fuel
    //      - Collision?
    //          - Removes the number of ships, which transitively affects cost
    //          - Does not directly affect cost, but has a bad heuristic
    //              - Could be as simple as the cost of the two ships
    //              - Or could factor in amount of halite left on the board
    //                and the amount of ships left so that in the late game
    //                ships are more likely to be sacrificed
    //
//}}}
}
