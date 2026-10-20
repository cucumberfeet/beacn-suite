//! Optional "Official Artwork" mode.
//!
//! Renders the Mix / Mix Create dial using the pre-rendered JPEG tiles that ship with
//! the official BEACN Windows app (`MixImages/*.images`). No artwork is bundled here:
//! users point the utility at files from their own installer.
//!
//! File format (reverse engineered, BEACN app 1.4.2.12):
//!   repeated `[u32 little-endian length][JPEG bytes]`, 30,300 records, no header.
//!
//! Index layout (high confidence from visual analysis, see docs):
//!   6 "faces" x 5,050 tiles. Within a face, tiles are triangular:
//!   for volume v in 1..=100, there are v meter levels m in 0..v (meter <= volume),
//!   index = face * 5050 + v*(v-1)/2 + m.
//!   Faces: 0 = numeric volume, 1 = muted (speaker), 2 = stream muted,
//!          3 = monitor/headphone muted, 4 = chat muted, 5 = media muted
//!   (faces 2..5 naming is medium confidence).
//!
//! Configuration: env `BEACN_OFFICIAL_ARTWORK=/path/to/default.images`, or a file at
//! `$XDG_CONFIG_HOME/beacn-utility/official-artwork` containing the path.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

pub const VOLUME_STEPS: usize = 100;
pub const TILES_PER_FACE: usize = VOLUME_STEPS * (VOLUME_STEPS + 1) / 2; // 5050
pub const FACE_COUNT: usize = 6;
pub const EXPECTED_TILES: usize = TILES_PER_FACE * FACE_COUNT; // 30300

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    Volume = 0,
    Muted = 1,
    StreamMuted = 2,
    MonitorMuted = 3,
    ChatMuted = 4,
    MediaMuted = 5,
}

/// Map a (face, volume 0..=100, meter 0..=100 *already scaled to volume*) to a tile index.
/// Volume 0 has no tile in the official set; the lowest tile (v=1, m=0) is used.
pub fn tile_index(face: Face, volume: u8, meter: u8) -> usize {
    let v = (volume as usize).clamp(1, VOLUME_STEPS);
    let m = (meter as usize).min(v - 1);
    face as usize * TILES_PER_FACE + v * (v - 1) / 2 + m
}

/// Inverse of `tile_index` (used for tests / tooling).
pub fn tile_state(index: usize) -> Option<(Face, u8, u8)> {
    if index >= EXPECTED_TILES {
        return None;
    }
    let face = match index / TILES_PER_FACE {
        0 => Face::Volume,
        1 => Face::Muted,
        2 => Face::StreamMuted,
        3 => Face::MonitorMuted,
        4 => Face::ChatMuted,
        _ => Face::MediaMuted,
    };
    let mut rem = index % TILES_PER_FACE;
    let mut v = 1;
    while rem >= v {
        rem -= v;
        v += 1;
    }
    Some((face, v as u8, rem as u8))
}

pub struct ImagePack {
    tiles: Vec<Arc<Vec<u8>>>,
    pub width: u32,
    pub height: u32,
}

impl ImagePack {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut tiles = Vec::with_capacity(EXPECTED_TILES);
        let mut i = 0usize;
        while i < data.len() {
            if i + 4 > data.len() {
                bail!("truncated length at offset {i}");
            }
            let len = u32::from_le_bytes(data[i..i + 4].try_into()?) as usize;
            let start = i + 4;
            let end = start.checked_add(len).context("overflow")?;
            if end > data.len() {
                bail!("record at {i} overruns file");
            }
            let jpeg = &data[start..end];
            if jpeg.len() < 2 || jpeg[0] != 0xFF || jpeg[1] != 0xD8 {
                bail!("record {} is not a JPEG", tiles.len());
            }
            tiles.push(Arc::new(jpeg.to_vec()));
            i = end;
        }
        if tiles.len() != EXPECTED_TILES {
            bail!("expected {EXPECTED_TILES} tiles, found {}", tiles.len());
        }
        let (width, height) = jpeg_dimensions(&tiles[0]).context("no JPEG SOF")?;
        Ok(Self { tiles, width, height })
    }

    pub fn load(path: &Path) -> Result<Self> {
        let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&data)
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn tile(&self, face: Face, volume: u8, meter: u8) -> Arc<Vec<u8>> {
        self.tiles[tile_index(face, volume, meter)].clone()
    }

    /// Offset inside the utility's dial slot so the official tile is centred on it.
    /// Signed because official tiles (e.g. 187x180) can exceed the 174x174 slot.
    pub fn centre_offset(&self, slot: (u32, u32)) -> (i32, i32) {
        (
            (slot.0 as i32 - self.width as i32) / 2,
            (slot.1 as i32 - self.height as i32) / 2,
        )
    }
}

/// Read width/height from a JPEG SOF0/SOF2 marker.
pub fn jpeg_dimensions(j: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    while i + 9 < j.len() {
        if j[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = j[i + 1];
        let len = u16::from_be_bytes([j[i + 2], j[i + 3]]) as usize;
        if matches!(marker, 0xC0 | 0xC1 | 0xC2) {
            let h = u16::from_be_bytes([j[i + 5], j[i + 6]]) as u32;
            let w = u16::from_be_bytes([j[i + 7], j[i + 8]]) as u32;
            return Some((w, h));
        }
        i += 2 + len;
    }
    None
}

pub(crate) fn configured_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("BEACN_OFFICIAL_ARTWORK") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    let dirs = directories::ProjectDirs::from("", "", "beacn-utility")?;
    let f = dirs.config_dir().join("official-artwork");
    let s = std::fs::read_to_string(f).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| PathBuf::from(s))
}

static PACK: OnceLock<Option<ImagePack>> = OnceLock::new();

/// The user-configured official pack, if any (loaded once, failures logged).
pub fn pack() -> Option<&'static ImagePack> {
    PACK.get_or_init(|| {
        let path = configured_path()?;
        match ImagePack::load(&path) {
            Ok(p) => {
                log::info!("Official artwork loaded from {} ({}x{})", path.display(), p.width, p.height);
                Some(p)
            }
            Err(e) => {
                log::warn!("Official artwork disabled: {e:#}");
                None
            }
        }
    })
    .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_jpeg(w: u16, h: u16, tag: u8) -> Vec<u8> {
        let mut j = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, tag, 0x00];
        j.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
        j.extend_from_slice(&h.to_be_bytes());
        j.extend_from_slice(&w.to_be_bytes());
        j.extend_from_slice(&[0x03; 10]);
        j.extend_from_slice(&[0xFF, 0xD9]);
        j
    }

    fn fake_pack(n: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for i in 0..n {
            let j = fake_jpeg(187, 180, (i % 251) as u8);
            out.extend_from_slice(&(j.len() as u32).to_le_bytes());
            out.extend_from_slice(&j);
        }
        out
    }

    #[test]
    fn index_known_points() {
        assert_eq!(tile_index(Face::Volume, 1, 0), 0);
        assert_eq!(tile_index(Face::Volume, 2, 0), 1);
        assert_eq!(tile_index(Face::Volume, 2, 1), 2);
        // Verified visually: record 500 shows "32"
        assert_eq!(tile_state(500).unwrap().1, 32);
        assert_eq!(tile_index(Face::Volume, 100, 99), TILES_PER_FACE - 1);
        assert_eq!(tile_index(Face::Muted, 1, 0), TILES_PER_FACE);
        assert_eq!(tile_index(Face::MediaMuted, 100, 99), EXPECTED_TILES - 1);
    }

    #[test]
    fn clamping() {
        assert_eq!(tile_index(Face::Volume, 0, 0), 0);
        assert_eq!(tile_index(Face::Volume, 200, 200), TILES_PER_FACE - 1);
        assert_eq!(tile_index(Face::Volume, 10, 10), tile_index(Face::Volume, 10, 9));
    }

    #[test]
    fn roundtrip_all() {
        for i in 0..EXPECTED_TILES {
            let (f, v, m) = tile_state(i).unwrap();
            assert_eq!(tile_index(f, v, m), i);
        }
        assert!(tile_state(EXPECTED_TILES).is_none());
    }

    #[test]
    fn parse_pack() {
        let p = ImagePack::parse(&fake_pack(EXPECTED_TILES)).unwrap();
        assert_eq!(p.len(), EXPECTED_TILES);
        assert_eq!((p.width, p.height), (187, 180));
        assert_eq!(p.centre_offset((174, 174)), (-6, -3));
        assert_eq!(p.tile(Face::Volume, 2, 1)[6], 2);
    }

    #[test]
    fn parse_rejects_bad() {
        assert!(ImagePack::parse(&fake_pack(10)).is_err());
        let mut d = fake_pack(EXPECTED_TILES);
        d.truncate(d.len() - 3);
        assert!(ImagePack::parse(&d).is_err());
        assert!(ImagePack::parse(&[5, 0, 0, 0, 1, 2, 3, 4, 5]).is_err());
    }

    /// Runs against a real pack when BEACN_TEST_IMAGES points at one.
    #[test]
    fn real_pack_if_available() {
        if let Ok(p) = std::env::var("BEACN_TEST_IMAGES") {
            let pack = ImagePack::load(Path::new(&p)).unwrap();
            assert_eq!(pack.len(), EXPECTED_TILES);
        }
    }
}
