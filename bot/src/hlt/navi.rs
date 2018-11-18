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
    Move(ShipId, Position, Position, usize),
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
        let constants = game.constants;
        let cells = map.cells.iter().map(|column| column.iter().map(|cell| cell.halite).collect()).collect();

        let ships = BiMap::from_iter(
            me.ship_ids
                .iter()
                .map(|id| (game.ships[id].position, id.clone())),
        );

        let cargo = me.ship_ids.iter().map(|id| (id.clone(), game.ships[id].halite)).collect();

        let mut dests: HashSet<_> = me.dropoff_ids.iter().map(|id| game.dropoffs[id].position).collect();
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

    #[inline]
    fn div(num: usize, by: usize) -> usize {
        (num + by - 1) / by
    }

    fn cell(&self, position: Position) -> usize {
        self.cells[position.y as usize][position.x as usize]
    }

    /// Creates a command, does not validate if it is possible to execute
    pub fn command(&self, ship_id: ShipId, direction: Direction) -> Command {
        if direction == Direction::Still {
            let position = self.ships.get_by_right(&ship_id).unwrap();
            let amount = Self::div(self.cell(*position), 4);

            Command::Harvest(ship_id, amount)
        } else {
            let old_pos = *self.ships.get_by_right(&ship_id).unwrap();
            let new_pos = self.normalize(old_pos.directional_offset(direction));
            let cost = self.cell(old_pos) / self.constants.move_cost_ratio;

            Command::Move(ship_id, old_pos, new_pos, cost)
        }
    }

    // Only really makes sense to apply full set of commands at once...
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
    // 3) 
    //

    // pub fn apply(&mut self, command: Command) {
    // }
    //
    // pub fn unapply(&mut self, command: Command) {
    //
    // }
}

pub struct Navi {
    pub width: usize,
    pub height: usize,
    pub moves: Vec<(ShipId, Direction)>,
    pub paths: HashSet<(Position, usize)>,
}

impl Navi {
    pub fn new(width: usize, height: usize) -> Navi {
        Navi {
            width,
            height,
            moves: Vec::new(),
            paths: HashSet::new(),
        }
    }

    // A node should be the state of the world
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

    /// IDA* like search, whereby successors are all combinations
    fn search<N, usize, FS, FH>(
        path: &mut Vec<Vec<N>>,
        cost: usize,
        bound: usize,
        successors: &mut FS,
        heuristic: &mut FH,
    ) -> Option<usize>
    where
        FS: FnMut(&Vec<N>) -> Vec<Vec<N>>,
        FH: FnMut(&N) -> usize,
    {
        None
    }

    pub fn update_frame(&mut self, game: &Game) {
        // Clear state
        self.moves.clear();
        self.paths.clear();
    }
}
