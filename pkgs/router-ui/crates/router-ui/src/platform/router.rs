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
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel};
use slint::platform::{Platform, PointerEventButton, WindowAdapter, WindowEvent};
use slint::{LogicalPosition, PhysicalSize, PlatformError};

const FRAME_BUDGET: Duration = Duration::from_millis(33);

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
    rotate_touch: bool,
    raw_touch_w: u32,
    raw_touch_h: u32,
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

        let rotate_touch = width > height || fb.var_screen_info.rotate != 0;

        let (tx, rx) = mpsc::channel();
        let touch_path_owned = touch_path.to_string();
        thread::spawn(move || {
            if let Err(e) = touch_loop(&touch_path_owned, &tx) {
                warn!("touch thread exited: {e:?}");
            }
        });

        let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        window.set_size(PhysicalSize::new(width, height));

        Ok(Self {
            window,
            fb: RefCell::new(fb),
            width,
            height,
            start: Instant::now(),
            rx: RefCell::new(rx),
            rotate_touch,
            raw_touch_w: 240,
            raw_touch_h: 320,
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
        let mut scratch = vec![Rgb565Pixel(0); width * height];
        let mut last_frame = Instant::now();

        info!(
            "event loop starting ({width}x{height} stride={stride} rot_touch={})",
            self.rotate_touch
        );

        while !SHOULD_EXIT.load(Ordering::SeqCst) {
            self.drain_input();
            slint::platform::update_timers_and_animations();

            let dirty = self.window.draw_if_needed(|renderer| {
                renderer.render(&mut scratch, width);
            });
            if dirty {
                self.flush(&scratch, stride);
            }

            let elapsed = last_frame.elapsed();
            if elapsed < FRAME_BUDGET {
                thread::sleep(FRAME_BUDGET - elapsed);
            }
            last_frame = Instant::now();
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
        let (x, y) = match ev {
            TouchEvent::Down { x, y } | TouchEvent::Move { x, y } | TouchEvent::Up { x, y } => {
                (x, y)
            }
        };
        debug!("touch {ev:?} -> ({x}, {y})");
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

    fn flush(&self, scratch: &[Rgb565Pixel], stride_u16: usize) {
        let w = self.width as usize;
        let h = self.height as usize;
        let mut fb = self.fb.borrow_mut();
        let frame = fb.frame.as_mut();

        let all_bytes: &[u8] = bytemuck::cast_slice(scratch);

        if stride_u16 == w {
            let n = all_bytes.len().min(frame.len());
            frame[..n].copy_from_slice(&all_bytes[..n]);
            return;
        }

        let row_bytes = w * 2;
        for y in 0..h {
            let src_off = y * row_bytes;
            let src_end = src_off + row_bytes;
            let dst_off = y * stride_u16 * 2;
            let dst_end = dst_off + row_bytes;
            if dst_end > frame.len() || src_end > all_bytes.len() {
                break;
            }
            frame[dst_off..dst_end].copy_from_slice(&all_bytes[src_off..src_end]);
        }
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

fn touch_loop(path: &str, tx: &mpsc::Sender<TouchEvent>) -> Result<()> {
    use evdev::{AbsoluteAxisCode, Device, EventSummary, KeyCode};

    let mut dev = Device::open(path).map_err(|e| anyhow!("open {path}: {e}"))?;
    info!("touch: {} ({})", path, dev.name().unwrap_or("?"));

    let (raw_w, raw_h) = dev.get_abs_state().ok().map_or((240, 320), |abs| {
        let x = abs[AbsoluteAxisCode::ABS_X.0 as usize].maximum.max(1) as u32 + 1;
        let y = abs[AbsoluteAxisCode::ABS_Y.0 as usize].maximum.max(1) as u32 + 1;
        (x, y)
    });
    info!("touch raw range: {raw_w}x{raw_h}");

    let mut cx = 0i32;
    let mut cy = 0i32;
    let mut pressed = false;
    let mut had_move = false;

    loop {
        let events = dev
            .fetch_events()
            .map_err(|e| anyhow!("fetch_events: {e}"))?;
        for ev in events {
            match ev.destructure() {
                EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_X, v) => {
                    cx = v;
                    had_move = true;
                }
                EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_Y, v) => {
                    cy = v;
                    had_move = true;
                }
                EventSummary::Key(_, KeyCode::BTN_TOUCH, v) => {
                    let now_pressed = v != 0;
                    let (lx, ly) = translate(cx as u32, cy as u32, raw_w, raw_h);
                    if now_pressed && !pressed {
                        let _ = tx.send(TouchEvent::Down { x: lx, y: ly });
                    } else if !now_pressed && pressed {
                        let _ = tx.send(TouchEvent::Up { x: lx, y: ly });
                    }
                    pressed = now_pressed;
                }
                EventSummary::Synchronization(_, _, _) => {
                    if pressed && had_move {
                        let (lx, ly) = translate(cx as u32, cy as u32, raw_w, raw_h);
                        let _ = tx.send(TouchEvent::Move { x: lx, y: ly });
                    }
                    had_move = false;
                }
                _ => {}
            }
        }
    }
}

const fn translate(px: u32, py: u32, _raw_w: u32, _raw_h: u32) -> (f32, f32) {
    (px as f32, py as f32)
}

fn read_int(path: &str) -> std::io::Result<i64> {
    let s = fs::read_to_string(path)?;
    s.trim()
        .parse::<i64>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[allow(dead_code)]
fn _force_use(_: Arc<()>, _: PathBuf, _: &dyn Fn() -> Result<(), Box<dyn std::error::Error>>) {}
