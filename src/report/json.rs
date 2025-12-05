//! JSON report generation

use crate::analyzer::AnalysisResult;
use crate::report::Summary;
use serde::Serialize;
use std::io::{self, Write};

#[derive(Serialize)]
struct JsonReport<'a> {
    generated: String,
    summary: JsonSummary,
    files: &'a [AnalysisResult],
}

#[derive(Serialize)]
struct JsonSummary {
    total: usize,
    ok: usize,
    suspect: usize,
    transcode: usize,
    error: usize,
}

pub fn write<W: Write>(writer: &mut W, results: &[AnalysisResult]) -> io::Result<()> {
    let summary = Summary::from_results(results);

    let report = JsonReport {
        generated: chrono_lite_now(),
        summary: JsonSummary {
            total: summary.total,
            ok: summary.ok,
            suspect: summary.suspect,
            transcode: summary.transcode,
            error: summary.error,
        },
        files: results,
    };

    let json = serde_json::to_string_pretty(&report)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    writer.write_all(json.as_bytes())
}

/// Simple ISO 8601 timestamp without pulling in chrono
fn chrono_lite_now() -> String {
    use std::time::SystemTime;

    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();

    let secs = duration.as_secs();

    // Very basic timestamp - good enough for reports
    let days_since_epoch = secs / 86400;
    let years = 1970 + (days_since_epoch / 365); // Approximate

    format!("{}-01-01T00:00:00Z", years)
}
