//! A rebuildable, cut-local bounding-volume index over authorized objects only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rect(pub i64, pub i64, pub i64, pub i64);
impl Rect {
    fn overlaps(self, b: Self) -> bool {
        self.0 <= b.1 && self.1 >= b.0 && self.2 <= b.3 && self.3 >= b.2
    }
    fn union(self, b: Self) -> Self {
        Self(
            self.0.min(b.0),
            self.1.max(b.1),
            self.2.min(b.2),
            self.3.max(b.3),
        )
    }
}
#[derive(Debug)]
enum Node {
    Leaf(Rect, Vec<(usize, Rect)>),
    Branch(Rect, Box<Node>, Box<Node>),
}
impl Node {
    fn build(mut objects: Vec<(usize, Rect)>) -> Self {
        let bounds = objects
            .iter()
            .skip(1)
            .fold(objects[0].1, |a, b| a.union(b.1));
        if objects.len() <= 16 {
            return Self::Leaf(bounds, objects);
        }
        let axis_x = i128::from(bounds.1) - i128::from(bounds.0)
            >= i128::from(bounds.3) - i128::from(bounds.2);
        objects.sort_unstable_by_key(|(id, r)| {
            (
                if axis_x {
                    i128::from(r.0) + i128::from(r.1)
                } else {
                    i128::from(r.2) + i128::from(r.3)
                },
                *id,
            )
        });
        let right = objects.split_off(objects.len() / 2);
        Self::Branch(
            bounds,
            Box::new(Self::build(objects)),
            Box::new(Self::build(right)),
        )
    }
    fn query(&self, bounds: Rect, out: &mut Vec<usize>, visited: &mut usize) {
        let own = match self {
            Self::Leaf(b, _) | Self::Branch(b, _, _) => *b,
        };
        if !own.overlaps(bounds) {
            return;
        }
        match self {
            Self::Leaf(_, objects) => {
                *visited += objects.len();
                out.extend(
                    objects
                        .iter()
                        .filter(|(_, r)| r.overlaps(bounds))
                        .map(|(id, _)| *id),
                );
            }
            Self::Branch(_, left, right) => {
                left.query(bounds, out, visited);
                right.query(bounds, out, visited);
            }
        }
    }
}
#[derive(Debug, Default)]
pub(super) struct SpatialIndex(Option<Node>);
impl SpatialIndex {
    pub(super) fn new(objects: Vec<(usize, Rect)>) -> Self {
        Self((!objects.is_empty()).then(|| Node::build(objects)))
    }
    pub(super) fn query(&self, bounds: Rect) -> Vec<usize> {
        let mut out = Vec::new();
        if let Some(root) = &self.0 {
            root.query(bounds, &mut out, &mut 0);
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn indexed_query_matches_scan_including_crossing_routes_and_i64_extremes() {
        let mut objects: Vec<_> = (0..10_000)
            .map(|i| {
                (
                    i,
                    Rect(i as i64 * 10, i as i64 * 10, i as i64 * 3, i as i64 * 3),
                )
            })
            .collect();
        objects.push((10_000, Rect(-100, 100_000, 100, 100)));
        objects.push((10_001, Rect(i64::MIN, i64::MIN, i64::MAX, i64::MAX)));
        let index = SpatialIndex::new(objects.clone());
        for bounds in [
            Rect(100, 500, 0, 150),
            Rect(-1, 1, 99, 101),
            Rect(i64::MIN, i64::MIN, i64::MAX, i64::MAX),
        ] {
            let mut actual = index.query(bounds);
            actual.sort_unstable();
            let mut expected: Vec<_> = objects
                .iter()
                .filter(|(_, r)| r.overlaps(bounds))
                .map(|(id, _)| *id)
                .collect();
            expected.sort_unstable();
            assert_eq!(actual, expected);
        }
        let mut out = Vec::new();
        let mut visited = 0;
        index
            .0
            .as_ref()
            .unwrap()
            .query(Rect(100, 110, 30, 33), &mut out, &mut visited);
        assert!(
            visited < 200,
            "a narrow query must prune unrelated leaves: {visited}"
        );
    }
}
