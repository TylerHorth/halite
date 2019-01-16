#[macro_use]
extern crate lazy_static;
extern crate pathfinding;
extern crate im_rc;

mod hlt;
mod state;
mod action;
mod cost;
mod timeline;
mod stats;

use hlt::*;
use std::collections::HashMap;
use timeline::Timeline;
use stats::Stats;

fn main() {
    let mut game = Game::new();

    // Constants
    let total_halite: usize = game.map.iter().map(|cell| cell.halite).sum();

    // Persistent state
    let mut paths = HashMap::new();
    let mut stats = Stats::new();
    let mut ships_last = HashMap::new();
    let mut nav = Navi::new(game.map.width, game.map.height);

    Game::ready("downside");

    loop {
        stats.start();
        game.update_frame();
        nav.update_frame(&game);

        ships_last.retain(|ship_id, _| !game.ships.contains_key(ship_id));
        let crashed = ships_last.drain().map(|(_, pos)| pos).collect();

        let mut timeline = Timeline::from(&game, crashed, &mut paths);
        let mut command_queue = Vec::new();

        for (&ship_id, ship) in &game.ships {
            if ship.owner != game.my_id {
                ships_last.insert(ship_id, ship.position);
            }
        }

        let halite_remaining: usize = game.map.iter().map(|cell| cell.halite).sum();
        let turn_limit = game.constants.max_turns * 3 / 4;
        let early_game = halite_remaining > total_halite / 2 && game.turn_number < turn_limit;

        if early_game {
            timeline.make_dropoff(&mut paths);
        }

        command_queue.extend(timeline.path_ships(&mut paths));

        if early_game && timeline.spawn_ship() {
            command_queue.push(Command::spawn_ship());
        }

        stats.end();
        game.end_turn(&command_queue);
    }
}
