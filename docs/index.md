---
layout: default
title: Home
---

# Losselot

Audio forensics. Detects fake lossless files.

---

## Use It

[**Browser Analyzer**](analyzer.html) - Client-side spectral + binary analysis

[**Decision Graph**](demo/) - See how this tool was built

[**Native CLI**](https://github.com/notactuallytreyanastasio/losselot) - Full power, parallel batch processing

---

## How It Works

When someone converts MP3 to FLAC, the removed frequencies don't come back. Losselot finds this:

- **Spectral** - FFT detects frequency cutoffs, rolloff patterns
- **Binary** - Finds encoder signatures (LAME, FFmpeg, etc.)
- **Combined** - Agreement between methods increases confidence

| Score | Verdict | Meaning |
|:-----:|:-------:|:--------|
| 0-34 | OK | Clean |
| 35-64 | SUSPECT | Possibly transcoded |
| 65+ | TRANSCODE | Definitely lossy origin |

---

## Quick Start

```bash
git clone https://github.com/notactuallytreyanastasio/losselot
cd losselot && cargo build --release

# Analyze
./target/release/losselot ~/Music/

# Web UI
./target/release/losselot serve ~/Music/
```

---

## Under the Hood

- [Decision Graph](decision-graph) - Queryable SQLite of every dev decision
- [Claude Tooling](claude-tooling) - AI development workflow
- [Story](story) - Evolution from simple FFT to multi-method analysis

[View on GitHub](https://github.com/notactuallytreyanastasio/losselot)
