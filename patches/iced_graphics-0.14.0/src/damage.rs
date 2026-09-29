//! Compute the damage between frames.
use crate::core::{Point, Rectangle};

/// Diffs the damage regions given some previous and current primitives.
pub fn diff<T>(
    previous: &[T],
    current: &[T],
    bounds: impl Fn(&T) -> Vec<Rectangle>,
    diff: impl Fn(&T, &T) -> Vec<Rectangle>,
) -> Vec<Rectangle> {
    let damage = previous.iter().zip(current).flat_map(|(a, b)| diff(a, b));

    if previous.len() == current.len() {
        damage.collect()
    } else {
        let (smaller, bigger) = if previous.len() < current.len() {
            (previous, current)
        } else {
            (current, previous)
        };

        // Extend damage by the added/removed primitives
        damage
            .chain(bigger[smaller.len()..].iter().flat_map(bounds))
            .collect()
    }
}

/// Computes the damage regions given some previous and current primitives.
pub fn list<T>(
    previous: &[T],
    current: &[T],
    bounds: impl Fn(&T) -> Vec<Rectangle>,
    are_equal: impl Fn(&T, &T) -> bool,
) -> Vec<Rectangle> {
    diff(previous, current, &bounds, |a, b| {
        if are_equal(a, b) {
            vec![]
        } else {
            bounds(a).into_iter().chain(bounds(b)).collect()
        }
    })
}

/// Groups the given damage regions that are close together inside the given
/// bounds.
pub fn group(mut damage: Vec<Rectangle>, bounds: Rectangle) -> Vec<Rectangle> {
    const AREA_THRESHOLD: f32 = 20_000.0;

    damage.sort_by(|a, b| {
        a.center()
            .distance(Point::ORIGIN)
            .total_cmp(&b.center().distance(Point::ORIGIN))
    });

    let mut output = Vec::new();
    let mut scaled = damage
        .into_iter()
        .filter_map(|region| region.intersection(&bounds))
        .filter(|region| region.width >= 1.0 && region.height >= 1.0);

    if let Some(mut current) = scaled.next() {
        for region in scaled {
            let union = current.union(&region);

            if union.area() - current.area() - region.area() <= AREA_THRESHOLD {
                current = union;
            } else {
                push_region(&mut output, current);
                current = region;
            }
        }

        push_region(&mut output, current);
    }

    output
}

/// A late, large region can cover previously emitted groups. Painting both
/// repeats all intersecting layers, including text and full-surface clip masks.
/// Preserve exactly the same coverage while dropping redundant groups.
fn push_region(output: &mut Vec<Rectangle>, region: Rectangle) {
    if output.iter().any(|other| region.is_within(other)) {
        return;
    }
    output.retain(|other| !other.is_within(&region));
    output.push(region);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_covering_region_removes_earlier_groups() {
        let bounds = Rectangle::new(Point::ORIGIN, crate::core::Size::new(1280.0, 800.0));
        let panel = Rectangle {
            x: 324.0,
            y: 56.0,
            width: 632.0,
            height: 688.0,
        };
        let labels = [
            Rectangle {
                x: 324.0,
                y: 56.0,
                width: 306.0,
                height: 210.0,
            },
            Rectangle {
                x: 324.0,
                y: 180.0,
                width: 531.0,
                height: 28.0,
            },
            Rectangle {
                x: 648.0,
                y: 56.0,
                width: 56.0,
                height: 85.0,
            },
            panel,
        ];
        assert_eq!(group(labels.to_vec(), bounds), vec![panel]);
    }

    #[test]
    fn disjoint_regions_still_stay_separate() {
        let bounds = Rectangle::new(Point::ORIGIN, crate::core::Size::new(1280.0, 800.0));
        let first = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        let last = Rectangle {
            x: 1200.0,
            y: 700.0,
            width: 10.0,
            height: 10.0,
        };
        assert_eq!(group(vec![first, last], bounds), vec![first, last]);
    }

    #[test]
    fn contained_and_duplicate_regions_do_not_add_work() {
        let panel = Rectangle {
            x: 20.0,
            y: 20.0,
            width: 600.0,
            height: 600.0,
        };
        let mut output = vec![panel];
        push_region(
            &mut output,
            Rectangle {
                x: 30.0,
                y: 30.0,
                width: 10.0,
                height: 10.0,
            },
        );
        push_region(&mut output, panel);
        assert_eq!(output, vec![panel]);
    }
}
