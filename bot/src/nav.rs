use hlt::*;
use std::collections::HashMap;
use std::collections::HashSet;
use pathfinding::directed::dijkstra::dijkstra_all;

#[inline]
fn div_ceil(num: usize, by: usize) -> usize {
    (num + by - 1) / by
}

pub fn targets(
    map: &GameMap,
    ships: &HashMap<ShipId, Ship>,
    ship_ids: &Vec<ShipId>,
    taken: &HashSet<Position>,
    shipyard_pos: &Position,
    constants: &Constants,
) -> HashMap<Position, ShipId> {
    let extract_ratio = constants.extract_ratio;
    let move_cost_ratio = constants.move_cost_ratio;
    let max_halite = constants.max_halite;

    let paths_home = dijkstra_all(shipyard_pos, |pos| {
        let dist = map.calculate_distance(pos, shipyard_pos);
        pos.get_surrounding_cardinals()
            .into_iter()
            .filter(move |p| map.calculate_distance(p, shipyard_pos) > dist)
            .map(|p| (map.normalize(&p), div_ceil(map.at_position(&p).halite, move_cost_ratio)))
    });

    let mut targets = HashMap::with_capacity(ships.len());

    let mut ship_ids = ship_ids.clone();
    ship_ids.sort_unstable_by_key(|id| id.0);

    for ship_id in &ship_ids {
        let ship = &ships[ship_id];
        let cargo_space = max_halite - ship.halite;

        let paths_ship = dijkstra_all(&ship.position, |pos| {
            let cost = div_ceil(map.at_position(pos).halite, move_cost_ratio);
            let dist = map.calculate_distance(pos, &ship.position);
            pos.get_surrounding_cardinals()
                .into_iter()
                .filter(move |p| map.calculate_distance(p, &ship.position) > dist)
                .map(move |p| (map.normalize(&p), cost))
        });

        let mut candidates: Vec<_> = paths_home.iter().map(|(&pos, &(_, cost_home))| {
            let cost_to = paths_ship.get(&pos).map(|p| p.1).unwrap_or(0);
            let dist_to = map.calculate_distance(&ship.position, &pos);
            let dist_home = map.calculate_distance(&pos, &shipyard_pos);

            let halite = map.at_position(&pos).halite / extract_ratio;
            // let value = ((halite as i32 - cost_to as i32).min(cargo_space as i32) - cost_home as i32) / (dist_to + dist_home) as i32;
            let value = halite / (dist_to + dist_home);

            (pos, value)
        }).collect();

        candidates.sort_unstable_by(|a, b| a.1.cmp(&b.1).reverse());

        if let Some(best) = candidates.iter().find(|(p, _)| !targets.contains_key(p) && !taken.contains(p)) {
            targets.insert(best.0, *ship_id);
        }
    }

    targets
}
