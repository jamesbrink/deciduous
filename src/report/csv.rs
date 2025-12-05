//! CSV report generation

use crate::analyzer::AnalysisResult;
use std::io::{self, Write};

pub fn write<W: Write>(writer: &mut W, results: &[AnalysisResult]) -> io::Result<()> {
    // Header
    writeln!(
        writer,
        "verdict,filepath,bitrate_kbps,combined_score,spectral_score,binary_score,flags,encoder,lowpass"
    )?;

    // Rows
    for r in results {
        let flags = if r.flags.is_empty() {
            "-".to_string()
        } else {
            r.flags.join(",")
        };

        let lowpass = r
            .lowpass
            .map(|l| l.to_string())
            .unwrap_or_else(|| "n/a".to_string());

        writeln!(
            writer,
            "{},{},{},{},{},{},{},{},{}",
            r.verdict,
            escape_csv(&r.file_path),
            r.bitrate,
            r.combined_score,
            r.spectral_score,
            r.binary_score,
            flags,
            escape_csv(&r.encoder),
            lowpass
        )?;
    }

    Ok(())
}

fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
