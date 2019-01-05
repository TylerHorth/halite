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

    Game::ready("downside");

    loop {
        stats.start();
        game.update_frame();

        ships_last.retain(|ship_id, _| !game.ships.contains_key(ship_id));
        let crashed = ships_last.drain().map(|(_, pos)| pos).collect();

        let mut timeline = Timeline::from(&game, crashed, &mut paths);
        let mut command_queue = Vec::new();

        for (&ship_id, ship) in &game.ships {
            ships_last.insert(ship_id, ship.position);
        }

        for action in timeline.unpathed_actions() {
            timeline.path_ship(action, &mut paths)
        }

        for (&ship_id, path) in paths.iter_mut() {
            let dir = path.pop_front().expect("Empty path").dir;
            command_queue.push(Command::move_ship(ship_id, dir));
        }

        let halite_remaining: usize = game.map.iter().map(|cell| cell.halite).sum();
        let turn_limit = game.constants.max_turns * 2 / 3;
        let early_game = halite_remaining > total_halite / 2 && game.turn_number < turn_limit;

        if early_game && timeline.spawn_ship() {
            command_queue.push(Command::spawn_ship());
        }

        stats.end();
        game.end_turn(&command_queue);
    }
}
