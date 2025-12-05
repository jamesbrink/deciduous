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

#[derive(Debug, Clone, Copy)]
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
    // ID3v2 header: "ID3" (3) + version (2) + flags (1) + size (4) = 10 bytes
    reader.seek(SeekFrom::Start(0))?;
    match reader.read_exact(&mut buf[..3]) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(stats),
        Err(e) => return Err(e),
    }

    if &buf[..3] == b"ID3" {
        // Skip version (2 bytes) and flags (1 byte), then read size (4 bytes)
        reader.seek(SeekFrom::Start(6))?;
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
    // ID3v2 header: "ID3" (3) + version (2) + flags (1) + size (4) = 10 bytes
    reader.seek(SeekFrom::Start(0))?;
    reader.read_exact(&mut buf[..3])?;

    let start_pos = if &buf[..3] == b"ID3" {
        // Skip to size field at offset 6, then read 4 bytes
        reader.seek(SeekFrom::Start(6))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // ==========================================================================
    // EDUCATIONAL BACKGROUND: Understanding MP3 Frame Structure
    // ==========================================================================
    //
    // An MP3 file is a sequence of "frames", each containing a small chunk of
    // compressed audio (typically 26ms at 44.1kHz). Each frame starts with a
    // 4-byte header that describes how to decode it.
    //
    // FRAME HEADER STRUCTURE (4 bytes = 32 bits):
    // ┌─────────┬───────┬───────┬───────┬────────┬────────┬─────┬─────┬──────┬─────┬───┬───┬────┐
    // │ Sync    │Version│ Layer │Protect│Bitrate │SampRate│Pad  │Priv │ChanMd│MdExt│Cpy│Org│Emph│
    // │11 bits  │2 bits │2 bits │1 bit  │4 bits  │2 bits  │1 bit│1 bit│2 bits│2bits│1b │1b │2b  │
    // └─────────┴───────┴───────┴───────┴────────┴────────┴─────┴─────┴──────┴─────┴───┴───┴────┘
    //  0xFFE or 0xFFF   See lookup tables below for decoding bitrate/sample rate
    //
    // SYNC WORD: All 1s (0xFFE0 or higher) - this is how players find frames
    // VERSION: 00=MPEG2.5, 10=MPEG2, 11=MPEG1 (01 is reserved)
    // LAYER: 01=Layer3, 10=Layer2, 11=Layer1 (00 is reserved)
    //
    // WHY THIS MATTERS FOR TRANSCODE DETECTION:
    // - Parsing frame headers lets us calculate actual bitrate
    // - VBR (Variable Bit Rate) files have different bitrates per frame
    // - If a "320kbps" file has mostly 128kbps frames, it's suspicious
    // - Frame size variation (CV) can indicate quality issues
    // ==========================================================================

    // ==========================================================================
    // BITRATE LOOKUP TABLES
    // ==========================================================================
    //
    // The 4-bit bitrate index in the header maps to different values depending
    // on MPEG version and layer. Here's the MPEG1 Layer3 table (most common):
    //
    // Index: 0    1    2    3    4    5    6    7    8    9   10   11   12   13   14   15
    // kbps:  -   32   40   48   56   64   80   96  112  128  160  192  224  256  320   -
    //
    // Index 0 = "free format" (rare), Index 15 = invalid/bad
    //
    // For our 128kbps example: index 9 = 0x9 → 128 kbps
    // For our 320kbps example: index 14 = 0xE → 320 kbps
    // ==========================================================================

    /// Create a valid MPEG1 Layer3 128kbps 44.1kHz stereo frame header
    ///
    /// Byte breakdown:
    /// - 0xFF: First 8 bits of sync word (all 1s)
    /// - 0xFB: Remaining sync (111), MPEG1 (11), Layer3 (01), no CRC (1)
    /// - 0x90: Bitrate index 9 (1001) = 128kbps, Sample rate 0 (00) = 44100Hz, no padding (0)
    /// - 0x00: Stereo (00), no mode ext, not copyrighted, original, no emphasis
    fn valid_mp3_header() -> [u8; 4] {
        [0xFF, 0xFB, 0x90, 0x00]
    }

    /// Create a valid MPEG1 Layer3 320kbps 44.1kHz stereo frame header
    ///
    /// 320kbps uses bitrate index 14 = 0xE, so byte 2 is 0xE0
    fn valid_mp3_header_320() -> [u8; 4] {
        [0xFF, 0xFB, 0xE0, 0x00]
    }

    // ==========================================================================
    // FRAME HEADER PARSING TESTS
    // These verify we correctly decode the binary header format
    // ==========================================================================

    #[test]
    fn test_parse_valid_header_128kbps() {
        // Most common MP3 format: MPEG1 Layer3, 128kbps, 44.1kHz, stereo
        // This is the format used by most "standard quality" MP3s
        let header = valid_mp3_header();
        let parsed = FrameHeader::parse(header).expect("Should parse valid header");

        assert_eq!(parsed.version, MpegVersion::Mpeg1);
        assert_eq!(parsed.layer, Layer::Layer3);
        assert_eq!(parsed.bitrate, 128);
        assert_eq!(parsed.sample_rate, 44100);
        assert!(!parsed.padding);
        assert_eq!(parsed.channel_mode, ChannelMode::Stereo);
        // MPEG1 Layer3 has 1152 samples per frame
        // At 44100Hz, that's about 26.1ms per frame
        assert_eq!(parsed.samples_per_frame, 1152);
    }

    #[test]
    fn test_parse_valid_header_320kbps() {
        // High quality MP3: 320kbps is the maximum standard bitrate
        // This preserves frequencies up to ~20kHz
        let header = valid_mp3_header_320();
        let parsed = FrameHeader::parse(header).expect("Should parse valid header");

        assert_eq!(parsed.version, MpegVersion::Mpeg1);
        assert_eq!(parsed.layer, Layer::Layer3);
        assert_eq!(parsed.bitrate, 320);
        assert_eq!(parsed.sample_rate, 44100);
    }

    #[test]
    fn test_parse_invalid_sync() {
        // The sync word must be 11 bits of 1s (0xFFE0 or higher)
        // This is how MP3 players find frame boundaries in the stream

        // Completely invalid - no sync bits
        let header = [0x00, 0x00, 0x00, 0x00];
        assert!(FrameHeader::parse(header).is_none());

        // Partial sync - first byte is 0xFF but second byte doesn't have high bits set
        // 0xFF 0x00 = 11111111 00000000, but we need 11111111 111xxxxx
        let header = [0xFF, 0x00, 0x00, 0x00];
        assert!(FrameHeader::parse(header).is_none());
    }

    #[test]
    fn test_parse_reserved_version() {
        // MPEG version bits: 00=2.5, 01=RESERVED, 10=2, 11=1
        // The reserved value (01) should fail parsing
        // 0xE8 = 11101000: sync bits OK, but version = 01 (reserved)
        let header = [0xFF, 0xE8, 0x90, 0x00];
        assert!(FrameHeader::parse(header).is_none());
    }

    #[test]
    fn test_parse_reserved_layer() {
        // Layer bits: 00=RESERVED, 01=Layer3, 10=Layer2, 11=Layer1
        // 0xE0 = 11100000: sync OK, MPEG2.5, but layer = 00 (reserved)
        let header = [0xFF, 0xE0, 0x90, 0x00];
        assert!(FrameHeader::parse(header).is_none());
    }

    #[test]
    fn test_parse_invalid_bitrate() {
        // Bitrate index 0 = "free format" (variable, rare)
        // Bitrate index 15 = invalid/bad frame
        // We reject both to avoid parsing garbage data

        // Index 15 (0xF in upper nibble of byte 2)
        let header = [0xFF, 0xFB, 0xF0, 0x00];
        assert!(FrameHeader::parse(header).is_none());

        // Index 0 (free format - we don't support)
        let header = [0xFF, 0xFB, 0x00, 0x00];
        assert!(FrameHeader::parse(header).is_none());
    }

    #[test]
    fn test_parse_invalid_sample_rate() {
        // Sample rate index: 00=44100, 01=48000, 10=32000, 11=RESERVED
        // 0x9C = 10011100: bitrate 9 (128kbps), sample rate = 11 (reserved)
        let header = [0xFF, 0xFB, 0x9C, 0x00];
        assert!(FrameHeader::parse(header).is_none());
    }

    // ==========================================================================
    // CHANNEL MODE TESTS
    // ==========================================================================
    //
    // MP3 supports four channel modes:
    // - Stereo (00): Independent left/right channels
    // - Joint Stereo (01): Exploits stereo redundancy for better compression
    // - Dual Channel (10): Two independent mono channels
    // - Mono (11): Single channel
    //
    // Joint Stereo is most common in modern MP3s - it uses "mid/side" encoding
    // to compress similar content between channels more efficiently.
    // ==========================================================================

    #[test]
    fn test_parse_mono_channel() {
        // Mono uses channel mode bits = 11 (0xC0 in byte 3)
        // Good for podcasts, audiobooks, or voice recordings
        let header = [0xFF, 0xFB, 0x90, 0xC0];
        let parsed = FrameHeader::parse(header).expect("Should parse");
        assert_eq!(parsed.channel_mode, ChannelMode::Mono);
    }

    #[test]
    fn test_parse_joint_stereo() {
        // Joint Stereo (01) is the most efficient stereo mode
        // It exploits the fact that left and right channels often have
        // similar content, encoding the "middle" and "side" instead
        let header = [0xFF, 0xFB, 0x90, 0x40];
        let parsed = FrameHeader::parse(header).expect("Should parse");
        assert_eq!(parsed.channel_mode, ChannelMode::JointStereo);
    }

    #[test]
    fn test_parse_with_padding() {
        // Padding adds 1 byte (or 4 for Layer1) to make frame sizes
        // average out correctly. Without padding, accumulated rounding
        // errors would cause audio drift over long files.
        //
        // Padding bit is bit 1 of byte 2 (0x02)
        // 0x92 = 0x90 | 0x02 = 128kbps with padding
        let header = [0xFF, 0xFB, 0x92, 0x00];
        let parsed = FrameHeader::parse(header).expect("Should parse");
        assert!(parsed.padding);
    }

    // ==========================================================================
    // MPEG VERSION TESTS
    // ==========================================================================
    //
    // MPEG Audio has evolved through versions:
    // - MPEG1 (1993): 32, 44.1, 48 kHz - highest quality, most common
    // - MPEG2 (1995): 16, 22.05, 24 kHz - for lower bandwidth applications
    // - MPEG2.5 (unofficial): 8, 11.025, 12 kHz - for very low bandwidth
    //
    // MPEG2/2.5 Layer3 has 576 samples per frame vs MPEG1's 1152.
    // ==========================================================================

    #[test]
    fn test_parse_mpeg2() {
        // MPEG2 uses half the sample rates of MPEG1
        // Version bits = 10, so byte 1 = 0xF3 (11110011)
        let header = [0xFF, 0xF3, 0x90, 0x00];
        let parsed = FrameHeader::parse(header).expect("Should parse");
        assert_eq!(parsed.version, MpegVersion::Mpeg2);
        assert_eq!(parsed.sample_rate, 22050); // Half of MPEG1's 44100
    }

    #[test]
    fn test_parse_mpeg25() {
        // MPEG2.5 uses quarter sample rates of MPEG1
        // Version bits = 00, so byte 1 = 0xE3 (11100011)
        let header = [0xFF, 0xE3, 0x90, 0x00];
        let parsed = FrameHeader::parse(header).expect("Should parse");
        assert_eq!(parsed.version, MpegVersion::Mpeg25);
        assert_eq!(parsed.sample_rate, 11025); // Quarter of MPEG1's 44100
    }

    // ==========================================================================
    // FRAME SIZE CALCULATION TESTS
    // ==========================================================================
    //
    // Frame size determines how many bytes until the next frame header.
    // The formula for Layer 2/3 is:
    //
    //   frame_size = 144 * bitrate / sample_rate + padding
    //
    // For Layer 1:
    //   frame_size = (12 * bitrate / sample_rate + padding) * 4
    //
    // Example: 128kbps at 44100Hz:
    //   144 * 128000 / 44100 = 417.95... → 417 bytes (no padding)
    //   417 + 1 = 418 bytes (with padding)
    //
    // The padding bit alternates to keep the average frame size correct.
    // ==========================================================================

    #[test]
    fn test_frame_size_calculation() {
        // MPEG1 Layer3 128kbps 44100Hz no padding
        // frame_size = floor(144 * 128000 / 44100) = floor(417.95) = 417 bytes
        let header = valid_mp3_header();
        let parsed = FrameHeader::parse(header).expect("Should parse");
        assert_eq!(parsed.frame_size, 417);
    }

    #[test]
    fn test_frame_size_with_padding() {
        // With padding, we add 1 byte to compensate for rounding
        // frame_size = floor(144 * 128000 / 44100) + 1 = 418 bytes
        let header = [0xFF, 0xFB, 0x92, 0x00];
        let parsed = FrameHeader::parse(header).expect("Should parse");
        assert_eq!(parsed.frame_size, 418);
    }

    // ==========================================================================
    // VBR DETECTION AND FRAME STATISTICS
    // ==========================================================================
    //
    // VBR (Variable Bit Rate) encoding uses different bitrates for different
    // parts of a song - more bits for complex sections, fewer for simple ones.
    // This gives better quality at the same average file size.
    //
    // We detect VBR by looking for multiple different bitrate values
    // across frames. CBR (Constant Bit Rate) will have all identical.
    //
    // Frame Size CV (Coefficient of Variation) measures how much frame
    // sizes vary. High CV = high variability = VBR or quality issues.
    // ==========================================================================

    #[test]
    fn test_frame_stats_cv_empty() {
        // Empty stats should have 0 CV (no variation)
        let stats = FrameStats::default();
        assert_eq!(stats.frame_size_cv(), 0.0);
    }

    #[test]
    fn test_frame_stats_cv_uniform() {
        // Uniform frame sizes = CV of 0 (CBR file)
        // All frames the same size means constant bitrate
        let stats = FrameStats {
            frame_sizes: vec![417, 417, 417, 417],
            ..Default::default()
        };
        assert_eq!(stats.frame_size_cv(), 0.0);
    }

    #[test]
    fn test_frame_stats_cv_variable() {
        // Variable frame sizes = positive CV (VBR file)
        // CV = (std_dev / mean) * 100%
        let stats = FrameStats {
            frame_sizes: vec![400, 500, 400, 500],
            ..Default::default()
        };
        let cv = stats.frame_size_cv();
        assert!(cv > 0.0, "VBR should have positive CV");
        assert!(cv < 20.0, "CV should be reasonable for this data");
    }

    // ==========================================================================
    // FRAME SCANNING TESTS
    // ==========================================================================
    //
    // scan_frames() walks through an MP3 file, finding each frame header
    // and collecting statistics. This is essential for:
    // - Calculating actual average bitrate
    // - Detecting VBR vs CBR
    // - Finding inconsistencies that indicate transcoding
    // ==========================================================================

    #[test]
    fn test_scan_frames_empty() {
        // Empty file should return empty stats without error
        let data: Vec<u8> = vec![];
        let mut cursor = Cursor::new(data);
        let stats = scan_frames(&mut cursor, 100).expect("Should not error on empty");
        assert_eq!(stats.frame_count, 0);
    }

    #[test]
    fn test_scan_frames_single_frame() {
        // Create a minimal valid MP3 frame (header + padding to frame size)
        let mut data = vec![0xFF, 0xFB, 0x90, 0x00]; // 128kbps header
        data.extend(vec![0u8; 413]); // Fill to frame size (417 total)

        let mut cursor = Cursor::new(data);
        let stats = scan_frames(&mut cursor, 100).expect("Should parse");

        assert_eq!(stats.frame_count, 1);
        assert_eq!(stats.avg_bitrate, 128);
        assert!(!stats.is_vbr, "Single frame should not be VBR");
    }

    #[test]
    fn test_scan_frames_with_id3v2() {
        // ID3v2 tags appear BEFORE the audio frames
        // We must skip them to find the first frame
        //
        // ID3v2 header format:
        // - "ID3" (3 bytes) - magic
        // - Version (2 bytes) - e.g., 0x04 0x00 = v2.4
        // - Flags (1 byte)
        // - Size (4 bytes) - syncsafe integer (7 bits per byte)
        let mut data = vec![
            b'I', b'D', b'3',  // ID3 magic
            0x04, 0x00,        // Version 2.4.0
            0x00,              // Flags
            0x00, 0x00, 0x00, 0x00, // Size = 0 bytes (no actual tag data)
        ];
        // Add a valid MP3 frame after the ID3 tag
        data.extend([0xFF, 0xFB, 0x90, 0x00]);
        data.extend(vec![0u8; 413]);

        let mut cursor = Cursor::new(data);
        let stats = scan_frames(&mut cursor, 100).expect("Should parse");
        assert_eq!(stats.frame_count, 1, "Should find frame after ID3 tag");
    }

    // ==========================================================================
    // SYNC FINDING TESTS
    // ==========================================================================
    //
    // find_sync() locates the first valid frame header in a file.
    // This is needed because:
    // - Files may have garbage/metadata before audio
    // - ID3v2 tags precede the audio
    // - Damaged files may have corruption before first good frame
    // ==========================================================================

    #[test]
    fn test_find_sync_at_start() {
        // Clean file with frame header at byte 0
        let mut data = vec![0xFF, 0xFB, 0x90, 0x00];
        data.extend(vec![0u8; 413]);

        let mut cursor = Cursor::new(data);
        let pos = find_sync(&mut cursor).expect("Should not error");
        assert_eq!(pos, Some(0), "Sync should be at position 0");
    }

    #[test]
    fn test_find_sync_with_garbage() {
        // File with 5 bytes of garbage before the first frame
        // This simulates a file with some corruption or unknown header
        let mut data = vec![0x00, 0x01, 0x02, 0x03, 0x04]; // Garbage
        data.extend([0xFF, 0xFB, 0x90, 0x00]); // Valid frame header
        data.extend(vec![0u8; 413]); // Frame data

        let mut cursor = Cursor::new(data);
        let pos = find_sync(&mut cursor).expect("Should not error");
        assert_eq!(pos, Some(5), "Sync should be at position 5");
    }

    #[test]
    fn test_find_sync_not_found() {
        // File with no valid sync word - just zeros
        // This represents a corrupted or non-MP3 file
        let data = vec![0u8; 100];
        let mut cursor = Cursor::new(data);
        let pos = find_sync(&mut cursor).expect("Should not error");
        assert_eq!(pos, None, "Should not find sync in all-zero data");
    }

    #[test]
    fn test_vbr_detection() {
        // VBR files have different bitrates for different frames
        // LAME V0/V2 etc. vary bitrate based on audio complexity
        //
        // This test creates two frames with different bitrates
        let mut data = Vec::new();

        // Frame 1: 128kbps (bitrate index 9)
        // frame_size = 144 * 128000 / 44100 = 417 bytes
        data.extend([0xFF, 0xFB, 0x90, 0x00]);
        data.extend(vec![0u8; 413]); // 417 - 4 = 413 bytes padding

        // Frame 2: 160kbps (bitrate index 10)
        // frame_size = 144 * 160000 / 44100 = 522 bytes
        data.extend([0xFF, 0xFB, 0xA0, 0x00]);
        data.extend(vec![0u8; 518]); // 522 - 4 = 518 bytes padding

        let mut cursor = Cursor::new(data);
        let stats = scan_frames(&mut cursor, 100).expect("Should parse");

        assert_eq!(stats.frame_count, 2);
        assert!(stats.is_vbr, "Multiple bitrates should indicate VBR");
        assert_eq!(stats.min_bitrate, 128);
        assert_eq!(stats.max_bitrate, 160);
        // Average: (128 + 160) / 2 = 144 kbps
        assert_eq!(stats.avg_bitrate, 144);
    }

    // ==========================================================================
    // BITRATE INDEX REFERENCE TABLE
    // ==========================================================================
    //
    // This test documents the complete bitrate lookup table for MPEG1 Layer3.
    // Useful for understanding the byte values you'll see in hex editors.
    // ==========================================================================

    #[test]
    fn test_bitrate_index_table_mpeg1_layer3() {
        // Test all valid bitrate indices for MPEG1 Layer3
        // This documents the mapping from header bytes to bitrates
        let test_cases = [
            (0x10, 32),   // Index 1
            (0x20, 40),   // Index 2
            (0x30, 48),   // Index 3
            (0x40, 56),   // Index 4
            (0x50, 64),   // Index 5
            (0x60, 80),   // Index 6
            (0x70, 96),   // Index 7
            (0x80, 112),  // Index 8
            (0x90, 128),  // Index 9  - "Standard quality"
            (0xA0, 160),  // Index 10
            (0xB0, 192),  // Index 11 - "Good quality"
            (0xC0, 224),  // Index 12
            (0xD0, 256),  // Index 13 - "High quality"
            (0xE0, 320),  // Index 14 - "Maximum quality"
        ];

        for (byte2, expected_bitrate) in test_cases {
            let header = [0xFF, 0xFB, byte2, 0x00];
            let parsed = FrameHeader::parse(header)
                .expect(&format!("Should parse header with byte 0x{:02X}", byte2));
            assert_eq!(
                parsed.bitrate, expected_bitrate,
                "Byte 0x{:02X} should give {}kbps",
                byte2, expected_bitrate
            );
        }
    }
}
