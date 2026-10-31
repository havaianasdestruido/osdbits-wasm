//! Safe, bounds-checked PlayStation 2 `ROMDIR` + `expand()` LZ resource parser,
//! PSMCT16/PSMCT32 pixel decoder, and pure-Rust FIPS 180-4 SHA-256 integrity hasher (`P02-003`).
//!
//! Designed for optional local user-imported BIOS / `TEXIMAGE` files. Never panics on
//! arbitrary, truncated, corrupt, or malicious inputs.

use crate::procedural_assets::RgbaTexture;
use crate::render_list::TextureSlot;

pub const MAX_IMPORT_INPUT_BYTES: usize = 8 * 1024 * 1024; // 8 MiB max container/BIOS size
pub const MAX_EXPANDED_SUBFILE_BYTES: usize = 1024 * 1024; // 1 MiB max single expanded resource
pub const MAX_ROMDIR_ENTRIES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportError {
    InputEmpty,
    InputTooLarge {
        len: usize,
        max: usize,
    },
    RomdirNotFound,
    RomdirTruncated,
    RomdirTooManyEntries,
    EntryOutOfBounds {
        name: String,
        offset: usize,
        size: usize,
    },
    ExpandHeaderTruncated,
    ExpandSizeTooLarge {
        declared: usize,
        max: usize,
    },
    ExpandStreamTruncated,
    ExpandInvalidBackref {
        offset: usize,
        available: usize,
    },
    NoRecognizedTextures,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RomdirEntry {
    pub name: String,
    pub offset: usize,
    pub size: usize,
}

#[derive(Clone, Debug)]
pub struct ValidatedImportBundle {
    pub sha256_hex: String,
    pub fnv1a64: u64,
    pub entry_count: usize,
    pub textures: Vec<RgbaTexture>,
}

/// Pure-Rust FIPS 180-4 SHA-256 digest (`[u8; 32]`).
#[must_use]
pub fn sha256_digest(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let process_block = |state: &mut [u32; 8], block: &[u8; 64]| {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = *state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(hh);
    };

    let (blocks, rem) = data.as_chunks::<64>();
    for block in blocks {
        process_block(&mut h, block);
    }
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut tail = [0u8; 128];
    tail[..rem.len()].copy_from_slice(rem);
    tail[rem.len()] = 0x80;
    let total_tail = if rem.len() < 56 { 64 } else { 128 };
    tail[total_tail - 8..total_tail].copy_from_slice(&bit_len.to_be_bytes());

    let mut first = [0u8; 64];
    first.copy_from_slice(&tail[..64]);
    process_block(&mut h, &first);
    if total_tail == 128 {
        let mut second = [0u8; 64];
        second.copy_from_slice(&tail[64..128]);
        process_block(&mut h, &second);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = sha256_digest(data);
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

#[must_use]
pub fn fnv1a64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Bounds-checked port of `ps2expand.c` (`Expand()`).
pub fn ps2_expand(src: &[u8], max_out: usize) -> Result<Vec<u8>, ImportError> {
    if src.len() < 4 {
        return Err(ImportError::ExpandHeaderTruncated);
    }
    let declared = u32::from_le_bytes([src[0], src[1], src[2], src[3]]) as usize;
    if declared > max_out {
        return Err(ImportError::ExpandSizeTooLarge {
            declared,
            max: max_out,
        });
    }
    let mut dst = Vec::with_capacity(declared);
    let mut p = 4usize;
    let mut ndesc = 0u32;
    let mut desc = 0u32;
    let mut shift = 14u32;
    let mut mask = 0x3fff_u16;

    while dst.len() < declared {
        if ndesc == 0 {
            if p + 4 > src.len() {
                return Err(ImportError::ExpandStreamTruncated);
            }
            desc = u32::from_be_bytes([src[p], src[p + 1], src[p + 2], src[p + 3]]);
            p += 4;
            let n = desc & 3;
            shift = 14 - n;
            mask = 0x3fff >> n;
            ndesc = 30;
        }
        if p >= src.len() {
            return Err(ImportError::ExpandStreamTruncated);
        }
        let b = src[p];
        p += 1;
        if (desc & 0x8000_0000) != 0 {
            if p >= src.len() {
                return Err(ImportError::ExpandStreamTruncated);
            }
            let h = (u16::from(b) << 8) | u16::from(src[p]);
            p += 1;
            let back = 1 + usize::from(h & mask);
            if back > dst.len() {
                return Err(ImportError::ExpandInvalidBackref {
                    offset: back,
                    available: dst.len(),
                });
            }
            let copy_len = 3 + usize::from(h >> shift);
            for _ in 0..copy_len {
                if dst.len() >= declared {
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

/// Clean-room encoder producing valid `ps2expand`-compatible streams (for deterministic tests).
#[must_use]
pub fn ps2_encode_literals(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + raw.len() + (raw.len() / 30 + 1) * 4);
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    for chunk in raw.chunks(30) {
        // Descriptor = 0 -> n = 0, 30 literal flags (MSB 0)
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(chunk);
    }
    out
}

/// Locate a `ROMDIR` directory table (`RESET\0\0\0\0\0` at `+0`, `ROMDIR` at `+16`).
pub fn find_romdir_offset(data: &[u8]) -> Result<usize, ImportError> {
    if data.len() < 32 {
        return Err(ImportError::RomdirNotFound);
    }
    const RESET: &[u8; 10] = b"RESET\0\0\0\0\0";
    const ROMDIR: &[u8; 6] = b"ROMDIR";
    let limit = data.len() - 32;
    let mut pos = 0usize;
    while pos <= limit {
        if &data[pos..pos + 10] == RESET && &data[pos + 16..pos + 22] == ROMDIR {
            return Ok(pos);
        }
        pos += 1;
    }
    Err(ImportError::RomdirNotFound)
}

/// Parse a `ROMDIR` directory table and validate every entry lies within `data`.
pub fn parse_romdir(data: &[u8], base: usize) -> Result<Vec<RomdirEntry>, ImportError> {
    let mut entries = Vec::new();
    let mut pos = base;
    let mut data_offset = 0usize;

    loop {
        if pos + 16 > data.len() {
            return Err(ImportError::RomdirTruncated);
        }
        let raw_name = &data[pos..pos + 10];
        let name_end = raw_name.iter().position(|&c| c == 0).unwrap_or(10);
        if name_end == 0 {
            break;
        }
        if entries.len() >= MAX_ROMDIR_ENTRIES {
            return Err(ImportError::RomdirTooManyEntries);
        }
        let name = String::from_utf8_lossy(&raw_name[..name_end]).into_owned();
        let size = u32::from_le_bytes([
            data[pos + 12],
            data[pos + 13],
            data[pos + 14],
            data[pos + 15],
        ]) as usize;

        let end = data_offset
            .checked_add(size)
            .ok_or_else(|| ImportError::EntryOutOfBounds {
                name: name.clone(),
                offset: data_offset,
                size,
            })?;
        if end > data.len() {
            return Err(ImportError::EntryOutOfBounds {
                name,
                offset: data_offset,
                size,
            });
        }

        entries.push(RomdirEntry {
            name,
            offset: data_offset,
            size,
        });

        data_offset = end
            .checked_add(0x0f)
            .map(|v| v & !0x0f)
            .ok_or(ImportError::RomdirTruncated)?;
        pos += 16;
    }

    Ok(entries)
}

fn decode_psmct16_to_rgba(raw: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let needed = (w as usize) * (h as usize) * 2;
    if raw.len() < needed {
        return None;
    }
    let mut out = Vec::with_capacity((w as usize) * (h as usize) * 4);
    for &px in raw[..needed].as_chunks::<2>().0 {
        let v = u16::from_le_bytes(px);
        let r = (((v & 0x001f) as u32) * 255 / 31) as u8;
        let g = ((((v >> 5) & 0x001f) as u32) * 255 / 31) as u8;
        let b = ((((v >> 10) & 0x001f) as u32) * 255 / 31) as u8;
        let a = if v == 0 {
            0
        } else if (v & 0x8000) != 0 {
            255
        } else {
            220
        };
        out.extend_from_slice(&[r, g, b, a]);
    }
    Some(out)
}

fn decode_psmct32_to_rgba(raw: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let needed = (w as usize) * (h as usize) * 4;
    if raw.len() < needed {
        return None;
    }
    let mut out = Vec::with_capacity(needed);
    for px in raw[..needed].as_chunks::<4>().0 {
        // PS2 alpha 0..=128 maps to 0..=255
        let a = ((u16::from(px[3]).min(128) * 255) / 128) as u8;
        out.extend_from_slice(&[px[0], px[1], px[2], a]);
    }
    Some(out)
}

/// Validate and extract recognized OSDSYS textures from a user-provided BIOS or `TEXIMAGE` slice.
pub fn validate_and_extract_import(input: &[u8]) -> Result<ValidatedImportBundle, ImportError> {
    if input.is_empty() {
        return Err(ImportError::InputEmpty);
    }
    if input.len() > MAX_IMPORT_INPUT_BYTES {
        return Err(ImportError::InputTooLarge {
            len: input.len(),
            max: MAX_IMPORT_INPUT_BYTES,
        });
    }

    let sha256_hex = sha256_hex(input);
    let fnv = fnv1a64(input);

    let root_base = find_romdir_offset(input)?;
    let root_entries = parse_romdir(input, root_base)?;

    let (container_slice, sub_entries) =
        if let Some(tex_entry) = root_entries.iter().find(|e| e.name == "TEXIMAGE") {
            let sub = &input[tex_entry.offset..tex_entry.offset + tex_entry.size];
            let sub_base = find_romdir_offset(sub)?;
            let entries = parse_romdir(sub, sub_base)?;
            (sub, entries)
        } else {
            (input, root_entries)
        };

    let mut textures = Vec::new();
    for entry in &sub_entries {
        if matches!(entry.name.as_str(), "RESET" | "ROMDIR" | "EXTINFO")
            || entry.name.starts_with('-')
            || entry.size == 0
        {
            continue;
        }
        let target: Option<(TextureSlot, u32, u32, bool)> = match entry.name.as_str() {
            "TEXOWAL0" => Some((TextureSlot::TowerWall, 256, 256, false)),
            "TEXOFOG0" => Some((TextureSlot::FogCloud, 128, 128, false)),
            "TEXOCRBL" => Some((TextureSlot::LightStreak, 64, 64, true)),
            "TEXOFLAR" => Some((TextureSlot::LensFlare, 128, 128, false)),
            "TEXOREF" => Some((TextureSlot::CubeReflect, 128, 128, false)),
            "TEXCKABE" => Some((TextureSlot::SmokeTunnel, 64, 64, true)),
            "TEXCBLUR" => Some((TextureSlot::OrbHalo, 64, 64, true)),
            "TEXCNAVI" => Some((TextureSlot::OrbCore, 64, 64, true)),
            "TEXCBUMP" => Some((TextureSlot::GlassBump, 64, 64, true)),
            "TEXCFLOW" => Some((TextureSlot::GlassFlow, 64, 64, true)),
            "TEXCREFA" => Some((TextureSlot::ConfigCubeRef, 64, 64, true)),
            _ => None,
        };
        if let Some((slot, w, h, is_32bit)) = target {
            let compressed = &container_slice[entry.offset..entry.offset + entry.size];
            let expanded = ps2_expand(compressed, MAX_EXPANDED_SUBFILE_BYTES)?;
            let rgba = if is_32bit {
                decode_psmct32_to_rgba(&expanded, w, h)
            } else {
                decode_psmct16_to_rgba(&expanded, w, h)
            };
            if let Some(pixels) = rgba {
                if slot == TextureSlot::GlassBump {
                    let mut inv = pixels.clone();
                    for px in inv.as_chunks_mut::<4>().0 {
                        px[0] = !px[0];
                        px[1] = !px[1];
                        px[2] = !px[2];
                    }
                    textures.push(RgbaTexture {
                        slot: TextureSlot::GlassBumpInv,
                        width: w,
                        height: h,
                        pixels: inv,
                    });
                }
                textures.push(RgbaTexture {
                    slot,
                    width: w,
                    height: h,
                    pixels,
                });
            }
        }
    }

    if textures.is_empty() {
        return Err(ImportError::NoRecognizedTextures);
    }

    Ok(ValidatedImportBundle {
        sha256_hex,
        fnv1a64: fnv,
        entry_count: sub_entries.len(),
        textures,
    })
}

/// Build a clean-room synthetic `TEXIMAGE` container in memory (zero Sony bytes)
/// for deterministic unit, integration, and browser import testing.
#[must_use]
pub fn build_synthetic_clean_teximage(seed: u32) -> Vec<u8> {
    let mut raw_64 = vec![0u8; 64 * 64 * 4];
    for (i, px) in raw_64.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let v = ((i as u32).wrapping_mul(17).wrapping_add(seed) & 0xff) as u8;
        px[0] = v;
        px[1] = v.wrapping_add(40);
        px[2] = 220;
        px[3] = 128;
    }
    let comp_kabe = ps2_encode_literals(&raw_64);
    let comp_bump = ps2_encode_literals(&raw_64);

    let align16 = |n: usize| (n + 15) & !15;
    // Entries: RESET (0 bytes), ROMDIR (5 * 16 = 80 bytes), TEXCKABE, TEXCBUMP, terminator (16 zero bytes)
    let romdir_size = 5 * 16;
    let mut table = Vec::with_capacity(romdir_size);
    let push_entry = |buf: &mut Vec<u8>, name: &[u8], size: u32| {
        let mut n10 = [0u8; 10];
        n10[..name.len()].copy_from_slice(name);
        buf.extend_from_slice(&n10);
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&size.to_le_bytes());
    };
    push_entry(&mut table, b"RESET", 0);
    push_entry(&mut table, b"ROMDIR", romdir_size as u32);
    push_entry(&mut table, b"TEXCKABE", comp_kabe.len() as u32);
    push_entry(&mut table, b"TEXCBUMP", comp_bump.len() as u32);
    table.extend_from_slice(&[0u8; 16]); // terminator

    let mut container = Vec::new();
    container.extend_from_slice(&table);
    container.resize(align16(container.len()), 0);
    container.extend_from_slice(&comp_kabe);
    container.resize(align16(container.len()), 0);
    container.extend_from_slice(&comp_bump);
    container.resize(align16(container.len()), 0);
    container
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::SplitMix64;

    #[test]
    fn sha256_matches_nist_fips_180_4_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn ps2_expand_handles_literals_and_backreferences_identically_to_c_tool() {
        // Construct a stream with 4 literals "ABCD" followed by a back-reference copying "ABCDABCD" (len=8, back=4)
        // n = 0 -> shift = 14, mask = 0x3fff
        // 4 literals (bits 31..28 = 0), 1 backref (bit 27 = 1): desc = 0x0800_0000
        // backref h: length = 8 -> (h >> 14) = 8 - 3 = 5? Wait: 2 bits above 14 means max m = 3 + 3 = 6 for n=0!
        // With n = 2 -> shift = 12, mask = 0x0fff, max m = 3 + 15 = 18!
        // desc = (1 << 27) | 2 = 0x0800_0002
        // back = 4 -> (h & 0x0fff) = 3; copy_len = 8 -> (h >> 12) = 5 -> h = (5 << 12) | 3 = 0x5003
        let mut src = Vec::new();
        src.extend_from_slice(&12u32.to_le_bytes());
        src.extend_from_slice(&0x0800_0002_u32.to_be_bytes());
        src.extend_from_slice(b"ABCD");
        src.extend_from_slice(&0x5003_u16.to_be_bytes());

        let expanded = ps2_expand(&src, 1024).unwrap();
        assert_eq!(&expanded, b"ABCDABCDABCD");
    }

    #[test]
    fn synthetic_clean_teximage_validates_and_extracts_textures_plus_complement() {
        let img = build_synthetic_clean_teximage(42);
        let bundle = validate_and_extract_import(&img).unwrap();
        assert_eq!(bundle.sha256_hex.len(), 64);
        assert_eq!(bundle.textures.len(), 3); // TEXCKABE + TEXCBUMP + synthesized TEXCBINV
        assert!(
            bundle
                .textures
                .iter()
                .any(|t| t.slot == TextureSlot::SmokeTunnel)
        );
        assert!(
            bundle
                .textures
                .iter()
                .any(|t| t.slot == TextureSlot::GlassBump)
        );
        assert!(
            bundle
                .textures
                .iter()
                .any(|t| t.slot == TextureSlot::GlassBumpInv)
        );
    }

    #[test]
    fn bounded_fuzz_arbitrary_and_mutated_inputs_never_panic() {
        let valid = build_synthetic_clean_teximage(7);
        let mut rng = SplitMix64::new(0xf022_7e57_2026_1031);
        let mut scratch = vec![0u8; 4096];

        // 1. Empty, truncated, and oversized edge cases
        assert_eq!(
            validate_and_extract_import(&[]).unwrap_err(),
            ImportError::InputEmpty
        );
        for cut in [1usize, 3, 4, 15, 31, 64, 96, valid.len() - 5] {
            let _ = validate_and_extract_import(&valid[..cut]);
            let _ = ps2_expand(&valid[..cut], 65_536);
        }

        // 2. 1,500 random & bit-flipped fuzz cases
        for case in 0..1_500 {
            let len = (rng.next_u32() as usize) % scratch.len();
            if case % 2 == 0 {
                rng.fill_bytes(&mut scratch[..len]);
                let _ = ps2_expand(&scratch[..len], 32_768);
                let _ = validate_and_extract_import(&scratch[..len]);
            } else {
                let mut mutated = valid.clone();
                let flips = 1 + (rng.next_u32() as usize % 8);
                for _ in 0..flips {
                    let idx = (rng.next_u32() as usize) % mutated.len();
                    mutated[idx] ^= (rng.next_u32() & 0xff) as u8;
                }
                let _ = validate_and_extract_import(&mutated);
            }
        }
    }
}
