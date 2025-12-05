//! HTML report generation

use crate::analyzer::{AnalysisResult, Verdict};
use crate::report::Summary;
use std::io::{self, Write};

pub fn write<W: Write>(writer: &mut W, results: &[AnalysisResult]) -> io::Result<()> {
    let summary = Summary::from_results(results);

    // Header
    writer.write_all(HTML_HEADER.as_bytes())?;

    // Stats
    write!(
        writer,
        r#"
            <div class="stat ok">
                <div class="stat-value">{}</div>
                <div class="stat-label">Clean</div>
            </div>
            <div class="stat suspect">
                <div class="stat-value">{}</div>
                <div class="stat-label">Suspect</div>
            </div>
            <div class="stat transcode">
                <div class="stat-value">{}</div>
                <div class="stat-label">Transcode</div>
            </div>
            <div class="stat">
                <div class="stat-value">{}</div>
                <div class="stat-label">Total Files</div>
            </div>
        </div>

        <table>
            <thead>
                <tr>
                    <th>Verdict</th>
                    <th>Score</th>
                    <th>Bitrate</th>
                    <th>Spectral</th>
                    <th>Binary</th>
                    <th>Encoder</th>
                    <th>Flags</th>
                    <th>File</th>
                </tr>
            </thead>
            <tbody>
"#,
        summary.ok, summary.suspect, summary.transcode, summary.total
    )?;

    // Sort by score descending
    let mut sorted_results: Vec<_> = results.iter().collect();
    sorted_results.sort_by(|a, b| b.combined_score.cmp(&a.combined_score));

    // Rows
    for r in sorted_results {
        let verdict_class = match r.verdict {
            Verdict::Ok => "ok",
            Verdict::Suspect => "suspect",
            Verdict::Transcode => "transcode",
            Verdict::Error => "error",
        };

        let score_class = if r.combined_score >= 65 {
            "high"
        } else if r.combined_score >= 35 {
            "medium"
        } else {
            "low"
        };

        let flags_html = if r.flags.is_empty() {
            r#"<span class="dim">—</span>"#.to_string()
        } else {
            r.flags
                .iter()
                .map(|f| format!(r#"<span class="flag">{}</span>"#, html_escape(f)))
                .collect::<Vec<_>>()
                .join("")
        };

        let lowpass_info = r
            .lowpass
            .map(|l| format!(" ({}Hz)", l))
            .unwrap_or_default();

        write!(
            writer,
            r#"
                <tr>
                    <td><span class="verdict {}">{}</span></td>
                    <td>
                        <div class="score-bar"><div class="score-fill {}" style="width: {}%"></div></div>
                        {}%
                    </td>
                    <td>{}k</td>
                    <td class="dim">{}%</td>
                    <td class="dim">{}%</td>
                    <td class="encoder">{}{}</td>
                    <td class="flags">{}</td>
                    <td class="filepath" title="{}">{}</td>
                </tr>
"#,
            verdict_class,
            r.verdict,
            score_class,
            r.combined_score,
            r.combined_score,
            r.bitrate,
            r.spectral_score,
            r.binary_score,
            html_escape(&r.encoder),
            lowpass_info,
            flags_html,
            html_escape(&r.file_path),
            html_escape(&r.file_name)
        )?;
    }

    // Footer
    writer.write_all(HTML_FOOTER.as_bytes())?;

    Ok(())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const HTML_HEADER: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>MP3 Transcode Analysis Report</title>
    <style>
        :root {
            --bg: #1a1a2e;
            --card: #16213e;
            --text: #eee;
            --dim: #888;
            --ok: #00d26a;
            --suspect: #f5a623;
            --transcode: #ff3860;
            --error: #666;
        }
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Segoe UI', sans-serif;
            background: var(--bg);
            color: var(--text);
            line-height: 1.6;
            padding: 2rem;
        }
        .container { max-width: 1400px; margin: 0 auto; }
        h1 { font-size: 1.8rem; margin-bottom: 0.5rem; }
        .subtitle { color: var(--dim); margin-bottom: 2rem; }
        .stats {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
            gap: 1rem;
            margin-bottom: 2rem;
        }
        .stat {
            background: var(--card);
            padding: 1.25rem;
            border-radius: 12px;
            text-align: center;
        }
        .stat-value { font-size: 2rem; font-weight: 700; }
        .stat-label { color: var(--dim); font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; }
        .stat.ok .stat-value { color: var(--ok); }
        .stat.suspect .stat-value { color: var(--suspect); }
        .stat.transcode .stat-value { color: var(--transcode); }

        table {
            width: 100%;
            border-collapse: collapse;
            background: var(--card);
            border-radius: 12px;
            overflow: hidden;
        }
        th, td { padding: 0.75rem 1rem; text-align: left; }
        th {
            background: rgba(255,255,255,0.05);
            font-weight: 600;
            font-size: 0.8rem;
            text-transform: uppercase;
            letter-spacing: 0.05em;
            color: var(--dim);
        }
        tr:not(:last-child) td { border-bottom: 1px solid rgba(255,255,255,0.05); }
        tr:hover td { background: rgba(255,255,255,0.02); }

        .verdict {
            display: inline-block;
            padding: 0.25rem 0.75rem;
            border-radius: 20px;
            font-size: 0.75rem;
            font-weight: 600;
            text-transform: uppercase;
        }
        .verdict.ok { background: rgba(0,210,106,0.15); color: var(--ok); }
        .verdict.suspect { background: rgba(245,166,35,0.15); color: var(--suspect); }
        .verdict.transcode { background: rgba(255,56,96,0.15); color: var(--transcode); }
        .verdict.error { background: rgba(102,102,102,0.15); color: var(--error); }

        .score-bar {
            width: 60px;
            height: 6px;
            background: rgba(255,255,255,0.1);
            border-radius: 3px;
            overflow: hidden;
            display: inline-block;
            vertical-align: middle;
            margin-right: 0.5rem;
        }
        .score-fill {
            height: 100%;
            border-radius: 3px;
            transition: width 0.3s;
        }
        .score-fill.low { background: var(--ok); }
        .score-fill.medium { background: var(--suspect); }
        .score-fill.high { background: var(--transcode); }

        .flags { font-size: 0.8rem; color: var(--dim); }
        .flag {
            display: inline-block;
            background: rgba(255,255,255,0.05);
            padding: 0.15rem 0.5rem;
            border-radius: 4px;
            margin: 0.1rem;
            font-family: 'SF Mono', 'Menlo', monospace;
        }
        .filepath {
            max-width: 300px;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
            font-family: 'SF Mono', 'Menlo', monospace;
            font-size: 0.85rem;
        }
        .filepath:hover {
            overflow: visible;
            white-space: normal;
            word-break: break-all;
        }
        .encoder { font-family: 'SF Mono', 'Menlo', monospace; font-size: 0.85rem; }
        .dim { color: var(--dim); }

        .legend {
            margin-top: 2rem;
            padding: 1.5rem;
            background: var(--card);
            border-radius: 12px;
        }
        .legend h3 { margin-bottom: 1rem; font-size: 1rem; }
        .legend-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 1rem; }
        .legend-item { font-size: 0.85rem; }
        .legend-item code {
            background: rgba(255,255,255,0.1);
            padding: 0.1rem 0.4rem;
            border-radius: 4px;
            font-family: 'SF Mono', monospace;
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>Losselot - MP3 Transcode Analysis Report</h1>
        <p class="subtitle">Generated by Losselot</p>

        <div class="stats">
"#;

const HTML_FOOTER: &str = r#"
            </tbody>
        </table>

        <div class="legend">
            <h3>Flag Reference</h3>
            <div class="legend-grid">
                <div class="legend-item"><code>steep_hf_rolloff</code> — High frequencies drop off too sharply for declared bitrate</div>
                <div class="legend-item"><code>dead_upper_band</code> — 17-20kHz range has almost no energy</div>
                <div class="legend-item"><code>silent_17k+</code> — Upper frequencies are essentially silent</div>
                <div class="legend-item"><code>lowpass_mismatch</code> — LAME header lowpass doesn't match bitrate</div>
                <div class="legend-item"><code>multi_encoder_sigs</code> — Multiple encoder signatures found in file</div>
                <div class="legend-item"><code>irregular_frames</code> — CBR frame sizes are inconsistent</div>
            </div>
        </div>
    </div>
</body>
</html>
"#;
