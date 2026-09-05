use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

#[cfg(feature = "router")]
pub(crate) mod router;
#[cfg(feature = "router")]
pub(crate) use router::FbdevPlatform;

const TOUCH_TRANSFORM_COUNT: u8 = 4;
const DEFAULT_TOUCH_TRANSFORM: u8 = 0;

static TOUCH_TRANSFORM: AtomicU8 = AtomicU8::new(DEFAULT_TOUCH_TRANSFORM);
static TOUCH_ACTIVITY: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TouchTransform {
    RotateClockwise,
    RotateCounterClockwise,
    MirrorDiagonal,
    MirrorAntiDiagonal,
}

impl TouchTransform {
    const fn from_index(index: u8) -> Self {
        match index % TOUCH_TRANSFORM_COUNT {
            0 => Self::RotateClockwise,
            1 => Self::RotateCounterClockwise,
            2 => Self::MirrorDiagonal,
            _ => Self::MirrorAntiDiagonal,
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::RotateClockwise => "mapping 1/4",
            Self::RotateCounterClockwise => "mapping 2/4",
            Self::MirrorDiagonal => "mapping 3/4",
            Self::MirrorAntiDiagonal => "mapping 4/4",
        }
    }

    pub(crate) fn translate(
        self,
        raw_x: u32,
        raw_y: u32,
        raw_width: u32,
        raw_height: u32,
        logical_width: u32,
        logical_height: u32,
    ) -> (f32, f32) {
        let x = scale(raw_x, raw_width, logical_height);
        let y = scale(raw_y, raw_height, logical_width);
        let max_x = logical_width.saturating_sub(1) as f32;
        let max_y = logical_height.saturating_sub(1) as f32;

        match self {
            Self::RotateClockwise => (y, max_y - x),
            Self::RotateCounterClockwise => (max_x - y, x),
            Self::MirrorDiagonal => (y, x),
            Self::MirrorAntiDiagonal => (max_x - y, max_y - x),
        }
    }
}

fn scale(value: u32, source_size: u32, target_size: u32) -> f32 {
    let source_max = source_size.saturating_sub(1).max(1);
    let target_max = target_size.saturating_sub(1);
    value.min(source_max) as f32 * target_max as f32 / source_max as f32
}

pub(crate) fn touch_transform() -> TouchTransform {
    TouchTransform::from_index(TOUCH_TRANSFORM.load(Ordering::Relaxed))
}

pub(crate) fn advance_touch_transform() -> TouchTransform {
    let current = TOUCH_TRANSFORM.load(Ordering::Relaxed);
    let next = (current + 1) % TOUCH_TRANSFORM_COUNT;
    TOUCH_TRANSFORM.store(next, Ordering::Relaxed);
    TouchTransform::from_index(next)
}

pub(crate) fn record_touch_activity() {
    TOUCH_ACTIVITY.store(true, Ordering::Relaxed);
}

pub(crate) fn take_touch_activity() -> bool {
    TOUCH_ACTIVITY.swap(false, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::TouchTransform;

    const RAW_WIDTH: u32 = 240;
    const RAW_HEIGHT: u32 = 320;
    const LOGICAL_WIDTH: u32 = 320;
    const LOGICAL_HEIGHT: u32 = 240;

    fn mapped(transform: TouchTransform, x: u32, y: u32) -> (u32, u32) {
        let (x, y) =
            transform.translate(x, y, RAW_WIDTH, RAW_HEIGHT, LOGICAL_WIDTH, LOGICAL_HEIGHT);
        (x as u32, y as u32)
    }

    #[test]
    fn transforms_cover_all_swapped_axis_orientations() {
        assert_eq!(mapped(TouchTransform::RotateClockwise, 0, 0), (0, 239));
        assert_eq!(
            mapped(TouchTransform::RotateCounterClockwise, 0, 0),
            (319, 0)
        );
        assert_eq!(mapped(TouchTransform::MirrorDiagonal, 0, 0), (0, 0));
        assert_eq!(mapped(TouchTransform::MirrorAntiDiagonal, 0, 0), (319, 239));
    }
}
