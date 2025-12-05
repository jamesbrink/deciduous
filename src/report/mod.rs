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
