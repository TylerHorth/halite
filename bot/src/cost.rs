use pathfinding::num_traits::identities::Zero;
use std::cmp::Ordering;

#[derive(Eq, Clone, Copy)]
pub struct Cost(pub usize, pub i32);

const TIME_RATIO: i32 = 100;

impl From<&Cost> for i32 {
    fn from(cost: &Cost) -> i32 {
        cost.0 as i32 * TIME_RATIO + cost.1
    }
}

impl Ord for Cost {
    fn cmp(&self, other: &Cost) -> Ordering {
        i32::from(self).cmp(&i32::from(other))
    }
}

impl PartialOrd for Cost {
    fn partial_cmp(&self, other: &Cost) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Cost {
    fn eq(&self, other: &Cost) -> bool {
        i32::from(self) == i32::from(other)
    }
}

impl std::ops::Add for Cost {
    type Output = Cost;

    fn add(self, rhs: Cost) -> Cost {
        Cost(self.0 + rhs.0, self.1 + rhs.1)
    }
}

impl Zero for Cost {
    fn zero() -> Cost {
        Cost(0, 0)
    }

    fn is_zero(&self) -> bool {
        self.0 == 0 && self.1 == 0
    }
}
