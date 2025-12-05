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
    /// Structure:
    /// - Xing/Info header at offset 0x24 (stereo) or 0x15 (mono) from frame start
    /// - LAME string follows Xing data
    /// - Lowpass is at LAME offset + 11, stored as Hz/100
    pub fn extract(data: &[u8]) -> Option<Self> {
        let mut header = LameHeader::default();

        // Look for Xing or Info header
        let xing_pos = find_pattern(data, b"Xing");
        let info_pos = find_pattern(data, b"Info");

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
            if pos + 8 <= data.len() {
                let flags = u32::from_be_bytes([
                    data[pos + 4],
                    data[pos + 5],
                    data[pos + 6],
                    data[pos + 7],
                ]);

                let mut offset = pos + 8;

                // Frames flag (bit 0)
                if flags & 0x01 != 0 && offset + 4 <= data.len() {
                    header.total_frames = Some(u32::from_be_bytes([
                        data[offset],
                        data[offset + 1],
                        data[offset + 2],
                        data[offset + 3],
                    ]));
                    offset += 4;
                }

                // Bytes flag (bit 1)
                if flags & 0x02 != 0 && offset + 4 <= data.len() {
                    header.total_bytes = Some(u32::from_be_bytes([
                        data[offset],
                        data[offset + 1],
                        data[offset + 2],
                        data[offset + 3],
                    ]));
                    offset += 4;
                }

                // TOC flag (bit 2) - skip 100 bytes
                if flags & 0x04 != 0 {
                    offset += 100;
                }

                // Quality flag (bit 3) - skip 4 bytes
                if flags & 0x08 != 0 {
                    let _ = offset + 4; // Quality indicator, not used further
                }

                // LAME header should be right after Xing data
                // But we'll also do a broader search
            }
        }

        // Look for LAME encoder string
        if let Some(lame_pos) = find_pattern(data, b"LAME") {
            // Extract version string (e.g., "LAME3.100" or "LAME3.99r")
            let version_end = (lame_pos + 9).min(data.len());
            if let Ok(version) = std::str::from_utf8(&data[lame_pos..version_end]) {
                header.encoder = version.trim_end_matches('\0').to_string();
            }

            // Lowpass filter is at offset 11 from LAME string
            // Stored as Hz/100 (so 160 = 16000 Hz)
            if lame_pos + 11 < data.len() {
                let lowpass_byte = data[lame_pos + 11];
                if lowpass_byte > 0 {
                    header.lowpass = Some(lowpass_byte as u32 * 100);
                }
            }

            // VBR method and quality are in the byte at offset 9
            if lame_pos + 9 < data.len() {
                let info_byte = data[lame_pos + 9];
                header.vbr_method = Some(info_byte & 0x0F);
                header.quality = Some((info_byte >> 4) & 0x0F);
            }

            return Some(header);
        }

        // If we found Xing/Info but no LAME, still return what we have
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
    match bitrate {
        320 => 20500,
        256 => 20000,
        224 => 19500,
        192 => 18500,
        160 => 17500,
        128 => 16000,
        112 => 15500,
        96 => 15000,
        _ => 14000,
    }
}

/// Check if lowpass frequency suggests transcoding
/// Returns (is_suspicious, expected_lowpass, reason)
pub fn check_lowpass_mismatch(bitrate: u32, actual_lowpass: u32) -> (bool, u32, Option<String>) {
    let expected = expected_lowpass_for_bitrate(bitrate);

    // If actual lowpass is significantly lower than expected, it's suspicious
    let threshold = match bitrate {
        320 => 19000,  // 320kbps should have at least 19kHz
        256 => 18500,  // 256kbps should have at least 18.5kHz
        224 => 18000,
        192 => 17500,
        160 => 16500,
        _ => 0, // Don't flag lower bitrates
    };

    if threshold > 0 && actual_lowpass > 0 && actual_lowpass < threshold {
        let likely_source = match actual_lowpass {
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
