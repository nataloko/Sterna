//! DEC sixel decoding.
//!
//! A sixel is six vertical pixels encoded in one byte. The surrounding DCS is
//! handled by `vte`; this module receives only the payload after the final
//! `q`, one byte at a time. Keeping it streaming matters: an image from a
//! serial device can be megabytes long, and buffering the DCS before decoding
//! would pay for a second copy while also making an unterminated string an
//! unbounded allocation.
//!
//! The grammar and pixel-aspect rules are from the VT330/VT340 Programmer
//! Reference Manual, Volume 2, chapter 14. The limits are ours. Every size and
//! repeat count on the wire is untrusted, so one image is capped at 4096 by
//! 4096 pixels (64 MiB as RGBA), the same geometry used by modern sixel
//! implementations. Data past an edge is parsed and discarded, which keeps
//! the parser in step without letting it grow memory.

use crate::palette::Rgb;
use tt_grid::{Cell, Grid};

pub(crate) const MAX_WIDTH: usize = 4096;
pub(crate) const MAX_HEIGHT: usize = 4096;
const MAX_PIXELS: usize = MAX_WIDTH * MAX_HEIGHT;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Raster {
    pub width: usize,
    pub height: usize,
    /// RGBA8888, top to bottom. Alpha is zero only for untouched pixels when
    /// the DCS selected transparent background mode (`P2 = 1`).
    pub pixels: Vec<u8>,
}

impl Raster {
    pub(crate) fn crop(mut self, max_width: usize, max_height: usize) -> Option<Self> {
        let width = self.width.min(max_width);
        let height = self.height.min(max_height);
        if width == 0 || height == 0 {
            return None;
        }
        if width == self.width && height == self.height {
            return Some(self);
        }
        let mut pixels = vec![0; width * height * 4];
        for y in 0..height {
            let source = y * self.width * 4;
            let target = y * width * 4;
            pixels[target..target + width * 4]
                .copy_from_slice(&self.pixels[source..source + width * 4]);
        }
        self.width = width;
        self.height = height;
        self.pixels = pixels;
        Some(self)
    }
}

#[derive(Clone, Debug)]
struct CellSnapshot {
    line: u64,
    column: usize,
    cell: Option<Cell>,
    pixel_x: usize,
    pixel_y: usize,
    pixel_width: usize,
    pixel_height: usize,
    cleared: bool,
}

/// One decoded image, anchored to terminal content rather than a viewport row.
///
/// The pixels are public through accessors and the cell snapshots are not. A
/// frontend paints the raster; the snapshots let the core erase an image tile
/// when later text changes that cell, without putting image bookkeeping into
/// every one of `Grid`'s editing operations.
#[derive(Clone, Debug)]
pub struct SixelImage {
    line: u64,
    column: usize,
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    alternate: bool,
    cells: Vec<CellSnapshot>,
}

impl SixelImage {
    pub(crate) fn new(
        raster: Raster,
        line: u64,
        column: usize,
        alternate: bool,
        cell_width: usize,
        cell_height: usize,
        grid: &Grid,
    ) -> Self {
        let mut cells = Vec::new();
        for pixel_y in (0..raster.height).step_by(cell_height) {
            let line = line + (pixel_y / cell_height) as u64;
            for pixel_x in (0..raster.width).step_by(cell_width) {
                let column = column + pixel_x / cell_width;
                cells.push(CellSnapshot {
                    line,
                    column,
                    cell: grid
                        .absolute_line(line)
                        .and_then(|row| row.get(column))
                        .copied(),
                    pixel_x,
                    pixel_y,
                    pixel_width: cell_width.min(raster.width - pixel_x),
                    pixel_height: cell_height.min(raster.height - pixel_y),
                    cleared: false,
                });
            }
        }
        let mut image = Self {
            line,
            column,
            width: raster.width,
            height: raster.height,
            pixels: raster.pixels,
            alternate,
            cells,
        };
        image.reconcile(grid);
        image
    }

    pub fn line(&self) -> u64 {
        self.line
    }

    pub fn column(&self) -> usize {
        self.column
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub(crate) fn alternate(&self) -> bool {
        self.alternate
    }

    /// Clear whole cell tiles whose text-grid contents changed since the
    /// image was painted. Existing text beneath transparent sixels remains;
    /// text written later erases the image before being drawn on top.
    pub(crate) fn reconcile(&mut self, grid: &Grid) {
        for snapshot in &mut self.cells {
            if snapshot.cleared {
                continue;
            }
            let now = grid
                .absolute_line(snapshot.line)
                .and_then(|row| row.get(snapshot.column));
            if now.copied() == snapshot.cell {
                continue;
            }
            for y in snapshot.pixel_y..snapshot.pixel_y + snapshot.pixel_height {
                for x in snapshot.pixel_x..snapshot.pixel_x + snapshot.pixel_width {
                    self.pixels[(y * self.width + x) * 4 + 3] = 0;
                }
            }
            snapshot.cleared = true;
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pixels.chunks_exact(4).all(|pixel| pixel[3] == 0)
    }
}

#[derive(Clone, Copy, Debug)]
struct Parameters<const N: usize> {
    values: [u32; N],
    len: usize,
    current: u32,
    saw_digit: bool,
}

impl<const N: usize> Default for Parameters<N> {
    fn default() -> Self {
        Self {
            values: [0; N],
            len: 0,
            current: 0,
            saw_digit: false,
        }
    }
}

impl<const N: usize> Parameters<N> {
    fn digit(&mut self, digit: u8) {
        self.saw_digit = true;
        self.current = self
            .current
            .saturating_mul(10)
            .saturating_add(u32::from(digit));
    }

    fn separator(&mut self) {
        self.commit();
    }

    fn commit(&mut self) {
        if self.len < N {
            self.values[self.len] = self.current;
            self.len += 1;
        }
        self.current = 0;
        self.saw_digit = false;
    }

    fn finish(mut self) -> ([u32; N], usize) {
        if self.saw_digit || self.len > 0 {
            self.commit();
        }
        (self.values, self.len)
    }
}

#[derive(Clone, Copy, Debug)]
enum Command {
    Ground,
    Repeat { count: u32, saw_digit: bool },
    Raster(Parameters<4>),
    Color(Parameters<5>),
}

/// One DCS sixel payload in progress.
pub(crate) struct Decoder {
    command: Command,
    palette: [Rgb; 256],
    color: usize,
    background: [u8; 4],
    pixels: Vec<u8>,
    stride: usize,
    allocated_height: usize,
    width: usize,
    height: usize,
    declared_width: usize,
    declared_height: usize,
    x: usize,
    y: usize,
    scale_y: usize,
    drew_data: bool,
}

impl Decoder {
    /// `macro_parameter` is DCS `P1`; `background_mode` is DCS `P2`.
    pub(crate) fn new(macro_parameter: u16, background_mode: u16, background: Rgb) -> Self {
        let (r, g, b) = background;
        Self {
            command: Command::Ground,
            palette: default_palette(),
            color: 0,
            background: if background_mode == 1 {
                [0, 0, 0, 0]
            } else {
                [r, g, b, 255]
            },
            pixels: Vec::new(),
            stride: 0,
            allocated_height: 0,
            width: 0,
            height: 0,
            declared_width: 0,
            declared_height: 0,
            x: 0,
            y: 0,
            scale_y: macro_scale(macro_parameter),
            drew_data: false,
        }
    }

    pub(crate) fn put(&mut self, byte: u8) {
        match self.command {
            Command::Ground => self.ground(byte),
            Command::Repeat {
                mut count,
                mut saw_digit,
            } => {
                if let Some(digit) = decimal(byte) {
                    saw_digit = true;
                    count = count.saturating_mul(10).saturating_add(u32::from(digit));
                    self.command = Command::Repeat { count, saw_digit };
                } else {
                    self.command = Command::Ground;
                    if (b'?'..=b'~').contains(&byte) {
                        let count = if saw_digit { count } else { 1 };
                        self.draw(byte - b'?', usize::try_from(count).unwrap_or(usize::MAX));
                    } else {
                        // A malformed repeat does not hide the control which
                        // followed it. Resynchronise at that byte.
                        self.ground(byte);
                    }
                }
            }
            Command::Raster(mut params) => {
                if let Some(digit) = decimal(byte) {
                    params.digit(digit);
                    self.command = Command::Raster(params);
                } else if byte == b';' {
                    params.separator();
                    self.command = Command::Raster(params);
                } else {
                    self.command = Command::Ground;
                    self.raster(params);
                    self.ground(byte);
                }
            }
            Command::Color(mut params) => {
                if let Some(digit) = decimal(byte) {
                    params.digit(digit);
                    self.command = Command::Color(params);
                } else if byte == b';' {
                    params.separator();
                    self.command = Command::Color(params);
                } else {
                    self.command = Command::Ground;
                    self.color(params);
                    self.ground(byte);
                }
            }
        }
    }

    pub(crate) fn finish(mut self) -> Option<Raster> {
        // A parameter command can end immediately before ST. Apply it even
        // though there was no following data byte to terminate it for us.
        match std::mem::replace(&mut self.command, Command::Ground) {
            Command::Raster(params) => self.raster(params),
            Command::Color(params) => self.color(params),
            Command::Ground | Command::Repeat { .. } => {}
        }

        let width = self.width.max(self.declared_width).min(MAX_WIDTH);
        let height = self.height.max(self.declared_height).min(MAX_HEIGHT);
        if width == 0 || height == 0 || !self.ensure(width, height) {
            return None;
        }

        let mut pixels = vec![0; width * height * 4];
        for y in 0..height {
            let source = y * self.stride * 4;
            let target = y * width * 4;
            pixels[target..target + width * 4]
                .copy_from_slice(&self.pixels[source..source + width * 4]);
        }
        Some(Raster {
            width,
            height,
            pixels,
        })
    }

    fn ground(&mut self, byte: u8) {
        match byte {
            b'!' => {
                self.command = Command::Repeat {
                    count: 0,
                    saw_digit: false,
                }
            }
            b'"' => self.command = Command::Raster(Parameters::default()),
            b'#' => self.command = Command::Color(Parameters::default()),
            b'$' => self.x = 0,
            b'-' => {
                self.x = 0;
                self.y = self.y.saturating_add(6).min(MAX_HEIGHT);
            }
            b'?'..=b'~' => self.draw(byte - b'?', 1),
            _ => {}
        }
    }

    fn draw(&mut self, bits: u8, repeat: usize) {
        self.drew_data = true;
        let repeat = repeat.min(MAX_WIDTH.saturating_sub(self.x));
        let scaled_y = self.y.saturating_mul(self.scale_y).min(MAX_HEIGHT);
        let band_height = 6usize.saturating_mul(self.scale_y);
        let end_y = scaled_y.saturating_add(band_height).min(MAX_HEIGHT);
        let end_x = self.x.saturating_add(repeat).min(MAX_WIDTH);
        self.width = self.width.max(end_x);
        self.height = self.height.max(end_y);
        if repeat == 0 || end_y == scaled_y || !self.ensure(end_x, end_y) {
            self.x = end_x;
            return;
        }

        let (r, g, b) = self.palette[self.color];
        for dx in self.x..end_x {
            for bit in 0..6 {
                if bits & (1 << bit) == 0 {
                    continue;
                }
                let top = scaled_y + bit * self.scale_y;
                for y in top..(top + self.scale_y).min(MAX_HEIGHT) {
                    let at = (y * self.stride + dx) * 4;
                    self.pixels[at..at + 4].copy_from_slice(&[r, g, b, 255]);
                }
            }
        }
        self.x = end_x;
    }

    fn raster(&mut self, params: Parameters<4>) {
        let ([pan, pad, width, height], len) = params.finish();
        if len >= 2 && pan != 0 && pad != 0 && !self.drew_data {
            // The VT300 rounds the vertical:horizontal aspect to the nearest
            // integer. Ratios below one still occupy one device pixel.
            self.scale_y = usize::try_from((pan + pad / 2) / pad)
                .unwrap_or(MAX_HEIGHT)
                .clamp(1, MAX_HEIGHT);
        }
        if len >= 3 {
            self.declared_width = usize::try_from(width).unwrap_or(MAX_WIDTH).min(MAX_WIDTH);
        }
        if len >= 4 {
            self.declared_height = usize::try_from(height)
                .unwrap_or(MAX_HEIGHT)
                .min(MAX_HEIGHT);
        }
    }

    fn color(&mut self, params: Parameters<5>) {
        let ([register, system, x, y, z], len) = params.finish();
        let Ok(register) = usize::try_from(register) else {
            return;
        };
        if register >= self.palette.len() {
            return;
        }
        self.color = register;
        if len < 5 {
            return;
        }
        match system {
            1 => self.palette[register] = hls(x, y, z),
            2 => {
                self.palette[register] = (percent(x), percent(y), percent(z));
            }
            _ => {}
        }
    }

    fn ensure(&mut self, width: usize, height: usize) -> bool {
        if width == 0 || height == 0 || width > MAX_WIDTH || height > MAX_HEIGHT {
            return false;
        }
        if width <= self.stride && height <= self.allocated_height {
            return true;
        }

        let stride = width.max(self.stride).next_power_of_two().min(MAX_WIDTH);
        let allocated_height = height
            .max(self.allocated_height)
            .next_power_of_two()
            .min(MAX_HEIGHT)
            .min(MAX_PIXELS / stride);
        if width > stride || height > allocated_height {
            return false;
        }

        let mut pixels = vec![0; stride * allocated_height * 4];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&self.background);
        }
        for y in 0..self.allocated_height {
            let old = y * self.stride * 4;
            let new = y * stride * 4;
            let count = self.stride * 4;
            pixels[new..new + count].copy_from_slice(&self.pixels[old..old + count]);
        }
        self.pixels = pixels;
        self.stride = stride;
        self.allocated_height = allocated_height;
        true
    }
}

fn decimal(byte: u8) -> Option<u8> {
    if byte.is_ascii_digit() {
        Some(byte - b'0')
    } else {
        None
    }
}

fn macro_scale(parameter: u16) -> usize {
    match parameter {
        2 => 5,
        3 | 4 => 3,
        7..=9 => 1,
        _ => 2,
    }
}

fn percent(value: u32) -> u8 {
    ((value.min(100) * 255 + 50) / 100) as u8
}

/// DEC HLS has blue at hue zero rather than red, so it is standard HSL with
/// the hue rotated by 240 degrees.
fn hls(hue: u32, lightness: u32, saturation: u32) -> Rgb {
    let h = f64::from((hue.min(360) + 240) % 360) / 360.0;
    let l = f64::from(lightness.min(100)) / 100.0;
    let s = f64::from(saturation.min(100)) / 100.0;
    if s == 0.0 {
        let gray = (l * 255.0).round() as u8;
        return (gray, gray, gray);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let channel = |mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        let value = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (value * 255.0).round() as u8
    };
    (channel(h + 1.0 / 3.0), channel(h), channel(h - 1.0 / 3.0))
}

/// The VT340's sixteen useful registers. Registers above these begin black
/// until a DCS defines them.
fn default_palette() -> [Rgb; 256] {
    let mut palette = [(0, 0, 0); 256];
    palette[..16].copy_from_slice(&[
        (0x00, 0x00, 0x00),
        (0x33, 0x33, 0xcc),
        (0xcc, 0x21, 0x21),
        (0x33, 0xcc, 0x33),
        (0xcc, 0x33, 0xcc),
        (0x33, 0xcc, 0xcc),
        (0xcc, 0xcc, 0x33),
        (0x87, 0x87, 0x87),
        (0x42, 0x42, 0x42),
        (0x54, 0x54, 0x99),
        (0x99, 0x2a, 0x2a),
        (0x54, 0x99, 0x54),
        (0x99, 0x54, 0x99),
        (0x54, 0x99, 0x99),
        (0x99, 0x99, 0x54),
        (0xcc, 0xcc, 0xcc),
    ]);
    palette
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(params: (u16, u16), data: &[u8]) -> Raster {
        let mut decoder = Decoder::new(params.0, params.1, (9, 8, 7));
        for &byte in data {
            decoder.put(byte);
        }
        decoder.finish().expect("raster")
    }

    fn pixel(raster: &Raster, x: usize, y: usize) -> [u8; 4] {
        let at = (y * raster.width + x) * 4;
        raster.pixels[at..at + 4].try_into().unwrap()
    }

    #[test]
    fn six_bits_run_from_top_to_bottom() {
        let raster = decode((7, 1), b"#2;2;100;0;0T");
        assert_eq!((raster.width, raster.height), (1, 6));
        // `T - ?` is 0b010101: top, third and fifth pixels.
        for y in 0..6 {
            assert_eq!(
                pixel(&raster, 0, y),
                if y % 2 == 0 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 0, 0]
                }
            );
        }
    }

    #[test]
    fn repeat_carriage_return_and_newline_compose_colors() {
        let raster = decode((7, 1), b"#1;2;0;100;0!3@-#2;2;100;0;0!2~$#3;2;0;0;100~");
        assert_eq!((raster.width, raster.height), (3, 12));
        assert_eq!(pixel(&raster, 0, 0), [0, 255, 0, 255]);
        assert_eq!(pixel(&raster, 2, 0), [0, 255, 0, 255]);
        // The blue overprint replaces the first red column only.
        assert_eq!(pixel(&raster, 0, 6), [0, 0, 255, 255]);
        assert_eq!(pixel(&raster, 1, 6), [255, 0, 0, 255]);
        assert_eq!(pixel(&raster, 2, 6), [0, 0, 0, 0]);
    }

    #[test]
    fn declared_size_paints_an_opaque_background() {
        let raster = decode((7, 2), b"\"1;1;4;3");
        assert_eq!((raster.width, raster.height), (4, 3));
        assert!(raster
            .pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [9, 8, 7, 255]));
    }

    #[test]
    fn raster_attributes_override_the_macro_aspect() {
        let macro_raster = decode((0, 1), b"@");
        assert_eq!(macro_raster.height, 12);
        assert_eq!(pixel(&macro_raster, 0, 0)[3], 255);
        assert_eq!(pixel(&macro_raster, 0, 1)[3], 255);

        let square = decode((0, 1), b"\"1;1;1;6@");
        assert_eq!(square.height, 6);
        assert_eq!(pixel(&square, 0, 0)[3], 255);
        assert_eq!(pixel(&square, 0, 1)[3], 0);
    }

    #[test]
    fn dec_hls_starts_at_blue() {
        let raster = decode((7, 1), b"#7;1;0;50;100@");
        assert_eq!(pixel(&raster, 0, 0), [0, 0, 255, 255]);
    }

    #[test]
    fn repeat_and_declared_geometry_are_bounded() {
        let raster = decode((7, 1), b"\"1;1;999999999;1!999999999@");
        assert_eq!((raster.width, raster.height), (MAX_WIDTH, 6));
        assert_eq!(raster.pixels.len(), MAX_WIDTH * 6 * 4);
    }
}
