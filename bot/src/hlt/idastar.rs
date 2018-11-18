//! Compute a shortest path using the [IDA* search
//! algorithm](https://en.wikipedia.org/wiki/Iterative_deepening_A*).
//! source: https://github.com/samueltardieu/pathfinding

use std::hash::Hash;

pub fn idastar<N, C, FN, IN, FH, FS>(
    start: &N,
    mut successors: FN,
    mut heuristic: FH,
    mut success: FS,
) -> Option<(Vec<N>, C)>
where
    N: Eq + Hash + Clone,
    C: Zero + Ord + Copy,
    FN: FnMut(&N) -> IN,
    IN: IntoIterator<Item = (N, C)>,
    FH: FnMut(&N) -> C,
    FS: FnMut(&N) -> bool,
{
    let mut bound = heuristic(start);
    let mut path = vec![start.clone()];
    loop {
        match search(
            &mut path,
            Zero::zero(),
            bound,
            &mut successors,
            &mut heuristic,
            &mut success,
        ) {
            Path::Found(path, cost) => return Some((path, cost)),
            Path::Minimum(min) => {
                if bound == min {
                    return None;
                }
                bound = min;
            }
            Path::Impossible => return None,
        }
    }
}

/ successsors 
/ Returns minimum cost if found
pub fn search<N, C, FN, IN, FH, FS>(
    path: &mut Vec<N>,
    cost: C,
    bound: C,
    successors: &mut FN,
    heuristic: &mut FH,
) -> Option<C>
where
    C: Zero + Ord + Copy,
    FN: FnMut(&N) -> IN,
    IN: IntoIterator<Item = (N, C)>,
    FH: FnMut(&N) -> C,
{
    let neighbs = {
        let start = &path[path.len() - 1];
        let f = cost + heuristic(start);
        if f > bound {
            return Some(f);
        }
        let mut neighbs = successors(start)
            .into_iter()
            .map(|(n, c)| {
                let h = heuristic(&n);
                (n, c, c + h)
            })
            .collect::<Vec<_>>();
        neighbs.sort_by_key(|&(_, _, c)| c);
        neighbs
    };
    let mut min = None;
    for (node, extra, _) in neighbs {
        path.push(node);
        match search(path, cost + extra, bound, successors, heuristic) {
            Some(m) => match min {
                None => min = Some(m),
                Some(n) if m < n => min = Some(m),
                Some(_) => (),
            },
            None => (),
        }
        path.pop();
    }

    min
}
