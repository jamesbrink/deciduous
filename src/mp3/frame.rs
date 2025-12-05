//! MP3 frame header parsing
//!
//! MP3 frames start with a sync word (11 bits of 1s) followed by header info.
//! Frame header structure (4 bytes):
//! AAAAAAAA AAABBCCD EEEEFFGH IIJJKLMM
//!
//! A = sync (11 bits)
//! B = MPEG version (2 bits): 00=2.5, 01=reserved, 10=2, 11=1
//! C = Layer (2 bits): 00=reserved, 01=III, 10=II, 11=I
//! D = Protection bit (CRC)
//! E = Bitrate index (4 bits)
//! F = Sample rate index (2 bits)
//! G = Padding bit
//! H = Private bit
//! I = Channel mode (2 bits)
//! J = Mode extension (2 bits)
//! K = Copyright
//! L = Original
//! M = Emphasis (2 bits)

use std::io::{self, Read, Seek, SeekFrom};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MpegVersion {
    Mpeg1,
    Mpeg2,
    Mpeg25,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Layer1,
    Layer2,
    Layer3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelMode {
    Stereo,
    JointStereo,
    DualChannel,
    Mono,
}

#[derive(Debug, Clone)]
pub struct FrameHeader {
    pub version: MpegVersion,
    pub layer: Layer,
    pub bitrate: u32,
    pub sample_rate: u32,
    pub padding: bool,
    pub channel_mode: ChannelMode,
    pub frame_size: u32,
    pub samples_per_frame: u32,
}

// Bitrate lookup tables (kbps)
// Index 0 = free, 15 = bad
const BITRATES_V1_L3: [u32; 16] = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0];
const BITRATES_V1_L2: [u32; 16] = [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 0];
const BITRATES_V1_L1: [u32; 16] = [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448, 0];
const BITRATES_V2_L3: [u32; 16] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0];
const BITRATES_V2_L2: [u32; 16] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0];
const BITRATES_V2_L1: [u32; 16] = [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256, 0];

// Sample rate lookup tables (Hz)
const SAMPLE_RATES_V1: [u32; 4] = [44100, 48000, 32000, 0];
const SAMPLE_RATES_V2: [u32; 4] = [22050, 24000, 16000, 0];
const SAMPLE_RATES_V25: [u32; 4] = [11025, 12000, 8000, 0];

impl FrameHeader {
    /// Parse a 4-byte MP3 frame header
    pub fn parse(header: [u8; 4]) -> Option<Self> {
        // Check sync word (11 bits of 1s)
        if header[0] != 0xFF || (header[1] & 0xE0) != 0xE0 {
            return None;
        }

        // MPEG version (bits 4-3 of byte 1)
        let version = match (header[1] >> 3) & 0x03 {
            0 => MpegVersion::Mpeg25,
            2 => MpegVersion::Mpeg2,
            3 => MpegVersion::Mpeg1,
            _ => return None, // Reserved
        };

        // Layer (bits 2-1 of byte 1)
        let layer = match (header[1] >> 1) & 0x03 {
            1 => Layer::Layer3,
            2 => Layer::Layer2,
            3 => Layer::Layer1,
            _ => return None, // Reserved
        };

        // Bitrate index (bits 7-4 of byte 2)
        let bitrate_idx = ((header[2] >> 4) & 0x0F) as usize;
        let bitrate = match (version, layer) {
            (MpegVersion::Mpeg1, Layer::Layer1) => BITRATES_V1_L1[bitrate_idx],
            (MpegVersion::Mpeg1, Layer::Layer2) => BITRATES_V1_L2[bitrate_idx],
            (MpegVersion::Mpeg1, Layer::Layer3) => BITRATES_V1_L3[bitrate_idx],
            (_, Layer::Layer1) => BITRATES_V2_L1[bitrate_idx],
            (_, Layer::Layer2) => BITRATES_V2_L2[bitrate_idx],
            (_, Layer::Layer3) => BITRATES_V2_L3[bitrate_idx],
        };

        if bitrate == 0 {
            return None; // Free or bad bitrate
        }

        // Sample rate index (bits 3-2 of byte 2)
        let sample_rate_idx = ((header[2] >> 2) & 0x03) as usize;
        let sample_rate = match version {
            MpegVersion::Mpeg1 => SAMPLE_RATES_V1[sample_rate_idx],
            MpegVersion::Mpeg2 => SAMPLE_RATES_V2[sample_rate_idx],
            MpegVersion::Mpeg25 => SAMPLE_RATES_V25[sample_rate_idx],
        };

        if sample_rate == 0 {
            return None;
        }

        // Padding (bit 1 of byte 2)
        let padding = (header[2] & 0x02) != 0;

        // Channel mode (bits 7-6 of byte 3)
        let channel_mode = match (header[3] >> 6) & 0x03 {
            0 => ChannelMode::Stereo,
            1 => ChannelMode::JointStereo,
            2 => ChannelMode::DualChannel,
            3 => ChannelMode::Mono,
            _ => unreachable!(),
        };

        // Samples per frame
        let samples_per_frame = match (version, layer) {
            (MpegVersion::Mpeg1, Layer::Layer1) => 384,
            (MpegVersion::Mpeg1, Layer::Layer2) => 1152,
            (MpegVersion::Mpeg1, Layer::Layer3) => 1152,
            (_, Layer::Layer1) => 384,
            (_, Layer::Layer2) => 1152,
            (_, Layer::Layer3) => 576,
        };

        // Frame size calculation
        let padding_size = if padding {
            match layer {
                Layer::Layer1 => 4,
                _ => 1,
            }
        } else {
            0
        };

        let frame_size = match layer {
            Layer::Layer1 => (12 * bitrate * 1000 / sample_rate + padding_size) * 4,
            _ => 144 * bitrate * 1000 / sample_rate + padding_size,
        };

        Some(FrameHeader {
            version,
            layer,
            bitrate,
            sample_rate,
            padding,
            channel_mode,
            frame_size,
            samples_per_frame,
        })
    }
}

/// Statistics about frames in an MP3 file
#[derive(Debug, Clone, Default)]
pub struct FrameStats {
    pub frame_count: usize,
    pub bitrates: Vec<u32>,
    pub frame_sizes: Vec<u32>,
    pub is_vbr: bool,
    pub avg_bitrate: u32,
    pub min_bitrate: u32,
    pub max_bitrate: u32,
}

impl FrameStats {
    /// Calculate coefficient of variation for frame sizes
    pub fn frame_size_cv(&self) -> f64 {
        if self.frame_sizes.is_empty() {
            return 0.0;
        }

        let mean: f64 = self.frame_sizes.iter().map(|&x| x as f64).sum::<f64>()
            / self.frame_sizes.len() as f64;

        if mean == 0.0 {
            return 0.0;
        }

        let variance: f64 = self.frame_sizes.iter()
            .map(|&x| {
                let diff = x as f64 - mean;
                diff * diff
            })
            .sum::<f64>() / self.frame_sizes.len() as f64;

        let stddev = variance.sqrt();
        (stddev / mean) * 100.0
    }
}

/// Scan an MP3 file and collect frame statistics
pub fn scan_frames<R: Read + Seek>(reader: &mut R, max_frames: usize) -> io::Result<FrameStats> {
    let mut stats = FrameStats::default();
    let mut buf = [0u8; 4];
    let mut unique_bitrates = std::collections::HashSet::new();

    // Skip ID3v2 tag if present
    reader.seek(SeekFrom::Start(0))?;
    reader.read_exact(&mut buf[..3])?;

    if &buf[..3] == b"ID3" {
        reader.read_exact(&mut buf)?;
        let size = ((buf[0] as u32 & 0x7F) << 21)
            | ((buf[1] as u32 & 0x7F) << 14)
            | ((buf[2] as u32 & 0x7F) << 7)
            | (buf[3] as u32 & 0x7F);
        reader.seek(SeekFrom::Start(10 + size as u64))?;
    } else {
        reader.seek(SeekFrom::Start(0))?;
    }

    // Scan for frames
    while stats.frame_count < max_frames {
        match reader.read_exact(&mut buf) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }

        if let Some(frame) = FrameHeader::parse(buf) {
            stats.frame_count += 1;
            stats.bitrates.push(frame.bitrate);
            stats.frame_sizes.push(frame.frame_size);
            unique_bitrates.insert(frame.bitrate);

            // Seek to next frame
            if frame.frame_size > 4 {
                reader.seek(SeekFrom::Current(frame.frame_size as i64 - 4))?;
            }
        } else {
            // Not a valid frame header, try next byte
            reader.seek(SeekFrom::Current(-3))?;
        }
    }

    if !stats.bitrates.is_empty() {
        stats.is_vbr = unique_bitrates.len() > 1;
        stats.avg_bitrate = stats.bitrates.iter().sum::<u32>() / stats.bitrates.len() as u32;
        stats.min_bitrate = *stats.bitrates.iter().min().unwrap();
        stats.max_bitrate = *stats.bitrates.iter().max().unwrap();
    }

    Ok(stats)
}

/// Find the sync position (first valid frame) in an MP3 file
pub fn find_sync<R: Read + Seek>(reader: &mut R) -> io::Result<Option<u64>> {
    let mut buf = [0u8; 4];

    // Skip ID3v2 tag if present
    reader.seek(SeekFrom::Start(0))?;
    reader.read_exact(&mut buf[..3])?;

    let start_pos = if &buf[..3] == b"ID3" {
        reader.read_exact(&mut buf)?;
        let size = ((buf[0] as u32 & 0x7F) << 21)
            | ((buf[1] as u32 & 0x7F) << 14)
            | ((buf[2] as u32 & 0x7F) << 7)
            | (buf[3] as u32 & 0x7F);
        10 + size as u64
    } else {
        0
    };

    reader.seek(SeekFrom::Start(start_pos))?;

    // Search for sync
    let mut pos = start_pos;
    loop {
        match reader.read_exact(&mut buf) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        }

        if FrameHeader::parse(buf).is_some() {
            return Ok(Some(pos));
        }

        reader.seek(SeekFrom::Current(-3))?;
        pos += 1;

        // Don't search forever
        if pos > start_pos + 10000 {
            return Ok(None);
        }
    }
}
