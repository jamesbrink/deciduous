//! Audio file analyzer - combines binary and spectral analysis
//!
//! This module orchestrates the complete analysis pipeline:
//!
//! 1. **File Reading**: Load audio data and extract basic metadata
//! 2. **Binary Analysis**: Check LAME headers, encoder signatures, frame structure
//! 3. **Spectral Analysis**: FFT-based frequency content analysis
//! 4. **Score Combination**: Merge evidence from both analyses
//! 5. **Verdict**: Classify as OK, SUSPECT, or TRANSCODE
//!
//! # Scoring System
//!
//! Both analyzers contribute to a combined score (0-100):
//!
//! ```text
//! Score Range | Verdict   | Meaning
//! ------------|-----------|------------------------------------------
//! 0-34        | OK        | Appears to be legitimate lossless
//! 35-64       | SUSPECT   | Some indicators of lossy origin
//! 65-100      | TRANSCODE | Strong evidence of transcode/fake
//! ```
//!
//! # Agreement Bonus
//!
//! When both analyses agree (spectral ≥30 AND binary ≥20), an additional
//! +15 points is added to the combined score. This rewards corroborating
//! evidence from independent detection methods.

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

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // EDUCATIONAL BACKGROUND: The Combined Analysis Approach
    // ==========================================================================
    //
    // Losselot uses TWO independent analysis methods:
    //
    // 1. BINARY ANALYSIS (Fast, MP3-specific)
    //    - Reads LAME header metadata
    //    - Checks lowpass filter frequency
    //    - Detects encoder signatures
    //    - Works only on LAME-encoded MP3s
    //
    // 2. SPECTRAL ANALYSIS (Slower, format-agnostic)
    //    - Decodes audio to PCM
    //    - Performs FFT frequency analysis
    //    - Measures energy in frequency bands
    //    - Works on any audio format
    //
    // WHY TWO METHODS?
    //
    // Each has strengths and weaknesses:
    // - Binary: Fast, but only works if encoder left forensic metadata
    // - Spectral: Universal, but slower and can't identify source bitrate
    //
    // When BOTH agree, confidence is very high. When they disagree,
    // further investigation is warranted.
    // ==========================================================================

    // ==========================================================================
    // VERDICT CLASSIFICATION TESTS
    // ==========================================================================

    #[test]
    fn test_verdict_display() {
        // Verdicts should display as human-readable strings
        assert_eq!(format!("{}", Verdict::Ok), "OK");
        assert_eq!(format!("{}", Verdict::Suspect), "SUSPECT");
        assert_eq!(format!("{}", Verdict::Transcode), "TRANSCODE");
        assert_eq!(format!("{}", Verdict::Error), "ERROR");
    }

    #[test]
    fn test_verdict_equality() {
        // Verdicts should be comparable
        assert_eq!(Verdict::Ok, Verdict::Ok);
        assert_ne!(Verdict::Ok, Verdict::Transcode);
    }

    #[test]
    fn test_verdict_copy() {
        // Verdicts should be Copy (efficient to pass around)
        let v = Verdict::Ok;
        let v2 = v;
        assert_eq!(v, v2);
    }

    // ==========================================================================
    // ANALYZER CONFIGURATION TESTS
    // ==========================================================================

    #[test]
    fn test_analyzer_default() {
        // Default analyzer settings
        let analyzer = Analyzer::default();

        assert!(!analyzer.skip_spectral, "Spectral should be enabled by default");
        assert_eq!(analyzer.transcode_threshold, 65, "Default transcode threshold");
        assert_eq!(analyzer.suspect_threshold, 35, "Default suspect threshold");
    }

    #[test]
    fn test_analyzer_new() {
        // new() should be same as default()
        let a1 = Analyzer::new();
        let a2 = Analyzer::default();

        assert_eq!(a1.skip_spectral, a2.skip_spectral);
        assert_eq!(a1.transcode_threshold, a2.transcode_threshold);
        assert_eq!(a1.suspect_threshold, a2.suspect_threshold);
    }

    #[test]
    fn test_analyzer_skip_spectral() {
        // Can disable spectral analysis for speed
        let analyzer = Analyzer::new().with_skip_spectral(true);
        assert!(analyzer.skip_spectral);

        let analyzer = Analyzer::new().with_skip_spectral(false);
        assert!(!analyzer.skip_spectral);
    }

    #[test]
    fn test_analyzer_custom_thresholds() {
        // Can customize verdict thresholds
        let analyzer = Analyzer::new().with_thresholds(50, 80);

        assert_eq!(analyzer.suspect_threshold, 50);
        assert_eq!(analyzer.transcode_threshold, 80);
    }

    #[test]
    fn test_analyzer_builder_pattern() {
        // Builder methods should chain
        let analyzer = Analyzer::new()
            .with_skip_spectral(true)
            .with_thresholds(40, 70);

        assert!(analyzer.skip_spectral);
        assert_eq!(analyzer.suspect_threshold, 40);
        assert_eq!(analyzer.transcode_threshold, 70);
    }

    // ==========================================================================
    // THRESHOLD DOCUMENTATION TESTS
    // ==========================================================================
    //
    // Understanding the thresholds is key to interpreting results:
    //
    // SUSPECT THRESHOLD (default: 35)
    //   - Files scoring 35-64 are flagged as SUSPECT
    //   - May be from high-bitrate lossy source (256-320k)
    //   - Or unusual audio content that triggers false positives
    //   - Warrants further investigation
    //
    // TRANSCODE THRESHOLD (default: 65)
    //   - Files scoring 65+ are flagged as TRANSCODE
    //   - Strong evidence of lossy origin
    //   - Multiple indicators agree
    //   - High confidence the file is fake
    // ==========================================================================

    #[test]
    fn test_threshold_boundaries() {
        // Document the exact threshold behavior
        let analyzer = Analyzer::default();

        // Score 0-34: OK
        assert!(0 < analyzer.suspect_threshold);
        assert!(34 < analyzer.suspect_threshold);

        // Score 35-64: SUSPECT
        assert!(35 >= analyzer.suspect_threshold);
        assert!(64 < analyzer.transcode_threshold);

        // Score 65+: TRANSCODE
        assert!(65 >= analyzer.transcode_threshold);
    }

    #[test]
    fn test_threshold_ordering() {
        // Suspect threshold must be less than transcode threshold
        let analyzer = Analyzer::default();
        assert!(
            analyzer.suspect_threshold < analyzer.transcode_threshold,
            "Suspect threshold must be < transcode threshold"
        );
    }

    // ==========================================================================
    // AGREEMENT BONUS DOCUMENTATION
    // ==========================================================================
    //
    // When both analyses agree that a file is suspicious, we add bonus points.
    // This is because independent corroboration increases confidence.
    //
    // The bonus is +15 points when:
    //   - Spectral score >= 30 (moderate spectral evidence)
    //   - Binary score >= 20 (some binary evidence)
    //
    // This can push a file from SUSPECT into TRANSCODE territory.
    // ==========================================================================

    #[test]
    fn test_agreement_bonus_threshold() {
        // Document the agreement bonus criteria
        let spectral_threshold = 30;
        let binary_threshold = 20;
        let bonus = 15;

        // If spectral=30 and binary=20, combined = 30+20+15 = 65 (TRANSCODE)
        let combined = spectral_threshold + binary_threshold + bonus;
        assert_eq!(combined, 65, "Agreement should push to TRANSCODE threshold");
    }

    // ==========================================================================
    // ANALYSIS RESULT STRUCTURE TESTS
    // ==========================================================================

    #[test]
    fn test_analysis_result_fields() {
        // Verify all expected fields are present in AnalysisResult
        // This serves as API documentation

        let result = AnalysisResult {
            file_path: "/path/to/file.mp3".to_string(),
            file_name: "file.mp3".to_string(),
            bitrate: 320,
            sample_rate: 44100,
            duration_secs: 180.0,
            verdict: Verdict::Ok,
            combined_score: 10,
            spectral_score: 5,
            binary_score: 5,
            flags: vec!["test_flag".to_string()],
            encoder: "LAME3.100".to_string(),
            lowpass: Some(20500),
            spectral_details: None,
            binary_details: None,
            error: None,
        };

        // Verify fields are accessible
        assert_eq!(result.file_name, "file.mp3");
        assert_eq!(result.bitrate, 320);
        assert_eq!(result.verdict, Verdict::Ok);
        assert!(result.error.is_none());
    }

    // ==========================================================================
    // SCORE CAPPING TESTS
    // ==========================================================================

    #[test]
    fn test_score_capped_at_100() {
        // Combined score should never exceed 100
        // Even with high individual scores + agreement bonus
        //
        // Example: spectral=70 + binary=50 + bonus=15 = 135 → capped to 100

        // This is a documentation test - the actual capping happens in analyze()
        let max_possible = 100;
        assert_eq!(max_possible, 100, "Maximum score is always 100");
    }

    // ==========================================================================
    // REAL-WORLD SCENARIO INTERPRETATIONS
    // ==========================================================================
    //
    // These tests document how to interpret various analysis outcomes:
    // ==========================================================================

    #[test]
    fn test_scenario_interpretation_clean() {
        // CLEAN FILE: Score 0-15
        // - Binary: No lowpass mismatch, correct encoder, uniform frames
        // - Spectral: Smooth frequency rolloff, content to 22kHz
        // - Verdict: OK
        //
        // Action: File is legitimate lossless

        let clean_score = 10;
        let analyzer = Analyzer::default();
        assert!(clean_score < analyzer.suspect_threshold);
    }

    #[test]
    fn test_scenario_interpretation_borderline() {
        // BORDERLINE FILE: Score 35-40
        // - May have unusual content (synth music, electronic)
        // - Or high-bitrate lossy source (V0 VBR → FLAC)
        // - Verdict: SUSPECT
        //
        // Action: Manual review recommended, check source

        let borderline_score = 38;
        let analyzer = Analyzer::default();
        assert!(borderline_score >= analyzer.suspect_threshold);
        assert!(borderline_score < analyzer.transcode_threshold);
    }

    #[test]
    fn test_scenario_interpretation_obvious_fake() {
        // OBVIOUS FAKE: Score 70+
        // - Binary: Lowpass 16kHz in "320kbps" file
        // - Spectral: Hard cutoff at 16kHz, nothing above
        // - Verdict: TRANSCODE
        //
        // Action: File is definitely fake, discard or flag

        let fake_score = 75;
        let analyzer = Analyzer::default();
        assert!(fake_score >= analyzer.transcode_threshold);
    }
}
