//! PS2 ROMDIR container parser and `ps2expand.c` LZSS `expand()` decompressor.
//!
//! Directly ported from `ps2expand.c` and `osdbits/tools/extract-res.py`.
//! Implements strict bounds checking so malformed or adversarial inputs never panic.

use std::fmt;

/// Maximum decompressed size permitted for a single ROMDIR resource (16 MiB safety cap).
pub const MAX_EXPAND_SIZE: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceError {
    InputTooShort,
    InvalidUncompressedLength(u32),
    UnexpectedEofInStream,
    BackReferenceOutOfBounds { offset: usize, dst_len: usize },
    OutputOverflow { expected: usize, actual: usize },
    RomDirNotFound,
    ContainerNotFound(String),
}

impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooShort => write!(f, "Compressed payload is shorter than 4-byte header"),
            Self::InvalidUncompressedLength(n) => {
                write!(f, "Uncompressed length {n} exceeds safety limit")
            }
            Self::UnexpectedEofInStream => {
                write!(
                    f,
                    "Compressed LZSS stream ended before reaching declared size"
                )
            }
            Self::BackReferenceOutOfBounds { offset, dst_len } => {
                write!(
                    f,
                    "LZSS back-reference offset -{offset} out of bounds at dst_len={dst_len}"
                )
            }
            Self::OutputOverflow { expected, actual } => {
                write!(
                    f,
                    "LZSS output length mismatch (expected {expected}, got {actual})"
                )
            }
            Self::RomDirNotFound => {
                write!(f, "No valid PS2 ROMDIR header (RESET/ROMDIR/EXTINFO) found")
            }
            Self::ContainerNotFound(name) => write!(f, "ROMDIR entry '{name}' not found"),
        }
    }
}

/// Exact port of `Expand()` from `ps2expand.c` (real `0x200c90` in OSDSYS) and `tools/extract-res.py`.
///
/// Format:
/// - Bytes `0..4`: little-endian `u32` uncompressed size (`total_len`).
/// - Followed by blocks controlled by a 32-bit control word (`cmd`), preceded by a `shift` byte (`b1`).
///   Each block runs 30 flag bits (`i = 29` down to `0` as `cmd <<= 1` and checking sign bit `(cmd as i32) < 0`):
///   - `bit == 0` (`(cmd as i32) >= 0`): 1 literal byte copied to `dst`.
///   - `bit == 1` (`(cmd as i32) < 0`): 16-bit little-endian token `t`:
///     - `count = (t >> b1) + 3`
///     - `back_offset = (t & ~(0xffff << b1)) + 1`
///     - copies `count` bytes from `dst[dst.len() - back_offset ..]`.
pub fn expand_lzss(src: &[u8]) -> Result<Vec<u8>, ResourceError> {
    if src.len() < 4 {
        return Err(ResourceError::InputTooShort);
    }
    let total_len = u32::from_le_bytes([src[0], src[1], src[2], src[3]]) as usize;
    if total_len > MAX_EXPAND_SIZE {
        return Err(ResourceError::InvalidUncompressedLength(total_len as u32));
    }
    if total_len == 0 {
        return Ok(Vec::new());
    }

    let mut dst = Vec::with_capacity(total_len);
    let mut sp = 4usize;

    while dst.len() < total_len {
        if sp + 5 > src.len() {
            return Err(ResourceError::UnexpectedEofInStream);
        }
        let b1 = src[sp] as u32;
        if b1 >= 16 {
            return Err(ResourceError::UnexpectedEofInStream);
        }
        let mut cmd = u32::from_le_bytes([src[sp + 1], src[sp + 2], src[sp + 3], src[sp + 4]]);
        sp += 5;

        let mask: u32 = !((0xffffu32) << b1);

        for _ in (0..30).rev() {
            if dst.len() >= total_len {
                break;
            }
            if (cmd as i32) >= 0 {
                if sp >= src.len() {
                    return Err(ResourceError::UnexpectedEofInStream);
                }
                dst.push(src[sp]);
                sp += 1;
            } else {
                if sp + 2 > src.len() {
                    return Err(ResourceError::UnexpectedEofInStream);
                }
                let t = u16::from_le_bytes([src[sp], src[sp + 1]]) as u32;
                sp += 2;
                let count = ((t >> b1) + 3) as usize;
                let back_offset = ((t & mask) + 1) as usize;

                if back_offset > dst.len() {
                    return Err(ResourceError::BackReferenceOutOfBounds {
                        offset: back_offset,
                        dst_len: dst.len(),
                    });
                }
                if dst.len() + count > total_len {
                    return Err(ResourceError::OutputOverflow {
                        expected: total_len,
                        actual: dst.len() + count,
                    });
                }
                let start = dst.len() - back_offset;
                for k in 0..count {
                    let b = dst[start + k];
                    dst.push(b);
                }
            }
            cmd <<= 1;
        }
    }

    Ok(dst)
}

/// Encodes arbitrary byte slices into a valid `ps2expand.c` LZSS stream (using literal blocks or back-references)
/// for round-trip unit and property testing.
pub fn compress_lzss(data: &[u8], shift: u8) -> Vec<u8> {
    let shift = shift.min(14);
    let mut out = Vec::with_capacity(4 + data.len() + (data.len() / 30 + 1) * 5);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());

    let max_offset = (1usize << shift).min(4096);
    let max_len = ((1usize << (16 - shift)) - 1) + 2;

    let mut pos = 0usize;
    while pos < data.len() {
        out.push(shift);
        let cmd_pos = out.len();
        out.extend_from_slice(&0u32.to_le_bytes());
        let mut cmd: u32 = 0;

        for bit_idx in 0..30 {
            if pos >= data.len() {
                break;
            }
            // Try to find a back-reference of at least 3 bytes
            let search_start = pos.saturating_sub(max_offset);
            let mut best_len = 0usize;
            let mut best_off = 0usize;
            let limit = (data.len() - pos).min(max_len);

            if limit >= 3 && pos > search_start {
                for cand in search_start..pos {
                    let mut mlen = 0;
                    while mlen < limit && data[cand + mlen] == data[pos + mlen] {
                        mlen += 1;
                    }
                    if mlen >= 3 && mlen > best_len {
                        best_len = mlen;
                        best_off = pos - cand;
                        if best_len == limit {
                            break;
                        }
                    }
                }
            }

            let bit_shift = 31 - bit_idx;
            if best_len >= 3 {
                cmd |= 1u32 << bit_shift;
                let t = (((best_len - 3) as u16) << shift) | ((best_off - 1) as u16);
                out.extend_from_slice(&t.to_le_bytes());
                pos += best_len;
            } else {
                out.push(data[pos]);
                pos += 1;
            }
        }

        let cmd_bytes = cmd.to_le_bytes();
        out[cmd_pos..cmd_pos + 4].copy_from_slice(&cmd_bytes);
    }

    out
}

/// A parsed 16-byte entry in a PS2 ROMDIR table (`docs/rom-survey.md` §0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomDirEntry {
    pub name: String,
    pub ext_info_size: u16,
    pub file_size: u32,
    pub data_offset: usize,
}

/// Scans `blob` on 16-byte boundaries for a valid PS2 ROMDIR table (`RESET` followed by `ROMDIR`
/// or a nested container starting with `RESET`/`ROMDIR`/`EXTINFO`) and returns all entries with
/// validated slice bounds.
pub fn parse_romdir(blob: &[u8]) -> Result<Vec<RomDirEntry>, ResourceError> {
    if blob.len() < 48 {
        return Err(ResourceError::RomDirNotFound);
    }

    // Locate the ROMDIR table by scanning 16-byte aligned offsets for "RESET\0" followed by "ROMDIR\0"
    let mut romdir_off: Option<usize> = None;
    let mut off = 0usize;
    while off + 48 <= blob.len() {
        if &blob[off..off + 6] == b"RESET\0" && &blob[off + 16..off + 23] == b"ROMDIR\0" {
            romdir_off = Some(off);
            break;
        }
        off += 16;
    }

    let table_start = romdir_off.ok_or(ResourceError::RomDirNotFound)?;
    let mut entries = Vec::new();
    let mut cur = table_start;
    let mut data_off = 0usize;

    while cur + 16 <= blob.len() {
        let raw_name = &blob[cur..cur + 10];
        if raw_name[0] == 0 {
            break;
        }
        let nul_pos = raw_name.iter().position(|&b| b == 0).unwrap_or(10);
        let name = String::from_utf8_lossy(&raw_name[..nul_pos]).into_owned();
        let ext_info_size = u16::from_le_bytes([blob[cur + 10], blob[cur + 11]]);
        let file_size = u32::from_le_bytes([
            blob[cur + 12],
            blob[cur + 13],
            blob[cur + 14],
            blob[cur + 15],
        ]);

        entries.push(RomDirEntry {
            name,
            ext_info_size,
            file_size,
            data_offset: data_off,
        });

        // Each file's start is the running sum of previous file_size values rounded up to 16
        let aligned_size = ((file_size as usize) + 15) & !15;
        data_off = data_off.saturating_add(aligned_size);
        cur += 16;
    }

    if entries.is_empty() {
        return Err(ResourceError::RomDirNotFound);
    }

    Ok(entries)
}

/// Extracts and LZSS-decompresses all non-header entries inside a ROMDIR container (`TEXIMAGE`, `FNTIMAGE`, `SNDIMAGE`, `ICOIMAGE`).
pub fn extract_romdir_container(
    container_blob: &[u8],
) -> Result<Vec<(String, Vec<u8>)>, ResourceError> {
    let entries = parse_romdir(container_blob)?;
    let mut out = Vec::new();

    for entry in entries {
        if entry.name == "RESET"
            || entry.name == "ROMDIR"
            || entry.name == "EXTINFO"
            || entry.name == "-"
            || entry.file_size == 0
        {
            continue;
        }
        let end = entry.data_offset.saturating_add(entry.file_size as usize);
        if end > container_blob.len() {
            return Err(ResourceError::UnexpectedEofInStream);
        }
        let slice = &container_blob[entry.data_offset..end];
        let expanded = expand_lzss(slice)?;
        out.push((entry.name, expanded));
    }

    Ok(out)
}

/// Builds a synthetic ROMDIR container from `(name, uncompressed_payload)` pairs for testing and export.
///
/// Layout matches the retail resource containers (`TEXIMAGE`, `FNTIMAGE`, ...) and the boot ROM
/// itself: the `RESET` entry comes first and its size covers the directory table (which is
/// appended to RESET, see `docs/rom-survey.md` §0), `ROMDIR` and `EXTINFO` follow with zero
/// sizes, then the resource entries. All file data is packed after the table, 16-byte aligned,
/// in entry order - which is exactly the running-offset arithmetic `parse_romdir` performs and
/// what `tools/extract-res.py` reads out of a real image.
pub fn build_synthetic_romdir_container(files: &[(&str, &[u8])]) -> Vec<u8> {
    let compressed_files: Vec<(String, Vec<u8>)> = files
        .iter()
        .map(|(name, data)| ((*name).to_string(), compress_lzss(data, 11)))
        .collect();

    let entry_count = compressed_files.len() + 4; // RESET, ROMDIR, EXTINFO, resources, terminator
    let table_len = (entry_count * 16) as u32;

    let write_entry = |buf: &mut Vec<u8>, name: &str, size: u32| {
        let mut name_bytes = [0u8; 10];
        let bytes = name.as_bytes();
        let copy_len = bytes.len().min(10);
        name_bytes[..copy_len].copy_from_slice(&bytes[..copy_len]);
        buf.extend_from_slice(&name_bytes);
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&size.to_le_bytes());
    };

    let mut blob = Vec::new();
    write_entry(&mut blob, "RESET", table_len);
    write_entry(&mut blob, "ROMDIR", 0);
    write_entry(&mut blob, "EXTINFO", 0);
    for (name, comp) in &compressed_files {
        write_entry(&mut blob, name, comp.len() as u32);
    }
    blob.extend_from_slice(&[0u8; 16]);
    debug_assert_eq!(blob.len(), table_len as usize);

    for (_, comp) in &compressed_files {
        blob.extend_from_slice(comp);
        while blob.len() % 16 != 0 {
            blob.push(0);
        }
    }
    blob
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::SplitMix64;

    #[test]
    fn test_lzss_roundtrip_structured_and_repetitive() {
        let mut original = Vec::new();
        for i in 0..1024 {
            original.push(((i / 8) & 0xff) as u8);
            original.push((i & 0x07) as u8);
        }
        let compressed = compress_lzss(&original, 11);
        let decompressed = expand_lzss(&compressed).expect("expand_lzss should succeed");
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_lzss_property_fuzz_roundtrip_and_malformed_safety() {
        let mut rng = SplitMix64::new(0xCAFE_BABE_1337_4242);
        // 1. Property test: 50 random payloads of varying lengths and shifts round-trip bit-exactly
        for _ in 0..50 {
            let len = (rng.next_u32() as usize) % 600;
            let shift = ((rng.next_u32() % 12) + 2) as u8;
            let mut payload = vec![0u8; len];
            for b in &mut payload {
                // Bias values so both literals and back-references trigger
                *b = (rng.next_u32() & 0x0F) as u8;
            }
            let comp = compress_lzss(&payload, shift);
            let decomp = expand_lzss(&comp).expect("roundtrip expand must succeed");
            assert_eq!(decomp, payload);
        }

        // 2. Fuzz test: 200 arbitrary malformed byte slices never panic
        for _ in 0..200 {
            let len = (rng.next_u32() as usize) % 128;
            let mut garbage = vec![0u8; len];
            for b in &mut garbage {
                *b = (rng.next_u32() & 0xFF) as u8;
            }
            // Force a small total_len header sometimes so inner loop executes on random bytes
            if len >= 4 && (rng.next_u32() & 1) == 0 {
                let fake_len = (rng.next_u32() % 256).to_le_bytes();
                garbage[0..4].copy_from_slice(&fake_len);
            }
            let _ = expand_lzss(&garbage);
            let _ = parse_romdir(&garbage);
        }
    }

    #[test]
    fn test_synthetic_romdir_container_roundtrip() {
        let tex_a = vec![0xAAu8; 256];
        let tex_b = (0..512).map(|i| (i & 0xff) as u8).collect::<Vec<u8>>();
        let container =
            build_synthetic_romdir_container(&[("TEXOFOG0", &tex_a), ("TEXOWAL0", &tex_b)]);
        let extracted = extract_romdir_container(&container).expect("container should parse");
        assert_eq!(extracted.len(), 2);
        assert_eq!(extracted[0].0, "TEXOFOG0");
        assert_eq!(extracted[0].1, tex_a);
        assert_eq!(extracted[1].0, "TEXOWAL0");
        assert_eq!(extracted[1].1, tex_b);
    }
}
