# Losselot

Detect MP3s that have been transcoded from lower quality sources.

## What It Does

Losselot combines **spectral analysis** and **binary forensics** to identify MP3 files that claim to be high bitrate but were actually encoded from lower quality sources.

### Detection Methods

1. **Binary Analysis** (no external dependencies)
   - Parses MP3 frame headers
   - Extracts LAME/Xing header information
   - **Key check**: LAME header contains lowpass filter frequency - if a "320kbps" file has lowpass=16000Hz, it was transcoded from 128kbps
   - Detects multiple encoder signatures
   - Analyzes frame size consistency

2. **Spectral Analysis**
   - Decodes MP3 to PCM using symphonia (pure Rust)
   - Performs FFT to measure energy in frequency bands
   - Compares energy dropoff between bands (10-15kHz, 15-20kHz, 17-20kHz)
   - Transcodes have a characteristic "cliff" where high frequencies die

### Scoring

- **0-34%**: OK (clean file)
- **35-64%**: SUSPECT (might be transcoded)
- **65-100%**: TRANSCODE (almost certainly transcoded)

## Installation

### From Source

```bash
git clone https://github.com/notactuallytreyanastasio/losselot.git
cd losselot
cargo build --release
./target/release/losselot --help
```

### Pre-built Binaries

Download from [Releases](https://github.com/notactuallytreyanastasio/losselot/releases):
- `losselot-darwin-amd64` - macOS Intel
- `losselot-darwin-arm64` - macOS Apple Silicon
- `losselot-linux-amd64` - Linux x86_64
- `losselot-windows-amd64.exe` - Windows x86_64

## Usage

```
losselot [OPTIONS] <PATH>

Arguments:
  <PATH>  File or directory to analyze

Options:
  -o, --output <FILE>      Output report file (.html, .csv, .json)
  -j, --jobs <NUM>         Number of parallel workers (default: CPU count)
      --no-spectral        Skip spectral analysis (faster, binary-only)
  -v, --verbose            Show detailed analysis
  -q, --quiet              Only show summary
      --threshold <NUM>    Transcode threshold percentage [default: 65]
  -h, --help               Print help
  -V, --version            Print version
```

### Examples

```bash
# Analyze a single file
losselot suspicious.mp3

# Analyze entire music library
losselot ~/Music/

# Generate HTML report
losselot -o report.html ~/Music/

# Quick scan (binary-only, no FFT)
losselot --no-spectral ~/Music/

# Parallel processing with 8 workers
losselot -j 8 ~/Music/
```

### Exit Codes

- `0`: All files clean
- `1`: Some files suspect
- `2`: Transcodes detected

## Report Formats

### HTML
Beautiful dark-mode report with:
- Summary statistics
- Color-coded verdicts
- Score progress bars
- Flag reference legend

### CSV
```csv
verdict,filepath,bitrate_kbps,combined_score,spectral_score,binary_score,flags,encoder,lowpass
TRANSCODE,/path/to/file.mp3,320,85,45,40,lowpass_mismatch(16000Hz),LAME3.100,16000
```

### JSON
```json
{
  "generated": "2024-01-01T00:00:00Z",
  "summary": {"total": 100, "ok": 85, "suspect": 10, "transcode": 5},
  "files": [...]
}
```

## Flags Reference

| Flag | Meaning |
|------|---------|
| `lowpass_mismatch` | LAME header lowpass frequency doesn't match declared bitrate (smoking gun!) |
| `multi_encoder_sigs` | Multiple encoder signatures found in file |
| `irregular_frames` | CBR frame sizes are inconsistent |
| `steep_hf_rolloff` | High frequencies drop off too sharply |
| `dead_upper_band` | 17-20kHz range has almost no energy |
| `silent_17k+` | Upper frequencies are essentially silent |

## How Transcoding Detection Works

When you encode audio to MP3, the encoder applies a lowpass filter based on the bitrate:
- 320kbps → ~20.5kHz lowpass
- 256kbps → ~20kHz lowpass
- 192kbps → ~18.5kHz lowpass
- 128kbps → ~16kHz lowpass

**The LAME encoder writes this lowpass frequency into a header field.**

If someone takes a 128kbps MP3 and re-encodes it at 320kbps:
- The file claims to be 320kbps
- But the LAME header still says lowpass=16000Hz
- And the spectral analysis shows no energy above 16kHz

This is the "smoking gun" that Losselot looks for.

## Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test
```

### Cross-compilation

The GitHub Actions workflow builds for all platforms automatically on release tags.

For manual cross-compilation:
```bash
# macOS (both architectures)
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin

# Requires cross or appropriate toolchain
cross build --release --target x86_64-unknown-linux-gnu
cross build --release --target x86_64-pc-windows-gnu
```

## License

MIT
