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
    let yellow = "#FFFF00";

    let total_halite: usize = game.map.iter().map(|cell| cell.halite).sum();

    let mut terminal = false;
    let mut returning: HashMap<ShipId, Position> = HashMap::new();
    let mut mining: HashMap<Position, (ShipId, usize)> = HashMap::new();

    Game::ready("downside");

    loop {
        game.update_frame();
        navi.update_frame(&game);

        let me = game.players.iter().find(|p| p.id == game.my_id).unwrap();
        let map = &game.map;

        let mut command_queue: Vec<Command> = Vec::new();

        let dropoffs: Vec<Position> = std::iter::once(me.shipyard.position)
            .chain(me.dropoff_ids.iter().map(|id| game.dropoffs[id].position))
            .collect();

        let nearest_dropoff = |pos: Position| dropoffs.iter().min_by_key(|d| map.calculate_distance(&pos, d)).unwrap().clone();

        let halite_remaining: usize = game.map.iter().map(|cell| cell.halite).sum();
        let turn_limit = game.constants.max_turns * 2 / 3;
        // let turn_limit = 2; //game.constants.max_turns * 2 / 3;
        
        let early_game = halite_remaining > total_halite / 2 && game.turn_number < turn_limit;

        let mut used_halite = 0usize;

        terminal = terminal || me.ship_ids
            .iter()
            .cloned()
            .any(|ship_id| {
                let ship = &game.ships[&ship_id];
                let dist = map.calculate_distance(&ship.position, &nearest_dropoff(ship.position));
                let turns_remaining = game.constants.max_turns - game.turn_number;

                dist + 7 > turns_remaining
            });

        if terminal {
            Log::log(me.shipyard.position, "_terminal_", red);
            mining.clear();
            returning.clear();
            navi.terminal = true;

            for ship_id in me.ship_ids.iter().cloned() {
                let ship = &game.ships[&ship_id];
                returning.insert(ship_id, nearest_dropoff(ship.position));
            }
        } else {
            // Remove ships that have reached their target and finished mining/depositing
            returning.retain(|ship_id, pos| {
                game.ships
                    .get(ship_id)
                    .map(|ship| &ship.position != pos)
                    .unwrap_or_default()
            });

            mining.retain(|_, (ship_id, count)| {
                game.ships
                    .get(ship_id)
                    .map(|ship| *count > 0 && ship.halite < game.constants.max_halite)
                    .unwrap_or_default()
            });
            
            // Paths (cost) from all cells back to nearest dropoff
            let paths_dropoff = dropoffs.iter().map(|dropoff| {
                dijkstra_all(dropoff, |pos| {
                    let dist = map.calculate_distance(pos, dropoff);
                    pos.get_surrounding_cardinals()
                        .into_iter()
                        .filter(move |p| map.calculate_distance(p, dropoff) > dist)
                        .map(|p| (map.normalize(&p), map.at_position(&p).halite / game.constants.move_cost_ratio))
                })
            }).fold(HashMap::new(), |mut res: HashMap<Position, usize>, cur| {
                for (pos, (_, cost)) in cur {
                    res.entry(pos)
                        .and_modify(|c| *c = (*c).min(cost))
                        .or_insert(cost);
                }
                res
            });

            let mut richness: HashMap<Position, usize> = HashMap::new();
            let mut r_count = 0;

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

                r_count = seen.len();

                richness.insert(origin, sum);
                Log::msg(origin, format!("_r{}_", sum));
            }

            let prime_ter: HashSet<Position> = if early_game {
                let passive_halite = halite_remaining / map.width / map.height * 7 / 8;
                let dropoff_richness: usize = dropoffs.iter().map(|d| richness[d]).sum();
                let avg_halite_before = dropoff_richness / dropoffs.len() / r_count / 8 + passive_halite;
                let turns_remaining = game.constants.max_turns - game.turn_number;
                let num_ships = me.ship_ids.len();

                richness.iter()
                    .filter(|(&pos, &sum)| {
                        let dist = map.calculate_distance(&nearest_dropoff(pos), &pos);
                        let cost_home = paths_dropoff.get(&pos).unwrap_or(&0);
                        let avg_halite_after = (dropoff_richness + sum) / (dropoffs.len() + 1) / r_count / 8 + passive_halite;
                        avg_halite_after * num_ships * turns_remaining > avg_halite_before * (num_ships + 6) * turns_remaining - (dist * avg_halite_before + cost_home) * (num_ships + 6) / dropoffs.len()
                    })
                    .map(|(&pos, _)| {
                        Log::color(pos, yellow);
                        pos
                    })
                    .collect()
            } else {
                HashSet::new()
            };

            if !prime_ter.is_empty() { 
                used_halite += game.constants.dropoff_cost;   
            }

            let mut converted: Option<ShipId> = None;
            for ship_id in &me.ship_ids {
                let ship = &game.ships[&ship_id];
                if (converted.is_none() && map.at_entity(ship).structure == Structure::None && map.at_entity(ship).halite + ship.halite + me.halite >= game.constants.dropoff_cost) && prime_ter.contains(&ship.position) {
                    used_halite += game.constants.dropoff_cost - map.at_entity(ship).halite - ship.halite;
                    
                    returning.remove(&ship_id);
                    mining.retain(|_, (id, _)| id != ship_id);

                    command_queue.push(ship.make_dropoff());
                    converted = Some(*ship_id);
                }
            }

            let ships_mining: HashSet<ShipId> = mining.values().map(|s| s.0).collect();
            let ship_ids: Vec<ShipId> = me.ship_ids.iter().filter(|id| !ships_mining.contains(id) && !returning.contains_key(id) && Some(id.clone()) != converted.as_ref()).cloned().collect();
            for ship_id in ship_ids {
                let ship = &game.ships[&ship_id];
                let cargo_space = game.constants.max_halite - ship.halite;

                if cargo_space < min_space {
                    returning.insert(ship_id, nearest_dropoff(ship.position));
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

                let mut candidates: Vec<_> = paths_dropoff.iter().map(|(&pos, &cost_home)| {
                    let cost_to = paths_ship.get(&pos).map(|p| p.1).unwrap_or(0);
                    let dist_to = map.calculate_distance(&ship.position, &pos) + 1;
                    let dist_home = map.calculate_distance(&pos, &nearest_dropoff(pos)) + 1;

                    // let halite = map.at_position(&pos).halite / game.constants.extract_ratio;
                    let halite = map.at_position(&pos).halite;

                    let value = (halite as i32 - cost_to as i32).min(cargo_space as i32) / dist_to as i32;
                    let value_exp = (richness[&pos] as i32 / r_count as i32).min(cargo_space as i32 - value) / (dist_to as i32 + kernel_size as i32);
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
        }

        for (ship_id, position) in returning.iter() {
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

        if early_game && 
            me.halite >= game.constants.ship_cost + used_halite &&
                navi.get(&me.shipyard.position).map_or(true, |ship| game.ships[&ship].owner != game.my_id)
        {
            command_queue.push(me.shipyard.spawn());
        }

        game.end_turn(&command_queue);
    }
}
