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
pub fn scan_encoder_signatures<R: Read + Seek>(reader: &mut R) -> io::Result<EncoderSignatures> {
    let mut sigs = EncoderSignatures::default();

    // Read first 64KB for signature scanning
    reader.seek(SeekFrom::Start(0))?;
    let mut buf = vec![0u8; 65536];
    let bytes_read = reader.read(&mut buf)?;
    buf.truncate(bytes_read);

    // Convert to string for pattern matching (lossy is fine, we're looking for ASCII)
    let text = String::from_utf8_lossy(&buf);

    // LAME - extract version
    if let Some(pos) = find_pattern(&buf, b"LAME") {
        let end = (pos + 20).min(buf.len());
        if let Ok(s) = std::str::from_utf8(&buf[pos..end]) {
            let version: String = s.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
                .collect();
            if !version.is_empty() {
                sigs.lame = Some(version);
            }
        }
    }

    // Fraunhofer
    if text.contains("Fraunhofer") || text.contains("FhG") {
        sigs.fraunhofer = true;
    }

    // iTunes
    if text.contains("iTunes") || text.contains("Lavf") && text.contains("Apple") {
        sigs.itunes = true;
    }

    // FFmpeg/Lavf
    if text.contains("Lavf") || text.contains("libmp3lame") {
        sigs.ffmpeg = true;
    }

    // Xing (sometimes standalone)
    if find_pattern(&buf, b"Xing").is_some() || find_pattern(&buf, b"Info").is_some() {
        sigs.xing = true;
    }

    Ok(sigs)
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

    #[test]
    fn test_lowpass_mismatch_detection() {
        // 320kbps with 16kHz lowpass = definitely a transcode from 128kbps
        let (suspicious, _, reason) = check_lowpass_mismatch(320, 16000);
        assert!(suspicious);
        assert!(reason.unwrap().contains("128kbps"));

        // 320kbps with 20kHz lowpass = legit
        let (suspicious, _, _) = check_lowpass_mismatch(320, 20000);
        assert!(!suspicious);
    }
}
