use hlt::direction::Direction;
use hlt::position::Position;
use hlt::ship::Ship;
use hlt::ShipId;
use hlt::game::Game;
use std::collections::HashMap;
use hlt::log::Log;

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

    pub fn move_ship(&mut self, ship_id: ShipId, moves: &mut Vec<(ShipId, Direction)>) {
        let blue = "#0000FF";
        let green = "#00FF00";

        if let Some((position, directions)) = self.moving.remove(&ship_id) {
            for dir in directions {
                Log::log(position, format!("-> {}", dir.get_char_encoding()), green);
                
                let new_pos = self.normalize(&position.directional_offset(dir));

                if let Some(ship_at_dest) = self.occupied[new_pos.y as usize][new_pos.x as usize] {
                    // Some bullship to subvert the borrow checker smh... cant wait for NLL
                    let other_ship_moving = if self.moving.contains_key(&ship_at_dest) {
                        Some(self.moving[&ship_at_dest].clone())
                    } else {
                        None
                    };

                    if let Some((_, other_dirs)) = other_ship_moving {
                        for odir in other_dirs {
                            let opos = self.normalize(&new_pos.directional_offset(odir.clone()));

                            // Ship at target wants to move to our position
                            if opos == position {
                                Log::log(position, "swap", blue);
                                Log::log(new_pos, "swap", blue);

                                moves.push((ship_id, dir));
                                moves.push((ship_at_dest, odir.clone()));
                                return
                            }
                        }

                        self.move_ship(ship_id, moves);
                    } else {
                        continue
                    }
                }

                if self.is_safe(&new_pos) {
                    self.mark_unsafe(&new_pos, ship_id);
                    self.mark_safe(&position);
                    moves.push((ship_id, dir));
                    return
                }
            }

            // No possible move :(
            Log::log(position, format!("{}:{} <still>", file!(), line!()), "#FF0000");
            moves.push((ship_id, Direction::Still));
        }
    }

    pub fn collect_moves(&mut self) -> Vec<(ShipId, Direction)> {
        let mut moves: Vec<(ShipId, Direction)> = Vec::new();
        let ships: Vec<ShipId> = self.moving.keys().cloned().collect();

        for ship_id in ships {
            self.move_ship(ship_id, &mut moves);
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
