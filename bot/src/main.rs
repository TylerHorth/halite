#[macro_use]
extern crate lazy_static;

use hlt::command::Command;
use hlt::game::Game;
use hlt::game_map::GameMap;
use hlt::navi::Navi;
use hlt::ShipId;
use hlt::log::Log;
use hlt::position::Position;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;

mod hlt;

fn find_target(pos: Position, map: &GameMap, target: usize, taken: &HashSet<Position>) -> Option<Position> {
    map.iter()
        .filter(|cell| cell.halite > target)
        .filter(|cell| !taken.contains(&cell.position))
        .min_by_key(|cell| map.calculate_distance(&pos, &cell.position))
        .map(|cell| cell.position.clone())
}

fn main() {
    let mut game = Game::new();
    let mut navi = Navi::new(game.map.width, game.map.height);
    // At this point "game" variable is populated with initial map data.
    // This is a good place to do computationally expensive start-up pre-processing.
    // As soon as you call "ready" function below, the 2 second per turn timer will start.
    Game::ready("bugs");

    let mut targets: HashMap<ShipId, Position> = HashMap::new();

    loop {
        game.update_frame();
        navi.update_frame(&game);

        let me = &game.players[game.my_id.0];
        let map = &game.map;

        let mut command_queue: Vec<Command> = Vec::new();

        // Remove ships which have reached their target
        targets.retain(|ship_id, &mut target| { 
            if let Some(ship) = game.ships.get(&ship_id) {
                ship.position != target
            } else {
                false
            }
        });

        for ship_id in me.ship_ids.iter().cloned() {
            let ship = &game.ships[&ship_id];
            let cell = map.at_entity(ship);

            let taken: HashSet<Position> = targets.values().cloned().collect();

            let can_move = ship.halite >= cell.halite / 10;

            match targets.entry(ship_id) {
                Entry::Occupied(target) => {
                    // We have a target, and we haven't reached it yet
                    if cell.halite < game.constants.max_halite / 10 || ship.halite as f32 > 0.9 * game.constants.max_halite as f32 {
                        if can_move {
                            navi.naive_navigate(&ship, target.get());
                        } else {
                            Log::log(ship.position, format!("{}:{} still", file!(), line!()), "#FF0000");
                            command_queue.push(ship.stay_still());
                        }
                    } else {
                        Log::log(ship.position, format!("{}:{} still", file!(), line!()), "#FF0000");
                        command_queue.push(ship.stay_still());
                    }
                }
                Entry::Vacant(target) => 
                    if ship.halite as f32 > 0.9 * game.constants.max_halite as f32 {
                        let target_pos = me.shipyard.position;

                        target.insert(target_pos);
                        if can_move {
                            navi.naive_navigate(&ship, &target_pos);
                        } else {
                            Log::log(ship.position, format!("{}:{} still", file!(), line!()), "#FF0000");
                            command_queue.push(ship.stay_still());
                        }
                    } else if cell.halite < game.constants.max_halite / 10 {
                        let target_val = game.constants.max_halite / 10;

                        if let Some(pos) = find_target(ship.position, &map, target_val, &taken) {
                            if can_move {
                                navi.naive_navigate(&ship, &pos);
                            } else {
                                Log::log(ship.position, format!("{}:{} still", file!(), line!()), "#FF0000");
                                command_queue.push(ship.stay_still());
                            }
                            target.insert(pos);
                        } else {
                            Log::log(ship.position, format!("{}:{} still", file!(), line!()), "#FF0000");
                            command_queue.push(ship.stay_still());
                        }
                    } else {
                        Log::log(ship.position, format!("{}:{} still", file!(), line!()), "#FF0000");
                        command_queue.push(ship.stay_still());
                    }
            };


            for (ship_id, dir) in navi.collect_moves() {
                Log::log(game.ships[&ship_id].position, format!("move {}", dir.get_char_encoding()), "#00FFFF");
                command_queue.push(Command::move_ship(ship_id, dir));
            }
        }

        if game.turn_number <= 200 &&
            me.halite >= game.constants.ship_cost &&
                navi.get(&me.shipyard.position).map_or(true, |ship| game.ships[&ship].owner != game.my_id)
        {
            command_queue.push(me.shipyard.spawn());
        }

        Game::end_turn(&command_queue);
    }
}
