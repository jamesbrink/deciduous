//! Binary/structural analysis of MP3 files
//!
//! Analyzes the binary structure of MP3 files to detect transcoding:
//! - LAME header lowpass mismatch (smoking gun)
//! - Multiple encoder signatures
//! - Frame size irregularities
//! - ID3 tag inconsistencies
//!
//! # How Binary Analysis Works
//!
//! Unlike spectral analysis (which looks at the actual audio content), binary
//! analysis examines the metadata and structure embedded in the file itself.
//!
//! ## Key Detection Methods:
//!
//! 1. **Lowpass Mismatch**: The LAME encoder records what lowpass filter it used.
//!    If a "320kbps" file has lowpass=16kHz, it was transcoded from 128kbps.
//!
//! 2. **Multiple Encoder Signatures**: If a file has both "LAME" and "Lavf"
//!    (FFmpeg) signatures, it was likely re-encoded at some point.
//!
//! 3. **Frame Size Irregularities**: CBR files should have uniform frame sizes.
//!    High variance in a "CBR 320kbps" file suggests something is wrong.
//!
//! Binary analysis is fast (just reads headers) but only works on MP3 files
//! encoded with LAME. Other formats (AAC, Opus, FLAC) need spectral analysis.

use crate::mp3::{frame, lame};
use serde::Serialize;
use std::io::{Read, Seek};

#[derive(Debug, Clone, Default, Serialize)]
pub struct BinaryDetails {
    pub lowpass: Option<u32>,
    pub expected_lowpass: Option<u32>,
    pub encoder_version: Option<String>,
    pub encoder_count: usize,
    pub frame_size_cv: f64,
    pub is_vbr: bool,
    pub total_frames: Option<u32>,
}

pub struct BinaryResult {
    pub score: u32,
    pub flags: Vec<String>,
    pub encoder: String,
    pub lowpass: Option<u32>,
    pub details: BinaryDetails,
}

impl Default for BinaryResult {
    fn default() -> Self {
        Self {
            score: 0,
            flags: vec![],
            encoder: "unknown".to_string(),
            lowpass: None,
            details: BinaryDetails::default(),
        }
    }
}

/// Perform binary analysis on MP3 data
pub fn analyze<R: Read + Seek>(data: &[u8], reader: &mut R, bitrate: u32) -> BinaryResult {
    let mut result = BinaryResult::default();

    // Extract LAME header
    if let Some(lame_header) = lame::LameHeader::extract(data) {
        result.encoder = if lame_header.encoder.is_empty() {
            "LAME".to_string()
        } else {
            lame_header.encoder.clone()
        };

        result.lowpass = lame_header.lowpass;
        result.details.lowpass = lame_header.lowpass;
        result.details.encoder_version = Some(lame_header.encoder);
        result.details.is_vbr = lame_header.is_vbr_header;
        result.details.total_frames = lame_header.total_frames;

        // KEY CHECK: Lowpass mismatch
        if let Some(actual_lowpass) = lame_header.lowpass {
            let (is_suspicious, expected, reason) =
                lame::check_lowpass_mismatch(bitrate, actual_lowpass);

            result.details.expected_lowpass = Some(expected);

            if is_suspicious {
                result.score += 35;
                result.flags.push(format!("lowpass_mismatch({}Hz)", actual_lowpass));

                if let Some(r) = reason {
                    // Log but don't add to flags (too verbose)
                    let _ = r;
                }
            }
        }
    } else {
        // Check for other encoders
        reader.seek(std::io::SeekFrom::Start(0)).ok();
        if let Ok(sigs) = lame::scan_encoder_signatures(reader) {
            if let Some(lame_ver) = sigs.lame {
                result.encoder = lame_ver;
            } else if sigs.fraunhofer {
                result.encoder = "Fraunhofer".to_string();
            } else if sigs.itunes {
                result.encoder = "iTunes".to_string();
            } else if sigs.ffmpeg {
                result.encoder = "FFmpeg".to_string();
            }
        }
    }

    // Check for multiple encoder signatures
    reader.seek(std::io::SeekFrom::Start(0)).ok();
    if let Ok(count) = lame::count_encoder_signatures(reader) {
        result.details.encoder_count = count;
        if count > 1 {
            result.score += 20;
            result.flags.push("multi_encoder_sigs".to_string());
        }
    }

    // Frame size analysis
    reader.seek(std::io::SeekFrom::Start(0)).ok();
    if let Ok(frame_stats) = frame::scan_frames(reader, 200) {
        let cv = frame_stats.frame_size_cv();
        result.details.frame_size_cv = cv;

        // High variance in high-bitrate CBR is suspicious
        if bitrate >= 256 && cv > 15.0 {
            result.score += 10;
            result.flags.push("irregular_frames".to_string());
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // ==========================================================================
    // EDUCATIONAL BACKGROUND: Binary Analysis for Transcode Detection
    // ==========================================================================
    //
    // Binary analysis examines the FILE STRUCTURE, not the audio content.
    // This is complementary to spectral analysis:
    //
    //   - Binary Analysis: Reads metadata/headers. Fast, but only works if
    //     the encoder left behind forensic evidence (e.g., LAME headers).
    //
    //   - Spectral Analysis: Looks at actual frequency content. Works on any
    //     format, but slower and requires decoding the audio.
    //
    // When both methods agree, we have high confidence in the verdict.
    //
    // SCORING SYSTEM:
    // The binary analyzer adds points for suspicious indicators:
    //   +35 points: Lowpass mismatch (the smoking gun)
    //   +20 points: Multiple encoder signatures (re-encoding evidence)
    //   +10 points: Irregular frame sizes in supposed CBR file
    //
    // A score of 0 means no binary evidence of transcoding.
    // Higher scores indicate higher likelihood of fake/transcoded content.
    // ==========================================================================

    /// Helper: Create a minimal MP3-like structure with LAME header
    fn create_test_mp3_data(
        encoder_version: &str,
        lowpass_hz: u32,
        is_vbr: bool,
    ) -> Vec<u8> {
        let mut data = Vec::new();

        // MP3 frame sync
        data.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);

        // Padding before Xing/Info header
        data.extend_from_slice(&[0x00; 32]);

        // Xing or Info marker
        if is_vbr {
            data.extend_from_slice(b"Xing");
        } else {
            data.extend_from_slice(b"Info");
        }

        // Xing flags (all fields present)
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x0F]);

        // Frames count (4 bytes)
        data.extend_from_slice(&[0x00, 0x00, 0x10, 0x00]);

        // Bytes count (4 bytes)
        data.extend_from_slice(&[0x00, 0x10, 0x00, 0x00]);

        // TOC (100 bytes)
        data.extend_from_slice(&[0x00; 100]);

        // Quality (4 bytes)
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x64]);

        // LAME version string (9 bytes)
        let version_bytes = encoder_version.as_bytes();
        let mut lame_tag = [0u8; 9];
        let copy_len = version_bytes.len().min(9);
        lame_tag[..copy_len].copy_from_slice(&version_bytes[..copy_len]);
        data.extend_from_slice(&lame_tag);

        // VBR method + quality byte
        data.push(0x24);

        // Lowpass frequency / 100
        let lowpass_byte = (lowpass_hz / 100) as u8;
        data.push(lowpass_byte);

        // Padding to make it look realistic
        data.extend_from_slice(&[0x00; 200]);

        data
    }

    // ==========================================================================
    // LOWPASS MISMATCH DETECTION TESTS
    // ==========================================================================
    //
    // The lowpass mismatch is the MOST RELIABLE indicator of transcoding.
    //
    // When LAME encodes from a lossless source at 320kbps, it uses:
    //   lowpass ≈ 20500 Hz (keeping nearly all audible frequencies)
    //
    // When someone takes a 128kbps MP3 and re-encodes it as "320kbps":
    //   lowpass = 16000 Hz (because the original only had frequencies up to 16kHz)
    //
    // The LAME encoder HONESTLY RECORDS this, creating a forensic trail!
    // ==========================================================================

    #[test]
    fn test_lowpass_mismatch_flags_transcode() {
        // SCENARIO: Fake 320kbps file transcoded from 128kbps source
        // EVIDENCE: lowpass=16000Hz instead of expected ~20500Hz

        let data = create_test_mp3_data("LAME3.100", 16000, false);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 320);

        // Should have significant score due to lowpass mismatch
        assert!(
            result.score >= 35,
            "Lowpass mismatch should add 35+ points, got {}",
            result.score
        );

        // Should flag the mismatch
        assert!(
            result.flags.iter().any(|f| f.contains("lowpass_mismatch")),
            "Should flag lowpass mismatch: {:?}",
            result.flags
        );

        // Should record the lowpass value
        assert_eq!(result.lowpass, Some(16000));
    }

    #[test]
    fn test_legitimate_320_not_flagged() {
        // SCENARIO: Legitimate 320kbps encoding from lossless source
        // EVIDENCE: lowpass=20500Hz (appropriate for 320kbps)

        let data = create_test_mp3_data("LAME3.100", 20500, false);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 320);

        // Should have low/no score
        assert!(
            result.score < 35,
            "Legitimate 320kbps should not be flagged, got score {}",
            result.score
        );

        // Should NOT have lowpass_mismatch flag
        assert!(
            !result.flags.iter().any(|f| f.contains("lowpass_mismatch")),
            "Should not flag legitimate file: {:?}",
            result.flags
        );
    }

    #[test]
    fn test_encoder_version_extraction() {
        // The encoder version string tells us what software created the file
        // Common versions: "LAME3.99r", "LAME3.100", "LAME3.99.5"

        let data = create_test_mp3_data("LAME3.100", 20000, false);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 256);

        assert_eq!(result.encoder, "LAME3.100");
        assert_eq!(result.details.encoder_version, Some("LAME3.100".to_string()));
    }

    // ==========================================================================
    // VBR VS CBR DETECTION TESTS
    // ==========================================================================
    //
    // VBR (Variable Bit Rate): Uses "Xing" header marker
    //   - More efficient, better quality at same average size
    //   - Common for LAME V0, V2 settings
    //
    // CBR (Constant Bit Rate): Uses "Info" header marker
    //   - Every frame same size
    //   - Common for 320kbps "max quality" encodes
    //
    // Detection matters because VBR files naturally have variable frame sizes,
    // while CBR files with variable sizes are suspicious.
    // ==========================================================================

    #[test]
    fn test_vbr_detection() {
        let data = create_test_mp3_data("LAME3.99r", 19500, true);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 245);

        assert!(result.details.is_vbr, "Should detect VBR file");
    }

    #[test]
    fn test_cbr_detection() {
        let data = create_test_mp3_data("LAME3.100", 20500, false);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 320);

        assert!(!result.details.is_vbr, "Should detect CBR file");
    }

    // ==========================================================================
    // BINARY DETAILS STRUCTURE TESTS
    // ==========================================================================

    #[test]
    fn test_binary_details_populated() {
        // Verify all relevant details are captured for reporting

        let data = create_test_mp3_data("LAME3.100", 18500, false);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 192);

        // Lowpass should be recorded
        assert_eq!(result.details.lowpass, Some(18500));

        // Expected lowpass should be calculated
        assert!(result.details.expected_lowpass.is_some());

        // Encoder version should be captured
        assert_eq!(result.details.encoder_version, Some("LAME3.100".to_string()));
    }

    #[test]
    fn test_no_lame_header_fallback() {
        // Files without LAME headers should still get basic analysis
        // The result should have default/unknown values

        let data = vec![0xFF, 0xFB, 0x90, 0x00, 0x00, 0x00]; // Just MP3 sync
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 128);

        // Should have zero score (no evidence)
        assert_eq!(result.score, 0);

        // No lowpass data available
        assert!(result.lowpass.is_none());
    }

    // ==========================================================================
    // SCORING BREAKDOWN TESTS
    // ==========================================================================
    //
    // The scoring system is designed to give clear verdicts:
    //   0-34:  CLEAN (no evidence of transcoding)
    //   35-64: SUSPECT (some indicators, needs investigation)
    //   65+:   TRANSCODE (strong evidence of fake file)
    //
    // Each indicator contributes:
    //   - Lowpass mismatch: +35 (strong evidence)
    //   - Multiple encoders: +20 (moderate evidence)
    //   - Frame irregularities: +10 (weak evidence)
    // ==========================================================================

    #[test]
    fn test_score_lowpass_only() {
        // Just lowpass mismatch = 35 points (SUSPECT range)

        let data = create_test_mp3_data("LAME3.100", 16000, false);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 320);

        assert!(
            result.score >= 35 && result.score < 65,
            "Lowpass mismatch alone should be SUSPECT (35-64), got {}",
            result.score
        );
    }

    #[test]
    fn test_default_result() {
        // Default result should be clean (no evidence)

        let result = BinaryResult::default();

        assert_eq!(result.score, 0);
        assert!(result.flags.is_empty());
        assert_eq!(result.encoder, "unknown");
        assert!(result.lowpass.is_none());
    }

    // ==========================================================================
    // REAL-WORLD SCENARIO TESTS
    // ==========================================================================

    #[test]
    fn test_scenario_youtube_to_320_transcode() {
        // SCENARIO: Someone rips audio from YouTube (typically ~128kbps AAC)
        // and re-encodes as "320kbps MP3" for uploading elsewhere.
        //
        // EVIDENCE: YouTube audio has ~17kHz cutoff, so lowpass=17000Hz

        let data = create_test_mp3_data("LAME3.100", 17000, false);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 320);

        assert!(
            result.score >= 35,
            "YouTube transcode should be flagged, got score {}",
            result.score
        );
    }

    #[test]
    fn test_scenario_legitimate_v0_vbr() {
        // SCENARIO: Legitimate V0 VBR encoding from CD rip
        // V0 averages ~245kbps with lowpass ~19.5-20.5kHz

        let data = create_test_mp3_data("LAME3.99r", 20000, true);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 245);

        assert!(
            result.score < 35,
            "Legitimate V0 should not be flagged, got score {}",
            result.score
        );
    }

    #[test]
    fn test_scenario_legitimate_128_cbr() {
        // SCENARIO: Legitimate 128kbps encoding from lossless source
        // 128kbps has lowpass ~16kHz, which is EXPECTED for this bitrate

        let data = create_test_mp3_data("LAME3.100", 16000, false);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 128);

        // 16kHz is expected for 128kbps - should NOT be flagged
        assert!(
            !result.flags.iter().any(|f| f.contains("lowpass_mismatch")),
            "128kbps with 16kHz lowpass is normal, should not flag"
        );
    }
}
