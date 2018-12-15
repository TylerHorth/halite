use bimap::BiMap;
use std::cmp::Ordering;
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

#[derive(Eq)]
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
}

const ALL_DIRS: [Direction; 5] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
    Direction::Still,
];

#[derive(Eq)]
struct Node {
    cmd: Command,
    value: i32,
    total: i32,
}

impl Ord for Node {
    fn cmp(&self, other: &Node) -> Ordering {
        self.total.cmp(&other.total)
    }
}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Node) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Node) -> bool {
        self.total == other.total
    }
}

pub struct Navi {
    state: State,
    ships: Option<(ShipId, bool)>,
}

impl Navi {
    pub fn moves(game: &Game) -> Vec<Command> {
        let navi = Navi::from(game);

        // Finds best moves for all ships:
        //
        //  For each ship:
        //  > Consider using some heuristic to choose ship priority
        //
        //      Sort all tiles by halite effeciency (h/t)
        //      > Use cell values from final node to avoid mined cells
        //      
        //      Use some heuristics to choose N best targets to evaluate the true value of
        //
        //      Use astar to determine real value/path to each target
        //      > Maximizing heuristic is target.value / dist(target, mypos)
        //      > This is an admissible heuristic as it will allways overestimate
        //      > since:
        //      >   We've chosen the maximal halite effeciency tile
        //      >   It cannot be more efficient to mine than to move
        //      > unless:
        //      >   Our selection heuristic elemenated a good cell
        //
        //
        //
        //
        //
        //

        Vec::new()
    } 

    fn from(game: &Game) -> Navi {
        let state = State::from(game);
        let ships = state.ships
            .right_values()
            .cloned()
            .next()
            .map(|id| (id, state.cargo[&id] > 900));

        Navi {
            state,
            ships,
        }
    }


}
