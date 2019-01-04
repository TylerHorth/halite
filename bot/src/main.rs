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

    Game::ready("downside");

    loop {
        stats.start();
        game.update_frame();

        let mut timeline = Timeline::from(&game, &mut paths);
        let mut command_queue = Vec::new();

        let me = game.players.iter().find(|p| p.id == game.my_id).unwrap();
        let halite_remaining: usize = game.map.iter().map(|cell| cell.halite).sum();
        let turn_limit = game.constants.max_turns * 2 / 3;
        let early_game = halite_remaining > total_halite / 2 && game.turn_number < turn_limit;

        for action in timeline.unpathed_actions() {
            timeline.path_ship(action, &mut paths)
        }

        for (&ship_id, path) in paths.iter_mut() {
            let dir = path.pop_front().expect("Empty path").dir;
            command_queue.push(Command::move_ship(ship_id, dir));
        }

        let can_afford_ship = me.halite >= game.constants.ship_cost;
        let is_safe = !timeline.state(1).taken.contains_key(&me.shipyard.position);
        if early_game && can_afford_ship && is_safe {
            command_queue.push(me.shipyard.spawn());
        }

        stats.end();
        game.end_turn(&command_queue);
    }
}
