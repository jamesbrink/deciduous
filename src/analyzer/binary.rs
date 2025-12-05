//! Binary/structural analysis of MP3 files
//!
//! Analyzes the binary structure of MP3 files to detect transcoding:
//! - LAME header lowpass mismatch (smoking gun)
//! - Multiple encoder signatures
//! - Frame size irregularities
//! - ID3 tag inconsistencies

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
