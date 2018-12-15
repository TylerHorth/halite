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

mod hlt;

fn find_target(pos: Position, map: &GameMap, target: usize, taken: &HashSet<Position>) -> Option<Position> {
    map.iter()
        // Cell has target halite
        .filter(|cell| cell.halite >= target)
        // Cell isn't already targeted by another ship
        .filter(|cell| !taken.contains(&cell.position))
        // Cell is not at current position (sanity)
        .filter(|cell| cell.position != pos)
        // Get closest
        .max_by_key(|cell| {
            let dist = map.calculate_distance(&pos, &cell.position) as f64;
            let hal = cell.halite as f64;

            (hal / (dist * dist)) as u64
        })
        .map(|cell| cell.position.clone())
}

fn main() {
    let mut game = Game::new();
    let mut navi = Navi::new(game.map.width, game.map.height);

    // Constants
    let mut target_halite = game.constants.max_halite / 10;
    let mut ship_full = 9 * target_halite;

    // Colors
    let red = "#FF0000";
    let purple = "#9d00ff";
    let orange = "#ff8800";
    let green = "#00ff48";
    let teal = "#42f4ee";
    let pink = "#ff00dc";

    let total_halite: usize = game.map.iter().map(|cell| cell.halite).sum();

    Game::ready("downside");

    let mut targets: HashMap<ShipId, Position> = HashMap::new();
    let mut terminal = false;

    loop {
        game.update_frame();
        navi.update_frame(&game);

        let me = &game.players[game.my_id.0];
        let map = &game.map;

        let mut command_queue: Vec<Command> = Vec::new();

        terminal = terminal || me.ship_ids
            .iter()
            .cloned()
            .any(|ship_id| {
                let ship = &game.ships[&ship_id];
                let dist = map.calculate_distance(&ship.position, &me.shipyard.position);
                let turns_remaining = game.constants.max_turns - game.turn_number;

                dist + 15 > turns_remaining
            });

        if terminal {
            Log::log(me.shipyard.position, "_terminal_", red);

            // Move towards target
            for ship_id in me.ship_ids.iter() {
                let ship = &game.ships[ship_id];
                let cell = map.at_entity(ship);

                // If we have enough halite to move
                if ship.halite >= cell.halite / 10 {
                    let mut unsafe_moves = navi.get_unsafe_moves(&ship.position, &me.shipyard.position).into_iter();
                    let command = loop {
                        if let Some(dir) = unsafe_moves.next() {
                            let target_pos = ship.position.directional_offset(dir);

                            if navi.is_safe(&target_pos) || target_pos == me.shipyard.position {
                                Log::log(ship.position, format!("_term_{}_", dir.get_char_encoding()), pink);
                                navi.mark_unsafe(&target_pos, ship.id);
                                break Some(ship.move_ship(dir));
                            }
                        } else {
                            break None;
                        }
                    };

                    if let Some(command) = command {
                        command_queue.push(command);
                    } else {
                        Log::log(ship.position, "_frozen_", teal);
                        command_queue.push(ship.stay_still());
                    }
                } else {
                    Log::log(ship.position, "_fuel_", orange);
                    command_queue.push(ship.stay_still());
                }
            }

            game.end_turn(&command_queue);

            continue
        }

        // Remove ships which have reached and mined their target
        targets.retain(|ship_id, &mut target| { 
            game.ships
                .get(&ship_id)
                .map(|ship| ship.position != target || (map.at_entity(ship).halite >= target_halite / 2 && ship.halite < ship_full)) 
                .unwrap_or_default()
        });

        // Add targets for ships that don't have one
        // If can't find target, stay still and log
        for ship_id in me.ship_ids.iter().cloned() {
            let ship = &game.ships[&ship_id];
            let taken: HashSet<Position> = targets.values().cloned().collect();

            if !targets.contains_key(&ship_id) {
                // If ship is full, return to base
                let target = if ship.halite >= ship_full {
                    Some(me.shipyard.position)
                // Otherwise, find a new cell
                } else {
                    // If cell not worth mining, find new cell
                    let mut target = find_target(ship.position, &map, target_halite, &taken);
                    while target.is_none() {
                        target_halite = target_halite / 2;
                        ship_full = 8 * target_halite;

                        Log::log(ship.position, format!("_tar_{}", target_halite), red);

                        target = find_target(ship.position, &map, target_halite, &taken);
                    }

                    target
                };

                if let Some(target) = target {
                    targets.insert(ship_id, target); 
                } else {
                    // Couldn't find a target. Should never happen.
                    command_queue.push(ship.stay_still());
                    Log::log(ship.position, "_not_", red);
                }
            }
        }

        // Move towards target (or mine if already on it)
        for (ship_id, position) in targets.iter() {
            // paint target
            Log::log(position.clone(), format!("_t{:?}_", ship_id.0), teal);

            let ship = &game.ships[ship_id];
            let cell = map.at_entity(ship);

            // If we are at mining location
            if &ship.position == position {
                Log::log(ship.position, "_mine_", purple);
                command_queue.push(ship.stay_still());

            // Cant afford to move
            } else if cell.halite / 10 > ship.halite {
                Log::log(ship.position, "_fuel_", orange);
                command_queue.push(ship.stay_still());

            // We have enough halite to move
            } else {
                navi.naive_navigate(ship, position);
            }
        }


        for (ship_id, dir) in navi.collect_moves() {
            Log::log(game.ships[&ship_id].position, format!("_{}_", dir.get_char_encoding()), green);
            command_queue.push(Command::move_ship(ship_id, dir));
        }

        let halite_remaining: usize = game.map.iter().map(|cell| cell.halite).sum();
        let turn_limit = game.constants.max_turns * 2 / 3;

        if (halite_remaining > total_halite / 2 && game.turn_number < turn_limit) &&
            me.halite >= game.constants.ship_cost &&
                navi.get(&me.shipyard.position).map_or(true, |ship| game.ships[&ship].owner != game.my_id)
        {
            command_queue.push(me.shipyard.spawn());
        }

        game.end_turn(&command_queue);
    }
}
