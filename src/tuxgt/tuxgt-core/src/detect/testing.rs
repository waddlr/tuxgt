use std::path::Path;

pub(crate) fn write_file(p: &Path, b: &[u8]) {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(p, b).unwrap();
}

/// Minimal i386 PE32. `security_past_eof` sets the Authenticode directory
/// past the file so a strict goblin parse fails the same way Just Cause 2 does.
pub(crate) fn pe32_i386(security_past_eof: bool) -> Vec<u8> {
    let mut b = vec![0u8; 0x400];
    b[0] = b'M';
    b[1] = b'Z';
    b[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
    let pe = 0x80usize;
    b[pe..pe + 4].copy_from_slice(b"PE\0\0");
    b[pe + 4..pe + 6].copy_from_slice(&0x014cu16.to_le_bytes());
    b[pe + 6..pe + 8].copy_from_slice(&1u16.to_le_bytes());
    b[pe + 20..pe + 22].copy_from_slice(&0x00e0u16.to_le_bytes());
    b[pe + 22..pe + 24].copy_from_slice(&0x0102u16.to_le_bytes());
    let opt = pe + 24;
    b[opt..opt + 2].copy_from_slice(&0x010bu16.to_le_bytes());
    b[opt + 16..opt + 20].copy_from_slice(&0x1000u32.to_le_bytes());
    b[opt + 20..opt + 24].copy_from_slice(&0x1000u32.to_le_bytes());
    b[opt + 24..opt + 28].copy_from_slice(&0x2000u32.to_le_bytes());
    b[opt + 28..opt + 32].copy_from_slice(&0x0040_0000u32.to_le_bytes());
    b[opt + 32..opt + 36].copy_from_slice(&0x1000u32.to_le_bytes());
    b[opt + 36..opt + 40].copy_from_slice(&0x200u32.to_le_bytes());
    b[opt + 40..opt + 42].copy_from_slice(&4u16.to_le_bytes());
    b[opt + 48..opt + 50].copy_from_slice(&4u16.to_le_bytes());
    b[opt + 56..opt + 60].copy_from_slice(&0x3000u32.to_le_bytes());
    b[opt + 60..opt + 64].copy_from_slice(&0x200u32.to_le_bytes());
    b[opt + 68..opt + 70].copy_from_slice(&2u16.to_le_bytes());
    b[opt + 72..opt + 76].copy_from_slice(&0x10_0000u32.to_le_bytes());
    b[opt + 76..opt + 80].copy_from_slice(&0x1000u32.to_le_bytes());
    b[opt + 80..opt + 84].copy_from_slice(&0x10_0000u32.to_le_bytes());
    b[opt + 84..opt + 88].copy_from_slice(&0x1000u32.to_le_bytes());
    b[opt + 92..opt + 96].copy_from_slice(&16u32.to_le_bytes());
    if security_past_eof {
        let cert = opt + 96 + 4 * 8;
        b[cert..cert + 4].copy_from_slice(&0x400u32.to_le_bytes());
        b[cert + 4..cert + 8].copy_from_slice(&0x200u32.to_le_bytes());
    }
    let sec = opt + 0xe0;
    b[sec..sec + 5].copy_from_slice(b".text");
    b[sec + 8..sec + 12].copy_from_slice(&0x200u32.to_le_bytes());
    b[sec + 12..sec + 16].copy_from_slice(&0x1000u32.to_le_bytes());
    b[sec + 16..sec + 20].copy_from_slice(&0x200u32.to_le_bytes());
    b[sec + 20..sec + 24].copy_from_slice(&0x200u32.to_le_bytes());
    b[sec + 36..sec + 40].copy_from_slice(&0x6000_0020u32.to_le_bytes());
    b
}

/// PE32 with IAT `d3d9.dll` and delay-import `d3d10.dll`.
/// `rva_based` is IMAGE_DELAYLOAD_DESCRIPTOR Attributes bit 0.
pub(crate) fn pe32_iat_d3d9_delay_d3d10(rva_based: bool) -> Vec<u8> {
    let mut b = pe32_i386(false);
    let opt = 0x80 + 24;
    let dd = opt + 96;
    b[dd + 8..dd + 12].copy_from_slice(&0x1000u32.to_le_bytes());
    b[dd + 12..dd + 16].copy_from_slice(&40u32.to_le_bytes());
    let delay_dd = dd + 13 * 8;
    b[delay_dd..delay_dd + 4].copy_from_slice(&0x1040u32.to_le_bytes());
    b[delay_dd + 4..delay_dd + 8].copy_from_slice(&64u32.to_le_bytes());
    let imp = 0x200;
    b[imp..imp + 4].copy_from_slice(&0x10c0u32.to_le_bytes());
    b[imp + 12..imp + 16].copy_from_slice(&0x10a0u32.to_le_bytes());
    b[imp + 16..imp + 20].copy_from_slice(&0x10c8u32.to_le_bytes());
    let dly = 0x240;
    let attrs: u32 = if rva_based { 1 } else { 0 };
    b[dly..dly + 4].copy_from_slice(&attrs.to_le_bytes());
    let name_field: u32 = if rva_based {
        0x10b0
    } else {
        0x0040_0000 + 0x10b0
    };
    b[dly + 4..dly + 8].copy_from_slice(&name_field.to_le_bytes());
    b[0x2a0..0x2a9].copy_from_slice(b"d3d9.dll\0");
    b[0x2b0..0x2ba].copy_from_slice(b"d3d10.dll\0");
    b[0x2c0..0x2c4].copy_from_slice(&0x8000_0001u32.to_le_bytes());
    b[0x2c8..0x2cc].copy_from_slice(&0x8000_0001u32.to_le_bytes());
    b
}

/// MZ + PE signature + i386 Machine only — too short for a full parse.
pub(crate) fn pe32_header_only() -> Vec<u8> {
    let mut b = vec![0u8; 0x88];
    b[0] = b'M';
    b[1] = b'Z';
    b[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
    b[0x80..0x84].copy_from_slice(b"PE\0\0");
    b[0x84..0x86].copy_from_slice(&0x014cu16.to_le_bytes());
    b
}
