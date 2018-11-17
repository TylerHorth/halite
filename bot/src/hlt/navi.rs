use hlt::direction::Direction;
use hlt::position::Position;
use hlt::ship::Ship;
use hlt::ShipId;
use hlt::game::Game;
use hlt::log::Log;
use std::collections::HashMap;
use std::collections::HashSet;

pub struct Navi {
    pub width: usize,
    pub height: usize,
    pub moving: HashMap<ShipId, (Position, Vec<Direction>)>,
    pub occupied: Vec<Vec<Option<ShipId>>>,
}

impl Navi {
    pub fn new(width: usize, height: usize) -> Navi {
        let mut occupied: Vec<Vec<Option<ShipId>>> = Vec::with_capacity(height);
        for _ in 0..height {
            occupied.push(vec![None; width]);
        }

        Navi { width, height, moving: HashMap::new(), occupied }
    }

    pub fn get(&self, position: &Position) -> Option<ShipId> {
        let position = self.normalize(position);
        self.occupied[position.y as usize][position.x as usize]
    }

    pub fn update_frame(&mut self, game: &Game) {
        self.clear();

        for player in &game.players {
            for ship_id in &player.ship_ids {
                let ship = &game.ships[ship_id];
                self.mark_unsafe_ship(&ship);
            }
        }
    }

    pub fn clear(&mut self) {
        self.moving.clear();
        for y in 0..self.height {
            for x in 0..self.width {
                self.occupied[y][x] = None;
            }
        }
    }

    pub fn is_safe(&self, position: &Position) -> bool {
        let position = self.normalize(position);
        self.occupied[position.y as usize][position.x as usize].is_none()
    }

    pub fn is_unsafe(&self, position: &Position) -> bool {
        !self.is_safe(position)
    }

    pub fn mark_unsafe(&mut self, position: &Position, ship_id: ShipId) {
        let position = self.normalize(position);
        self.occupied[position.y as usize][position.x as usize] = Some(ship_id);
    }

    pub fn mark_safe(&mut self, position: &Position) {
        let position = self.normalize(position);
        self.occupied[position.y as usize][position.x as usize] = None;
    }

    pub fn mark_unsafe_ship(&mut self, ship: &Ship) {
        self.mark_unsafe(&ship.position, ship.id);
    }

    pub fn get_unsafe_moves(&self, source: &Position, destination: &Position) -> Vec<Direction> {
        let normalized_source = self.normalize(source);
        let normalized_destination = self.normalize(destination);

        let dx = (normalized_source.x - normalized_destination.x).abs() as usize;
        let dy = (normalized_source.y - normalized_destination.y).abs() as usize;

        let wrapped_dx = self.width - dx;
        let wrapped_dy = self.height - dy;

        let mut possible_moves: Vec<Direction> = Vec::new();

        if normalized_source.x < normalized_destination.x {
            possible_moves.push(if dx > wrapped_dx { Direction::West } else { Direction::East });
        } else if normalized_source.x > normalized_destination.x {
            possible_moves.push(if dx < wrapped_dx { Direction::West } else { Direction::East });
        }

        if normalized_source.y < normalized_destination.y {
            possible_moves.push(if dy > wrapped_dy { Direction::North } else { Direction::South });
        } else if normalized_source.y > normalized_destination.y {
            possible_moves.push(if dy < wrapped_dy { Direction::North } else { Direction::South });
        }

        possible_moves
    }

    pub fn naive_navigate(&mut self, ship: &Ship, destination: &Position) {
        let ship_position = ship.position;

        // get_unsafe_moves normalizes for us
        let directions = self.get_unsafe_moves(&ship_position, destination);

        self.moving.insert(ship.id, (ship_position, directions));
    }

    pub fn move_ship(&mut self, ship_id: ShipId, old: Position, new: Position) {
        self.mark_safe(&old);
        self.mark_unsafe(&new, ship_id);
    }

    pub fn swap_ships(
        &mut self,
        (pos1, ship1): (Position, ShipId),
        (pos2, ship2): (Position, ShipId),
    ) {
        self.mark_unsafe(&pos1, ship2);
        self.mark_unsafe(&pos2, ship1);
    }

    pub fn signal_move(
        &mut self,
        ship_id: ShipId,
        moves: &mut Vec<(ShipId, Direction)>,
        signals: &mut HashMap<Position, HashSet<ShipId>>,
    ) {
        let yellow = "#e2f442";

        // If we want to move
        if let Some((position, directions)) = self.moving.remove(&ship_id) {
            // For each potential movement direction
            for dir in directions {
                let new_pos = self.normalize(&position.directional_offset(dir));

                // Ship at target position
                if let Some(unsafe_ship) = self.get(&new_pos) {
                    // Ship wants to swap if they signal for my position
                    if signals.get_mut(&position).map(|ships| ships.remove(&unsafe_ship)).unwrap_or_default() {
                        self.swap_ships((position, ship_id), (new_pos, unsafe_ship));
                        Log::log(position, "_swap1_", yellow);
                        moves.push((ship_id, dir));
                        moves.push((unsafe_ship, dir.invert_direction()));
                        return
                    }

                    // Ship doesn't want to swap, so signal them to move or swap
                    signals.entry(new_pos) 
                        .and_modify(|ships| {
                            ships.insert(ship_id); 
                        }).or_insert_with(|| {
                            let mut ships = HashSet::new();
                            ships.insert(ship_id);
                            ships
                        });
                    self.signal_move(unsafe_ship, moves, signals);

                    // If we swapped, return
                    if self.get(&new_pos) == Some(ship_id) {
                        Log::log(position, "_swap2_", yellow);
                        return
                    }
                }

                // Its safe to move, so move
                if self.is_safe(&new_pos) {
                    self.move_ship(ship_id, position, new_pos);
                    moves.push((ship_id, dir));
                    return
                }
            }

            // No possible move :(
            Log::log(position, "_nomov_", yellow);
            moves.push((ship_id, Direction::Still));
        }
    }


    pub fn collect_moves(&mut self) -> Vec<(ShipId, Direction)> {
        let mut moves: Vec<(ShipId, Direction)> = Vec::new();
        let ships: Vec<ShipId> = self.moving.keys().cloned().collect();

        for ship_id in ships {
            self.signal_move(ship_id, &mut moves, &mut HashMap::new());
        }

        moves
    }

    pub fn normalize(&self, position: &Position) -> Position {
        let width = self.width as i32;
        let height = self.height as i32;
        let x = ((position.x % width) + width) % width;
        let y = ((position.y % height) + height) % height;
        Position { x, y }
    }
}
