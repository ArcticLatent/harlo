//! OpenCode's colour arithmetic for its desktop themes, in OKLCH: colour
//! scales from a seed, mixing, shifting and blending.
//!
//! Every step ends on an 8-bit hex colour, as OpenCode's does, so the
//! results match its colours exactly rather than nearly.
//!
//! Ported from OpenCode's `packages/ui/src/theme/color.ts`, Copyright (c)
//! 2025 opencode, under the MIT licence in `themes/opencode/LICENSE`.

// Rust guideline compliant 2026-10-09

/// An sRGB colour with 8-bit channels.
pub(super) type Hex = [u8; 3];

/// A colour in OKLCH: lightness from 0 to 1, chroma, and hue in degrees.
#[derive(Debug, Clone, Copy)]
pub(super) struct Oklch {
    pub(super) l: f64,
    pub(super) c: f64,
    pub(super) h: f64,
}

/// `hex`'s channels from 0 to 1.
fn channels(hex: Hex) -> [f64; 3] {
    hex.map(|v| f64::from(v) / 255.0)
}

/// Rounds channels from 0 to 1 to 8 bits, clamping first.
fn to_hex(r: f64, g: f64, b: f64) -> Hex {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0..=1 first, so the scaled value fits a u8"
    )]
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [byte(r), byte(g), byte(b)]
}

fn to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(c: f64) -> f64 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// `hex` in OKLCH.
#[expect(
    clippy::many_single_char_names,
    reason = "the OKLab formulas' own names: cone responses l, m, s and axes a, b"
)]
pub(super) fn oklch(hex: Hex) -> Oklch {
    let [r, g, b] = channels(hex).map(to_linear);
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    let lightness = 0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s;
    let a = 1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s;
    let b = 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s;
    let mut hue = b.atan2(a).to_degrees();
    if hue < 0.0 {
        hue += 360.0;
    }
    Oklch {
        l: lightness,
        c: a.hypot(b),
        h: hue,
    }
}

/// `color` in sRGB channels, which may fall outside 0 to 1.
#[expect(
    clippy::many_single_char_names,
    reason = "the OKLab formulas' own names: cone responses l, m, s and axes a, b"
)]
fn rgb(color: Oklch) -> [f64; 3] {
    let (a, b) = (
        color.c * color.h.to_radians().cos(),
        color.c * color.h.to_radians().sin(),
    );
    let l = color.l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m = color.l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s = color.l - 0.089_484_177_5 * a - 1.291_485_548 * b;
    let (l, m, s) = (l * l * l, m * m * m, s * s * s);
    [
        4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
        -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701 * s,
    ]
    .map(from_linear)
}

/// Whether every channel is a colour sRGB can show.
fn in_gamut(channels: [f64; 3]) -> bool {
    channels.iter().all(|v| (0.0..=1.0).contains(v))
}

/// `color` brought into sRGB by lowering its chroma.
fn fit(color: Oklch) -> Oklch {
    let base = Oklch {
        l: color.l.clamp(0.0, 1.0),
        c: color.c.max(0.0),
        h: color.h.rem_euclid(360.0),
    };
    if in_gamut(rgb(base)) {
        return base;
    }
    let mut c = base.c;
    for _ in 0..24 {
        c *= 0.9;
        let next = Oklch { c, ..base };
        if in_gamut(rgb(next)) {
            return next;
        }
    }
    Oklch { c: 0.0, ..base }
}

/// `color` as the nearest hex colour sRGB can show.
pub(super) fn hex(color: Oklch) -> Hex {
    let [r, g, b] = rgb(fit(color));
    to_hex(r, g, b)
}

/// Twelve steps of `seed`'s hue, from the background's end to the text's.
pub(super) fn scale(seed: Hex, dark: bool) -> [Hex; 12] {
    let base = oklch(seed);
    let lightness = if dark {
        [
            0.118,
            0.138,
            0.167,
            0.202,
            0.246,
            0.304,
            0.378,
            0.468,
            (base.l * 0.825).clamp(0.53, 0.705),
            (base.l * 0.89).clamp(0.61, 0.79),
            (base.l + 0.033).clamp(0.868, 0.943),
            0.984,
        ]
    } else {
        [
            0.993,
            0.983,
            0.962,
            0.936,
            0.906,
            0.866,
            0.811,
            0.74,
            base.l,
            (base.l - 0.036).max(0.0),
            0.49,
            0.27,
        ]
    };
    let chroma = if dark {
        [
            0.52, 0.68, 0.86, 1.02, 1.14, 1.24, 1.36, 1.48, 1.56, 1.64, 1.62, 1.15,
        ]
    } else {
        [
            0.12, 0.24, 0.46, 0.68, 0.84, 0.98, 1.08, 1.16, 1.22, 1.26, 1.18, 0.98,
        ]
    };
    std::array::from_fn(|i| {
        hex(Oklch {
            l: lightness[i],
            c: base.c * chroma[i],
            h: base.h,
        })
    })
}

/// Twelve greys from the background to the text: the background from
/// `seed`, mixed towards `ink`; without ink, greys of `seed`'s hue.
pub(super) fn neutral_scale(seed: Hex, dark: bool, ink: Option<Hex>) -> [Hex; 12] {
    let base = oklch(seed);
    if let Some(ink) = ink {
        let lift = |tone: f64| {
            hex(Oklch {
                l: base.l + (1.0 - base.l) * tone,
                c: base.c * (1.0 - tone).max(0.0),
                h: base.h,
            })
        };
        let sink = |tone: f64| {
            hex(Oklch {
                l: base.l * (1.0 - tone),
                c: base.c * (1.0 - tone * if dark { 0.12 } else { 0.3 }).max(0.0),
                h: base.h,
            })
        };
        let background = if dark {
            sink((0.19 + (base.l - 0.12).max(0.0) * 0.33 + base.c * 1.95).clamp(0.17, 0.27))
        } else if base.l < 0.82 {
            lift(0.86)
        } else {
            lift((0.1 + base.c * 3.2 + (0.95 - base.l).max(0.0) * 0.35).clamp(0.1, 0.28))
        };
        let steps = if dark {
            [
                0.0, 0.018, 0.039, 0.064, 0.097, 0.143, 0.212, 0.31, 0.46, 0.649, 0.845, 0.984,
            ]
        } else {
            [
                0.0, 0.022, 0.042, 0.068, 0.102, 0.146, 0.208, 0.296, 0.432, 0.61, 0.81, 0.965,
            ]
        };
        return steps.map(|step| mix(background, ink, step));
    }
    let chroma = base.c.min(if dark { 0.068 } else { 0.04 });
    let lightness = if dark {
        [
            0.138,
            0.156,
            0.178,
            0.202,
            0.232,
            0.272,
            0.326,
            0.404,
            (base.l * 0.83).clamp(0.43, 0.55),
            0.596,
            0.719,
            0.956,
        ]
    } else {
        [
            0.991, 0.979, 0.964, 0.946, 0.931, 0.913, 0.891, 0.83, base.l, 0.617, 0.542, 0.205,
        ]
    };
    lightness.map(|l| {
        hex(Oklch {
            l,
            c: chroma,
            h: base.h,
        })
    })
}

/// `from` moved `amount` of the way to `to`, in OKLCH, by the shorter way
/// round the hue circle.
pub(super) fn mix(from: Hex, to: Hex, amount: f64) -> Hex {
    let (a, b) = (oklch(from), oklch(to));
    let delta = ((b.h - a.h) % 360.0 + 540.0) % 360.0 - 180.0;
    hex(Oklch {
        l: a.l + (b.l - a.l) * amount,
        c: a.c + (b.c - a.c) * amount,
        h: a.h + delta * amount,
    })
}

/// `color` with `l` added to its lightness, its chroma times `c`, and `h`
/// added to its hue.
pub(super) fn shift(color: Hex, l: f64, c: f64, h: f64) -> Hex {
    let base = oklch(color);
    hex(Oklch {
        l: base.l + l,
        c: base.c * c,
        h: base.h + h,
    })
}

/// `color` laid over `background` at `alpha`, in sRGB.
pub(super) fn blend(color: Hex, background: Hex, alpha: f64) -> Hex {
    let (fg, bg) = (channels(color), channels(background));
    let channel = |i: usize| fg[i] * alpha + bg[i] * (1.0 - alpha);
    to_hex(channel(0), channel(1), channel(2))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLES: [Hex; 6] = [
        [0x28, 0x2a, 0x36],
        [0xff, 0x79, 0xc6],
        [0x00, 0x72, 0xc3],
        [0xfc, 0xfc, 0xfc],
        [0x00, 0x00, 0x00],
        [0x8e, 0x8e, 0x8e],
    ];

    #[test]
    fn oklch_comes_back_to_the_same_hex() {
        for sample in [SAMPLES[0], SAMPLES[2], SAMPLES[3], SAMPLES[4], SAMPLES[5]] {
            assert_eq!(hex(oklch(sample)), sample, "{sample:02x?}");
        }
        // A full channel can come back a hair over 1, which the gamut fit
        // takes as outside sRGB and softens, as OpenCode's own code does.
        assert_eq!(hex(oklch(SAMPLES[1])), [0xf8, 0x80, 0xc4]);
    }

    #[test]
    fn white_and_black_have_no_chroma() {
        let white = oklch([255, 255, 255]);
        assert!((white.l - 1.0).abs() < 1e-6 && white.c < 1e-6);
        let black = oklch([0, 0, 0]);
        assert!(black.l.abs() < 1e-9 && black.c < 1e-9);
    }

    #[test]
    fn mixing_and_blending_reach_their_ends() {
        let (a, b) = (SAMPLES[0], SAMPLES[2]);
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
        assert_eq!(blend(a, b, 1.0), a);
        assert_eq!(blend(a, b, 0.0), b);
        assert_eq!(blend([255, 255, 255], [0, 0, 0], 0.5), [128, 128, 128]);
        assert_eq!(shift(a, 0.0, 1.0, 0.0), a);
    }

    #[test]
    fn scales_run_from_background_to_text() {
        let seed = SAMPLES[2];
        let dark = scale(seed, true);
        assert!(
            oklch(dark[0]).l < oklch(dark[11]).l,
            "dark runs light-wards"
        );
        let light = scale(seed, false);
        assert!(
            oklch(light[0]).l > oklch(light[11]).l,
            "light runs dark-wards"
        );
        // The light scale's ninth step is the seed's own lightness.
        assert!((oklch(light[8]).l - oklch(seed).l).abs() < 0.01);
        let greys = neutral_scale([0x28, 0x2a, 0x36], true, Some([0xf8, 0xf8, 0xf2]));
        assert_eq!(greys[11], mix(greys[0], [0xf8, 0xf8, 0xf2], 0.984));
    }
}
