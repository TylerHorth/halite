use hlt::game_map::GameMap;
use hlt::map_cell::MapCell;
use hlt::position::Position;

pub struct MapCellIterator<'a> {
    pub map: &'a GameMap,
    pub pos: Position
}

impl<'a> Iterator for MapCellIterator<'a> {
    type Item = &'a MapCell;

    fn next(&mut self) -> Option<&'a MapCell> {
        if self.pos.x as usize == self.map.width {
            self.pos.x = 0;
            self.pos.y += 1;
        }

        if self.pos.y as usize == self.map.height {
            None
        } else {
            let cell = self.map.at_position(&self.pos);

            self.pos.x += 1;

            Some(cell)
        }
    }
}
