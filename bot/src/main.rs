#[macro_use]
extern crate lazy_static;
extern crate pathfinding;

use hlt::command::Command;
use hlt::game::Game;
use hlt::game_map::GameMap;
use hlt::navi::Navi;
use hlt::ShipId;
use hlt::log::Log;
use hlt::position::Position;
use hlt::flow::FlowField;
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

fn color(i: usize, h: usize) -> String {
    let i = (i / 4).min(255);
    let h = (h / 4).min(255);
    format!("#{:x}{:x}{:x}", i, h, h)
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

    let width = game.map.width as i32;
    let height = game.map.height as i32;

    let mut flow = FlowField::new(width, height);

    Game::ready("bugs");

    let mut targets: HashMap<ShipId, Position> = HashMap::new();
    let mut terminal = false;

    loop {
        game.update_frame();
        navi.update_frame(&game);
        flow.update(&game);

        for cell in game.map.iter() {
            let flow = flow.at(cell.position);
            // Log::log(cell.position, format!("_f{}_", flow), color(flow, cell.halite));
            Log::msg(cell.position, format!("_f{}_", flow));
        }

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

        // Remove ships which have reached their target
        targets.retain(|ship_id, &mut target| { 
            game.ships
                .get(&ship_id)
                .map(|ship| ship.position != target)
                .unwrap_or_default()
        });

        // Add targets for ships that don't have one
        // If can't find target, stay still and log
        for ship_id in me.ship_ids.iter().cloned() {
            let ship = &game.ships[&ship_id];
            let cell = map.at_entity(ship);
            let taken: HashSet<Position> = targets.values().cloned().collect();

            if !targets.contains_key(&ship_id) {
                let target = if ship.halite >= ship_full {
                    // If ship full, return to base
                    Some(me.shipyard.position)
                } else if cell.halite < target_halite / 2  {
                    // If cell not worth mining, find new cell
                    let mut target = find_target(ship.position, &map, target_halite, &taken);
                    while target.is_none() {
                        target_halite = target_halite / 2;
                        ship_full = 8 * target_halite;

                        Log::log(ship.position, format!("_tar_{}", target_halite), red);

                        target = find_target(ship.position, &map, target_halite, &taken);
                    }

                    target
                } else {
                    // Ship not full, cell worth mining, should stay still
                    Log::log(ship.position, "_mine_", purple);
                    None
                };

                if let Some(target) = target {
                    targets.insert(ship_id, target); 
                } else {
                    // No target. Either cell worth mining, or no target could be found
                    command_queue.push(ship.stay_still());
                }
            }
        }

        // Move towards target
        for (ship_id, position) in targets.iter() {
            // paint target
            Log::log(position.clone(), format!("_t{:?}_", ship_id.0), teal);

            let ship = &game.ships[ship_id];
            let cell = map.at_entity(ship);

            // If we have enough halite to move
            if ship.halite >= cell.halite / 10 {
                navi.naive_navigate(ship, position);
            } else {
                Log::log(ship.position, "_fuel_", orange);
                command_queue.push(ship.stay_still());
            }
        }


        for (ship_id, dir) in navi.collect_moves() {
            Log::log(game.ships[&ship_id].position, format!("_{}_", dir.get_char_encoding()), green);
            command_queue.push(Command::move_ship(ship_id, dir));
        }

        if game.turn_number <= game.constants.max_turns / 2 &&
            me.halite >= game.constants.ship_cost &&
                navi.get(&me.shipyard.position).map_or(true, |ship| game.ships[&ship].owner != game.my_id)
        {
            command_queue.push(me.shipyard.spawn());
        }

        game.end_turn(&command_queue);
    }
}
