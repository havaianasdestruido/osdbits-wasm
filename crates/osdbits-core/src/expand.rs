//! Safe, bounds-checked port of `ps2expand.c` and `tools/extract-res.py` (`ROMDIR` + OSDSYS LZSS)
//! for validating and unpacking optional user-supplied resource containers in memory without panics.

use core::fmt;

/// Maximum decompressed resource size accepted (`16 MiB` safety cap against decompression bombs).
pub const MAX_EXPANDED_SIZE: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceError {
    TruncatedHeader,
    OutputTooLarge(u32),
    UnexpectedEof,
    InvalidBackReference { offset: usize, available: usize },
    NoRomdirFound,
    MalformedRomdir,
}

impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TruncatedHeader => write!(f, "compressed stream shorter than 4-byte size header"),
            Self::OutputTooLarge(sz) => {
                write!(f, "declared expanded size {sz} exceeds safety limit")
            }
            Self::InvalidBackReference { offset, available } => write!(
                f,
                "LZ back-reference offset {offset} exceeds produced output {available}"
            ),
            Self::UnexpectedEof => write!(f, "unexpected end of compressed stream"),
            Self::NoRomdirFound => write!(f, "no RESET/ROMDIR header found in image"),
            Self::MalformedRomdir => write!(f, "malformed ROMDIR entry table"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RomdirEntry {
    pub name: String,
    pub offset: usize,
    pub size: usize,
    pub ext_size: u16,
}

/// Decompresses an OSDSYS `expand` stream (`ps2expand.c`).
pub fn expand_ps2_lz(src: &[u8]) -> Result<Vec<u8>, ResourceError> {
    if src.len() < 4 {
        return Err(ResourceError::TruncatedHeader);
    }
    let declared_size = u32::from_le_bytes([src[0], src[1], src[2], src[3]]);
    let target_len = declared_size as usize;
    if target_len > MAX_EXPANDED_SIZE {
        return Err(ResourceError::OutputTooLarge(declared_size));
    }

    let mut dst = Vec::with_capacity(target_len.min(65536));
    let mut p = 4_usize;
    let mut ndesc = 0_u32;
    let mut desc = 0_u32;
    let mut shift = 14_u32;
    let mut mask = 0x3FFF_usize;

    while dst.len() < target_len {
        if ndesc == 0 {
            if p + 4 > src.len() {
                return Err(ResourceError::UnexpectedEof);
            }
            desc = u32::from_be_bytes([src[p], src[p + 1], src[p + 2], src[p + 3]]);
            p += 4;
            let n = desc & 3;
            shift = 14 - n;
            mask = (0x3FFF_usize) >> n;
            ndesc = 30;
        }

        if p >= src.len() {
            return Err(ResourceError::UnexpectedEof);
        }
        let b = src[p];
        p += 1;

        if (desc & 0x8000_0000) != 0 {
            if p >= src.len() {
                return Err(ResourceError::UnexpectedEof);
            }
            let h = (usize::from(b) << 8) | usize::from(src[p]);
            p += 1;
            let back = 1 + (h & mask);
            let copy_len = 3 + (h >> shift);
            if back > dst.len() {
                return Err(ResourceError::InvalidBackReference {
                    offset: back,
                    available: dst.len(),
                });
            }
            for _ in 0..copy_len {
                if dst.len() >= target_len {
                    break;
                }
                let byte = dst[dst.len() - back];
                dst.push(byte);
            }
        } else {
            dst.push(b);
        }

        desc <<= 1;
        ndesc -= 1;
    }

    Ok(dst)
}

/// Encodes arbitrary bytes into a valid OSDSYS `expand` stream (using literal + RLE blocks)
/// for deterministic round-trip unit tests without shipping any proprietary data.
pub fn encode_ps2_lz_clean(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + payload.len() + (payload.len() / 30 + 1) * 4);
    let len_u32 = payload.len() as u32;
    out.extend_from_slice(&len_u32.to_le_bytes());

    let mut i = 0;
    while i < payload.len() {
        let block_pos = out.len();
        out.extend_from_slice(&[0, 0, 0, 0]);
        let mut desc = 0_u32; // low 2 bits = n = 0 => shift = 14, mask = 0x3fff

        for step in 0..30 {
            if i >= payload.len() {
                break;
            }
            // Check simple match of length 3..=6 against up to 256 bytes back
            let mut best_back = 0_usize;
            let mut best_len = 0_usize;
            let max_back = i.min(256);
            for back in 1..=max_back {
                let mut mlen = 0;
                while mlen < 6
                    && i + mlen < payload.len()
                    && payload[i + mlen] == payload[i + mlen - back]
                {
                    mlen += 1;
                }
                if mlen >= 3 && mlen > best_len {
                    best_len = mlen;
                    best_back = back;
                    if best_len == 6 {
                        break;
                    }
                }
            }

            if best_len >= 3 {
                desc |= 1_u32 << (31 - step);
                let h = (((best_len - 3) & 0x3) << 14) | ((best_back - 1) & 0x3FFF);
                out.push(((h >> 8) & 0xFF) as u8);
                out.push((h & 0xFF) as u8);
                i += best_len;
            } else {
                out.push(payload[i]);
                i += 1;
            }
        }

        let desc_bytes = desc.to_be_bytes();
        out[block_pos..block_pos + 4].copy_from_slice(&desc_bytes);
    }

    out
}

/// Locates the `RESET\0\0\0\0\0` + `ROMDIR` table offset inside a container (`tools/extract-res.py`).
pub fn find_romdir(data: &[u8]) -> Result<usize, ResourceError> {
    if data.len() < 32 {
        return Err(ResourceError::NoRomdirFound);
    }
    let needle = b"RESET\0\0\0\0\0";
    let mut pos = 0;
    while pos + 22 <= data.len() {
        if &data[pos..pos + 10] == needle && &data[pos + 16..pos + 22] == b"ROMDIR" {
            return Ok(pos);
        }
        pos += 1;
    }
    Err(ResourceError::NoRomdirFound)
}

/// Parses a `ROMDIR` directory table at `base`, returning validated entries.
pub fn parse_romdir(data: &[u8], base: usize) -> Result<Vec<RomdirEntry>, ResourceError> {
    if base >= data.len() {
        return Err(ResourceError::MalformedRomdir);
    }
    let mut entries = Vec::new();
    let mut offset = 0_usize;
    let mut pos = base;

    loop {
        if pos + 16 > data.len() {
            return Err(ResourceError::MalformedRomdir);
        }
        let raw_name = &data[pos..pos + 10];
        let name_end = raw_name
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(raw_name.len());
        if name_end == 0 {
            break;
        }
        if !raw_name[..name_end]
            .iter()
            .all(|&b| b.is_ascii_graphic() || b == b'-' || b == b'_')
        {
            return Err(ResourceError::MalformedRomdir);
        }
        let name = String::from_utf8_lossy(&raw_name[..name_end]).into_owned();
        let ext_size = u16::from_le_bytes([data[pos + 10], data[pos + 11]]);
        let size = u32::from_le_bytes([
            data[pos + 12],
            data[pos + 13],
            data[pos + 14],
            data[pos + 15],
        ]) as usize;

        if offset.saturating_add(size) > data.len() {
            return Err(ResourceError::MalformedRomdir);
        }

        entries.push(RomdirEntry {
            name,
            offset,
            size,
            ext_size,
        });

        offset = offset.saturating_add((size + 0xF) & !0xF);
        pos += 16;
        if entries.len() > 4096 {
            return Err(ResourceError::MalformedRomdir);
        }
    }

    if entries.is_empty() {
        return Err(ResourceError::MalformedRomdir);
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_roundtrips_clean_synthetic_payloads() {
        let mut sample = Vec::with_capacity(2048);
        for i in 0..2048 {
            sample.push(((i / 5) ^ (i & 7)) as u8);
        }
        let compressed = encode_ps2_lz_clean(&sample);
        assert!(compressed.len() < sample.len());
        let expanded = expand_ps2_lz(&compressed).expect("expand should succeed");
        assert_eq!(expanded, sample);
    }

    #[test]
    fn expand_rejects_malformed_truncated_and_bomb_inputs_safely() {
        assert_eq!(expand_ps2_lz(&[1, 2]), Err(ResourceError::TruncatedHeader));
        // Bomb header (32 MiB > 16 MiB cap)
        let bomb = 33_554_432_u32.to_le_bytes();
        assert_eq!(
            expand_ps2_lz(&bomb),
            Err(ResourceError::OutputTooLarge(33_554_432))
        );
        // Invalid back-reference at byte 0 (`desc = 0x8000_0000` -> first token is backref when dst is empty!)
        let mut bad_backref = Vec::new();
        bad_backref.extend_from_slice(&8_u32.to_le_bytes());
        bad_backref.extend_from_slice(&0x8000_0000_u32.to_be_bytes());
        bad_backref.extend_from_slice(&[0x00, 0x00]);
        assert!(matches!(
            expand_ps2_lz(&bad_backref),
            Err(ResourceError::InvalidBackReference { .. })
        ));
    }

    #[test]
    fn romdir_parser_handles_synthetic_archive_and_rejects_garbage() {
        // Build a tiny synthetic clean-room ROMDIR container: RESET (0 bytes), ROMDIR (64 bytes), TEST (5 bytes)
        let mut img = vec![0_u8; 128];
        // Entry 0 at offset 0: "RESET" (size = 0)
        img[0..5].copy_from_slice(b"RESET");
        // Entry 1 at offset 16: "ROMDIR" (size = 64)
        img[16..22].copy_from_slice(b"ROMDIR");
        img[28..32].copy_from_slice(&64_u32.to_le_bytes());
        // Entry 2 at offset 32: "SYNTH" (size = 5) -> lives at aligned offset 64!
        img[32..37].copy_from_slice(b"SYNTH");
        img[44..48].copy_from_slice(&5_u32.to_le_bytes());
        // Entry 3 at offset 48: terminator (all zeros)
        img[64..69].copy_from_slice(b"HELLO");

        let base = find_romdir(&img).expect("should find ROMDIR");
        assert_eq!(base, 0);
        let entries = parse_romdir(&img, base).expect("should parse ROMDIR");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[2].name, "SYNTH");
        assert_eq!(entries[2].offset, 64);
        assert_eq!(
            &img[entries[2].offset..entries[2].offset + entries[2].size],
            b"HELLO"
        );

        assert_eq!(
            find_romdir(b"not a bios"),
            Err(ResourceError::NoRomdirFound)
        );
    }
}
