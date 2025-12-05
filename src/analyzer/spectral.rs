//! Spectral analysis of MP3 files
//!
//! Uses FFT to analyze frequency content and detect transcoding:
//! - Measures energy in frequency bands (10-15kHz, 15-20kHz, 17-20kHz)
//! - Transcodes have a characteristic "cliff" where high frequencies die
//! - Legitimate high-bitrate files have gradual rolloff

use rustfft::{num_complex::Complex, FftPlanner};
use serde::Serialize;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

const FFT_SIZE: usize = 8192;
const SAMPLE_RATE: u32 = 44100;

#[derive(Debug, Clone, Default, Serialize)]
pub struct SpectralDetails {
    /// RMS level of full signal (dB)
    pub rms_full: f64,
    /// RMS level of 10-15kHz band (dB)
    pub rms_mid_high: f64,
    /// RMS level of 15-20kHz band (dB)
    pub rms_high: f64,
    /// RMS level of 17-20kHz band (dB)
    pub rms_upper: f64,
    /// Drop from full to high band (dB)
    pub high_drop: f64,
    /// Drop from mid-high to upper band (dB)
    pub upper_drop: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SpectralResult {
    pub score: u32,
    pub flags: Vec<String>,
    pub details: SpectralDetails,
}

/// Hanning window function
fn hanning_window(size: usize) -> Vec<f64> {
    (0..size)
        .map(|i| {
            0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / (size - 1) as f64).cos())
        })
        .collect()
}

/// Convert linear magnitude to dB
fn to_db(value: f64) -> f64 {
    if value <= 0.0 {
        -96.0
    } else {
        20.0 * value.log10()
    }
}

/// Calculate RMS of a slice
fn rms(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&x| x * x).sum();
    (sum_sq / samples.len() as f64).sqrt()
}

/// Decode MP3 to PCM samples using symphonia
fn decode_mp3(data: &[u8]) -> Option<(Vec<f64>, u32)> {
    let cursor = std::io::Cursor::new(data.to_vec());
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

    let mut hint = Hint::new();
    hint.with_extension("mp3");

    let format_opts = FormatOptions::default();
    let metadata_opts = MetadataOptions::default();
    let decoder_opts = DecoderOptions::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .ok()?;

    let mut format = probed.format;
    let track = format.default_track()?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(SAMPLE_RATE);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &decoder_opts)
        .ok()?;

    let mut samples = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    // Decode up to ~15 seconds from middle of file
    let max_samples = (sample_rate as usize) * 15;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break,
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue,
        };

        if sample_buf.is_none() {
            let spec = *decoded.spec();
            let duration = decoded.capacity() as u64;
            sample_buf = Some(SampleBuffer::new(duration, spec));
        }

        if let Some(ref mut buf) = sample_buf {
            // Get channel count before moving decoded
            let channel_count = decoded.spec().channels.count();
            buf.copy_interleaved_ref(decoded);

            // Convert to mono f64
            for chunk in buf.samples().chunks(channel_count) {
                let mono: f64 = chunk.iter().map(|&s| s as f64).sum::<f64>() / channel_count as f64;
                samples.push(mono);
            }

            if samples.len() >= max_samples {
                break;
            }
        }
    }

    if samples.is_empty() {
        return None;
    }

    Some((samples, sample_rate))
}

/// Calculate energy in a frequency band using FFT results
fn band_energy(fft_result: &[Complex<f64>], sample_rate: u32, low_hz: u32, high_hz: u32) -> f64 {
    let bin_resolution = sample_rate as f64 / FFT_SIZE as f64;
    let low_bin = (low_hz as f64 / bin_resolution) as usize;
    let high_bin = (high_hz as f64 / bin_resolution).min((FFT_SIZE / 2) as f64) as usize;

    let mut energy = 0.0;
    for bin in low_bin..=high_bin.min(fft_result.len() - 1) {
        let mag = fft_result[bin].norm();
        energy += mag * mag;
    }

    energy.sqrt()
}

/// Perform spectral analysis on MP3 data
pub fn analyze(data: &[u8], _declared_sample_rate: u32) -> SpectralResult {
    let mut result = SpectralResult::default();

    // Decode MP3 to PCM
    let (samples, sample_rate) = match decode_mp3(data) {
        Some(s) => s,
        None => return result,
    };

    if samples.len() < FFT_SIZE {
        return result;
    }

    // Calculate overall RMS
    let rms_full = to_db(rms(&samples));
    result.details.rms_full = rms_full;

    // Set up FFT
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let window = hanning_window(FFT_SIZE);

    // Process overlapping windows and average the results
    let hop_size = FFT_SIZE / 2;
    let num_windows = (samples.len() - FFT_SIZE) / hop_size + 1;

    let mut avg_mid_high = 0.0;
    let mut avg_high = 0.0;
    let mut avg_upper = 0.0;

    for i in 0..num_windows {
        let start = i * hop_size;
        let end = start + FFT_SIZE;

        if end > samples.len() {
            break;
        }

        // Apply window and convert to complex
        let mut buffer: Vec<Complex<f64>> = samples[start..end]
            .iter()
            .zip(window.iter())
            .map(|(&s, &w)| Complex::new(s * w, 0.0))
            .collect();

        // Perform FFT
        fft.process(&mut buffer);

        // Calculate band energies
        avg_mid_high += band_energy(&buffer, sample_rate, 10000, 15000);
        avg_high += band_energy(&buffer, sample_rate, 15000, 20000);
        avg_upper += band_energy(&buffer, sample_rate, 17000, 20000);
    }

    let num_windows = num_windows.max(1) as f64;
    avg_mid_high /= num_windows;
    avg_high /= num_windows;
    avg_upper /= num_windows;

    // Convert to dB
    result.details.rms_mid_high = to_db(avg_mid_high);
    result.details.rms_high = to_db(avg_high);
    result.details.rms_upper = to_db(avg_upper);

    // Calculate drops
    result.details.high_drop = rms_full - result.details.rms_high;
    result.details.upper_drop = result.details.rms_mid_high - result.details.rms_upper;

    // Score based on analysis
    // These thresholds are tuned to match the reference bash implementation

    // Steep high frequency rolloff (comparing full signal to 15-20kHz band)
    if result.details.high_drop > 28.0 {
        result.score += 25;
        result.flags.push("steep_hf_rolloff".to_string());
    }

    // Dead upper band (17-20kHz much quieter than 10-15kHz)
    if result.details.upper_drop > 22.0 {
        result.score += 25;
        result.flags.push("dead_upper_band".to_string());
    }

    // Silent 17kHz+ (absolute threshold)
    if result.details.rms_upper < -85.0 {
        result.score += 20;
        result.flags.push("silent_17k+".to_string());
    }

    result
}
