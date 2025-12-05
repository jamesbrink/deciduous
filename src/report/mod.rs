//! Report generation for analysis results
//!
//! This module provides output formatters for analysis results in multiple formats:
//!
//! - **HTML**: Interactive report with D3.js visualizations (spectral waterfall, charts)
//! - **JSON**: Machine-readable format for programmatic consumption
//! - **CSV**: Spreadsheet-compatible format for bulk analysis
//!
//! # Usage
//!
//! ```ignore
//! use losselot::report;
//!
//! // Automatically picks format based on extension
//! report::generate("report.html", &results)?;  // HTML
//! report::generate("report.json", &results)?;  // JSON
//! report::generate("report.csv", &results)?;   // CSV
//! ```

pub mod csv;
pub mod html;
pub mod json;

use crate::analyzer::AnalysisResult;
use std::io;
use std::path::Path;

/// Generate a report in the appropriate format based on file extension
pub fn generate<P: AsRef<Path>>(path: P, results: &[AnalysisResult]) -> io::Result<()> {
    let path = path.as_ref();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let mut file = std::fs::File::create(path)?;

    match ext.as_str() {
        "html" | "htm" => html::write(&mut file, results),
        "json" => json::write(&mut file, results),
        _ => csv::write(&mut file, results),
    }
}

/// Summary statistics for a batch of results
#[derive(Debug, Clone, Default)]
pub struct Summary {
    pub total: usize,
    pub ok: usize,
    pub suspect: usize,
    pub transcode: usize,
    pub error: usize,
}

impl Summary {
    pub fn from_results(results: &[AnalysisResult]) -> Self {
        let mut summary = Self::default();
        summary.total = results.len();

        for r in results {
            match r.verdict {
                crate::analyzer::Verdict::Ok => summary.ok += 1,
                crate::analyzer::Verdict::Suspect => summary.suspect += 1,
                crate::analyzer::Verdict::Transcode => summary.transcode += 1,
                crate::analyzer::Verdict::Error => summary.error += 1,
            }
        }

        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::Verdict;

    // ==========================================================================
    // SUMMARY STATISTICS TESTS
    // ==========================================================================
    //
    // The Summary struct aggregates verdict counts for a batch of files.
    // This is displayed at the top of reports to give an overview.
    // ==========================================================================

    fn create_test_result(verdict: Verdict) -> AnalysisResult {
        AnalysisResult {
            file_path: "/test/file.mp3".to_string(),
            file_name: "file.mp3".to_string(),
            bitrate: 320,
            sample_rate: 44100,
            duration_secs: 180.0,
            verdict,
            combined_score: 0,
            spectral_score: 0,
            binary_score: 0,
            flags: vec![],
            encoder: "LAME".to_string(),
            lowpass: None,
            spectral_details: None,
            binary_details: None,
            error: None,
        }
    }

    #[test]
    fn test_summary_empty() {
        let results: Vec<AnalysisResult> = vec![];
        let summary = Summary::from_results(&results);

        assert_eq!(summary.total, 0);
        assert_eq!(summary.ok, 0);
        assert_eq!(summary.suspect, 0);
        assert_eq!(summary.transcode, 0);
        assert_eq!(summary.error, 0);
    }

    #[test]
    fn test_summary_all_ok() {
        let results = vec![
            create_test_result(Verdict::Ok),
            create_test_result(Verdict::Ok),
            create_test_result(Verdict::Ok),
        ];
        let summary = Summary::from_results(&results);

        assert_eq!(summary.total, 3);
        assert_eq!(summary.ok, 3);
        assert_eq!(summary.suspect, 0);
        assert_eq!(summary.transcode, 0);
    }

    #[test]
    fn test_summary_mixed() {
        let results = vec![
            create_test_result(Verdict::Ok),
            create_test_result(Verdict::Ok),
            create_test_result(Verdict::Suspect),
            create_test_result(Verdict::Transcode),
            create_test_result(Verdict::Error),
        ];
        let summary = Summary::from_results(&results);

        assert_eq!(summary.total, 5);
        assert_eq!(summary.ok, 2);
        assert_eq!(summary.suspect, 1);
        assert_eq!(summary.transcode, 1);
        assert_eq!(summary.error, 1);
    }

    #[test]
    fn test_summary_all_transcodes() {
        // Worst case: entire library is fake
        let results = vec![
            create_test_result(Verdict::Transcode),
            create_test_result(Verdict::Transcode),
        ];
        let summary = Summary::from_results(&results);

        assert_eq!(summary.total, 2);
        assert_eq!(summary.ok, 0);
        assert_eq!(summary.transcode, 2);
    }

    #[test]
    fn test_summary_default() {
        let summary = Summary::default();

        assert_eq!(summary.total, 0);
        assert_eq!(summary.ok, 0);
        assert_eq!(summary.suspect, 0);
        assert_eq!(summary.transcode, 0);
        assert_eq!(summary.error, 0);
    }
}
