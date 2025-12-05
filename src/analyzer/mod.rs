pub mod binary;
pub mod spectral;

use crate::mp3;
use serde::Serialize;
use std::path::Path;

/// Combined analysis result for a single file
#[derive(Debug, Clone, Serialize)]
pub struct AnalysisResult {
    pub file_path: String,
    pub file_name: String,
    pub bitrate: u32,
    pub sample_rate: u32,
    pub duration_secs: f64,
    pub verdict: Verdict,
    pub combined_score: u32,
    pub spectral_score: u32,
    pub binary_score: u32,
    pub flags: Vec<String>,
    pub encoder: String,
    pub lowpass: Option<u32>,
    pub spectral_details: Option<spectral::SpectralDetails>,
    pub binary_details: Option<binary::BinaryDetails>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Verdict {
    Ok,
    Suspect,
    Transcode,
    Error,
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Verdict::Ok => write!(f, "OK"),
            Verdict::Suspect => write!(f, "SUSPECT"),
            Verdict::Transcode => write!(f, "TRANSCODE"),
            Verdict::Error => write!(f, "ERROR"),
        }
    }
}

/// Main analyzer that combines binary and spectral analysis
pub struct Analyzer {
    /// Skip spectral analysis (faster but less accurate)
    pub skip_spectral: bool,
    /// Threshold for transcode verdict (default: 65)
    pub transcode_threshold: u32,
    /// Threshold for suspect verdict (default: 35)
    pub suspect_threshold: u32,
}

impl Default for Analyzer {
    fn default() -> Self {
        Self {
            skip_spectral: false,
            transcode_threshold: 65,
            suspect_threshold: 35,
        }
    }
}

impl Analyzer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_skip_spectral(mut self, skip: bool) -> Self {
        self.skip_spectral = skip;
        self
    }

    pub fn with_thresholds(mut self, suspect: u32, transcode: u32) -> Self {
        self.suspect_threshold = suspect;
        self.transcode_threshold = transcode;
        self
    }

    /// Analyze a single MP3 file
    pub fn analyze<P: AsRef<Path>>(&self, path: P) -> AnalysisResult {
        let path = path.as_ref();
        let file_path = path.display().to_string();
        let file_name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.clone());

        // Read file
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                return AnalysisResult {
                    file_path,
                    file_name,
                    bitrate: 0,
                    sample_rate: 0,
                    duration_secs: 0.0,
                    verdict: Verdict::Error,
                    combined_score: 0,
                    spectral_score: 0,
                    binary_score: 0,
                    flags: vec![],
                    encoder: String::new(),
                    lowpass: None,
                    spectral_details: None,
                    binary_details: None,
                    error: Some(format!("Failed to read file: {}", e)),
                };
            }
        };

        // Get basic file info
        let mut cursor = std::io::Cursor::new(&data);
        let frame_stats = mp3::frame::scan_frames(&mut cursor, 200).unwrap_or_default();

        let bitrate = frame_stats.avg_bitrate;
        let sample_rate = if !frame_stats.frame_sizes.is_empty() {
            // Try to get from first frame
            cursor.set_position(0);
            if let Ok(Some(pos)) = mp3::frame::find_sync(&mut cursor) {
                cursor.set_position(pos);
                let mut header_buf = [0u8; 4];
                if cursor.read_exact(&mut header_buf).is_ok() {
                    if let Some(frame) = mp3::frame::FrameHeader::parse(header_buf) {
                        frame.sample_rate
                    } else {
                        44100
                    }
                } else {
                    44100
                }
            } else {
                44100
            }
        } else {
            44100
        };

        // Estimate duration
        let duration_secs = if bitrate > 0 {
            (data.len() as f64 * 8.0) / (bitrate as f64 * 1000.0)
        } else {
            0.0
        };

        // Binary analysis
        cursor.set_position(0);
        let binary_result = binary::analyze(&data, &mut cursor, bitrate);

        // Spectral analysis (if not skipped)
        let spectral_result = if self.skip_spectral {
            spectral::SpectralResult::default()
        } else {
            spectral::analyze(&data, sample_rate)
        };

        // Combine scores
        let mut combined_score = binary_result.score + spectral_result.score;

        // Bonus if both analyses agree
        if spectral_result.score >= 30 && binary_result.score >= 20 {
            combined_score += 15;
        }

        combined_score = combined_score.min(100);

        // Merge flags
        let mut flags = binary_result.flags.clone();
        flags.extend(spectral_result.flags.clone());

        // Determine verdict
        let verdict = if combined_score >= self.transcode_threshold {
            Verdict::Transcode
        } else if combined_score >= self.suspect_threshold {
            Verdict::Suspect
        } else {
            Verdict::Ok
        };

        AnalysisResult {
            file_path,
            file_name,
            bitrate,
            sample_rate,
            duration_secs,
            verdict,
            combined_score,
            spectral_score: spectral_result.score,
            binary_score: binary_result.score,
            flags,
            encoder: binary_result.encoder,
            lowpass: binary_result.lowpass,
            spectral_details: Some(spectral_result.details),
            binary_details: Some(binary_result.details),
            error: None,
        }
    }
}

use std::io::Read;
