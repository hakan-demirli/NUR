#![allow(clippy::too_many_lines)]

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::Result;
use log::{info, warn};

use router_auth::{Argon2Hasher, Authenticator, BackoffPolicy, User, Verdict};

mod ipc;
mod platform;
mod qr;
mod state;

use ipc::{FanMode, System, WifiKind};
use state::{IdlePolicy, IdleTracker, Screen};

// the workspace-wide deny(unsafe_code) doesn't reject the macro expansion.
#[allow(
    unsafe_code,
    unreachable_pub,
    missing_debug_implementations,
    clippy::all,
    clippy::pedantic,
    clippy::nursery
)]
mod slint_generated {
    slint::include_modules!();
}
use slint_generated::*;

struct App {
    sys: Box<dyn System>,
    hasher: Argon2Hasher,
    auth: Authenticator,
    screen: Screen,
    idle: IdleTracker,
    last_clock_minute: String,
}

impl App {
    fn new(sys: Box<dyn System>) -> Self {
        let idle_policy = match sys.idle_seconds() {
            Some(s) if s > 0 => state::IdlePolicy {
                timeout: std::time::Duration::from_secs(u64::from(s)),
            },
            _ => IdlePolicy::default(),
        };
        log::info!("idle timeout: {:?}", idle_policy.timeout);

        Self {
            sys,
            hasher: Argon2Hasher,
            auth: Authenticator::new(BackoffPolicy::default()),
            screen: Screen::Lockscreen,
            idle: IdleTracker::new(idle_policy),
            last_clock_minute: String::new(),
        }
    }

    fn on_touch(&mut self) {
        self.idle.touch(Instant::now());
    }

    fn on_wake(&mut self) {
        self.screen = self.screen.on_wake();
        if let Err(e) = self.sys.blank_display(false) {
            warn!("unblank: {e}");
        }
        if let Ok(m) = self.sys.max_backlight() {
            let _ = self.sys.set_backlight(m);
        }
    }

    fn on_swipe_up(&mut self) {
        if matches!(self.screen, Screen::Screensaver) {
            self.screen = Screen::Lockscreen;
        }
    }

    fn on_pick_user(&mut self, user: User) {
        self.screen = Screen::PinEntry {
            user,
            digits: String::new(),
            message: String::new(),
            locked_until: self.auth.locked_until(user),
        };
    }

    fn on_pin_digit(&mut self, d: u8) {
        let Screen::PinEntry { ref mut digits, .. } = self.screen else {
            return;
        };
        if digits.len() >= 4 {
            return;
        }
        digits.push(char::from(b'0' + d));

        if digits.len() == 4 {
            self.submit_pin();
        }
    }

    fn on_pin_backspace(&mut self) {
        if let Screen::PinEntry {
            digits, message, ..
        } = &mut self.screen
        {
            digits.pop();
            message.clear();
        }
    }

    fn on_pin_cancel(&mut self) {
        self.screen = Screen::Lockscreen;
    }

    fn on_open_fan(&mut self) {
        if let Screen::AdminMenu { user } = self.screen {
            self.screen = Screen::Fan { user };
        }
    }

    fn on_open_wifi(&mut self) {
        match self.screen {
            Screen::AdminMenu { user } => {
                self.screen = Screen::Wifi {
                    user,
                    kind: WifiKind::Admin,
                }
            }
            Screen::GuestMenu { user } => {
                self.screen = Screen::Wifi {
                    user,
                    kind: WifiKind::Guest,
                }
            }
            _ => {}
        }
    }

    fn on_back(&mut self) {
        self.screen = match &self.screen {
            Screen::Fan { user } => Screen::AdminMenu { user: *user },
            Screen::Wifi { user, .. } => match *user {
                User::Admin => Screen::AdminMenu { user: User::Admin },
                User::Guest => Screen::GuestMenu { user: User::Guest },
            },
            other => other.clone(),
        };
    }

    fn on_lock(&mut self) {
        self.screen = Screen::Blank;
        self.engage_blank();
    }

    fn on_set_fan_mode(&self, mode: FanMode) {
        match self.sys.set_fan_mode(mode) {
            Ok(()) => info!("fan mode -> {}", mode.as_str()),
            Err(e) => warn!("set_fan_mode: {e}"),
        }
    }

    fn submit_pin(&mut self) {
        let Screen::PinEntry { user, digits, .. } = self.screen.clone() else {
            return;
        };
        let cfg = match self.sys.auth_config() {
            Ok(c) => c,
            Err(e) => {
                warn!("auth_config: {e}");
                return;
            }
        };
        let stored = cfg.hash_for(user).to_string();

        let verdict = self
            .auth
            .try_pin(user, &digits, &stored, &self.hasher, Instant::now());
        match verdict {
            Verdict::Ok => {
                info!("login ok: {user}");
                self.screen = match user {
                    User::Admin => Screen::AdminMenu { user },
                    User::Guest => Screen::GuestMenu { user },
                };
            }
            Verdict::Wrong { attempts, lockout } => {
                let msg = if let Some(d) = lockout {
                    format!("Wrong PIN. Locked {} s.", d.as_secs())
                } else {
                    format!("Wrong PIN ({attempts}).")
                };
                self.screen = Screen::PinEntry {
                    user,
                    digits: String::new(),
                    message: msg,
                    locked_until: self.auth.locked_until(user),
                };
            }
            Verdict::LockedOut { remaining } => {
                self.screen = Screen::PinEntry {
                    user,
                    digits: String::new(),
                    message: format!("Locked. Wait {} s.", remaining.as_secs() + 1),
                    locked_until: self.auth.locked_until(user),
                };
            }
        }
    }

    fn engage_blank(&self) {
        if let Err(e) = self.sys.blank_display(true) {
            warn!("blank: {e}");
        }
        let _ = self.sys.set_backlight(0);
    }

    fn tick(&mut self) -> bool {
        let now = Instant::now();

        if !matches!(self.screen, Screen::Blank) && self.idle.should_blank(now) {
            self.screen = Screen::Blank;
            self.engage_blank();
            return true;
        }

        if let Screen::PinEntry {
            locked_until,
            message,
            ..
        } = &mut self.screen
        {
            if let Some(t) = *locked_until {
                if now >= t {
                    *locked_until = None;
                    message.clear();
                    return true;
                }
            }
        }

        if matches!(self.screen, Screen::Screensaver) {
            let (current, _) = format_clock_now();
            if current != self.last_clock_minute {
                self.last_clock_minute.clone_from(&current);
                return true;
            }
        }

        false
    }
}

fn format_clock_now() -> (String, String) {
    let now = chrono::Local::now();
    (
        now.format("%H:%M").to_string(),
        now.format("%a %-d %b").to_string(),
    )
}

const fn screen_kind(s: &Screen) -> ScreenKind {
    match s {
        Screen::Blank => ScreenKind::Blank,
        Screen::Screensaver => ScreenKind::Screensaver,
        Screen::Lockscreen => ScreenKind::Lockscreen,
        Screen::PinEntry { .. } => ScreenKind::Pinpad,
        Screen::AdminMenu { .. } => ScreenKind::AdminMenu,
        Screen::GuestMenu { .. } => ScreenKind::GuestMenu,
        Screen::Fan { .. } => ScreenKind::Fan,
        Screen::Wifi {
            kind: WifiKind::Admin,
            ..
        } => ScreenKind::WifiAdmin,
        Screen::Wifi {
            kind: WifiKind::Guest,
            ..
        } => ScreenKind::WifiGuest,
    }
}

fn publish(app: &App, win: &AppWindow) {
    win.set_screen(screen_kind(&app.screen));

    if let Screen::PinEntry {
        user,
        digits,
        message,
        locked_until,
    } = &app.screen
    {
        win.set_pin_who(slint::SharedString::from(user.to_string()));
        win.set_pin_digits_entered(digits.len() as i32);
        win.set_pin_message(slint::SharedString::from(message.as_str()));
        win.set_pin_locked(locked_until.is_some());
    }

    if matches!(app.screen, Screen::Fan { .. }) {
        if let (Ok(t), Ok(f)) = (app.sys.temps(), app.sys.fan_status()) {
            win.set_cpu_milli_c(t.cpu);
            win.set_phy_milli_c(t.phy);
            win.set_fan_mode(slint::SharedString::from(f.mode.as_str()));
            win.set_fan_rpm(f.rpm as i32);
            win.set_fan_pwm(i32::from(f.pwm));
        }
    }
    if let Screen::Wifi { kind, .. } = app.screen {
        if let Ok(Some(info)) = app.sys.wifi_info(kind) {
            let uri = qr::wifi_uri(&info.ssid, &info.password);
            let qr_img = qr::render(&uri, 130).unwrap_or_default();
            win.set_wifi_available(true);
            win.set_wifi_ssid(slint::SharedString::from(info.ssid));
            win.set_wifi_password(slint::SharedString::from(info.password));
            win.set_wifi_qr(qr_img);
        } else {
            win.set_wifi_available(false);
            win.set_wifi_ssid(slint::SharedString::default());
            win.set_wifi_password(slint::SharedString::default());
            win.set_wifi_qr(slint::Image::default());
        }
    }
}

fn wire(app: &Rc<RefCell<App>>, win: &AppWindow) {
    macro_rules! handler {
        (|$a:ident, $w:ident| $body:expr) => {{
            let app = app.clone();
            let weak = win.as_weak();
            move || {
                if let Some(w) = weak.upgrade() {
                    #[allow(unused_mut)]
                    let mut $a = app.borrow_mut();
                    let $w = &w;
                    $body
                    publish(&$a, $w);
                }
            }
        }};
        (($p:ident: $pt:ty) |$a:ident, $w:ident| $body:expr) => {{
            let app = app.clone();
            let weak = win.as_weak();
            move |$p: $pt| {
                if let Some(w) = weak.upgrade() {
                    #[allow(unused_mut)]
                    let mut $a = app.borrow_mut();
                    let $w = &w;
                    $body
                    publish(&$a, $w);
                }
            }
        }};
    }

    win.on_any_touch(handler!(|a, _w| {
        a.on_touch();
    }));
    win.on_wake(handler!(|a, _w| {
        a.on_wake();
    }));
    win.on_swipe_up(handler!(|a, _w| {
        a.on_swipe_up();
    }));

    win.on_pick_user(handler!((u: slint::SharedString) |a, _w| {
        let user = if u == "admin" { User::Admin } else { User::Guest };
        a.on_pick_user(user);
    }));

    win.on_pin_digit(handler!((d: i32) |a, _w| {
        a.on_pin_digit(d.clamp(0, 9) as u8);
    }));
    win.on_pin_backspace(handler!(|a, _w| {
        a.on_pin_backspace();
    }));
    win.on_pin_cancel(handler!(|a, _w| {
        a.on_pin_cancel();
    }));

    win.on_nav_back(handler!(|a, _w| {
        a.on_back();
    }));
    win.on_nav_lock(handler!(|a, _w| {
        a.on_lock();
    }));

    win.on_open_fan(handler!(|a, _w| {
        a.on_open_fan();
    }));
    win.on_open_wifi(handler!(|a, _w| {
        a.on_open_wifi();
    }));

    win.on_set_fan_mode(handler!((m: slint::SharedString) |a, _w| {
        if let Some(mode) = FanMode::parse(m.as_str()) {
            a.on_set_fan_mode(mode);
        }
    }));
}

fn build_system() -> Result<Box<dyn System>> {
    #[cfg(feature = "desktop")]
    {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("fixtures");
        Ok(Box::new(ipc::DesktopSystem::load(&dir)?))
    }
    #[cfg(all(feature = "router", not(feature = "desktop")))]
    {
        return Ok(Box::new(ipc::RouterSystem::default()));
    }
    #[cfg(not(any(feature = "desktop", feature = "router")))]
    compile_error!("enable exactly one of features 'desktop' or 'router'");
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("hash") => {
            let pin = args
                .next()
                .ok_or_else(|| anyhow::anyhow!("usage: router-ui hash PIN"))?;
            let h = Argon2Hasher
                .hash(&pin)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("{h}");
            return Ok(());
        }
        Some("verify") => {
            let pin = args
                .next()
                .ok_or_else(|| anyhow::anyhow!("usage: router-ui verify PIN HASH"))?;
            let hash = args
                .next()
                .ok_or_else(|| anyhow::anyhow!("usage: router-ui verify PIN HASH"))?;
            let ok = Argon2Hasher.verify(&pin, &hash).unwrap_or(false);
            std::process::exit(i32::from(!ok));
        }
        Some("version" | "--version" | "-V") => {
            println!("router-ui {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("help" | "--help" | "-h") => {
            println!(
                "router-ui — touchscreen UI for the GL-BE10000 LCD\n\
                 \n\
                 USAGE:\n  \
                 router-ui                    run the UI (default)\n  \
                 router-ui run                same as above\n  \
                 router-ui hash <PIN>         print argon2id hash of PIN (used by router-ui-set-pin)\n  \
                 router-ui verify <PIN> <H>   exit 0 iff PIN verifies against hash H\n  \
                 router-ui version            print version and exit\n",
            );
            return Ok(());
        }
        Some("run") | None => {}
        Some(other) => {
            return Err(anyhow::anyhow!(
                "unknown subcommand {other:?}; try `router-ui help`"
            ));
        }
    }

    #[cfg(all(feature = "router", not(feature = "desktop")))]
    {
        use crate::platform::FbdevPlatform;
        crate::platform::router::install_signal_handler()?;
        let plat = FbdevPlatform::new("/dev/fb0", "/dev/input/event0")
            .map_err(|e| anyhow::anyhow!("FbdevPlatform: {e}"))?;
        slint::platform::set_platform(Box::new(plat))
            .map_err(|e| anyhow::anyhow!("set_platform: {e}"))?;
    }

    let sys = build_system()?;
    let app = Rc::new(RefCell::new(App::new(sys)));

    let win = AppWindow::new()?;
    wire(&app, &win);
    publish(&app.borrow(), &win);

    let tick_app = app;
    let tick_weak = win.as_weak();
    let _timer = {
        let t = slint::Timer::default();
        t.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(250),
            move || {
                if let Some(w) = tick_weak.upgrade() {
                    let changed = tick_app.borrow_mut().tick();
                    if changed {
                        publish(&tick_app.borrow(), &w);
                    }
                }
            },
        );
        t
    };

    win.run()?;
    Ok(())
}
