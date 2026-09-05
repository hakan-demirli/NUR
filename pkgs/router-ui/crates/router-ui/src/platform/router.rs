use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use framebuffer::Framebuffer;
use log::{debug, error, info, warn};
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, RenderingRotation, RepaintBufferType, Rgb565Pixel,
};
use slint::platform::{Platform, PointerEventButton, WindowAdapter, WindowEvent};
use slint::{LogicalPosition, PhysicalSize, PlatformError};

const IDLE_POLL: Duration = Duration::from_millis(16);
const MIN_POLL: Duration = Duration::from_millis(2);
const FRAME_INTERVAL: Duration = Duration::from_millis(33);

const STATS_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Default)]
struct FrameStats {
    frames: u32,
    render_us: u64,
    worst_us: u64,
    damaged_px: u64,
}

#[derive(Debug)]
struct FrameSummary {
    frames: u32,
    avg_us: u64,
    worst_us: u64,
    avg_damage_pct: f64,
}

impl FrameStats {
    fn record(&mut self, us: u64, damaged_px: u32) {
        self.frames += 1;
        self.render_us += us;
        self.worst_us = self.worst_us.max(us);
        self.damaged_px += u64::from(damaged_px);
    }

    fn drain(&mut self, panel_px: u64) -> Option<FrameSummary> {
        if self.frames == 0 {
            return None;
        }
        let frames = u64::from(self.frames);
        let out = FrameSummary {
            frames: self.frames,
            avg_us: self.render_us / frames,
            worst_us: self.worst_us,
            avg_damage_pct: (self.damaged_px as f64 * 100.0)
                / (frames as f64 * panel_px.max(1) as f64),
        };
        *self = Self::default();
        Some(out)
    }
}

const VTCON_PATH: &str = "/sys/class/vtconsole/vtcon1/bind";
const BL_BRIGHTNESS: &str = "/sys/class/backlight/backlight/brightness";
const FB_BLANK: &str = "/sys/class/graphics/fb0/blank";

static SHOULD_EXIT: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy)]
enum TouchEvent {
    Down { x: f32, y: f32 },
    Move { x: f32, y: f32 },
    Up { x: f32, y: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TouchPhase {
    Down,
    Move,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RawTouchEvent {
    phase: TouchPhase,
    x: i32,
    y: i32,
}

#[derive(Debug, Default)]
struct TouchFrame {
    x: i32,
    y: i32,
    pressed: bool,
    pending_pressed: Option<bool>,
    position_changed: bool,
}

impl TouchFrame {
    fn set_x(&mut self, x: i32) {
        self.x = x;
        self.position_changed = true;
    }

    fn set_y(&mut self, y: i32) {
        self.y = y;
        self.position_changed = true;
    }

    fn set_pressed(&mut self, pressed: bool) {
        self.pending_pressed = Some(pressed);
    }

    fn finish(&mut self) -> Option<RawTouchEvent> {
        let next_pressed = self.pending_pressed.take().unwrap_or(self.pressed);
        let phase = match (self.pressed, next_pressed, self.position_changed) {
            (false, true, _) => Some(TouchPhase::Down),
            (true, false, _) => Some(TouchPhase::Up),
            (true, true, true) => Some(TouchPhase::Move),
            _ => None,
        };

        self.pressed = next_pressed;
        self.position_changed = false;
        phase.map(|phase| RawTouchEvent {
            phase,
            x: self.x,
            y: self.y,
        })
    }
}

#[derive(Debug, Default)]
struct SavedState {
    backlight: Option<u32>,
    vtcon_was_bound: bool,
}

pub(crate) struct FbdevPlatform {
    window: Rc<MinimalSoftwareWindow>,
    fb: RefCell<Framebuffer>,
    width: u32,
    height: u32,
    start: Instant,
    rx: RefCell<Receiver<TouchEvent>>,
    saved: RefCell<SavedState>,
}

impl FbdevPlatform {
    pub(crate) fn new(fb_path: &str, touch_path: &str) -> Result<Self> {
        let saved = SavedState {
            backlight: read_int(BL_BRIGHTNESS).ok().map(|v| v.max(0) as u32),
            vtcon_was_bound: read_int(VTCON_PATH).unwrap_or(0) == 1,
        };

        let _ = fs::write(VTCON_PATH, "0");

        let fb = Framebuffer::new(fb_path).map_err(|e| anyhow!("open {fb_path}: {e:?}"))?;
        let width = fb.var_screen_info.xres;
        let height = fb.var_screen_info.yres;
        let bpp = fb.var_screen_info.bits_per_pixel;
        if bpp != 16 {
            return Err(anyhow!("expected 16bpp fb, got {bpp}"));
        }
        info!(
            "fb0 ready: {width}x{height} @ {bpp}bpp ({} bytes line)",
            fb.fix_screen_info.line_length
        );

        let (tx, rx) = mpsc::channel();
        let touch_path_owned = touch_path.to_string();
        let logical_width = height;
        let logical_height = width;
        thread::spawn(move || {
            if let Err(e) = touch_loop(&touch_path_owned, &tx, logical_width, logical_height) {
                warn!("touch thread exited: {e:?}");
            }
        });

        let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        window.set_size(PhysicalSize::new(height, width));

        Ok(Self {
            window,
            fb: RefCell::new(fb),
            width,
            height,
            start: Instant::now(),
            rx: RefCell::new(rx),
            saved: RefCell::new(saved),
        })
    }
}

impl Platform for FbdevPlatform {
    fn create_window_adapter(&self) -> std::result::Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }

    fn duration_since_start(&self) -> core::time::Duration {
        self.start.elapsed()
    }

    fn run_event_loop(&self) -> std::result::Result<(), PlatformError> {
        self.window.request_redraw();

        let width = self.width as usize;
        let height = self.height as usize;
        let stride = self.fb.borrow().fix_screen_info.line_length as usize / 2;
        let needed = stride * height;
        let available = self.fb.borrow().frame.len() / std::mem::size_of::<Rgb565Pixel>();
        if available < needed {
            error!("framebuffer too small: {available} px < {needed} px needed");
            return Ok(());
        }

        let mut scratch = vec![Rgb565Pixel(0); needed];
        let mut next_frame = Instant::now();
        let mut stats = FrameStats::default();
        let mut last_stats = Instant::now();

        info!("event loop starting (panel {width}x{height}, ui {height}x{width} rotated 90, stride={stride}, buffered 30 fps)");

        while !SHOULD_EXIT.load(Ordering::SeqCst) {
            self.drain_input();
            slint::platform::update_timers_and_animations();

            if Instant::now() >= next_frame {
                let began = Instant::now();
                let mut damaged_px = 0u32;
                let drew = self.window.draw_if_needed(|renderer| {
                    renderer.set_rendering_rotation(RenderingRotation::Rotate90);
                    let region = renderer.render(&mut scratch, stride);
                    let size = region.bounding_box_size();
                    damaged_px = size.width.saturating_mul(size.height);
                });

                if drew {
                    self.flush(&scratch);
                    stats.record(began.elapsed().as_micros() as u64, damaged_px);
                }
                next_frame = Instant::now() + FRAME_INTERVAL;
            }

            if last_stats.elapsed() >= STATS_INTERVAL {
                if let Some(s) = stats.drain((width * height) as u64) {
                    info!(
                        "render: {} frames, avg {:.1} ms, worst {:.1} ms, avg damage {:.1}% of panel",
                        s.frames,
                        s.avg_us as f64 / 1000.0,
                        s.worst_us as f64 / 1000.0,
                        s.avg_damage_pct
                    );
                }
                last_stats = Instant::now();
            }

            let wait = slint::platform::duration_until_next_timer_update()
                .unwrap_or(IDLE_POLL)
                .min(IDLE_POLL)
                .min(next_frame.saturating_duration_since(Instant::now()))
                .max(MIN_POLL);
            thread::sleep(wait);
        }

        info!("event loop exiting on signal");
        self.cleanup();
        Ok(())
    }
}

impl FbdevPlatform {
    fn drain_input(&self) {
        let rx = self.rx.borrow();
        loop {
            match rx.try_recv() {
                Ok(ev) => self.dispatch(ev),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    warn!("touch channel disconnected");
                    break;
                }
            }
        }
    }

    fn dispatch(&self, ev: TouchEvent) {
        crate::platform::record_touch_activity();
        let (x, y) = match ev {
            TouchEvent::Down { x, y } | TouchEvent::Move { x, y } | TouchEvent::Up { x, y } => {
                (x, y)
            }
        };
        debug!("touch raw {ev:?} -> logical ({x}, {y})");
        let event = match ev {
            TouchEvent::Down { .. } => WindowEvent::PointerPressed {
                position: LogicalPosition::new(x, y),
                button: PointerEventButton::Left,
            },
            TouchEvent::Move { .. } => WindowEvent::PointerMoved {
                position: LogicalPosition::new(x, y),
            },
            TouchEvent::Up { .. } => WindowEvent::PointerReleased {
                position: LogicalPosition::new(x, y),
                button: PointerEventButton::Left,
            },
        };
        self.window.dispatch_event(event);
    }

    fn flush(&self, scratch: &[Rgb565Pixel]) {
        let bytes: &[u8] = bytemuck::cast_slice(scratch);
        let mut fb = self.fb.borrow_mut();
        fb.frame[..bytes.len()].copy_from_slice(bytes);
    }

    fn cleanup(&self) {
        let saved = self.saved.borrow();
        let _ = saved.vtcon_was_bound;

        if let Err(e) = fs::write(FB_BLANK, "0") {
            debug!("cleanup: unblank fb: {e}");
        }

        if let Ok(mut fb) = self.fb.try_borrow_mut() {
            let frame = fb.frame.as_mut();
            for b in frame.iter_mut() {
                *b = 0;
            }
        }

        if let Some(b) = saved.backlight {
            if let Err(e) = fs::write(BL_BRIGHTNESS, b.to_string()) {
                debug!("cleanup: restore backlight: {e}");
            }
        }
    }
}

pub(crate) fn install_signal_handler() -> Result<()> {
    extern "C" fn handler(_sig: libc::c_int) {
        SHOULD_EXIT.store(true, Ordering::SeqCst);
    }
    #[allow(unsafe_code)]
    unsafe {
        libc::signal(libc::SIGTERM, handler as libc::sighandler_t);
        libc::signal(libc::SIGINT, handler as libc::sighandler_t);
        libc::signal(libc::SIGHUP, handler as libc::sighandler_t);
    }
    Ok(())
}

fn touch_loop(
    path: &str,
    tx: &mpsc::Sender<TouchEvent>,
    logical_width: u32,
    logical_height: u32,
) -> Result<()> {
    use evdev::{AbsoluteAxisCode, Device, EventSummary, KeyCode, SynchronizationCode};

    let mut dev = Device::open(path).map_err(|e| anyhow!("open {path}: {e}"))?;
    info!("touch: {} ({})", path, dev.name().unwrap_or("?"));

    let (raw_w, raw_h) = dev.get_abs_state().ok().map_or((240, 320), |abs| {
        let x = abs[AbsoluteAxisCode::ABS_X.0 as usize].maximum.max(1) as u32 + 1;
        let y = abs[AbsoluteAxisCode::ABS_Y.0 as usize].maximum.max(1) as u32 + 1;
        (x, y)
    });
    info!("touch raw range: {raw_w}x{raw_h}");

    let mut frame = TouchFrame::default();

    loop {
        let events = dev
            .fetch_events()
            .map_err(|e| anyhow!("fetch_events: {e}"))?;
        for ev in events {
            match ev.destructure() {
                EventSummary::AbsoluteAxis(
                    _,
                    AbsoluteAxisCode::ABS_X | AbsoluteAxisCode::ABS_MT_POSITION_X,
                    value,
                ) => {
                    frame.set_x(value);
                }
                EventSummary::AbsoluteAxis(
                    _,
                    AbsoluteAxisCode::ABS_Y | AbsoluteAxisCode::ABS_MT_POSITION_Y,
                    value,
                ) => {
                    frame.set_y(value);
                }
                EventSummary::Key(_, KeyCode::BTN_TOUCH, value) => {
                    frame.set_pressed(value != 0);
                }
                EventSummary::Synchronization(_, SynchronizationCode::SYN_REPORT, _) => {
                    if let Some(raw) = frame.finish() {
                        let (x, y) = crate::platform::touch_transform().translate(
                            raw.x.max(0) as u32,
                            raw.y.max(0) as u32,
                            raw_w,
                            raw_h,
                            logical_width,
                            logical_height,
                        );
                        let event = match raw.phase {
                            TouchPhase::Down => TouchEvent::Down { x, y },
                            TouchPhase::Move => TouchEvent::Move { x, y },
                            TouchPhase::Up => TouchEvent::Up { x, y },
                        };
                        let _ = tx.send(event);
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RawTouchEvent, TouchFrame, TouchPhase};

    #[test]
    fn press_uses_coordinates_from_the_complete_input_frame() {
        let mut frame = TouchFrame::default();
        frame.set_x(10);
        frame.set_y(20);
        frame.set_pressed(true);
        assert_eq!(
            frame.finish(),
            Some(RawTouchEvent {
                phase: TouchPhase::Down,
                x: 10,
                y: 20,
            })
        );

        frame.set_pressed(false);
        assert_eq!(
            frame.finish(),
            Some(RawTouchEvent {
                phase: TouchPhase::Up,
                x: 10,
                y: 20,
            })
        );

        frame.set_pressed(true);
        frame.set_x(180);
        frame.set_y(250);
        assert_eq!(
            frame.finish(),
            Some(RawTouchEvent {
                phase: TouchPhase::Down,
                x: 180,
                y: 250,
            })
        );
    }
}

fn read_int(path: &str) -> std::io::Result<i64> {
    let s = fs::read_to_string(path)?;
    s.trim()
        .parse::<i64>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[allow(dead_code)]
fn _force_use(_: Arc<()>, _: PathBuf, _: &dyn Fn() -> Result<(), Box<dyn std::error::Error>>) {}
