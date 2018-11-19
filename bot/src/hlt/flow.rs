use hlt::game::Game;
use hlt::position::Position;
use std::collections::HashSet;
use pathfinding::prelude::*;

pub struct FlowField {
    width: i32,
    height: i32,
    ek: DenseCapacity<i32>,
}

impl FlowField {
    pub fn new(width: i32, height: i32) -> FlowField {
        // Number of nodes + 1 for the source (feeding halite into cells)
        let size = width * height + 2;

        let mut ek = DenseCapacity::new(size as usize, 0, 1);
        ek.omit_detailed_flows();

        FlowField {
            width,
            height,
            ek,
        }
    }

    pub fn at(&self, pos: Position) -> usize {
        let index = self.index(pos.x, pos.y);
        // pos.get_surrounding_cardinals()
        //     .into_iter()
        //     .map(|Position {x, y}| self.ek.flow(self.index(x, y), index))
        //     .sum()

        self.ek.flows_from(index).iter().sum()
    }

    #[inline]
    fn index(&self, x: i32, y: i32) -> usize {
        let x = ((x % self.width) + self.width) % self.width;
        let y = ((y % self.height) + self.height) % self.height;

        (2 + (y * self.width) + x) as usize
    }

    pub fn update(&mut self, game: &Game) {
        let ships: HashSet<(i32, i32)> = game.ships.values().map(|ship| {
            let Position { x, y } = ship.position;
            (x, y)
        }).collect();

        let cap = game.map.iter().map(|cell| cell.halite as i32).sum();

        for player in &game.players {
            let pos = player.shipyard.position;
            let index = self.index(pos.x, pos.y);
            self.ek.set_capacity(index, 1, cap)
        }

        for y in 0..self.height {
            for x in 0..self.width {
                // Set capacities between nodes. 
                let cur = self.index(x, y);
                let right = self.index(x + 1, y);
                let down = self.index(x, y + 1);
                let left = self.index(x - 1, y);
                let up = self.index(x, y - 1);

                // No capacity if blocked by ship
                if !ships.contains(&(x, y)) {
                    if !ships.contains(&(x + 1, y)) {
                        self.ek.set_capacity(cur, right, cap);
                    }
                    if !ships.contains(&(x, y + 1)) {
                        self.ek.set_capacity(cur, down, cap);
                    }
                    if !ships.contains(&(x - 1, y)) {
                        self.ek.set_capacity(cur, left, cap);
                    }
                    if !ships.contains(&(x, y - 1)) {
                        self.ek.set_capacity(cur, up, cap);
                    }
                }

                // Set capacity entering the game as halite sources
                let halite = game.map.cells[y as usize][x as usize].halite;
                self.ek.set_capacity(0, cur, halite as i32);
            }
        }

        self.ek.augment();
    }
}
