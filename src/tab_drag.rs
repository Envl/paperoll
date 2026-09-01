use std::collections::HashMap;

use gpui::{Bounds, Pixels, px};
use uuid::Uuid;

const DRAG_THRESHOLD: Pixels = px(4.);

pub enum TabDragFinish {
    Click(Uuid),
    Reorder(Vec<Uuid>),
}

#[derive(Default)]
pub struct TabDragState {
    preview_order: Option<Vec<Uuid>>,
    dragged_id: Option<Uuid>,
    start_x: Pixels,
    compensation: Pixels,
    drag_offset: Pixels,
    moved: bool,
    frames: HashMap<Uuid, Bounds<Pixels>>,
    settling: HashMap<Uuid, (Pixels, u64)>,
    animation_epoch: u64,
}

impl TabDragState {
    pub fn order(&self, canonical: impl IntoIterator<Item = Uuid>) -> Vec<Uuid> {
        self.preview_order
            .clone()
            .unwrap_or_else(|| canonical.into_iter().collect())
    }

    pub fn record_frame(&mut self, id: Uuid, bounds: Bounds<Pixels>) {
        self.frames.insert(id, bounds);
    }

    pub fn begin(&mut self, id: Uuid, pointer_x: Pixels, canonical: Vec<Uuid>) {
        self.preview_order = Some(canonical);
        self.dragged_id = Some(id);
        self.start_x = pointer_x;
        self.compensation = px(0.);
        self.drag_offset = px(0.);
        self.moved = false;
    }

    pub fn update(&mut self, pointer_x: Pixels) -> bool {
        let Some(dragged_id) = self.dragged_id else {
            return false;
        };

        let raw_offset = pointer_x - self.start_x;
        if !self.moved && raw_offset.as_f32().abs() < DRAG_THRESHOLD.as_f32() {
            return false;
        }

        self.moved = true;
        self.drag_offset = raw_offset + self.compensation;
        let mut changed = true;

        while changed {
            changed = false;
            let Some(order) = self.preview_order.as_mut() else {
                break;
            };
            let Some(index) = order.iter().position(|id| *id == dragged_id) else {
                break;
            };
            let Some(dragged_frame) = self.frames.get(&dragged_id).copied() else {
                break;
            };
            let dragged_center = dragged_frame.center().x + self.drag_offset;

            if index > 0 {
                let neighbor_id = order[index - 1];
                if let Some(neighbor_frame) = self.frames.get(&neighbor_id).copied()
                    && dragged_center < neighbor_frame.center().x
                {
                    order.swap(index - 1, index);
                    self.compensation += neighbor_frame.size.width;
                    self.drag_offset += neighbor_frame.size.width;

                    let mut new_dragged_frame = dragged_frame;
                    new_dragged_frame.origin.x = neighbor_frame.origin.x;
                    let mut new_neighbor_frame = neighbor_frame;
                    new_neighbor_frame.origin.x =
                        neighbor_frame.origin.x + dragged_frame.size.width;
                    self.frames.insert(dragged_id, new_dragged_frame);
                    self.frames.insert(neighbor_id, new_neighbor_frame);
                    self.settle(neighbor_id, -dragged_frame.size.width);
                    changed = true;
                    continue;
                }
            }

            if index + 1 < order.len() {
                let neighbor_id = order[index + 1];
                if let Some(neighbor_frame) = self.frames.get(&neighbor_id).copied()
                    && dragged_center > neighbor_frame.center().x
                {
                    order.swap(index, index + 1);
                    self.compensation -= neighbor_frame.size.width;
                    self.drag_offset -= neighbor_frame.size.width;

                    let mut new_dragged_frame = dragged_frame;
                    new_dragged_frame.origin.x = dragged_frame.origin.x + neighbor_frame.size.width;
                    let mut new_neighbor_frame = neighbor_frame;
                    new_neighbor_frame.origin.x = dragged_frame.origin.x;
                    self.frames.insert(dragged_id, new_dragged_frame);
                    self.frames.insert(neighbor_id, new_neighbor_frame);
                    self.settle(neighbor_id, dragged_frame.size.width);
                    changed = true;
                }
            }
        }

        true
    }

    pub fn finish(&mut self) -> Option<TabDragFinish> {
        let dragged_id = self.dragged_id.take()?;
        let result = if self.moved {
            self.settle(dragged_id, self.drag_offset);
            TabDragFinish::Reorder(self.preview_order.take().unwrap_or_default())
        } else {
            self.preview_order = None;
            TabDragFinish::Click(dragged_id)
        };
        self.compensation = px(0.);
        self.drag_offset = px(0.);
        self.moved = false;
        Some(result)
    }

    pub fn drag_offset(&self, id: Uuid) -> Option<Pixels> {
        (self.dragged_id == Some(id) && self.moved).then_some(self.drag_offset)
    }

    pub fn settling_offset(&self, id: Uuid) -> Option<(Pixels, u64)> {
        self.settling.get(&id).copied()
    }

    fn settle(&mut self, id: Uuid, offset: Pixels) {
        self.animation_epoch = self.animation_epoch.wrapping_add(1);
        self.settling.insert(id, (offset, self.animation_epoch));
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Bounds, point, size};

    use super::*;

    fn frame(x: f32, width: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(0.)), size(px(width), px(32.)))
    }

    #[test]
    fn click_does_not_reorder() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut state = TabDragState::default();
        state.begin(first, px(10.), vec![first, second]);

        assert!(matches!(state.finish(), Some(TabDragFinish::Click(id)) if id == first));
    }

    #[test]
    fn crossing_neighbor_midpoint_swaps_preview_order() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut state = TabDragState::default();
        state.record_frame(first, frame(0., 80.));
        state.record_frame(second, frame(80., 120.));
        state.begin(first, px(40.), vec![first, second]);

        assert!(state.update(px(151.)));
        assert_eq!(state.order([first, second]), vec![second, first]);
        assert!(matches!(
            state.finish(),
            Some(TabDragFinish::Reorder(order)) if order == vec![second, first]
        ));
    }
}
