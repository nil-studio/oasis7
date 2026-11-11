//! Original Meta progress arithmetic and ordering, with caller-owned admission.
//! This pure kernel confers no action authority. Callers reserve all allocations
//! before entry; `visit` charges each actual traversal/comparison and may cancel.
use crate::MetaProgressState;
fn equal<E>(a: &str, b: &str, visit: &mut impl FnMut(usize) -> Result<(), E>) -> Result<bool, E> {
    visit(a.len().max(b.len()))?;
    Ok(a == b)
}
fn contains<E>(
    values: &[String],
    needle: &str,
    visit: &mut impl FnMut(usize) -> Result<(), E>,
) -> Result<bool, E> {
    for value in values {
        if equal(value, needle, visit)? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn down<E>(
    values: &mut [String],
    mut root: usize,
    end: usize,
    visit: &mut impl FnMut(usize) -> Result<(), E>,
) -> Result<(), E> {
    loop {
        visit(0)?;
        let Some(left) = root.checked_mul(2).and_then(|n| n.checked_add(1)) else {
            return Ok(());
        };
        if left >= end {
            return Ok(());
        }
        let mut child = left;
        if left + 1 < end {
            visit(values[left].len().max(values[left + 1].len()))?;
            if values[left] < values[left + 1] {
                child = left + 1;
            }
        }
        visit(values[root].len().max(values[child].len()))?;
        if values[root] >= values[child] {
            return Ok(());
        }
        values.swap(root, child);
        root = child;
    }
}
fn order_unique<E>(
    values: &mut Vec<String>,
    visit: &mut impl FnMut(usize) -> Result<(), E>,
) -> Result<(), E> {
    visit(0)?;
    let len = values.len();
    for root in (0..len / 2).rev() {
        down(values, root, len, visit)?;
    }
    for end in (1..len).rev() {
        visit(0)?;
        values.swap(0, end);
        down(values, 0, end, visit)?;
    }
    let mut write = 0;
    for read in 0..values.len() {
        visit(0)?;
        if write == 0 || !equal(&values[write - 1], &values[read], visit)? {
            values.swap(write, read);
            write += 1;
        }
    }
    // Destruction is also an observed traversal, including duplicate strings.
    for value in &values[write..] {
        visit(value.len())?;
    }
    values.truncate(write);
    visit(0)
}
/// Exactly the old saturation, persistent tiers and global achievement ordering.
/// `track` and optional achievement are already normalized by the business owner.
/// There is intentionally no agent/World lookup or acceptance receipt here.
pub fn apply_meta_progress_delta_v1<E>(
    progress: &mut MetaProgressState,
    track: &str,
    points: i64,
    achievement: Option<&str>,
    now: u64,
    mut visit: impl FnMut(usize) -> Result<(), E>,
) -> Result<(), E> {
    visit(0)?;
    // A linear charged upper bound covers the BTree lookup/insert comparisons;
    // no unmetered comparator callback is hidden behind a new sorted container.
    for key in progress.track_points.keys() {
        visit(key.len().max(track.len()))?;
    }
    let next = progress
        .track_points
        .get(track)
        .copied()
        .unwrap_or(0)
        .saturating_add(points);
    visit(track.len())?;
    progress.track_points.insert(track.to_owned(), next);
    progress.total_points = progress.total_points.saturating_add(points);
    progress.last_granted_at = now;
    if let Some(id) = achievement {
        if !contains(&progress.achievements, id, &mut visit)? {
            visit(id.len())?;
            progress.achievements.push(id.to_owned());
        }
    }
    for (tier, threshold) in [("bronze", 20), ("silver", 50), ("gold", 100)] {
        visit(0)?;
        if next >= threshold {
            for key in progress.unlocked_tiers.keys() {
                visit(key.len().max(track.len()))?;
            }
            visit(track.len())?;
            let tiers = progress.unlocked_tiers.entry(track.to_owned()).or_default();
            if !contains(tiers, tier, &mut visit)? {
                visit(tier.len())?;
                tiers.push(tier.to_owned());
            }
            visit(track.len().saturating_add(tier.len()).saturating_add(6))?;
            let id = format!("tier.{track}.{tier}");
            if !contains(&progress.achievements, &id, &mut visit)? {
                progress.achievements.push(id);
            }
        }
    }
    for key in progress.unlocked_tiers.keys() {
        visit(key.len().max(track.len()))?;
    }
    if let Some(tiers) = progress.unlocked_tiers.get_mut(track) {
        order_unique(tiers, &mut visit)?;
    }
    order_unique(&mut progress.achievements, &mut visit)?;
    visit(0)
}
