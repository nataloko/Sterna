//! The 256-colour palette, and the nearest-colour search truecolor SGR uses.
//!
//! Tera Term stores one byte of colour per cell, so `SGR 38;2;r;g;b` does not
//! store the RGB triple — it resolves to the closest palette index and stores
//! that. Reproducing the *search* therefore matters as much as reproducing the
//! parse: a different palette gives a different index and the grid differs.

use std::sync::OnceLock;

pub type Rgb = (u8, u8, u8);

/// Tera Term's default palette.
///
/// Entries 0..16 are `ttset.c:797`'s default `ANSIColor` string, permuted
/// through `vtdisp.c:GetIndex256From16` — the 16-colour table is ordered
/// dim-then-bright and `ANSIColor[256]` is ordered bright-then-dim, so the two
/// halves swap. Note these are the *VGA* values (128/192/255), not xterm's
/// (205/238/229); using xterm's is a plausible-looking mistake that moves the
/// answer for most truecolor input.
///
/// Entries 16.. are `defaultcolortable.c`, which is the standard xterm 6×6×6
/// cube followed by the 24-step greyscale ramp, so they are generated rather
/// than transcribed.
pub fn default_palette() -> &'static [Rgb; 256] {
    static PALETTE: OnceLock<[Rgb; 256]> = OnceLock::new();
    PALETTE.get_or_init(|| {
        let mut p = [(0u8, 0u8, 0u8); 256];
        const BASE: [Rgb; 16] = [
            (0, 0, 0),
            (128, 0, 0),
            (0, 128, 0),
            (128, 128, 0),
            (0, 0, 128),
            (128, 0, 128),
            (0, 128, 128),
            (192, 192, 192),
            (128, 128, 128),
            (255, 0, 0),
            (0, 255, 0),
            (255, 255, 0),
            (0, 0, 255),
            (255, 0, 255),
            (0, 255, 255),
            (255, 255, 255),
        ];
        p[..16].copy_from_slice(&BASE);
        for (i, slot) in p.iter_mut().enumerate().take(232).skip(16) {
            let n = i - 16;
            let step = |v: usize| -> u8 {
                if v == 0 {
                    0
                } else {
                    (v * 40 + 55) as u8
                }
            };
            *slot = (step((n / 36) % 6), step((n / 6) % 6), step(n % 6));
        }
        for (i, slot) in p.iter_mut().enumerate().skip(232) {
            let v = ((i - 232) * 10 + 8) as u8;
            *slot = (v, v, v);
        }
        p
    })
}

/// `vtdisp.c:DispFindClosestColor`. Returns `None` for out-of-range input,
/// which is upstream's `-1` and makes the SGR parser leave the colour alone.
///
/// The final flip is the part that surprises: with any full-colour mode on —
/// and 256-colour is on by default — a result inside the base 16 with a
/// non-zero low three bits is XORed with 8, so pure red resolves to index 1
/// ("dark red") rather than 9. The drawing path applies the inverse when it
/// converts a sequence index back to a palette index, so the round trip is
/// consistent; index 1 is simply what the cell stores.
pub fn find_closest(palette: &[Rgb; 256], r: i32, g: i32, b: i32, full_color: bool) -> Option<u32> {
    if !(0..=255).contains(&r) || !(0..=255).contains(&g) || !(0..=255).contains(&b) {
        return None;
    }
    let mut best = 0usize;
    let mut best_d = i32::MAX;
    for (i, &(pr, pg, pb)) in palette.iter().enumerate() {
        let (dr, dg, db) = (r - pr as i32, g - pg as i32, b - pb as i32);
        let d = dr * dr + dg * dg + db * db;
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    if full_color && best < 16 && best & 7 != 0 {
        best ^= 8;
    }
    Some(best as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_base_colours_flip_between_the_bright_and_dim_halves() {
        let p = default_palette();
        assert_eq!(find_closest(p, 255, 0, 0, true), Some(1));
        assert_eq!(find_closest(p, 0, 255, 0, true), Some(2));
        assert_eq!(find_closest(p, 0, 0, 255, true), Some(4));
        assert_eq!(find_closest(p, 255, 255, 255, true), Some(7));
    }

    #[test]
    fn a_zero_low_nibble_is_left_alone() {
        let p = default_palette();
        assert_eq!(find_closest(p, 0, 0, 0, true), Some(0));
        assert_eq!(find_closest(p, 128, 128, 128, true), Some(8));
    }

    #[test]
    fn without_full_colour_the_flip_does_not_happen() {
        assert_eq!(find_closest(default_palette(), 255, 0, 0, false), Some(9));
    }

    #[test]
    fn out_of_range_is_rejected_rather_than_clamped() {
        let p = default_palette();
        assert_eq!(find_closest(p, -1, 0, 0, true), None);
        assert_eq!(find_closest(p, 0, 256, 0, true), None);
    }

    #[test]
    fn greyscale_ramp_is_reachable() {
        // #080808 is index 232, the first greyscale step.
        assert_eq!(find_closest(default_palette(), 10, 10, 10, true), Some(232));
    }

    #[test]
    fn the_search_uses_the_terminals_palette() {
        let mut p = *default_palette();
        p[42] = (1, 2, 3);
        assert_eq!(find_closest(&p, 1, 2, 3, true), Some(42));
    }
}
