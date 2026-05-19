pub const WIPE_WIDTH: usize = 8;

pub const HOLD_END: usize = 9;

pub const HOLD_START: usize = 30;

pub const WIPE_INTERVAL_MS: u128 = 40;

pub const ACTIVE: char = '■';

pub const INACTIVE: char = '⬝';

pub const TRAIL_LENGTH: i32 = 6;

pub const CYCLE_FRAMES: usize = WIPE_WIDTH + HOLD_END + (WIPE_WIDTH - 1) + HOLD_START;

pub fn wipe_frame_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    wipe_frame_for(now_ms)
}

pub fn wipe_frame_for(now_ms: u128) -> String {
    let frame_index = ((now_ms / WIPE_INTERVAL_MS) % CYCLE_FRAMES as u128) as usize;
    render_frame(frame_index)
}

pub fn render_frame(frame_index: usize) -> String {
    let mut out = String::with_capacity(WIPE_WIDTH * 3);
    for char_index in 0..WIPE_WIDTH {
        let color_index = calculate_color_index(frame_index, char_index);
        let active = (0..TRAIL_LENGTH).contains(&color_index);
        out.push(if active { ACTIVE } else { INACTIVE });
    }
    out
}

fn calculate_color_index(frame_index: usize, char_index: usize) -> i32 {
    let total_chars = WIPE_WIDTH;
    let forward_frames = total_chars;
    let hold_end_frames = HOLD_END;
    let backward_frames = total_chars - 1;

    let (active_position, is_holding, hold_progress, is_moving_forward) =
        if frame_index < forward_frames {
            (frame_index as i32, false, 0i32, true)
        } else if frame_index < forward_frames + hold_end_frames {
            (
                total_chars as i32 - 1,
                true,
                (frame_index - forward_frames) as i32,
                true,
            )
        } else if frame_index < forward_frames + hold_end_frames + backward_frames {
            let backward_index = frame_index - forward_frames - hold_end_frames;
            (
                total_chars as i32 - 2 - backward_index as i32,
                false,
                0,
                false,
            )
        } else {
            (
                0,
                true,
                (frame_index - forward_frames - hold_end_frames - backward_frames) as i32,
                false,
            )
        };

    let directional_distance = if is_moving_forward {
        active_position - char_index as i32
    } else {
        char_index as i32 - active_position
    };

    if is_holding {
        return directional_distance + hold_progress;
    }

    if directional_distance > 0 && directional_distance < TRAIL_LENGTH {
        return directional_distance;
    }
    if directional_distance == 0 {
        return 0;
    }
    -1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_zero_lights_only_the_leftmost_cell() {
        assert_eq!(render_frame(0), "■⬝⬝⬝⬝⬝⬝⬝");
    }

    #[test]
    fn frame_at_end_of_forward_sweep_lights_rightmost_plus_trail() {
        let f = render_frame(WIPE_WIDTH - 1);
        assert_eq!(f, "⬝⬝■■■■■■", "frame={f}");
    }

    #[test]
    fn frame_holding_at_right_progressively_drains_trail() {
        let hold_first = render_frame(WIPE_WIDTH);
        assert_eq!(hold_first, "⬝⬝■■■■■■");

        let hold_mid = render_frame(WIPE_WIDTH + 3);
        assert_eq!(hold_mid, "⬝⬝⬝⬝⬝■■■");
    }

    #[test]
    fn frame_backward_sweep_lights_cell_to_the_left_of_previous() {
        let start_of_backward = WIPE_WIDTH + HOLD_END;
        let f = render_frame(start_of_backward);
        assert_eq!(f, "⬝⬝⬝⬝⬝⬝■■");
    }

    #[test]
    fn cycle_wraps_via_modulo() {
        let cycle_ms = (CYCLE_FRAMES as u128) * WIPE_INTERVAL_MS;
        assert_eq!(wipe_frame_for(cycle_ms), wipe_frame_for(0));
        assert_eq!(wipe_frame_for(cycle_ms + 41), wipe_frame_for(40));
    }

    #[test]
    fn frame_string_is_always_exactly_eight_cells() {
        for i in 0..CYCLE_FRAMES {
            let f = render_frame(i);
            assert_eq!(
                f.chars().count(),
                WIPE_WIDTH,
                "frame {i} should be {} cells: {f:?}",
                WIPE_WIDTH,
            );
        }
    }

    #[test]
    fn cycle_frames_matches_opencode_total_formula() {
        assert_eq!(
            CYCLE_FRAMES,
            WIPE_WIDTH + HOLD_END + (WIPE_WIDTH - 1) + HOLD_START,
        );
        assert_eq!(CYCLE_FRAMES, 54);
    }
}
