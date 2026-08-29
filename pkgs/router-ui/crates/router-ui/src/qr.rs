use qrcode::{EcLevel, QrCode};
use slint::{Image, Rgb8Pixel, SharedPixelBuffer};

#[must_use]
pub(crate) fn wifi_uri(ssid: &str, password: &str) -> String {
    let auth = if password.is_empty() { "nopass" } else { "WPA" };
    format!(
        "WIFI:T:{auth};S:{};P:{};H:false;;",
        escape(ssid),
        escape(password),
    )
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | ';' | ',' | ':' | '"') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

pub(crate) fn render(data: &str, target_px: u32) -> Option<Image> {
    let code = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::M).ok()?;
    let modules = code.width();
    if modules == 0 {
        return None;
    }

    let quiet = 2u32;
    let mods_total = modules as u32 + 2 * quiet;
    let scale = (target_px / mods_total).max(1);
    let side = mods_total * scale;

    let mut buf = SharedPixelBuffer::<Rgb8Pixel>::new(side, side);
    let stride = side as usize;
    {
        let px = buf.make_mut_slice();
        for p in px.iter_mut() {
            *p = Rgb8Pixel {
                r: 255,
                g: 255,
                b: 255,
            };
        }
        let bits: Vec<bool> = (0..modules as u32 * modules as u32)
            .map(|i| {
                let x = (i % modules as u32) as usize;
                let y = (i / modules as u32) as usize;
                code[(x, y)] == qrcode::Color::Dark
            })
            .collect();
        for my in 0..modules as u32 {
            for mx in 0..modules as u32 {
                if !bits[(my * modules as u32 + mx) as usize] {
                    continue;
                }
                let x0 = (quiet + mx) * scale;
                let y0 = (quiet + my) * scale;
                for dy in 0..scale {
                    let row_start = ((y0 + dy) as usize) * stride;
                    let cs = x0 as usize;
                    for dx in 0..scale {
                        px[row_start + cs + dx as usize] = Rgb8Pixel { r: 0, g: 0, b: 0 };
                    }
                }
            }
        }
    }
    Some(Image::from_rgb8(buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wifi_uri_escape() {
        assert_eq!(
            wifi_uri("a;b", "p:w"),
            "WIFI:T:WPA;S:a\\;b;P:p\\:w;H:false;;"
        );
    }

    #[test]
    fn wifi_uri_open_network() {
        assert_eq!(wifi_uri("Cafe", ""), "WIFI:T:nopass;S:Cafe;P:;H:false;;");
    }

    #[test]
    fn render_succeeds_for_typical_wifi_uri() {
        let img = render(&wifi_uri("example-ssid", "rolling-stones-pizza-42"), 130);
        assert!(img.is_some());
        let img = img.unwrap();
        assert!(img.size().width > 0);
        assert!(img.size().height > 0);
    }
}
