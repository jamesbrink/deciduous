# Losselot

Detect "lossless" audio files that were actually created from lossy sources.

## Download

| Platform | Download | GUI |
|----------|----------|-----|
| macOS Apple Silicon | [losselot-darwin-arm64](https://github.com/notactuallytreyanastasio/losselot/releases/latest/download/losselot-darwin-arm64) | Yes |
| macOS Intel | [losselot-darwin-amd64](https://github.com/notactuallytreyanastasio/losselot/releases/latest/download/losselot-darwin-amd64) | Yes |
| Linux x86_64 (AppImage) | [losselot-linux-amd64.AppImage](https://github.com/notactuallytreyanastasio/losselot/releases/latest/download/losselot-linux-amd64.AppImage) | Yes |
| Linux x86_64 (static) | [losselot-linux-amd64](https://github.com/notactuallytreyanastasio/losselot/releases/latest/download/losselot-linux-amd64) | CLI only |
| Windows x86_64 | [losselot-windows-amd64.exe](https://github.com/notactuallytreyanastasio/losselot/releases/latest/download/losselot-windows-amd64.exe) | Yes |

**No dependencies** - just download and run. Linux AppImage includes GTK for GUI support.

## Just Double-Click

1. Download the binary for your platform
2. Double-click it from your Downloads folder (or anywhere)
3. Select a folder or file to analyze
4. Get an interactive HTML report in your browser

That's it. No terminal, no commands, no setup.

## Interactive Reports

Losselot generates beautiful dark-mode HTML reports with D3.js visualizations:

![Losselot Report Example](docs/report-example.png)

**What you see:**
- **Summary cards** - Quick counts of Clean, Suspect, and Transcode files
- **Verdict distribution** - Donut chart showing your library's health
- **Score distribution** - Bar chart of all files sorted by suspicion score
- **Detailed analysis** - Click any file to see frequency band energy, spectral metrics, and detection flags
- **Searchable table** - Sort and filter your results

In this example, `sus2.mp3` is flagged as **SUSPECT (45%)** - a 320kbps iTunes file with `hf_cutoff_detected` and `weak_ultrasonic_content` flags. The frequency band chart shows healthy low frequencies but degraded upper/ultrasonic bands.

## The Problem

You download a FLAC or WAV file labeled as "lossless" - but how do you know it wasn't just an MP3 that someone converted? Once audio goes through lossy compression (MP3, AAC, etc.), the lost frequencies are gone forever. Converting to FLAC doesn't bring them back.

**Losselot detects these fake lossless files.**

## How It Works

Lossy codecs like MP3 work by removing high frequencies that are "less audible." A 128kbps MP3 typically cuts everything above ~16kHz. When you convert that MP3 to FLAC, the cutoff remains - it's a permanent scar.

Losselot performs **spectral analysis** to measure energy in different frequency bands:
- Real lossless audio has gradual, natural high-frequency rolloff
- Fake lossless (from MP3/AAC) has a sharp cliff where the original encoder cut frequencies

### What It Detects

| Source | Detection |
|--------|-----------|
| MP3 128kbps → FLAC | Easily detected (hard cutoff at ~16kHz) |
| MP3 192kbps → FLAC | Usually detected (cutoff at ~18kHz) |
| MP3 320kbps → FLAC | Detected via ultrasonic analysis (no content >20kHz) |
| AAC 128kbps → FLAC | Sometimes detected (AAC is more efficient) |
| MP3 → MP3 transcode | Detected via spectral + LAME header analysis |
| Real lossless | Shows 0% score, natural rolloff |

## Understanding Results

### Verdicts

- **CLEAN (0-34%)**: Appears to be genuine lossless
- **SUSPECT (35-64%)**: Might have lossy origins, worth investigating
- **TRANSCODE (65-100%)**: Almost certainly from a lossy source

### Key Metrics in Reports

| Metric | What It Means | Clean Value | Transcode Value |
|--------|---------------|-------------|-----------------|
| Upper Drop | Energy loss from 10-15kHz to 17-20kHz | ~4-8 dB | ~40-70 dB |
| Ultrasonic Drop | Energy loss from 19-20kHz to 20-22kHz | ~1-2 dB | ~40-50 dB |
| Flatness (19-21k) | Content complexity above 20kHz | ~0.8-0.99 | ~0.01-0.1 |

### Flags

| Flag | Meaning |
|------|---------|
| `severe_hf_damage` | Major high frequency loss (probably from 128kbps or lower) |
| `hf_cutoff_detected` | Clear lossy cutoff pattern detected |
| `possible_lossy_origin` | Mild HF damage, possibly from high-bitrate lossy |
| `cliff_at_20khz` | Sharp cutoff at 20kHz (320kbps MP3 signature) |
| `weak_ultrasonic_content` | Low energy above 20kHz |
| `dead_ultrasonic_band` | No content above 20kHz (strong 320k indicator) |
| `lowpass_mismatch` | (MP3 only) LAME header lowpass doesn't match bitrate |

## Supported Formats

**Input formats:** FLAC, WAV, AIFF, MP3, M4A, AAC, OGG, Opus, WMA, ALAC

The primary use case is analyzing FLAC/WAV files, but Losselot can also detect MP3→MP3 transcodes.

## Limitations

- **High-bitrate lossy is harder**: MP3 320kbps has cutoff near 20kHz, but ultrasonic analysis helps
- **Some codecs are stealthier**: AAC and Vorbis are more efficient than MP3, leaving less obvious damage
- **Dark/quiet recordings**: Low energy in high frequencies is normal for some content
- **Not 100% definitive**: Use as one data point, not absolute proof

---

## CLI Usage

For power users, Losselot has a full command-line interface:

```bash
# Make executable (macOS/Linux)
chmod +x losselot-*

# Analyze a single file
losselot myfile.flac

# Analyze your whole library
losselot ~/Music/

# Verbose output (see spectral details)
losselot -v myfile.flac

# Generate HTML report to specific path
losselot -o report.html ~/Music/

# Launch GUI mode from terminal
losselot --gui
```

### CLI Reference

```
losselot [OPTIONS] [PATH]

Arguments:
  [PATH]  File or directory to analyze (optional in GUI mode)

Options:
      --gui                  Launch GUI file picker
  -o, --output <FILE>        Output report file (.html, .csv, .json)
      --report-dir <DIR>     Directory for auto-generated reports [default: losselot-reports]
      --no-report            Don't auto-generate HTML report
      --no-open              Don't prompt to open report
  -j, --jobs <NUM>           Number of parallel workers (default: CPU count)
      --no-spectral          Skip spectral analysis (faster, binary-only)
  -v, --verbose              Show detailed analysis
  -q, --quiet                Only show summary
      --threshold <NUM>      Transcode threshold percentage [default: 65]
  -h, --help                 Print help
  -V, --version              Print version
```

### Exit Codes

- `0`: All files clean
- `1`: Some files suspect
- `2`: Transcodes detected

### Report Formats

**HTML** - Interactive dark-mode report with D3.js charts (default)

**CSV**
```csv
verdict,filepath,bitrate_kbps,combined_score,spectral_score,binary_score,flags,encoder,lowpass
TRANSCODE,/path/to/fake.flac,0,80,80,0,severe_hf_damage,,
```

**JSON**
```json
{
  "generated": "2024-01-01T00:00:00Z",
  "summary": {"total": 100, "ok": 85, "suspect": 10, "transcode": 5},
  "files": [...]
}
```

## Install to PATH (optional)

```bash
# macOS/Linux
sudo mv losselot-* /usr/local/bin/losselot

# Then run from anywhere
losselot ~/Music/
```

## Build from Source

```bash
# Requires Rust (https://rustup.rs)
git clone https://github.com/notactuallytreyanastasio/losselot.git
cd losselot
cargo build --release
./target/release/losselot --gui
```

---

## Technical Deep Dive

### Why Lossy Compression Leaves Scars

MP3 and other lossy codecs use **psychoacoustic models** to remove frequencies humans supposedly can't hear. The encoder applies a **lowpass filter** before encoding:

| Bitrate | Typical Lowpass | What Gets Cut |
|---------|-----------------|---------------|
| 320 kbps | ~20.5 kHz | Almost nothing audible |
| 256 kbps | ~19.5-20 kHz | Subtle air/shimmer |
| 192 kbps | ~18.5 kHz | High harmonics |
| 160 kbps | ~17.5 kHz | Noticeable on cymbals |
| 128 kbps | ~16 kHz | Obvious on all material |
| 96 kbps | ~15 kHz | Severe damage |

When you re-encode or convert to lossless, **these frequencies don't come back**. The lowpass filter's cutoff frequency becomes a permanent signature.

### Spectral Analysis Method

Losselot uses FFT (Fast Fourier Transform) to decompose audio into frequency components:

1. **Decode to PCM** - Using symphonia (pure Rust, no ffmpeg dependency)
2. **Apply Hanning window** - 8192-sample windows with 50% overlap
3. **FFT analysis** - Convert time domain to frequency domain
4. **Band energy measurement** - Calculate RMS energy in specific bands:
   - Full spectrum: 20 Hz - 20 kHz
   - Mid-high: 10-15 kHz (reference band, usually healthy)
   - High: 15-20 kHz (damaged by low bitrate)
   - Upper: 17-20 kHz (damaged by medium bitrate)
   - Pre-ultrasonic: 19-20 kHz (damaged by high bitrate)
   - Ultrasonic: 20-22 kHz (key for 320k detection)

### The 320kbps Detection Problem

320kbps MP3 is tricky because its ~20.5kHz cutoff is near the edge of human hearing. Traditional spectral analysis looking at 17-20kHz won't catch it.

**Solution: Ultrasonic analysis**

Real lossless audio (from CD/vinyl/studio) contains content above 20kHz:
- Recording equipment captures it
- Natural harmonics extend past 20kHz
- Room noise/ambience has ultrasonic components

320kbps MP3 has **nothing** above 20kHz - it's a hard cliff.

```
Real lossless at 20-21kHz: -32.5 dB (content present)
320k transcode at 20-21kHz: -82.6 dB (dead silence)
```

### Architecture

```
src/
├── main.rs           # CLI entry point + GUI detection
├── lib.rs            # Library exports
├── analyzer/
│   ├── mod.rs        # Analyzer orchestration
│   ├── spectral.rs   # FFT-based frequency analysis
│   └── binary.rs     # MP3 header forensics
├── mp3/
│   ├── mod.rs        # MP3 module
│   ├── frame.rs      # Frame header parsing
│   └── lame.rs       # LAME/Xing header extraction
└── report/
    ├── mod.rs        # Report generation
    ├── html.rs       # D3.js HTML reports
    ├── csv.rs        # CSV export
    └── json.rs       # JSON export
```

**Key dependencies:**
- `symphonia` - Pure Rust audio decoder (MP3, FLAC, WAV, OGG, etc.)
- `rustfft` - Pure Rust FFT implementation
- `rayon` - Parallel file processing
- `rfd` - Native file dialogs for GUI mode

No external binaries (ffmpeg, sox) required.

## License

MIT
