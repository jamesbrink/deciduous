//! LAME/Xing header extraction
//!
//! LAME encodes a VBR info header in the first frame of the MP3.
//! This header contains crucial forensic information including:
//! - Encoder version string
//! - Lowpass filter frequency (THE KEY for transcode detection)
//! - VBR method used
//! - Encoding quality settings

use std::io::{self, Read, Seek, SeekFrom};

/// Information extracted from LAME header
#[derive(Debug, Clone, Default)]
pub struct LameHeader {
    /// Encoder version string (e.g., "LAME3.100")
    pub encoder: String,
    /// Lowpass filter frequency in Hz (e.g., 16000, 19500, 20500)
    /// This is THE smoking gun for transcode detection
    pub lowpass: Option<u32>,
    /// VBR method (0 = CBR, 1-5 = various VBR methods)
    pub vbr_method: Option<u8>,
    /// Encoding quality (0-9, lower = better)
    pub quality: Option<u8>,
    /// Whether this is a Xing header (VBR) or Info header (CBR)
    pub is_vbr_header: bool,
    /// Total frames reported by header
    pub total_frames: Option<u32>,
    /// Total bytes reported by header
    pub total_bytes: Option<u32>,
}

/// Other encoder signatures we might find
#[derive(Debug, Clone)]
pub struct EncoderSignatures {
    pub lame: Option<String>,
    pub fraunhofer: bool,
    pub itunes: bool,
    pub ffmpeg: bool,
    pub xing: bool,
    pub other: Vec<String>,
    /// All encoder strings found (for detecting re-encoding)
    pub all_encoders: Vec<String>,
    /// Number of times LAME signature appears (multiple = re-encoded)
    pub lame_count: usize,
    /// Number of times Lavf/Lavc appears (FFmpeg processing)
    pub lavf_count: usize,
    /// Number of times Fraunhofer signature appears
    pub fraunhofer_count: usize,
    /// Total encoder signature count (sum of all encoder occurrences)
    pub total_encoder_passes: usize,
}

impl Default for EncoderSignatures {
    fn default() -> Self {
        Self {
            lame: None,
            fraunhofer: false,
            itunes: false,
            ffmpeg: false,
            xing: false,
            other: Vec::new(),
            all_encoders: Vec::new(),
            lame_count: 0,
            lavf_count: 0,
            fraunhofer_count: 0,
            total_encoder_passes: 0,
        }
    }
}

impl EncoderSignatures {
    /// Check if file shows evidence of re-encoding
    ///
    /// Re-encoding indicators:
    /// - Multiple encoder signatures of any type (encoded more than once)
    /// - Mixed encoder chain (e.g., LAME + FFmpeg = transcoded)
    /// - Total encoder passes > 1
    pub fn shows_reencoding(&self) -> bool {
        // Multiple occurrences of same encoder
        if self.lame_count > 1 || self.lavf_count > 1 || self.fraunhofer_count > 1 {
            return true;
        }

        // Total passes detected is more than 1
        if self.total_encoder_passes > 1 {
            return true;
        }

        // Mixed encoder chain (different encoders touched the file)
        let encoder_count = self.unique_encoder_count();
        encoder_count > 1
    }

    /// Count unique encoders that touched this file
    pub fn unique_encoder_count(&self) -> usize {
        let mut count = 0;
        if self.lame.is_some() { count += 1; }
        if self.fraunhofer { count += 1; }
        if self.itunes { count += 1; }
        if self.ffmpeg { count += 1; }
        count
    }

    /// Get a human-readable encoding chain description
    pub fn encoding_chain_description(&self) -> Option<String> {
        if !self.shows_reencoding() {
            return None;
        }

        let mut parts = Vec::new();

        if self.lame_count > 1 {
            parts.push(format!("LAME x{}", self.lame_count));
        } else if self.lame.is_some() {
            parts.push("LAME".to_string());
        }

        if self.lavf_count > 1 {
            parts.push(format!("FFmpeg x{}", self.lavf_count));
        } else if self.ffmpeg {
            parts.push("FFmpeg".to_string());
        }

        if self.fraunhofer {
            parts.push("Fraunhofer".to_string());
        }

        if self.itunes {
            parts.push("iTunes".to_string());
        }

        if parts.len() > 1 {
            Some(parts.join(" → "))
        } else if self.lame_count > 1 || self.lavf_count > 1 {
            Some(parts.join(""))
        } else {
            None
        }
    }
}

impl LameHeader {
    /// Extract LAME header from MP3 file data
    ///
    /// The LAME header is located after the Xing/Info header in the first frame.
    /// We only search in the first 2KB to avoid false matches in audio data.
    pub fn extract(data: &[u8]) -> Option<Self> {
        let mut header = LameHeader::default();

        // Only search in the first frame region (first 2KB should be plenty)
        let search_region = &data[..data.len().min(2048)];

        // Look for Xing or Info header
        let xing_pos = find_pattern(search_region, b"Xing");
        let info_pos = find_pattern(search_region, b"Info");

        let vbr_header_pos = match (xing_pos, info_pos) {
            (Some(x), _) => {
                header.is_vbr_header = true;
                Some(x)
            }
            (_, Some(i)) => {
                header.is_vbr_header = false;
                Some(i)
            }
            _ => None,
        };

        // Parse Xing/Info header if found
        if let Some(pos) = vbr_header_pos {
            if pos + 8 <= search_region.len() {
                let flags = u32::from_be_bytes([
                    search_region[pos + 4],
                    search_region[pos + 5],
                    search_region[pos + 6],
                    search_region[pos + 7],
                ]);

                let mut offset = pos + 8;

                // Frames flag (bit 0)
                if flags & 0x01 != 0 && offset + 4 <= search_region.len() {
                    header.total_frames = Some(u32::from_be_bytes([
                        search_region[offset],
                        search_region[offset + 1],
                        search_region[offset + 2],
                        search_region[offset + 3],
                    ]));
                    offset += 4;
                }

                // Bytes flag (bit 1)
                if flags & 0x02 != 0 && offset + 4 <= search_region.len() {
                    header.total_bytes = Some(u32::from_be_bytes([
                        search_region[offset],
                        search_region[offset + 1],
                        search_region[offset + 2],
                        search_region[offset + 3],
                    ]));
                    offset += 4;
                }

                // TOC flag (bit 2) - skip 100 bytes
                if flags & 0x04 != 0 {
                    offset += 100;
                }

                // Quality flag (bit 3) - skip 4 bytes
                if flags & 0x08 != 0 {
                    offset += 4;
                }

                // Look for LAME tag right after Xing data (within ~50 bytes)
                // The LAME tag immediately follows the Xing/Info structure
                let lame_search_start = offset;
                let lame_search_end = (offset + 50).min(search_region.len());

                if let Some(rel_pos) = find_pattern(&search_region[lame_search_start..lame_search_end], b"LAME") {
                    let lame_pos = lame_search_start + rel_pos;

                    // Extract version string
                    let version_end = (lame_pos + 9).min(search_region.len());
                    if let Ok(version) = std::str::from_utf8(&search_region[lame_pos..version_end]) {
                        header.encoder = version.trim_end_matches('\0').to_string();
                    }

                    // Lowpass filter is at offset 10 from LAME string
                    // Stored as Hz/100 (so 160 = 16000 Hz, 170 = 17000 Hz)
                    if lame_pos + 10 < search_region.len() {
                        let lowpass_byte = search_region[lame_pos + 10];
                        // Sanity check: valid lowpass values are 50-220 (5kHz to 22kHz)
                        if lowpass_byte >= 50 && lowpass_byte <= 220 {
                            header.lowpass = Some(lowpass_byte as u32 * 100);
                        }
                    }

                    // VBR method and quality are in the byte at offset 9
                    if lame_pos + 9 < search_region.len() {
                        let info_byte = search_region[lame_pos + 9];
                        header.vbr_method = Some(info_byte & 0x0F);
                        header.quality = Some((info_byte >> 4) & 0x0F);
                    }

                    return Some(header);
                }

                // Check for Lavc (ffmpeg/libav) encoder - doesn't have lowpass info
                if let Some(rel_pos) = find_pattern(&search_region[lame_search_start..lame_search_end], b"Lavc") {
                    let lavc_pos = lame_search_start + rel_pos;
                    let version_end = (lavc_pos + 12).min(search_region.len());
                    if let Ok(version) = std::str::from_utf8(&search_region[lavc_pos..version_end]) {
                        header.encoder = version.trim_end_matches('\0').to_string();
                    }
                    // Lavc doesn't include lowpass info, so we leave it as None
                    return Some(header);
                }
            }
        }

        // Fallback: search first 500 bytes for LAME (for files without Xing header)
        if let Some(lame_pos) = find_pattern(&search_region[..search_region.len().min(500)], b"LAME") {
            let version_end = (lame_pos + 9).min(search_region.len());
            if let Ok(version) = std::str::from_utf8(&search_region[lame_pos..version_end]) {
                header.encoder = version.trim_end_matches('\0').to_string();
            }

            if lame_pos + 10 < search_region.len() {
                let lowpass_byte = search_region[lame_pos + 10];
                if lowpass_byte >= 50 && lowpass_byte <= 220 {
                    header.lowpass = Some(lowpass_byte as u32 * 100);
                }
            }

            return Some(header);
        }

        // If we found Xing/Info but no encoder tag, still return what we have
        if vbr_header_pos.is_some() {
            return Some(header);
        }

        None
    }
}

/// Scan file for all encoder signatures
///
/// This function not only detects which encoders touched a file, but also
/// counts how many times each encoder signature appears. Multiple occurrences
/// of the same encoder suggest the file was re-encoded multiple times.
pub fn scan_encoder_signatures<R: Read + Seek>(reader: &mut R) -> io::Result<EncoderSignatures> {
    let mut sigs = EncoderSignatures::default();

    // Read first 64KB for signature scanning
    reader.seek(SeekFrom::Start(0))?;
    let mut buf = vec![0u8; 65536];
    let bytes_read = reader.read(&mut buf)?;
    buf.truncate(bytes_read);

    // Convert to string for pattern matching (lossy is fine, we're looking for ASCII)
    let text = String::from_utf8_lossy(&buf);

    // Count LAME signatures more carefully:
    // Only count "LAME3." or "LAME " patterns (actual encoder tags)
    // This avoids false positives from random audio data matching "LAME"
    sigs.lame_count = count_valid_lame_signatures(&buf);

    // Extract first LAME version for display
    if let Some(pos) = find_pattern(&buf, b"LAME3.") {
        let end = (pos + 20).min(buf.len());
        if let Ok(s) = std::str::from_utf8(&buf[pos..end]) {
            let version: String = s.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
                .collect();
            if !version.is_empty() {
                sigs.lame = Some(version.clone());
                sigs.all_encoders.push(version);
            }
        }
    } else if let Some(pos) = find_pattern(&buf, b"LAME") {
        // Fallback to generic LAME if no version found
        let end = (pos + 20).min(buf.len());
        if let Ok(s) = std::str::from_utf8(&buf[pos..end]) {
            let version: String = s.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
                .collect();
            if !version.is_empty() {
                sigs.lame = Some(version.clone());
                sigs.all_encoders.push(version);
            }
        }
    }

    // Count Lavf/Lavc occurrences (FFmpeg processing)
    // These are more specific patterns so less prone to false positives
    sigs.lavf_count = count_valid_ffmpeg_signatures(&buf);

    // Fraunhofer - count occurrences
    let header_region = &buf[..buf.len().min(4096)];
    sigs.fraunhofer_count = count_pattern_occurrences(header_region, b"Fraunhofer")
        + count_pattern_occurrences(header_region, b"FhG");
    if sigs.fraunhofer_count > 0 {
        sigs.fraunhofer = true;
        sigs.all_encoders.push("Fraunhofer".to_string());
    }

    // iTunes
    if text.contains("iTunes") || (text.contains("Lavf") && text.contains("Apple")) {
        sigs.itunes = true;
        sigs.all_encoders.push("iTunes".to_string());
    }

    // FFmpeg/Lavf
    if text.contains("Lavf") || text.contains("Lavc") || text.contains("libmp3lame") {
        sigs.ffmpeg = true;
        if !sigs.all_encoders.iter().any(|e| e.contains("FFmpeg")) {
            sigs.all_encoders.push("FFmpeg".to_string());
        }
    }

    // Xing (sometimes standalone)
    if find_pattern(&buf, b"Xing").is_some() || find_pattern(&buf, b"Info").is_some() {
        sigs.xing = true;
    }

    // GOGO encoder (Japanese MP3 encoder)
    if text.contains("GOGO") || text.contains("GOGOMP3") {
        sigs.other.push("GOGO".to_string());
        sigs.all_encoders.push("GOGO".to_string());
    }

    // BladeEnc
    if text.contains("BladeEnc") || text.contains("Blade") {
        sigs.other.push("BladeEnc".to_string());
        sigs.all_encoders.push("BladeEnc".to_string());
    }

    // Shine encoder (simple/fast encoder)
    if text.contains("Shine") {
        sigs.other.push("Shine".to_string());
        sigs.all_encoders.push("Shine".to_string());
    }

    // Helix encoder
    if text.contains("Helix") || text.contains("RealNetworks") {
        sigs.other.push("Helix".to_string());
        sigs.all_encoders.push("Helix".to_string());
    }

    // Calculate total encoder passes
    sigs.total_encoder_passes = sigs.lame_count
        .max(1) // At least 1 if LAME detected
        .saturating_sub(1) // Don't count first LAME as a re-encode
        + sigs.lavf_count
        + sigs.fraunhofer_count
        + if sigs.itunes { 1 } else { 0 }
        + sigs.other.len();

    // If we have multiple unique encoders, that's at least 2 passes
    if sigs.unique_encoder_count() > 1 && sigs.total_encoder_passes < 2 {
        sigs.total_encoder_passes = sigs.unique_encoder_count();
    }

    Ok(sigs)
}

/// Count how many times a pattern appears in the buffer
pub fn count_pattern_occurrences(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || haystack.len() < needle.len() {
        return 0;
    }

    let mut count = 0;
    let mut pos = 0;

    while pos <= haystack.len() - needle.len() {
        if let Some(found) = find_pattern(&haystack[pos..], needle) {
            count += 1;
            pos += found + needle.len(); // Move past this occurrence
        } else {
            break;
        }
    }

    count
}

/// Count valid LAME encoder signatures (more specific than just "LAME")
///
/// To avoid false positives from random audio data, we ONLY search in the
/// first 4KB (header region) where encoder tags actually appear. Audio
/// data beyond this can contain random bytes that match "LAME" patterns.
fn count_valid_lame_signatures(buf: &[u8]) -> usize {
    // Only search in header region to avoid false positives from audio data
    // MP3 encoder tags appear in first few KB (VBR header, ID3 tags, etc.)
    let header_region = &buf[..buf.len().min(4096)];

    let mut count = 0;

    // Count "LAME3." patterns (LAME 3.x versions - most common)
    count += count_pattern_occurrences(header_region, b"LAME3.");

    // Count "LAME " patterns (space after LAME indicates tag)
    if count == 0 {
        count += count_pattern_occurrences(header_region, b"LAME ");
    }

    // Also check for older LAME versions (rare but possible)
    count += count_pattern_occurrences(header_region, b"LAME2.");

    // If still 0, check for bare LAME
    if count == 0 {
        if find_pattern(header_region, b"LAME").is_some() {
            count = 1;
        }
    }

    count
}

/// Count valid FFmpeg signatures (Lavf/Lavc patterns)
///
/// FFmpeg signatures appear in the header region. We limit our search
/// to the first 4KB to avoid false positives from audio data.
fn count_valid_ffmpeg_signatures(buf: &[u8]) -> usize {
    // Only search in header region
    let header_region = &buf[..buf.len().min(4096)];

    // Count Lavf patterns (format library)
    let lavf_count = count_pattern_occurrences(header_region, b"Lavf");

    // Count Lavc patterns (codec library)
    let lavc_count = count_pattern_occurrences(header_region, b"Lavc");

    // Both usually appear together, so take the max rather than sum
    // to avoid double-counting the same encoding pass
    let mut count = lavf_count.max(lavc_count);

    // But if both appear more times than expected, count separately
    // (indicates multiple FFmpeg passes)
    if lavf_count > 1 && lavc_count > 1 {
        count = lavf_count + lavc_count;
    }

    count
}

/// Count unique encoder signatures in file
pub fn count_encoder_signatures<R: Read + Seek>(reader: &mut R) -> io::Result<usize> {
    let sigs = scan_encoder_signatures(reader)?;
    let mut count = 0;

    if sigs.lame.is_some() {
        count += 1;
    }
    if sigs.fraunhofer {
        count += 1;
    }
    if sigs.itunes {
        count += 1;
    }
    if sigs.ffmpeg {
        count += 1;
    }

    Ok(count)
}

/// Find a byte pattern in a slice
fn find_pattern(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

/// Expected lowpass frequencies for different bitrates
/// If actual lowpass is significantly lower than expected, it's likely a transcode
pub fn expected_lowpass_for_bitrate(bitrate: u32) -> u32 {
    // Approximate expected lowpass based on bitrate
    // These are rough estimates; actual values vary by encoder
    if bitrate >= 320 {
        20500
    } else if bitrate >= 256 {
        20000
    } else if bitrate >= 224 {
        19500
    } else if bitrate >= 192 {
        18500
    } else if bitrate >= 160 {
        17500
    } else if bitrate >= 128 {
        16000
    } else if bitrate >= 112 {
        15500
    } else if bitrate >= 96 {
        15000
    } else {
        14000
    }
}

/// Minimum acceptable lowpass for a bitrate (below this = suspicious)
fn min_acceptable_lowpass(bitrate: u32) -> u32 {
    if bitrate >= 256 {
        18000  // 256+ kbps should have at least 18kHz
    } else if bitrate >= 192 {
        17000  // 192+ kbps should have at least 17kHz
    } else if bitrate >= 160 {
        16000  // 160+ kbps should have at least 16kHz
    } else if bitrate >= 128 {
        15000  // 128+ kbps should have at least 15kHz
    } else {
        0  // Don't flag very low bitrates
    }
}

/// Check if lowpass frequency suggests transcoding
/// Returns (is_suspicious, expected_lowpass, reason)
pub fn check_lowpass_mismatch(bitrate: u32, actual_lowpass: u32) -> (bool, u32, Option<String>) {
    let expected = expected_lowpass_for_bitrate(bitrate);
    let threshold = min_acceptable_lowpass(bitrate);

    // If actual lowpass is significantly lower than expected, it's suspicious
    if threshold > 0 && actual_lowpass > 0 && actual_lowpass < threshold {
        let likely_source = match actual_lowpass {
            lp if lp <= 11000 => "64kbps or lower",
            lp if lp <= 14000 => "96kbps",
            lp if lp <= 16000 => "128kbps",
            lp if lp <= 17500 => "160kbps",
            lp if lp <= 18500 => "192kbps",
            _ => "lower bitrate",
        };

        (
            true,
            expected,
            Some(format!(
                "Lowpass {}Hz suggests transcode from {} source",
                actual_lowpass, likely_source
            )),
        )
    } else {
        (false, expected, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // ==========================================================================
    // EDUCATIONAL BACKGROUND: Understanding LAME Headers for Transcode Detection
    // ==========================================================================
    //
    // MP3 encoding permanently loses audio information. When you encode audio
    // to MP3, the encoder applies a "lowpass filter" that removes frequencies
    // above a certain threshold to save space. Lower bitrates mean lower cutoffs:
    //
    //   128 kbps → removes everything above ~16 kHz
    //   192 kbps → removes everything above ~18 kHz
    //   256 kbps → removes everything above ~19-20 kHz
    //   320 kbps → removes everything above ~20-21 kHz
    //
    // The LAME encoder (the most popular MP3 encoder) honestly records this
    // lowpass frequency in its header. This is THE smoking gun for transcode
    // detection:
    //
    //   If someone takes a 128kbps MP3 → converts to WAV → re-encodes as 320kbps,
    //   the LAME header will still say lowpass=16000 Hz, revealing the original
    //   source quality even though the file claims to be 320kbps.
    //
    // This module extracts and analyzes these headers.
    // ==========================================================================

    /// Helper: Create minimal MP3-like data with LAME header for testing
    fn create_lame_header_data(
        encoder_version: &str,
        lowpass_hz: u32,
        use_xing: bool,  // true = VBR (Xing), false = CBR (Info)
    ) -> Vec<u8> {
        let mut data = Vec::new();

        // MP3 frame sync and header (simplified)
        data.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);  // Valid MP3 header

        // Some padding before Xing/Info header (as in real files)
        data.extend_from_slice(&[0x00; 32]);

        // Xing or Info marker (4 bytes)
        if use_xing {
            data.extend_from_slice(b"Xing");
        } else {
            data.extend_from_slice(b"Info");
        }

        // Xing flags: all fields present (frames, bytes, TOC, quality)
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x0F]);

        // Frames count (4 bytes)
        data.extend_from_slice(&[0x00, 0x00, 0x10, 0x00]);  // 4096 frames

        // Bytes count (4 bytes)
        data.extend_from_slice(&[0x00, 0x10, 0x00, 0x00]);  // 1MB

        // TOC (100 bytes) - just zeros for test
        data.extend_from_slice(&[0x00; 100]);

        // Quality (4 bytes)
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x64]);  // Quality 100

        // LAME version string (9 bytes)
        let version_bytes = encoder_version.as_bytes();
        let mut lame_tag = [0u8; 9];
        let copy_len = version_bytes.len().min(9);
        lame_tag[..copy_len].copy_from_slice(&version_bytes[..copy_len]);
        data.extend_from_slice(&lame_tag);

        // Byte 9 after LAME: VBR method (low nibble) + quality (high nibble)
        data.push(0x24);  // Quality 2, VBR method 4

        // Byte 10 after LAME: Lowpass frequency / 100
        let lowpass_byte = (lowpass_hz / 100) as u8;
        data.push(lowpass_byte);

        // Padding to make it look realistic
        data.extend_from_slice(&[0x00; 100]);

        data
    }

    // ==========================================================================
    // LOWPASS FREQUENCY TESTS
    // These tests demonstrate the core transcode detection technique
    // ==========================================================================

    #[test]
    fn test_lowpass_reveals_128kbps_transcode() {
        // SCENARIO: Someone downloads a 128kbps MP3, converts it to WAV,
        // then re-encodes it as "320kbps" hoping to fool people.
        //
        // PROBLEM: The LAME header records lowpass=16000Hz (16kHz),
        // which is the telltale sign of 128kbps encoding. A legitimate
        // 320kbps file would have lowpass=20500Hz or higher.
        //
        // This test verifies we correctly flag this mismatch.

        let (is_suspicious, expected, reason) = check_lowpass_mismatch(320, 16000);

        assert!(is_suspicious, "Should detect transcode");
        assert!(expected >= 20000, "Expected lowpass for 320kbps should be ~20kHz+");
        assert!(
            reason.as_ref().unwrap().contains("128kbps"),
            "Should identify likely source as 128kbps: {:?}",
            reason
        );
    }

    #[test]
    fn test_lowpass_reveals_192kbps_transcode() {
        // SCENARIO: 192kbps source re-encoded as 320kbps
        // 192kbps typically has lowpass around 18-18.5kHz
        //
        // Note: The threshold for 320kbps is 18000Hz, so we use 17500Hz
        // which is definitely from a 160kbps or lower source.

        let (is_suspicious, _, reason) = check_lowpass_mismatch(320, 17500);

        assert!(is_suspicious, "Should detect transcode from lower bitrate");
        assert!(
            reason.as_ref().unwrap().contains("160kbps"),
            "Should identify likely source: {:?}",
            reason
        );
    }

    #[test]
    fn test_legitimate_320kbps_not_flagged() {
        // SCENARIO: A legitimately encoded 320kbps MP3 from a lossless source.
        // LAME would use lowpass=20500Hz or similar.
        //
        // This should NOT be flagged as suspicious.

        let (is_suspicious, _, reason) = check_lowpass_mismatch(320, 20500);

        assert!(!is_suspicious, "Legitimate 320kbps should not be flagged");
        assert!(reason.is_none(), "Should have no suspicious reason");
    }

    #[test]
    fn test_lowpass_threshold_table() {
        // EDUCATIONAL: This test documents the lowpass thresholds for each bitrate.
        // Understanding these helps interpret analysis results.
        //
        // The table below shows what lowpass frequency we expect for each bitrate
        // when encoding from a lossless source:

        let expected_lowpass_table = [
            (320, 20500),  // 320kbps: ~20.5kHz (nearly full bandwidth)
            (256, 20000),  // 256kbps: ~20kHz
            (224, 19500),  // 224kbps: ~19.5kHz
            (192, 18500),  // 192kbps: ~18.5kHz
            (160, 17500),  // 160kbps: ~17.5kHz
            (128, 16000),  // 128kbps: ~16kHz (major quality loss)
            (112, 15500),  // 112kbps: ~15.5kHz
            (96,  15000),  // 96kbps:  ~15kHz (significant loss)
            (64,  14000),  // 64kbps:  ~14kHz or lower (severe loss)
        ];

        for (bitrate, expected) in expected_lowpass_table {
            let actual = expected_lowpass_for_bitrate(bitrate);
            assert_eq!(
                actual, expected,
                "Bitrate {}kbps should expect {}Hz lowpass",
                bitrate, expected
            );
        }
    }

    // ==========================================================================
    // LAME HEADER EXTRACTION TESTS
    // These tests verify we correctly parse the binary LAME header format
    // ==========================================================================

    #[test]
    fn test_extract_lame_version() {
        // The LAME encoder writes its version string in the header.
        // Common versions: "LAME3.99r", "LAME3.100", "LAME3.99.5"
        //
        // This is useful for forensics - knowing the encoder helps
        // understand what settings were likely used.

        let data = create_lame_header_data("LAME3.100", 20500, false);
        let header = LameHeader::extract(&data).expect("Should extract header");

        assert_eq!(header.encoder, "LAME3.100");
    }

    #[test]
    fn test_extract_lowpass_frequency() {
        // The lowpass frequency is stored as (Hz / 100) in a single byte.
        // So 160 means 16000 Hz, 205 means 20500 Hz.

        let data = create_lame_header_data("LAME3.100", 16000, false);
        let header = LameHeader::extract(&data).expect("Should extract header");

        assert_eq!(header.lowpass, Some(16000));
    }

    #[test]
    fn test_extract_vbr_vs_cbr_header() {
        // LAME marks VBR (variable bitrate) files with "Xing" header
        // and CBR (constant bitrate) files with "Info" header.
        //
        // VBR files vary their bitrate throughout the song - more bits
        // for complex sections, fewer for simple ones. CBR uses a
        // constant bitrate throughout.

        // VBR file (Xing header)
        let vbr_data = create_lame_header_data("LAME3.99r", 19000, true);
        let vbr_header = LameHeader::extract(&vbr_data).expect("Should extract");
        assert!(vbr_header.is_vbr_header, "Should detect VBR (Xing) header");

        // CBR file (Info header)
        let cbr_data = create_lame_header_data("LAME3.100", 20500, false);
        let cbr_header = LameHeader::extract(&cbr_data).expect("Should extract");
        assert!(!cbr_header.is_vbr_header, "Should detect CBR (Info) header");
    }

    #[test]
    fn test_extract_frame_and_byte_counts() {
        // The Xing/Info header includes total frame and byte counts.
        // These can help verify file integrity and calculate duration.

        let data = create_lame_header_data("LAME3.100", 20000, false);
        let header = LameHeader::extract(&data).expect("Should extract");

        // Our test data has 4096 frames and ~1MB
        assert!(header.total_frames.is_some());
        assert!(header.total_bytes.is_some());
    }

    #[test]
    fn test_no_lame_header_returns_none() {
        // Not all MP3s have LAME headers. Files from other encoders
        // (Fraunhofer, iTunes AAC→MP3, etc.) may lack this info.

        let data = vec![0xFF, 0xFB, 0x90, 0x00, 0x00, 0x00]; // Just MP3 sync
        let header = LameHeader::extract(&data);

        assert!(header.is_none(), "Should return None for non-LAME file");
    }

    // ==========================================================================
    // ENCODER SIGNATURE TESTS
    // Different encoders leave identifiable fingerprints in the file
    // ==========================================================================

    #[test]
    fn test_detect_lame_encoder() {
        let mut data = vec![0u8; 1000];
        // Insert LAME signature
        data[100..109].copy_from_slice(b"LAME3.100");

        let mut cursor = Cursor::new(data);
        let sigs = scan_encoder_signatures(&mut cursor).expect("Should scan");

        assert!(sigs.lame.is_some(), "Should detect LAME encoder");
        assert_eq!(sigs.lame.unwrap(), "LAME3.100");
    }

    #[test]
    fn test_detect_fraunhofer_encoder() {
        // Fraunhofer IIS created the original MP3 codec.
        // Their encoder is used in some professional tools.

        let mut data = vec![0u8; 1000];
        data[100..110].copy_from_slice(b"Fraunhofer");

        let mut cursor = Cursor::new(data);
        let sigs = scan_encoder_signatures(&mut cursor).expect("Should scan");

        assert!(sigs.fraunhofer, "Should detect Fraunhofer encoder");
    }

    #[test]
    fn test_detect_ffmpeg_encoder() {
        // FFmpeg's libmp3lame wrapper is commonly used for transcoding.
        // It leaves "Lavf" (libavformat) or "Lavc" (libavcodec) signatures.

        let mut data = vec![0u8; 1000];
        data[100..104].copy_from_slice(b"Lavf");

        let mut cursor = Cursor::new(data);
        let sigs = scan_encoder_signatures(&mut cursor).expect("Should scan");

        assert!(sigs.ffmpeg, "Should detect FFmpeg/Lavf encoder");
    }

    #[test]
    fn test_count_multiple_encoders() {
        // Files that have been re-encoded multiple times may have
        // multiple encoder signatures - a red flag for transcoding!

        let mut data = vec![0u8; 2000];
        data[100..109].copy_from_slice(b"LAME3.100");
        data[500..504].copy_from_slice(b"Lavf");

        let mut cursor = Cursor::new(data);
        let count = count_encoder_signatures(&mut cursor).expect("Should count");

        assert!(count >= 2, "Should detect multiple encoder signatures");
    }

    // ==========================================================================
    // REAL-WORLD TRANSCODE DETECTION SCENARIOS
    // These tests simulate common transcode scenarios
    // ==========================================================================

    #[test]
    fn test_scenario_128_to_320_transcode() {
        // COMMON SCAM: Take a 128kbps download, re-encode as "320kbps"
        // to upload as "high quality" to a torrent or streaming site.
        //
        // THE TELL: LAME still reports lowpass=16000Hz (128kbps characteristic)
        // even though the file bitrate is 320kbps.

        let data = create_lame_header_data("LAME3.99r", 16000, false);
        let header = LameHeader::extract(&data).expect("Should extract");

        // File claims to be from LAME 3.99r (good encoder)
        // But lowpass=16000 means the SOURCE was 128kbps
        assert_eq!(header.lowpass, Some(16000));

        let (suspicious, _, reason) = check_lowpass_mismatch(320, 16000);
        assert!(suspicious);
        assert!(reason.unwrap().contains("128kbps"));
    }

    #[test]
    fn test_scenario_youtube_rip() {
        // SCENARIO: Someone rips audio from YouTube (typically 128kbps AAC)
        // and re-encodes to MP3.
        //
        // YouTube's audio quality is typically equivalent to 128-192kbps.
        // A "320kbps MP3" from YouTube will still show the source limitations.

        // YouTube rip typically has ~17kHz cutoff
        let (suspicious, _, _) = check_lowpass_mismatch(320, 17000);
        assert!(suspicious, "YouTube rips should be detectable");
    }

    #[test]
    fn test_scenario_legitimate_v0_vbr() {
        // SCENARIO: Legitimate V0 VBR encoding from a CD rip.
        //
        // LAME V0 (highest VBR quality, ~245kbps average) should have
        // lowpass around 19.5-20.5kHz - perfectly legitimate.

        let (suspicious, _, _) = check_lowpass_mismatch(245, 19500);
        assert!(!suspicious, "V0 VBR from lossless source should not be flagged");
    }

    // ==========================================================================
    // HELPER FUNCTION TESTS
    // ==========================================================================

    #[test]
    fn test_find_pattern_basic() {
        let haystack = b"hello LAME3.100 world";
        let pos = find_pattern(haystack, b"LAME");
        assert_eq!(pos, Some(6));
    }

    #[test]
    fn test_find_pattern_not_found() {
        let haystack = b"hello world";
        let pos = find_pattern(haystack, b"LAME");
        assert_eq!(pos, None);
    }

    #[test]
    fn test_find_pattern_at_start() {
        let haystack = b"LAME3.100";
        let pos = find_pattern(haystack, b"LAME");
        assert_eq!(pos, Some(0));
    }

    #[test]
    fn test_lowpass_validation_bounds() {
        // Lowpass byte must be 50-220 (5kHz to 22kHz) to be valid.
        // Values outside this range are ignored (likely garbage data).
        //
        // The LAME header stores lowpass as a single byte = Hz / 100.
        // Valid range: 50-220 (representing 5000-22000 Hz)

        fn is_valid_lowpass_byte(byte: u8) -> bool {
            byte >= 50 && byte <= 220
        }

        // Valid minimum: 50 = 5000Hz
        assert!(is_valid_lowpass_byte(50), "5kHz should be valid");

        // Valid maximum: 220 = 22000Hz
        assert!(is_valid_lowpass_byte(220), "22kHz should be valid");

        // Common values
        assert!(is_valid_lowpass_byte(160), "16kHz (128kbps) should be valid");
        assert!(is_valid_lowpass_byte(185), "18.5kHz (192kbps) should be valid");
        assert!(is_valid_lowpass_byte(205), "20.5kHz (320kbps) should be valid");

        // Invalid: 49 = 4900Hz (too low - would be below human hearing range)
        assert!(!is_valid_lowpass_byte(49), "4.9kHz should be invalid");

        // Invalid: 221 = 22100Hz (too high - above Nyquist for 44.1kHz)
        assert!(!is_valid_lowpass_byte(221), "22.1kHz should be invalid");

        // Invalid: 0 (would mean no lowpass, likely garbage)
        assert!(!is_valid_lowpass_byte(0), "0 should be invalid");
    }

    // ==========================================================================
    // RE-ENCODING DETECTION TESTS
    // ==========================================================================
    //
    // These tests verify the ability to detect files that have been encoded
    // multiple times. This addresses the scenario: "what if a lossy file was
    // encoded previously at a *higher* rate?"
    //
    // Detection methods:
    // 1. Multiple LAME signatures = encoded multiple times with LAME
    // 2. Multiple Lavf signatures = processed multiple times by FFmpeg
    // 3. Mixed encoders (LAME + FFmpeg) = transcoded through different tools
    // ==========================================================================

    #[test]
    fn test_encoder_signatures_default() {
        let sigs = EncoderSignatures::default();

        assert!(sigs.lame.is_none());
        assert!(!sigs.fraunhofer);
        assert!(!sigs.itunes);
        assert!(!sigs.ffmpeg);
        assert!(!sigs.xing);
        assert_eq!(sigs.lame_count, 0);
        assert_eq!(sigs.lavf_count, 0);
        assert!(sigs.all_encoders.is_empty());
    }

    #[test]
    fn test_shows_reencoding_single_encoder() {
        // Single encoder = NOT re-encoded
        let mut sigs = EncoderSignatures::default();
        sigs.lame = Some("LAME3.100".to_string());
        sigs.lame_count = 1;

        assert!(!sigs.shows_reencoding(), "Single LAME encode should not be flagged");
        assert_eq!(sigs.unique_encoder_count(), 1);
    }

    #[test]
    fn test_shows_reencoding_multiple_lame_passes() {
        // SCENARIO: File was encoded with LAME, converted to WAV, then
        // re-encoded with LAME again (maybe at higher bitrate).
        // This should be detected!

        let mut sigs = EncoderSignatures::default();
        sigs.lame = Some("LAME3.100".to_string());
        sigs.lame_count = 2; // Two LAME headers found!

        assert!(sigs.shows_reencoding(), "Multiple LAME passes should be flagged");
    }

    #[test]
    fn test_shows_reencoding_multiple_ffmpeg_passes() {
        // SCENARIO: File was processed through FFmpeg multiple times
        // (e.g., format conversion, then later another conversion)

        let mut sigs = EncoderSignatures::default();
        sigs.ffmpeg = true;
        sigs.lavf_count = 3; // Three FFmpeg processings!

        assert!(sigs.shows_reencoding(), "Multiple FFmpeg passes should be flagged");
    }

    #[test]
    fn test_shows_reencoding_mixed_encoders() {
        // SCENARIO: File was encoded with LAME, then processed by FFmpeg
        // (common when someone transcodes: LAME → WAV → FFmpeg)

        let mut sigs = EncoderSignatures::default();
        sigs.lame = Some("LAME3.100".to_string());
        sigs.lame_count = 1;
        sigs.ffmpeg = true;
        sigs.lavf_count = 1;

        assert!(sigs.shows_reencoding(), "Mixed LAME + FFmpeg should be flagged");
        assert_eq!(sigs.unique_encoder_count(), 2);
    }

    #[test]
    fn test_encoding_chain_description_single() {
        // Single encoder = no chain to report
        let mut sigs = EncoderSignatures::default();
        sigs.lame = Some("LAME3.100".to_string());
        sigs.lame_count = 1;

        assert!(sigs.encoding_chain_description().is_none());
    }

    #[test]
    fn test_encoding_chain_description_lame_to_ffmpeg() {
        // Common transcode pattern: LAME → FFmpeg
        let mut sigs = EncoderSignatures::default();
        sigs.lame = Some("LAME3.100".to_string());
        sigs.lame_count = 1;
        sigs.ffmpeg = true;
        sigs.lavf_count = 1;

        let chain = sigs.encoding_chain_description();
        assert!(chain.is_some());
        assert!(chain.as_ref().unwrap().contains("LAME"));
        assert!(chain.as_ref().unwrap().contains("FFmpeg"));
        assert!(chain.unwrap().contains("→"), "Should show chain with arrow");
    }

    #[test]
    fn test_encoding_chain_description_multiple_lame() {
        // File encoded twice with LAME
        let mut sigs = EncoderSignatures::default();
        sigs.lame = Some("LAME3.100".to_string());
        sigs.lame_count = 3;

        let chain = sigs.encoding_chain_description();
        assert!(chain.is_some());
        assert!(chain.as_ref().unwrap().contains("LAME x3"));
    }

    #[test]
    fn test_encoding_chain_description_complex() {
        // Complex chain: LAME → FFmpeg → Fraunhofer
        let mut sigs = EncoderSignatures::default();
        sigs.lame = Some("LAME3.100".to_string());
        sigs.lame_count = 1;
        sigs.ffmpeg = true;
        sigs.lavf_count = 1;
        sigs.fraunhofer = true;

        let chain = sigs.encoding_chain_description();
        assert!(chain.is_some());
        // Should include all encoders
        let desc = chain.unwrap();
        assert!(desc.contains("LAME"));
        assert!(desc.contains("FFmpeg"));
        assert!(desc.contains("Fraunhofer"));
    }

    #[test]
    fn test_count_pattern_occurrences_none() {
        let data = b"hello world this has no encoder signatures";
        let count = count_pattern_occurrences(data, b"LAME");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_count_pattern_occurrences_single() {
        let data = b"header stuff LAME3.100 more stuff";
        let count = count_pattern_occurrences(data, b"LAME");
        assert_eq!(count, 1);
    }

    #[test]
    fn test_count_pattern_occurrences_multiple() {
        // Simulates a file that was encoded with LAME twice
        let data = b"header LAME3.99r padding LAME3.100 more LAME3.100 end";
        let count = count_pattern_occurrences(data, b"LAME");
        assert_eq!(count, 3);
    }

    #[test]
    fn test_scan_encoder_signatures_counts_lame() {
        // Create buffer with multiple LAME signatures
        let mut data = vec![0u8; 2000];
        data[100..109].copy_from_slice(b"LAME3.99r");
        data[500..509].copy_from_slice(b"LAME3.100");

        let mut cursor = Cursor::new(data);
        let sigs = scan_encoder_signatures(&mut cursor).expect("Should scan");

        assert_eq!(sigs.lame_count, 2, "Should count both LAME occurrences");
        assert!(sigs.shows_reencoding(), "Multiple LAME = re-encoding");
    }

    #[test]
    fn test_scan_encoder_signatures_counts_lavf() {
        // Create buffer with multiple Lavf signatures (FFmpeg)
        // New counting logic: takes max(Lavf, Lavc) unless both > 1
        let mut data = vec![0u8; 2000];
        data[100..104].copy_from_slice(b"Lavf");
        data[500..504].copy_from_slice(b"Lavf");
        data[800..804].copy_from_slice(b"Lavc");

        let mut cursor = Cursor::new(data);
        let sigs = scan_encoder_signatures(&mut cursor).expect("Should scan");

        // 2 Lavf, 1 Lavc -> max(2, 1) = 2 (Lavc count not > 1, so no sum)
        assert_eq!(sigs.lavf_count, 2, "Should count max of Lavf/Lavc");
        assert!(sigs.shows_reencoding(), "Multiple FFmpeg passes = re-encoding");
    }

    // ==========================================================================
    // SCENARIO: Higher bitrate re-encode detection
    // ==========================================================================
    //
    // This is the key scenario from user feedback:
    // "what if a lossy file was encoded previously at a *higher* rate?"
    //
    // Example: Someone has a 320kbps MP3, converts to WAV, then re-encodes
    // as 192kbps to "save space". The result is worse than original 192kbps
    // because it went through lossy compression twice.
    //
    // Detection: Multiple encoder signatures reveal the re-encoding chain,
    // even if we can't determine the exact original bitrate.
    // ==========================================================================

    #[test]
    fn test_scenario_320_to_wav_to_192() {
        // SCENARIO: 320kbps LAME → WAV → 192kbps FFmpeg
        // This is detectable because both encoder signatures are present

        let mut sigs = EncoderSignatures::default();
        sigs.lame = Some("LAME3.100".to_string());
        sigs.lame_count = 1;
        sigs.ffmpeg = true;
        sigs.lavf_count = 1;

        assert!(sigs.shows_reencoding());
        assert_eq!(sigs.unique_encoder_count(), 2);

        let chain = sigs.encoding_chain_description().unwrap();
        assert!(chain.contains("LAME → FFmpeg"));
    }

    #[test]
    fn test_scenario_multiple_lame_encodes() {
        // SCENARIO: File encoded multiple times with LAME
        // 320kbps → WAV → 256kbps → WAV → 192kbps
        // Each LAME pass leaves a header

        let mut sigs = EncoderSignatures::default();
        sigs.lame = Some("LAME3.100".to_string());
        sigs.lame_count = 3; // Three LAME headers!

        assert!(sigs.shows_reencoding());
        assert_eq!(sigs.unique_encoder_count(), 1); // Same encoder...
        // ...but multiple passes!

        let chain = sigs.encoding_chain_description().unwrap();
        assert!(chain.contains("LAME x3"));
    }
}
