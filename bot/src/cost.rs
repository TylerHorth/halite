use pathfinding::num_traits::identities::Zero;

#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Copy)]
pub struct Cost(pub usize, pub i32);

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
