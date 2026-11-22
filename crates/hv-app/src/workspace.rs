//! Tabs and split panes (F2-01, docs/architecture.md 5): a pure model with layout, focus
//! navigation and resizing. Owns no terminals: panes are ids that the app maps to sessions.

/// Id of a pane, unique for the life of the app.
pub(crate) type PaneId = u64;

/// How a split divides its space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
    /// Side by side (a divider that runs top to bottom).
    Horizontal,
    /// Stacked (a divider that runs left to right).
    Vertical,
}

/// Focus movement direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    fn axis(self) -> Axis {
        match self {
            Self::Left | Self::Right => Axis::Horizontal,
            Self::Up | Self::Down => Axis::Vertical,
        }
    }
}

/// Smallest share of a split either side can get.
const MIN_RATIO: f32 = 0.1;

/// A tree of panes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PaneNode {
    Leaf(PaneId),
    Split {
        axis: Axis,
        /// Share of the first child, 0..1.
        ratio: f32,
        a: Box<PaneNode>,
        b: Box<PaneNode>,
    },
}

/// A divider between two parts of a split, for drawing and dragging.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Divider {
    /// `[x, y, w, h]` in physical pixels.
    pub(crate) rect: [f32; 4],
    pub(crate) axis: Axis,
    /// Path to the split (false = first child, true = second) from the root.
    pub(crate) path: Vec<bool>,
    /// The space the split divides, to turn a drag position into a ratio.
    pub(crate) span: [f32; 4],
}

/// Where everything goes in one frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Layout {
    pub(crate) panes: Vec<(PaneId, [f32; 4])>,
    pub(crate) dividers: Vec<Divider>,
}

impl Layout {
    /// The area of `pane`, if it is laid out.
    pub(crate) fn area(&self, pane: PaneId) -> Option<[f32; 4]> {
        self.panes
            .iter()
            .find(|(id, _)| *id == pane)
            .map(|(_, a)| *a)
    }

    /// The pane under a point.
    pub(crate) fn pane_at(&self, x: f32, y: f32) -> Option<PaneId> {
        self.panes
            .iter()
            .find(|(_, a)| contains(*a, x, y, 0.0))
            .map(|(id, _)| *id)
    }

    /// The divider under a point, with a few pixels of slack so thin lines are easy to grab.
    pub(crate) fn divider_at(&self, x: f32, y: f32, slack: f32) -> Option<&Divider> {
        self.dividers.iter().find(|d| contains(d.rect, x, y, slack))
    }
}

fn contains([ax, ay, aw, ah]: [f32; 4], x: f32, y: f32, slack: f32) -> bool {
    x >= ax - slack && x < ax + aw + slack && y >= ay - slack && y < ay + ah + slack
}

impl PaneNode {
    /// Every pane in the tree, in reading order.
    pub(crate) fn panes(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect(&self, out: &mut Vec<PaneId>) {
        match self {
            Self::Leaf(id) => out.push(*id),
            Self::Split { a, b, .. } => {
                a.collect(out);
                b.collect(out);
            }
        }
    }

    /// Whether `pane` is in the tree.
    pub(crate) fn contains(&self, pane: PaneId) -> bool {
        match self {
            Self::Leaf(id) => *id == pane,
            Self::Split { a, b, .. } => a.contains(pane) || b.contains(pane),
        }
    }

    /// Lays the tree out in `area`, leaving `gap` pixels for each divider.
    pub(crate) fn layout(&self, area: [f32; 4], gap: f32) -> Layout {
        let mut layout = Layout::default();
        self.layout_into(area, gap, &mut Vec::new(), &mut layout);
        layout
    }

    fn layout_into(&self, area: [f32; 4], gap: f32, path: &mut Vec<bool>, out: &mut Layout) {
        match self {
            Self::Leaf(id) => out.panes.push((*id, area)),
            Self::Split { axis, ratio, a, b } => {
                let [x, y, w, h] = area;
                let (first, divider, second) = match axis {
                    Axis::Horizontal => {
                        let wa = ((w - gap) * ratio).round().max(0.0);
                        (
                            [x, y, wa, h],
                            [x + wa, y, gap, h],
                            [x + wa + gap, y, (w - wa - gap).max(0.0), h],
                        )
                    }
                    Axis::Vertical => {
                        let ha = ((h - gap) * ratio).round().max(0.0);
                        (
                            [x, y, w, ha],
                            [x, y + ha, w, gap],
                            [x, y + ha + gap, w, (h - ha - gap).max(0.0)],
                        )
                    }
                };
                out.dividers.push(Divider {
                    rect: divider,
                    axis: *axis,
                    path: path.clone(),
                    span: area,
                });
                path.push(false);
                a.layout_into(first, gap, path, out);
                path.pop();
                path.push(true);
                b.layout_into(second, gap, path, out);
                path.pop();
            }
        }
    }

    /// Splits `target` in two, putting `new` after it. Returns false if `target` is not here.
    pub(crate) fn split(&mut self, target: PaneId, new: PaneId, axis: Axis) -> bool {
        match self {
            Self::Leaf(id) if *id == target => {
                *self = Self::Split {
                    axis,
                    ratio: 0.5,
                    a: Box::new(Self::Leaf(target)),
                    b: Box::new(Self::Leaf(new)),
                };
                true
            }
            Self::Leaf(_) => false,
            Self::Split { a, b, .. } => a.split(target, new, axis) || b.split(target, new, axis),
        }
    }

    /// The tree without `target`: its sibling takes the parent's place. `None` if it was the
    /// last pane.
    pub(crate) fn remove(self, target: PaneId) -> Option<Self> {
        match self {
            Self::Leaf(id) if id == target => None,
            leaf @ Self::Leaf(_) => Some(leaf),
            Self::Split { axis, ratio, a, b } => match (a.remove(target), b.remove(target)) {
                (Some(a), Some(b)) => Some(Self::Split {
                    axis,
                    ratio,
                    a: Box::new(a),
                    b: Box::new(b),
                }),
                (Some(only), None) | (None, Some(only)) => Some(only),
                (None, None) => None,
            },
        }
    }

    /// Moves the divider of the closest split along `axis` that contains `target` by `delta`
    /// (a share of the split, positive = towards the second child). Returns true if anything
    /// changed.
    pub(crate) fn resize(&mut self, target: PaneId, axis: Axis, delta: f32) -> bool {
        self.resize_nearest(target, axis, delta).unwrap_or(false)
    }

    /// `Some(changed)` once the closest split along `axis` around `target` was found (even
    /// if it is already at its limit), `None` if there is none in this subtree.
    fn resize_nearest(&mut self, target: PaneId, axis: Axis, delta: f32) -> Option<bool> {
        match self {
            Self::Leaf(_) => None,
            Self::Split {
                axis: own,
                ratio,
                a,
                b,
            } => {
                let in_a = a.contains(target);
                if !in_a && !b.contains(target) {
                    return None;
                }
                let child = if in_a { a } else { b };
                if let Some(changed) = child.resize_nearest(target, axis, delta) {
                    return Some(changed);
                }
                if *own != axis {
                    return None;
                }
                let new = (*ratio + delta).clamp(MIN_RATIO, 1.0 - MIN_RATIO);
                let changed = (new - *ratio).abs() > f32::EPSILON;
                *ratio = new;
                Some(changed)
            }
        }
    }

    /// Sets the ratio of the split at `path` (from a divider drag).
    pub(crate) fn set_ratio(&mut self, path: &[bool], value: f32) {
        match (self, path.split_first()) {
            (Self::Split { ratio, .. }, None) => {
                *ratio = value.clamp(MIN_RATIO, 1.0 - MIN_RATIO);
            }
            (Self::Split { a, b, .. }, Some((second, rest))) => {
                if *second { b } else { a }.set_ratio(rest, value);
            }
            (Self::Leaf(_), _) => {}
        }
    }
}

/// The pane next to `from` in `dir`: the closest one that overlaps it on the other axis.
pub(crate) fn neighbor(layout: &Layout, from: PaneId, dir: Direction) -> Option<PaneId> {
    let [fx, fy, fw, fh] = layout.area(from)?;
    let overlap = |a0: f32, a1: f32, b0: f32, b1: f32| a0.max(b0) < a1.min(b1);
    layout
        .panes
        .iter()
        .filter(|(id, _)| *id != from)
        .filter_map(|&(id, [x, y, w, h])| {
            let distance = match dir {
                Direction::Left if x + w <= fx + 0.5 => fx - (x + w),
                Direction::Right if x >= fx + fw - 0.5 => x - (fx + fw),
                Direction::Up if y + h <= fy + 0.5 => fy - (y + h),
                Direction::Down if y >= fy + fh - 0.5 => y - (fy + fh),
                _ => return None,
            };
            let lined_up = match dir.axis() {
                Axis::Horizontal => overlap(y, y + h, fy, fy + fh),
                Axis::Vertical => overlap(x, x + w, fx, fx + fw),
            };
            lined_up.then_some((distance, id))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
}

/// One tab: a pane tree, the focused pane and whether it is maximized.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tab {
    pub(crate) root: PaneNode,
    pub(crate) focused: PaneId,
    /// Show only the focused pane.
    pub(crate) zoomed: bool,
    /// A name the user gave the tab.
    pub(crate) title_override: Option<String>,
}

impl Tab {
    pub(crate) fn new(pane: PaneId) -> Self {
        Self {
            root: PaneNode::Leaf(pane),
            focused: pane,
            zoomed: false,
            title_override: None,
        }
    }

    /// The layout of this tab in `area`.
    pub(crate) fn layout(&self, area: [f32; 4], gap: f32) -> Layout {
        if self.zoomed {
            Layout {
                panes: vec![(self.focused, area)],
                dividers: Vec::new(),
            }
        } else {
            self.root.layout(area, gap)
        }
    }
}

/// The tabs of one window.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Tabs {
    pub(crate) list: Vec<Tab>,
    pub(crate) active: usize,
}

impl Tabs {
    pub(crate) fn active(&self) -> Option<&Tab> {
        self.list.get(self.active)
    }

    pub(crate) fn active_mut(&mut self) -> Option<&mut Tab> {
        self.list.get_mut(self.active)
    }

    /// The focused pane of the active tab.
    pub(crate) fn focused(&self) -> Option<PaneId> {
        self.active().map(|t| t.focused)
    }

    /// Adds a tab after the active one and activates it.
    pub(crate) fn open(&mut self, pane: PaneId) {
        let at = if self.list.is_empty() {
            0
        } else {
            self.active + 1
        };
        self.list.insert(at, Tab::new(pane));
        self.active = at;
    }

    /// Removes a pane wherever it is. Closes its tab if it was the last pane there. Returns
    /// true if no tabs are left.
    pub(crate) fn remove_pane(&mut self, pane: PaneId) -> bool {
        if let Some(index) = self.list.iter().position(|t| t.root.contains(pane)) {
            let tab = &mut self.list[index];
            let root = std::mem::replace(&mut tab.root, PaneNode::Leaf(0));
            match root.remove(pane) {
                Some(root) => {
                    if tab.focused == pane {
                        tab.focused = root.panes()[0];
                    }
                    tab.root = root;
                    tab.zoomed = false;
                }
                None => {
                    self.list.remove(index);
                    if self.active > index || self.active >= self.list.len() {
                        self.active = self.active.saturating_sub(1);
                    }
                }
            }
        }
        self.list.is_empty()
    }

    /// Activates the tab holding `pane` and focuses it.
    pub(crate) fn focus_pane(&mut self, pane: PaneId) -> bool {
        let Some(index) = self.list.iter().position(|t| t.root.contains(pane)) else {
            return false;
        };
        self.active = index;
        self.list[index].focused = pane;
        true
    }

    /// Cycles the active tab by `step` (wrapping).
    pub(crate) fn cycle(&mut self, step: isize) {
        let n = self.list.len() as isize;
        if n > 0 {
            self.active = (self.active as isize + step).rem_euclid(n) as usize;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: [f32; 4] = [0.0, 0.0, 1001.0, 601.0];

    fn three() -> PaneNode {
        // 1 | (2 over 3)
        let mut root = PaneNode::Leaf(1);
        assert!(root.split(1, 2, Axis::Horizontal));
        assert!(root.split(2, 3, Axis::Vertical));
        root
    }

    #[test]
    fn layout_tiles_the_area_without_overlap() {
        let layout = three().layout(AREA, 1.0);
        assert_eq!(layout.panes.len(), 3);
        assert_eq!(layout.dividers.len(), 2);
        let area: f32 = layout
            .panes
            .iter()
            .map(|(_, [_, _, w, h])| w * h)
            .sum::<f32>()
            + layout
                .dividers
                .iter()
                .map(|d| d.rect[2] * d.rect[3])
                .sum::<f32>();
        // The vertical divider inside the right half is counted once.
        assert!((area - AREA[2] * AREA[3]).abs() < 1.0, "{area}");
        assert_eq!(layout.area(1), Some([0.0, 0.0, 500.0, 601.0]));
        assert_eq!(layout.pane_at(800.0, 500.0), Some(3));
    }

    #[test]
    fn neighbours_follow_the_geometry() {
        let layout = three().layout(AREA, 1.0);
        assert_eq!(neighbor(&layout, 1, Direction::Right), Some(2));
        assert_eq!(neighbor(&layout, 3, Direction::Up), Some(2));
        assert_eq!(neighbor(&layout, 3, Direction::Left), Some(1));
        assert_eq!(neighbor(&layout, 1, Direction::Left), None);
        assert_eq!(neighbor(&layout, 2, Direction::Up), None);
    }

    #[test]
    fn removing_a_pane_promotes_its_sibling() {
        let root = three().remove(2).unwrap();
        assert_eq!(root.panes(), vec![1, 3]);
        assert!(matches!(
            root,
            PaneNode::Split {
                axis: Axis::Horizontal,
                ..
            }
        ));
        assert_eq!(PaneNode::Leaf(1).remove(1), None);
    }

    #[test]
    fn resize_moves_the_nearest_matching_divider_and_clamps() {
        let mut root = three();
        assert!(root.resize(3, Axis::Horizontal, 0.2)); // outer split
        let PaneNode::Split { ratio, .. } = &root else {
            panic!()
        };
        assert!((ratio - 0.7).abs() < 1e-6);
        assert!(root.resize(3, Axis::Vertical, -0.9)); // inner split, clamped
        let PaneNode::Split { b, .. } = &root else {
            panic!()
        };
        let PaneNode::Split { ratio, .. } = b.as_ref() else {
            panic!()
        };
        assert!((ratio - MIN_RATIO).abs() < 1e-6);
        // At its limit the inner divider stays put, and the outer one is not moved instead.
        let before = root.clone();
        assert!(!root.resize(3, Axis::Vertical, -0.5));
        assert_eq!(root, before);
        assert!(!PaneNode::Leaf(1).resize(1, Axis::Vertical, 0.1));
    }

    #[test]
    fn dividers_carry_a_path_for_dragging() {
        let mut root = three();
        let layout = root.layout(AREA, 1.0);
        let inner = layout
            .dividers
            .iter()
            .find(|d| d.axis == Axis::Vertical)
            .unwrap();
        assert_eq!(inner.path, vec![true]);
        root.set_ratio(&inner.path.clone(), 0.25);
        let after = root.layout(AREA, 1.0);
        let [_, _, _, h2] = after.area(2).unwrap();
        assert!((h2 - 150.0).abs() <= 1.0, "{h2}");
    }

    #[test]
    fn tabs_open_close_and_cycle() {
        let mut tabs = Tabs::default();
        tabs.open(1);
        tabs.open(2);
        assert_eq!(tabs.focused(), Some(2));
        tabs.cycle(1);
        assert_eq!(tabs.focused(), Some(1));
        tabs.cycle(-1);
        assert_eq!(tabs.focused(), Some(2));
        assert!(tabs.active_mut().unwrap().root.split(2, 3, Axis::Vertical));
        assert!(!tabs.remove_pane(2));
        assert_eq!(tabs.focused(), Some(3));
        assert!(!tabs.remove_pane(3)); // closes tab 2, tab 1 remains
        assert_eq!(tabs.list.len(), 1);
        assert_eq!(tabs.focused(), Some(1));
        assert!(tabs.remove_pane(1));
    }

    #[test]
    fn zoom_shows_only_the_focused_pane() {
        let mut tab = Tab::new(1);
        tab.root = three();
        tab.focused = 3;
        tab.zoomed = true;
        let layout = tab.layout(AREA, 1.0);
        assert_eq!(layout.panes, vec![(3, AREA)]);
    }
}
