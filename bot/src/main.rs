#[macro_use]
extern crate lazy_static;
extern crate bimap;

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

    Game::ready("bugs");

    let mut targets: HashMap<ShipId, Position> = HashMap::new();
    let mut terminal = false;

    loop {
        game.update_frame();
        navi.update_frame(&game);

        let me = &game.players[game.my_id.0];
        let map = &game.map;

        let mut command_queue: Vec<Command> = Vec::new();

        // new HashMap ship_id -> Vec<Position>   /// paths
        //
        // for each of my ships, A* a path, avoiding other ships
        // // use 

        if game.turn_number <= game.constants.max_turns / 2 &&
            me.halite >= game.constants.ship_cost //&&
                //TODO: Detect if one of my ships is going to be on shipyard before creating
                // navi.get(&me.shipyard.position).map_or(true, |ship| game.ships[&ship].owner != game.my_id)
        {
            command_queue.push(me.shipyard.spawn());
        }

        game.end_turn(&command_queue);
    }
}
