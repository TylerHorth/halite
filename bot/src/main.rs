#[macro_use]
extern crate lazy_static;
extern crate pathfinding;

use hlt::*;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use pathfinding::directed::dijkstra::dijkstra_all;

mod hlt;

fn main() {
    let mut game = Game::new();
    let mut navi = Navi::new(game.map.width, game.map.height);

    // Constants
    let min_space = 100;
    let kernel_size = 8;

    // Colors
    let red = "#FF0000";
    let purple = "#9d00ff";
    let orange = "#ff8800";
    let green = "#00ff48";
    let teal = "#42f4ee";
    let pink = "#ff00dc";

    let total_halite: usize = game.map.iter().map(|cell| cell.halite).sum();

    let mut terminal = false;
    let mut returning: HashSet<ShipId> = HashSet::new();
    let mut mining: HashMap<Position, (ShipId, usize)> = HashMap::new();

    Game::ready("downside");

    loop {
        game.update_frame();
        navi.update_frame(&game);

        let me = game.players.iter().find(|p| p.id == game.my_id).unwrap();
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
        
        // Remove ships that have reached their target and finished mining/depositing
        returning.retain(|ship_id| {
            game.ships
                .get(ship_id)
                .map(|ship| ship.position != me.shipyard.position)
                .unwrap_or_default()
        });

        mining.retain(|_, (ship_id, count)| {
            game.ships
                .get(ship_id)
                .map(|ship| *count > 0 && ship.halite < game.constants.max_halite)
                .unwrap_or_default()
        });
        
        // Paths from all cells back to the shipyard
        let shipyard_pos = &me.shipyard.position;
        let paths_home = dijkstra_all(shipyard_pos, |pos| {
            let dist = map.calculate_distance(pos, shipyard_pos);
            pos.get_surrounding_cardinals()
                .into_iter()
                .filter(move |p| map.calculate_distance(p, shipyard_pos) > dist)
                .map(|p| (map.normalize(&p), map.at_position(&p).halite / game.constants.move_cost_ratio))
        });

        let mut richness: HashMap<Position, usize> = HashMap::new();

        // Calculate value of neighbourhood arround each cell
        for origin in map.iter().map(|cell| cell.position) {
            let mut q = VecDeque::new();
            q.push_back((origin, 1));

            let mut seen = HashSet::new();
            seen.insert(origin);

            let mut sum = 0usize;
            while let Some((pos, dist)) = q.pop_front() {
                // sum += map.at_position(&pos).halite / game.constants.extract_ratio;
                sum += map.at_position(&pos).halite;
                
                let dist = dist + 1;
                if dist < kernel_size {
                    for next in pos.get_surrounding_cardinals() {
                        let next = map.normalize(&next);
                        if !seen.contains(&next) {
                            q.push_back((next, dist));
                            seen.insert(next);
                        }
                    }
                }
            }

            sum /= seen.len();

            richness.insert(origin, sum);
            Log::msg(origin, format!("_r{}_", sum));
        }


        let ships_mining: HashSet<ShipId> = mining.values().map(|s| s.0).collect();
        let ship_ids: Vec<ShipId> = me.ship_ids.iter().filter(|id| !ships_mining.contains(id) && !returning.contains(id)).cloned().collect();
        for ship_id in ship_ids {
            let ship = &game.ships[&ship_id];
            let cargo_space = game.constants.max_halite - ship.halite;

            if cargo_space < min_space {
                returning.insert(ship_id);
                continue
            }

            let paths_ship = dijkstra_all(&ship.position, |pos| {
                let cost = map.at_position(pos).halite / game.constants.move_cost_ratio;
                let dist = map.calculate_distance(pos, &ship.position);
                pos.get_surrounding_cardinals()
                    .into_iter()
                    .filter(move |p| map.calculate_distance(p, &ship.position) > dist)
                    .map(move |p| (map.normalize(&p), cost))
            });

            let mut candidates: Vec<_> = paths_home.iter().map(|(&pos, &(_, cost_home))| {
                let cost_to = paths_ship.get(&pos).map(|p| p.1).unwrap_or(0);
                let dist_to = map.calculate_distance(&ship.position, &pos) + 1;
                let dist_home = map.calculate_distance(&pos, &shipyard_pos) + 1;

                // let halite = map.at_position(&pos).halite / game.constants.extract_ratio;
                let halite = map.at_position(&pos).halite;

                let value = (halite as i32 - cost_to as i32).min(cargo_space as i32) / dist_to as i32;
                let value_exp = (richness[&pos] as i32).min(cargo_space as i32 - value) / (dist_to as i32 + kernel_size as i32);
                let value_home = (halite as i32 - cost_home as i32) / (dist_to as i32 + dist_home as i32);

                // let rate = value + value_exp;
                let rate = value + (value_exp * cargo_space as i32 / 1000 + value_home * ship.halite as i32 / 1000);
                // let rate =  if cargo_space > richness[&pos] {
                //     value + value_exp
                // } else {
                //     value + value_home
                // };

                (pos, 6, rate)
            }).collect();

            candidates.sort_unstable_by(|a, b| a.2.cmp(&b.2).reverse());

            if let Some(best) = candidates.iter().find(|(p, _, _)| !mining.contains_key(p)) {
                mining.insert(best.0, (ship_id, best.1));
            } else {
                Log::warn("Could not find target");
            } 
        }

        // Move towards target (or mine if already on it)
        for (position, (ship_id, count)) in mining.iter_mut() {
            // paint target
            Log::log(position.clone(), format!("_t{:?}_", ship_id.0), teal);

            let ship = &game.ships[ship_id];
            let cell = map.at_entity(ship);

            // If we are at mining location
            if &ship.position == position {
                Log::log(ship.position, format!("_m{}_", count), purple);
                command_queue.push(ship.stay_still());
                *count -= 1;

            // Cant afford to move
            } else if cell.halite / game.constants.move_cost_ratio > ship.halite {
                Log::log(ship.position, "_fuel_", orange);
                command_queue.push(ship.stay_still());

            // We have enough halite to move
            } else {
                navi.naive_navigate(ship, position, map);
            }
        }

        for ship_id in returning.iter() {
            let position = &me.shipyard.position;
            // paint target
            Log::log(position.clone(), format!("_t{:?}_", ship_id.0), teal);

            let ship = &game.ships[ship_id];
            let cell = map.at_entity(ship);

            if &ship.position == position {
                Log::warn("Already at shipyard, should have set a new target by this point");

            // Cant afford to move
            } else if cell.halite / game.constants.move_cost_ratio > ship.halite {
                Log::log(ship.position, "_fuel_", orange);
                command_queue.push(ship.stay_still());

            // We have enough halite to move
            } else {
                navi.naive_navigate(ship, position, map);
            }
        }

        for (ship_id, dir) in navi.collect_moves() {
            Log::log(game.ships[&ship_id].position, format!("_{}_", dir.get_char_encoding()), green);
            command_queue.push(Command::move_ship(ship_id, dir));
        }

        let halite_remaining: usize = game.map.iter().map(|cell| cell.halite).sum();
        let turn_limit = game.constants.max_turns * 2 / 3;
        // let turn_limit = 2; //game.constants.max_turns * 2 / 3;

        if (halite_remaining > total_halite / 2 && game.turn_number < turn_limit) &&
            me.halite >= game.constants.ship_cost &&
                navi.get(&me.shipyard.position).map_or(true, |ship| game.ships[&ship].owner != game.my_id)
        {
            command_queue.push(me.shipyard.spawn());
        }

        game.end_turn(&command_queue);
    }
}
