use std::f32::consts::PI;

#[derive(Clone, Debug, PartialEq)]
pub struct ColorResult {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub format_name: &'static str,
    pub summary: String,
}

impl ColorResult {
    pub fn to_hex(&self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

pub fn detect_color(input: &str) -> Option<ColorResult> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(res) = parse_hex(trimmed) {
        return Some(res);
    }
    if let Some(res) = parse_rgb(trimmed) {
        return Some(res);
    }
    if let Some(res) = parse_hsl(trimmed) {
        return Some(res);
    }
    if let Some(res) = parse_hsv(trimmed) {
        return Some(res);
    }
    if let Some(res) = parse_cmyk(trimmed) {
        return Some(res);
    }
    if let Some(res) = parse_oklch(trimmed) {
        return Some(res);
    }

    None
}

fn parse_hex(input: &str) -> Option<ColorResult> {
    let has_hash = input.starts_with('#');
    let hex_part = input.trim_start_matches('#');
    if !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    let (r, g, b, a) = match hex_part.len() {
        3 if has_hash => {
            let r = u8::from_str_radix(&hex_part[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex_part[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex_part[2..3], 16).ok()? * 17;
            (r, g, b, 255)
        }
        4 if has_hash => {
            let r = u8::from_str_radix(&hex_part[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex_part[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex_part[2..3], 16).ok()? * 17;
            let a = u8::from_str_radix(&hex_part[3..4], 16).ok()? * 17;
            (r, g, b, a)
        }
        6 => {
            let r = u8::from_str_radix(&hex_part[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex_part[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex_part[4..6], 16).ok()?;
            (r, g, b, 255)
        }
        8 => {
            let r = u8::from_str_radix(&hex_part[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex_part[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex_part[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex_part[6..8], 16).ok()?;
            (r, g, b, a)
        }
        _ => return None,
    };

    let summary = if a == 255 {
        format!("rgb({r}, {g}, {b})")
    } else {
        format!("rgba({r}, {g}, {b}, {:.2})", a as f32 / 255.0)
    };

    Some(ColorResult {
        r,
        g,
        b,
        a,
        format_name: "HEX",
        summary,
    })
}

fn extract_inner<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    let lower = input.to_ascii_lowercase();
    if lower.starts_with(prefix) && lower.ends_with(')') {
        Some(input[prefix.len()..input.len() - 1].trim())
    } else {
        None
    }
}

fn parse_tokens(inner: &str) -> Vec<&str> {
    inner
        .split(|c: char| c == ',' || c == '/' || c.is_whitespace())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_percent_or_num(s: &str, max: f32) -> Option<f32> {
    if let Some(pct) = s.strip_suffix('%') {
        let val = pct.trim().parse::<f32>().ok()?;
        Some((val / 100.0) * max)
    } else {
        s.parse::<f32>().ok()
    }
}

fn parse_alpha(s: &str) -> Option<u8> {
    if let Some(pct) = s.strip_suffix('%') {
        let val = pct.trim().parse::<f32>().ok()?;
        Some(((val / 100.0) * 255.0).clamp(0.0, 255.0).round() as u8)
    } else {
        let val = s.parse::<f32>().ok()?;
        if val <= 1.0 {
            Some((val * 255.0).clamp(0.0, 255.0).round() as u8)
        } else {
            Some(val.clamp(0.0, 255.0).round() as u8)
        }
    }
}

fn parse_rgb(input: &str) -> Option<ColorResult> {
    let inner = extract_inner(input, "rgb(").or_else(|| extract_inner(input, "rgba("))?;
    let tokens = parse_tokens(inner);
    if tokens.len() < 3 {
        return None;
    }

    let r = parse_percent_or_num(tokens[0], 255.0)?
        .clamp(0.0, 255.0)
        .round() as u8;
    let g = parse_percent_or_num(tokens[1], 255.0)?
        .clamp(0.0, 255.0)
        .round() as u8;
    let b = parse_percent_or_num(tokens[2], 255.0)?
        .clamp(0.0, 255.0)
        .round() as u8;
    let a = if tokens.len() >= 4 {
        parse_alpha(tokens[3])?
    } else {
        255
    };

    let hex = if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    };

    Some(ColorResult {
        r,
        g,
        b,
        a,
        format_name: "RGB",
        summary: hex,
    })
}

fn parse_angle(s: &str) -> Option<f32> {
    if let Some(deg) = s.strip_suffix("deg") {
        deg.trim().parse::<f32>().ok()
    } else if let Some(rad) = s.strip_suffix("rad") {
        rad.trim().parse::<f32>().ok().map(|r| r * 180.0 / PI)
    } else if let Some(turn) = s.strip_suffix("turn") {
        turn.trim().parse::<f32>().ok().map(|t| t * 360.0)
    } else {
        s.parse::<f32>().ok()
    }
}

fn parse_hsl(input: &str) -> Option<ColorResult> {
    let inner = extract_inner(input, "hsl(").or_else(|| extract_inner(input, "hsla("))?;
    let tokens = parse_tokens(inner);
    if tokens.len() < 3 {
        return None;
    }

    let h = parse_angle(tokens[0])?;
    let s = parse_percent_or_num(tokens[1], 1.0)?;
    let l = parse_percent_or_num(tokens[2], 1.0)?;
    let a = if tokens.len() >= 4 {
        parse_alpha(tokens[3])?
    } else {
        255
    };

    let [r, g, b] = hsl_to_rgb(h, s, l);
    let hex = format!("#{r:02x}{g:02x}{b:02x}");
    let summary = format!("{hex} · rgb({r}, {g}, {b})");

    Some(ColorResult {
        r,
        g,
        b,
        a,
        format_name: "HSL",
        summary,
    })
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [u8; 3] {
    let h = ((h % 360.0) + 360.0) % 360.0;
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = match (h / 60.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    [
        ((r1 + m) * 255.0).clamp(0.0, 255.0).round() as u8,
        ((g1 + m) * 255.0).clamp(0.0, 255.0).round() as u8,
        ((b1 + m) * 255.0).clamp(0.0, 255.0).round() as u8,
    ]
}

fn parse_hsv(input: &str) -> Option<ColorResult> {
    let inner = extract_inner(input, "hsv(")
        .or_else(|| extract_inner(input, "hsva("))
        .or_else(|| extract_inner(input, "hsb("))
        .or_else(|| extract_inner(input, "hsba("))?;
    let tokens = parse_tokens(inner);
    if tokens.len() < 3 {
        return None;
    }

    let h = parse_angle(tokens[0])?;
    let s = parse_percent_or_num(tokens[1], 1.0)?;
    let v = parse_percent_or_num(tokens[2], 1.0)?;
    let a = if tokens.len() >= 4 {
        parse_alpha(tokens[3])?
    } else {
        255
    };

    let [r, g, b] = hsv_to_rgb(h, s, v);
    let hex = format!("#{r:02x}{g:02x}{b:02x}");
    let summary = format!("{hex} · rgb({r}, {g}, {b})");

    Some(ColorResult {
        r,
        g,
        b,
        a,
        format_name: "HSV",
        summary,
    })
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = ((h % 360.0) + 360.0) % 360.0;
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r1, g1, b1) = match (h / 60.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    [
        ((r1 + m) * 255.0).clamp(0.0, 255.0).round() as u8,
        ((g1 + m) * 255.0).clamp(0.0, 255.0).round() as u8,
        ((b1 + m) * 255.0).clamp(0.0, 255.0).round() as u8,
    ]
}

fn parse_cmyk(input: &str) -> Option<ColorResult> {
    let inner = extract_inner(input, "cmyk(").or_else(|| extract_inner(input, "device-cmyk("))?;
    let tokens = parse_tokens(inner);
    if tokens.len() < 4 {
        return None;
    }

    let c = parse_percent_or_num(tokens[0], 1.0)?.clamp(0.0, 1.0);
    let m = parse_percent_or_num(tokens[1], 1.0)?.clamp(0.0, 1.0);
    let y = parse_percent_or_num(tokens[2], 1.0)?.clamp(0.0, 1.0);
    let k = parse_percent_or_num(tokens[3], 1.0)?.clamp(0.0, 1.0);

    let r = (255.0 * (1.0 - c) * (1.0 - k)).round() as u8;
    let g = (255.0 * (1.0 - m) * (1.0 - k)).round() as u8;
    let b = (255.0 * (1.0 - y) * (1.0 - k)).round() as u8;

    let hex = format!("#{r:02x}{g:02x}{b:02x}");
    let summary = format!("{hex} · rgb({r}, {g}, {b})");

    Some(ColorResult {
        r,
        g,
        b,
        a: 255,
        format_name: "CMYK",
        summary,
    })
}

fn parse_oklch(input: &str) -> Option<ColorResult> {
    let inner = extract_inner(input, "oklch(")?;
    let tokens = parse_tokens(inner);
    if tokens.len() < 3 {
        return None;
    }

    let l = parse_percent_or_num(tokens[0], 1.0)?.clamp(0.0, 1.0);
    let c = tokens[1].parse::<f32>().ok()?.max(0.0);
    let h = parse_angle(tokens[2])?;
    let a = if tokens.len() >= 4 {
        parse_alpha(tokens[3])?
    } else {
        255
    };

    let [r, g, b] = oklch_to_rgb(l, c, h);
    let hex = format!("#{r:02x}{g:02x}{b:02x}");
    let summary = format!("{hex} · rgb({r}, {g}, {b})");

    Some(ColorResult {
        r,
        g,
        b,
        a,
        format_name: "OKLCH",
        summary,
    })
}

#[allow(clippy::excessive_precision)]
fn oklch_to_rgb(l: f32, c: f32, h_deg: f32) -> [u8; 3] {
    let h_rad = h_deg.to_radians();
    let a_lab = c * h_rad.cos();
    let b_lab = c * h_rad.sin();

    // Oklab to LMS
    let l_ = l + 0.3963377774 * a_lab + 0.2158037573 * b_lab;
    let m_ = l - 0.1055613458 * a_lab - 0.0638541728 * b_lab;
    let s_ = l - 0.0894841775 * a_lab - 1.2914855480 * b_lab;

    let l_cubed = l_ * l_ * l_;
    let m_cubed = m_ * m_ * m_;
    let s_cubed = s_ * s_ * s_;

    // LMS to Linear sRGB
    let r_lin = 4.0767434090 * l_cubed - 3.3077115913 * m_cubed + 0.2309699292 * s_cubed;
    let g_lin = -1.2684380046 * l_cubed + 2.6097574011 * m_cubed - 0.3413193965 * s_cubed;
    let b_lin = -0.0041960863 * l_cubed - 0.7034186147 * m_cubed + 1.7076147010 * s_cubed;

    fn srgb_transfer(val: f32) -> f32 {
        if val <= 0.0031308 {
            12.92 * val
        } else {
            1.055 * val.powf(1.0 / 2.4) - 0.055
        }
    }

    let r = (srgb_transfer(r_lin).clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (srgb_transfer(g_lin).clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (srgb_transfer(b_lin).clamp(0.0, 1.0) * 255.0).round() as u8;

    [r, g, b]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_colors() {
        let res = detect_color("#ff5733").unwrap();
        assert_eq!((res.r, res.g, res.b, res.a), (255, 87, 51, 255));
        assert_eq!(res.format_name, "HEX");
        assert_eq!(res.to_hex(), "#ff5733");

        let res = detect_color("#3498db80").unwrap();
        assert_eq!((res.r, res.g, res.b, res.a), (52, 152, 219, 128));

        let res = detect_color("#fff").unwrap();
        assert_eq!((res.r, res.g, res.b, res.a), (255, 255, 255, 255));
    }

    #[test]
    fn parses_rgb_colors() {
        let res = detect_color("rgb(255, 0, 128)").unwrap();
        assert_eq!((res.r, res.g, res.b, res.a), (255, 0, 128, 255));
        assert_eq!(res.format_name, "RGB");

        let res = detect_color("rgba(52, 152, 219, 0.5)").unwrap();
        assert_eq!((res.r, res.g, res.b), (52, 152, 219));
        assert_eq!(res.a, 128);

        let res = detect_color("rgb(100% 0% 50% / 50%)").unwrap();
        assert_eq!((res.r, res.g, res.b), (255, 0, 128));
        assert_eq!(res.a, 128);
    }

    #[test]
    fn parses_hsl_colors() {
        let res = detect_color("hsl(210, 100%, 50%)").unwrap();
        assert_eq!((res.r, res.g, res.b), (0, 128, 255));
        assert_eq!(res.format_name, "HSL");
    }

    #[test]
    fn parses_hsv_colors() {
        let res = detect_color("hsv(0, 100%, 100%)").unwrap();
        assert_eq!((res.r, res.g, res.b), (255, 0, 0));
        assert_eq!(res.format_name, "HSV");
    }

    #[test]
    fn parses_cmyk_colors() {
        let res = detect_color("cmyk(0%, 100%, 100%, 0%)").unwrap();
        assert_eq!((res.r, res.g, res.b), (255, 0, 0));
        assert_eq!(res.format_name, "CMYK");
    }

    #[test]
    fn parses_oklch_colors() {
        let res = detect_color("oklch(0.65 0.24 25)").unwrap();
        assert_eq!(res.format_name, "OKLCH");
        assert!(res.r > 200);
        assert!(res.g < 100);
    }
}
