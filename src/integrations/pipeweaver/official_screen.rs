//! "Official look" layout for the Mix / Mix Create screen (BEACN default theme).
//!
//! The dial faces come from the official `.images` pack (see `official.rs`). Everything else on
//! the screen (background texture, header, channel cards, label band, mute buttons, mix toggle,
//! page dots, boot logo) is drawn here from a small "screen kit" that is generated on the
//! user's own machine from their own copy of the official BEACN app (`make-official-kit.py`).
//! Nothing from BEACN is bundled with this program.
//!
//! Geometry was measured from the official app's own 800x480 preview renders and matches the
//! sizes of the component SVGs embedded in BEACN.exe 1.4.2.12 (cards 187x276, band 187x36,
//! buttons 105x32, toggle 58x32...).
//!
//! Kit location: env `BEACN_OFFICIAL_SCREEN`, or `$XDG_CONFIG_HOME/beacn-utility/official-screen`
//! containing a path, or `screen-default/` next to the `MixImages/` folder of the configured pack.

use crate::integrations::pipeweaver::helpers::Mix;
use crate::integrations::pipeweaver::official::{self, Face, ImagePack};
use anyhow::{Context, Result};
use fontdue::Font;
use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, Rgb, RgbImage, Rgba, RgbaImage, imageops};
use log::{info, warn};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

pub type Jpeg = Arc<Vec<u8>>;

// ---- Geometry (absolute screen pixels, 800x480) ----
pub const SCREEN: (u32, u32) = (800, 480);
pub const CARD_X: [u32; 4] = [12, 208, 405, 601];
pub const CARD_Y: u32 = 111;
pub const CARD_W: u32 = 187;
pub const CARD_H: u32 = 276;
pub const TILE_H: u32 = 180;
const BAND_Y: u32 = TILE_H; // card-local
const BAND_H: u32 = 36;
const LINE_Y: u32 = BAND_Y + BAND_H; // 3px colour line
const LINE_H: u32 = 3;
const SUB_LINE_CENTRES: [u32; 2] = [240, 264]; // card-local text centres
pub const BUTTON_W: u32 = 105;
pub const BUTTON_H: u32 = 32;
pub const BUTTON_Y: u32 = 433;
pub const PILL_POS: (u32, u32) = (371, 69);
pub const PILL_SIZE: (u32, u32) = (58, 32);
pub const DOTS_POS: (u32, u32) = (300, 398);
pub const DOTS_SIZE: (u32, u32) = (200, 24);
pub const STATUS_POS: (u32, u32) = (0, 392);
const STATUS_SIZE: (u32, u32) = (800, 40);

pub fn button_x(slot: usize) -> u32 {
    CARD_X[slot] + (CARD_W - BUTTON_W) / 2
}

// ---- Colours (sampled from the official renders) ----
const LABEL_COLOUR: Rgba<u8> = Rgba([160, 166, 190, 255]);
const SUB_COLOUR: Rgba<u8> = Rgba([150, 155, 172, 255]);
const BUTTON_ON_TEXT: Rgba<u8> = Rgba([239, 56, 62, 255]);
const BUTTON_OFF_TEXT: Rgba<u8> = Rgba([155, 160, 180, 255]);
const STATUS_COLOUR: Rgba<u8> = Rgba([160, 166, 190, 255]);

// Roboto is Apache-2.0 (resources/fonts/roboto/LICENSE) and is the face the official UI uses.
static ROBOTO_BOLD: &[u8] = include_bytes!("../../../resources/fonts/roboto/Roboto-Bold.ttf");
static ROBOTO_REGULAR: &[u8] = include_bytes!("../../../resources/fonts/roboto/Roboto-Regular.ttf");
const LABEL_SIZE: f32 = 16.0;
const SUB_SIZE: f32 = 14.5;
const BUTTON_SIZE: f32 = 14.0;

fn font(bold: bool) -> &'static Font {
    static B: OnceLock<Font> = OnceLock::new();
    static R: OnceLock<Font> = OnceLock::new();
    let (cell, bytes) = if bold { (&B, ROBOTO_BOLD) } else { (&R, ROBOTO_REGULAR) };
    cell.get_or_init(|| Font::from_bytes(bytes, fontdue::FontSettings::default()).expect("font"))
}

pub struct ScreenKit {
    base: RgbImage, // background texture + header strip
    splash: RgbImage,
    card: RgbaImage,
    band: RgbaImage,
    button_on: RgbaImage,
    button_off: RgbaImage,
    dot_on: RgbaImage,
    dot_off: RgbaImage,
    dots_pill: RgbaImage,
    mix_pill: RgbaImage,
    knob_a: RgbaImage,
    knob_b: RgbaImage,
    zero: Jpeg,
}

fn load_rgba(dir: &Path, name: &str) -> Result<RgbaImage> {
    let p = dir.join(name);
    Ok(image::open(&p).with_context(|| format!("loading {}", p.display()))?.to_rgba8())
}

impl ScreenKit {
    pub fn load(dir: &Path) -> Result<Self> {
        let mut base = load_rgba(dir, "background.png")?;
        let header = load_rgba(dir, "header.png")?;
        imageops::overlay(&mut base, &header, 0, 0);
        let zero = std::fs::read(dir.join("zero.jpg")).context("zero.jpg")?;
        Ok(Self {
            base: image::DynamicImage::ImageRgba8(base).to_rgb8(),
            splash: load_rgba(dir, "splash.png").map(|i| image::DynamicImage::ImageRgba8(i).to_rgb8())?,
            card: load_rgba(dir, "card.png")?,
            band: load_rgba(dir, "band.png")?,
            button_on: load_rgba(dir, "button_on.png")?,
            button_off: load_rgba(dir, "button_off.png")?,
            dot_on: load_rgba(dir, "dot_on.png")?,
            dot_off: load_rgba(dir, "dot_off.png")?,
            dots_pill: load_rgba(dir, "dots_pill.png")?,
            mix_pill: load_rgba(dir, "mix_pill.png")?,
            knob_a: load_rgba(dir, "knob_a.png")?,
            knob_b: load_rgba(dir, "knob_b.png")?,
            zero: Arc::new(zero),
        })
    }

    fn region(&self, x: u32, y: u32, w: u32, h: u32) -> RgbaImage {
        image::DynamicImage::ImageRgb8(imageops::crop_imm(&self.base, x, y, w, h).to_image()).to_rgba8()
    }

    /// The full background (texture + BEACN header), sent once per full redraw.
    pub fn background(&self) -> Result<Jpeg> {
        encode_rgb(&self.base, 92)
    }

    /// Boot logo with an optional status line, for splash / status screens.
    pub fn splash(&self, status: Option<&str>) -> Result<Jpeg> {
        let mut img = image::DynamicImage::ImageRgb8(self.splash.clone()).to_rgba8();
        if let Some(text) = status {
            let (x, y) = STATUS_POS;
            draw_text_centered(&mut img, text, x, y, STATUS_SIZE.0, STATUS_SIZE.1, true, 22.0, STATUS_COLOUR);
        }
        encode_rgba(&img, 90)
    }

    /// A status line ("Loading...", "Connection to Pipeweaver lost") over the splash.
    pub fn status_strip(&self, text: &str) -> Result<Jpeg> {
        let (x, y) = STATUS_POS;
        let (w, h) = STATUS_SIZE;
        let mut img = image::DynamicImage::ImageRgb8(imageops::crop_imm(&self.splash, x, y, w, h).to_image()).to_rgba8();
        draw_text_centered(&mut img, text, 0, 0, w, h, true, 22.0, STATUS_COLOUR);
        encode_rgba(&img, 90)
    }

    /// One channel card: background plate, label band with the channel name, the channel colour
    /// line, and up to two lines of "what's in this channel" text. The dial tile goes on top.
    pub fn card(&self, slot: usize, title: &str, colour: [u8; 3], lines: &[String]) -> Result<Jpeg> {
        let mut img = self.region(CARD_X[slot], CARD_Y, CARD_W, CARD_H);
        // The official app stacks two plates, which gives the measured (31,30,35) fill.
        imageops::overlay(&mut img, &self.card, 0, 0);
        imageops::overlay(&mut img, &self.card, 0, 0);
        imageops::overlay(&mut img, &self.band, 0, BAND_Y as i64);
        for y in LINE_Y..LINE_Y + LINE_H {
            for x in 0..CARD_W {
                img.put_pixel(x, y, Rgba([colour[0], colour[1], colour[2], 255]));
            }
        }
        draw_text_centered(&mut img, title, 0, BAND_Y, CARD_W, BAND_H, true, LABEL_SIZE, LABEL_COLOUR);
        for (line, centre) in lines.iter().take(2).zip(SUB_LINE_CENTRES) {
            draw_text_centered(&mut img, line, 6, centre - 11, CARD_W - 12, 22, false, SUB_SIZE, SUB_COLOUR);
        }
        encode_rgba(&img, 92)
    }

    /// Whatever is behind a card when a slot is empty.
    pub fn blank_card(&self, slot: usize) -> Result<Jpeg> {
        encode_rgba(&self.region(CARD_X[slot], CARD_Y, CARD_W, CARD_H), 92)
    }

    /// Mix Create mute button under a card ("AUDIENCE", red when this mute is active).
    pub fn button(&self, slot: usize, label: &str, active: bool) -> Result<Jpeg> {
        let mut img = self.region(button_x(slot), BUTTON_Y, BUTTON_W, BUTTON_H);
        let (frame, colour) = match active {
            true => (&self.button_on, BUTTON_ON_TEXT),
            false => (&self.button_off, BUTTON_OFF_TEXT),
        };
        imageops::overlay(&mut img, frame, 0, 0);
        draw_text_centered(&mut img, label, 8, 0, BUTTON_W - 16, BUTTON_H, true, BUTTON_SIZE, colour);
        encode_rgba(&img, 92)
    }

    pub fn blank_button(&self, slot: usize) -> Result<Jpeg> {
        encode_rgba(&self.region(button_x(slot), BUTTON_Y, BUTTON_W, BUTTON_H), 92)
    }

    /// Personal (Mix A) / Audience (Mix B) toggle at the top. `None` blanks it (Targets view).
    pub fn mix_pill(&self, mix: Option<Mix>) -> Result<Jpeg> {
        let (px, py) = PILL_POS;
        let mut img = self.region(px, py, PILL_SIZE.0, PILL_SIZE.1);
        if let Some(mix) = mix {
            imageops::overlay(&mut img, &self.mix_pill, 0, 0);
            let (knob, kx) = match mix {
                Mix::A => (&self.knob_a, 4),
                Mix::B => (&self.knob_b, PILL_SIZE.0 as i64 - 4 - self.knob_b.width() as i64),
            };
            let ky = (PILL_SIZE.1 as i64 - knob.height() as i64) / 2;
            imageops::overlay(&mut img, knob, kx, ky);
        }
        encode_rgba(&img, 92)
    }

    /// Page indicator under the cards. Hidden when there is only one page.
    pub fn page_dots(&self, pages: u8, active: u8) -> Result<Jpeg> {
        let (dx, dy) = DOTS_POS;
        let (dw, dh) = DOTS_SIZE;
        let mut img = self.region(dx, dy, dw, dh);
        if pages > 1 {
            let n = pages.min(10) as u32;
            let width = (16 * n + 25).min(dw);
            let pill = stretch_pill(&self.dots_pill, width);
            let ox = (dw - width) / 2;
            let oy = (dh - pill.height()) / 2;
            imageops::overlay(&mut img, &pill, ox as i64, oy as i64);
            let first = ox + (width - 16 * (n - 1)) / 2;
            for i in 0..n {
                let dot = if i as u8 == active { &self.dot_on } else { &self.dot_off };
                let cx = first + 16 * i;
                let cy = dh / 2;
                imageops::overlay(
                    &mut img,
                    dot,
                    cx as i64 - dot.width() as i64 / 2,
                    cy as i64 - dot.height() as i64 / 2,
                );
            }
        }
        encode_rgba(&img, 92)
    }

    /// The official dial face. Volume 0 has no official tile, the kit synthesises one.
    pub fn tile(&self, pack: &ImagePack, face: Face, volume: u8, meter: u8) -> Jpeg {
        if volume == 0 && face == Face::Volume {
            return self.zero.clone();
        }
        pack.tile(face, volume, meter)
    }
}

/// 3-slice horizontal stretch of a rounded pill.
fn stretch_pill(src: &RgbaImage, width: u32) -> RgbaImage {
    let (w, h) = src.dimensions();
    if width <= w {
        return src.clone();
    }
    let cap = h / 2;
    let mut out = RgbaImage::new(width, h);
    imageops::overlay(&mut out, &imageops::crop_imm(src, 0, 0, cap, h).to_image(), 0, 0);
    let mid = imageops::crop_imm(src, w / 2, 0, 1, h).to_image();
    for x in cap..width - cap {
        imageops::overlay(&mut out, &mid, x as i64, 0);
    }
    imageops::overlay(
        &mut out,
        &imageops::crop_imm(src, w - cap, 0, cap, h).to_image(),
        (width - cap) as i64,
        0,
    );
    out
}

pub fn text_width(text: &str, bold: bool, size: f32) -> f32 {
    let f = font(bold);
    text.chars().map(|c| f.metrics(c, size).advance_width).sum()
}

/// Shortens `text` with an ellipsis until it fits `max` pixels.
pub fn fit_text(text: &str, bold: bool, size: f32, max: f32) -> String {
    if text_width(text, bold, size) <= max {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().collect::<String>().trim_end().to_string() + "...";
        if text_width(&candidate, bold, size) <= max {
            return candidate;
        }
    }
    "...".into()
}

#[allow(clippy::too_many_arguments)]
fn draw_text_centered(
    img: &mut RgbaImage,
    text: &str,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    bold: bool,
    size: f32,
    colour: Rgba<u8>,
) {
    let text = fit_text(text, bold, size, w as f32);
    let f = font(bold);
    let lm = f.horizontal_line_metrics(size).expect("line metrics");
    let total = lm.ascent - lm.descent;
    let baseline = y as f32 + (h as f32 - total) / 2.0 + lm.ascent;
    let width = text_width(&text, bold, size);
    let mut pen = x as f32 + ((w as f32 - width) / 2.0).max(0.0);
    for c in text.chars() {
        let (m, bitmap) = f.rasterize(c, size);
        let gx = pen.round() as i32 + m.xmin;
        let gy = baseline.round() as i32 - m.ymin - m.height as i32;
        for row in 0..m.height {
            for col in 0..m.width {
                let a = bitmap[row * m.width + col] as f32 / 255.0;
                if a <= 0.0 {
                    continue;
                }
                let (px, py) = (gx + col as i32, gy + row as i32);
                if px < 0 || py < 0 || px >= img.width() as i32 || py >= img.height() as i32 {
                    continue;
                }
                let dst = img.get_pixel_mut(px as u32, py as u32);
                for i in 0..3 {
                    dst[i] = (colour[i] as f32 * a + dst[i] as f32 * (1.0 - a)).round() as u8;
                }
            }
        }
        pen += m.advance_width;
    }
}

fn encode_rgb(img: &RgbImage, quality: u8) -> Result<Jpeg> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut Cursor::new(&mut out), quality).encode(
        img.as_raw(),
        img.width(),
        img.height(),
        ExtendedColorType::Rgb8,
    )?;
    Ok(Arc::new(out))
}

fn encode_rgba(img: &RgbaImage, quality: u8) -> Result<Jpeg> {
    let rgb: RgbImage = RgbImage::from_fn(img.width(), img.height(), |x, y| {
        let p = img.get_pixel(x, y);
        Rgb([p[0], p[1], p[2]])
    });
    encode_rgb(&rgb, quality)
}

fn configured_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("BEACN_OFFICIAL_SCREEN")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    if let Some(dirs) = directories::ProjectDirs::from("", "", "beacn-utility")
        && let Ok(s) = std::fs::read_to_string(dirs.config_dir().join("official-screen"))
        && !s.trim().is_empty()
    {
        return Some(PathBuf::from(s.trim()));
    }
    // Default: <extracted>/screen-default next to <extracted>/MixImages/default.images
    let pack = official::configured_path()?;
    Some(pack.parent()?.parent()?.join("screen-default"))
}

static KIT: OnceLock<Option<ScreenKit>> = OnceLock::new();

/// The official screen kit, only when the official dial pack is also available.
pub fn kit() -> Option<&'static ScreenKit> {
    official::pack()?;
    KIT.get_or_init(|| {
        let dir = configured_dir()?;
        match ScreenKit::load(&dir) {
            Ok(k) => {
                info!("Official screen kit loaded from {}", dir.display());
                Some(k)
            }
            Err(e) => {
                warn!("Official screen kit not loaded ({}): {e:#}", dir.display());
                None
            }
        }
    })
    .as_ref()
}

/// Which official dial face shows for a channel, given the mix being viewed.
pub fn face_for(is_source: bool, mute_a: (bool, bool), mute_b: bool, mix: Mix) -> Face {
    let (a_active, a_to_all) = mute_a;
    if !is_source {
        return if a_active { Face::Muted } else { Face::Volume };
    }
    if a_active && (a_to_all || mix == Mix::A) {
        Face::Muted
    } else if mute_b && mix == Mix::B {
        Face::StreamMuted
    } else {
        Face::Volume
    }
}

// ---------------------------------------------------------------------------------------------
// Preview rendering: composes full screens exactly the way the device receives them (JPEG pieces
// at x,y) so the look can be checked without hardware.
//   BEACN_TEST_IMAGES=/path/default.images BEACN_OFFICIAL_SCREEN=/path/screen-default \
//   BEACN_PREVIEW_OUT=/tmp/out cargo test --release official_preview -- --ignored
// ---------------------------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    pub struct PreviewSlot<'a> {
        pub title: &'a str,
        pub colour: [u8; 3],
        pub lines: Vec<String>,
        pub volume: u8,
        pub meter: u8,
        pub face: Face,
        pub button: Option<(&'a str, bool)>,
    }

    fn paste(screen: &mut RgbImage, jpeg: &[u8], x: u32, y: u32) {
        let img = image::load_from_memory(jpeg).unwrap().to_rgb8();
        imageops::replace(screen, &img, x as i64, y as i64);
    }

    fn compose(
        kit: &ScreenKit,
        pack: &ImagePack,
        slots: &[Option<PreviewSlot>],
        mix: Option<Mix>,
        pages: u8,
        page: u8,
        create: bool,
    ) -> RgbImage {
        let mut s = RgbImage::new(SCREEN.0, SCREEN.1);
        paste(&mut s, &kit.background().unwrap(), 0, 0);
        paste(&mut s, &kit.mix_pill(mix).unwrap(), PILL_POS.0, PILL_POS.1);
        paste(&mut s, &kit.page_dots(pages, page).unwrap(), DOTS_POS.0, DOTS_POS.1);
        for (i, slot) in slots.iter().enumerate() {
            match slot {
                None => {
                    paste(&mut s, &kit.blank_card(i).unwrap(), CARD_X[i], CARD_Y);
                    if create {
                        paste(&mut s, &kit.blank_button(i).unwrap(), button_x(i), BUTTON_Y);
                    }
                }
                Some(c) => {
                    paste(&mut s, &kit.card(i, c.title, c.colour, &c.lines).unwrap(), CARD_X[i], CARD_Y);
                    let m = ((c.meter as f32 / 100.0) * c.volume as f32).round() as u8;
                    let tile = kit.tile(pack, c.face, c.volume, m);
                    let (dx, dy) = pack.centre_offset((CARD_W, TILE_H));
                    paste(&mut s, &tile, (CARD_X[i] as i32 + dx) as u32, (CARD_Y as i32 + dy) as u32);
                    if create {
                        match c.button {
                            Some((l, a)) => paste(&mut s, &kit.button(i, l, a).unwrap(), button_x(i), BUTTON_Y),
                            None => paste(&mut s, &kit.blank_button(i).unwrap(), button_x(i), BUTTON_Y),
                        }
                    }
                }
            }
        }
        s
    }

    #[test]
    fn face_rules() {
        assert_eq!(face_for(true, (true, true), false, Mix::B), Face::Muted);
        assert_eq!(face_for(true, (true, false), false, Mix::A), Face::Muted);
        assert_eq!(face_for(true, (true, false), false, Mix::B), Face::Volume);
        assert_eq!(face_for(true, (false, false), true, Mix::B), Face::StreamMuted);
        assert_eq!(face_for(true, (false, false), true, Mix::A), Face::Volume);
        assert_eq!(face_for(false, (true, true), false, Mix::A), Face::Muted);
    }

    #[test]
    fn ellipsis_fits() {
        let long = "Microphone Array (Realtek High Definition Audio)";
        let t = fit_text(long, false, SUB_SIZE, 175.0);
        assert!(t.ends_with("..."));
        assert!(text_width(&t, false, SUB_SIZE) <= 175.0);
        assert_eq!(fit_text("Mic", true, LABEL_SIZE, 175.0), "Mic");
    }

    #[test]
    #[ignore]
    fn official_preview() {
        let pack = ImagePack::load(Path::new(&std::env::var("BEACN_TEST_IMAGES").unwrap())).unwrap();
        let kit = ScreenKit::load(Path::new(&std::env::var("BEACN_OFFICIAL_SCREEN").unwrap())).unwrap();
        let out = PathBuf::from(std::env::var("BEACN_PREVIEW_OUT").unwrap());
        std::fs::create_dir_all(&out).unwrap();
        let s = |v: Vec<&str>| v.into_iter().map(String::from).collect::<Vec<_>>();

        // Sources, Mix A (personal), Mix Create: a typical real-world Pipeweaver layout
        let sources = |mix: Mix| {
            vec![
                Some(PreviewSlot { title: "System", colour: [0, 177, 255], lines: s(vec!["Firefox", "Steam"]), volume: 76, meter: 55, face: Face::Volume, button: Some(("AUDIENCE", false)) }),
                Some(PreviewSlot { title: "Browser", colour: [255, 64, 64], lines: s(vec!["Brave"]), volume: 63, meter: 0, face: face_for(true, (true, true), false, mix), button: Some(("AUDIENCE", false)) }),
                Some(PreviewSlot { title: "Music", colour: [170, 0, 255], lines: s(vec!["Spotify"]), volume: if mix == Mix::A { 41 } else { 20 }, meter: 80, face: face_for(true, (false, false), true, mix), button: Some(("AUDIENCE", true)) }),
                Some(PreviewSlot { title: "Microphone", colour: [255, 240, 0], lines: s(vec!["BEACN Studio Microphone Input (Mono)"]), volume: 100, meter: 40, face: Face::Volume, button: Some(("MUTE ALL", false)) }),
            ]
        };
        compose(&kit, &pack, &sources(Mix::A), Some(Mix::A), 2, 0, true).save(out.join("mixcreate-sources-mixA.png")).unwrap();
        compose(&kit, &pack, &sources(Mix::B), Some(Mix::B), 2, 0, true).save(out.join("mixcreate-sources-mixB-audience.png")).unwrap();

        let page2 = vec![
            Some(PreviewSlot { title: "Game", colour: [0, 230, 118], lines: s(vec![]), volume: 0, meter: 0, face: Face::Volume, button: Some(("AUDIENCE", false)) }),
            Some(PreviewSlot { title: "Chat", colour: [0, 255, 255], lines: s(vec!["Discord"]), volume: 88, meter: 60, face: Face::Volume, button: Some(("AUDIENCE", false)) }),
            None,
            None,
        ];
        compose(&kit, &pack, &page2, Some(Mix::A), 2, 1, true).save(out.join("mixcreate-sources-page2.png")).unwrap();

        // Hold a dial: Targets view (no mix toggle, no mute buttons)
        let targets = vec![
            Some(PreviewSlot { title: "Headphones", colour: [0, 177, 255], lines: s(vec!["BEACN Studio Headphones"]), volume: 70, meter: 50, face: Face::Volume, button: None }),
            Some(PreviewSlot { title: "Speakers", colour: [255, 140, 0], lines: s(vec!["AORUS FV43U"]), volume: 35, meter: 20, face: face_for(false, (true, true), false, Mix::A), button: None }),
            Some(PreviewSlot { title: "Chat Mic", colour: [0, 255, 255], lines: s(vec!["Mix A"]), volume: 100, meter: 30, face: Face::Volume, button: None }),
            Some(PreviewSlot { title: "Audience", colour: [255, 64, 128], lines: s(vec!["Mix B"]), volume: 100, meter: 65, face: Face::Volume, button: None }),
        ];
        compose(&kit, &pack, &targets, None, 1, 0, true).save(out.join("mixcreate-targets.png")).unwrap();

        std::fs::write(out.join("splash-loading.jpg"), kit.splash(Some("Loading...")).unwrap().as_slice()).unwrap();
    }
}
