use std::collections::BTreeSet;

use goblin::pe::section_table::SectionTable;
use goblin::pe::PE;

const DESC: usize = 32;
const RVA_BASED: u32 = 1;
const MAX_DESCS: usize = 256;
const MAX_NAME: usize = 256;

/// Delay-import DLL names (lowercase). goblin `PE.libraries` is IAT only.
pub(crate) fn delay_libs(bytes: &[u8], pe: &PE<'_>) -> BTreeSet<String> {
    let Some(oh) = pe.header.optional_header else {
        return BTreeSet::new();
    };
    let Some(dir) = oh.data_directories.get_delay_import_descriptor() else {
        return BTreeSet::new();
    };
    collect_delay_names(
        bytes,
        &pe.sections,
        pe.image_base,
        dir.virtual_address,
        dir.size,
    )
}

pub(crate) fn collect_delay_names(
    bytes: &[u8],
    sections: &[SectionTable],
    image_base: u64,
    dir_rva: u32,
    dir_size: u32,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if dir_rva == 0 {
        return out;
    }
    let Some(off) = rva_to_offset(sections, dir_rva) else {
        return out;
    };
    let n = if dir_size == 0 {
        MAX_DESCS
    } else {
        (dir_size as usize / DESC).min(MAX_DESCS)
    };
    for i in 0..n {
        let start = match off.checked_add(i * DESC) {
            Some(s) if s.saturating_add(DESC) <= bytes.len() => s,
            _ => break,
        };
        let desc = &bytes[start..start + DESC];
        if desc.iter().all(|&b| b == 0) {
            break;
        }
        let attrs = u32::from_le_bytes(desc[0..4].try_into().unwrap());
        let name_field = u32::from_le_bytes(desc[4..8].try_into().unwrap());
        if name_field == 0 {
            continue;
        }
        let name_rva = if attrs & RVA_BASED != 0 {
            name_field
        } else {
            let va = u64::from(name_field);
            if va < image_base {
                continue;
            }
            (va - image_base) as u32
        };
        if let Some(name) = read_cstr(bytes, sections, name_rva) {
            out.insert(name);
        }
    }
    out
}

fn rva_to_offset(sections: &[SectionTable], rva: u32) -> Option<usize> {
    let rva = rva as usize;
    for s in sections {
        let va = s.virtual_address as usize;
        let span = (s.virtual_size as usize).max(s.size_of_raw_data as usize);
        if span == 0 || rva < va || rva >= va + span {
            continue;
        }
        return (s.pointer_to_raw_data as usize).checked_add(rva - va);
    }
    None
}

fn read_cstr(bytes: &[u8], sections: &[SectionTable], rva: u32) -> Option<String> {
    let off = rva_to_offset(sections, rva)?;
    let slice = bytes.get(off..)?;
    let end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
    let end = end.min(MAX_NAME);
    if end == 0 {
        return None;
    }
    let s = std::str::from_utf8(&slice[..end]).ok()?;
    Some(s.to_ascii_lowercase())
}
