//! Friendly tag-name lookup for Reach MCC `.map` files.
//!
//! `forge_halo.map` is the canonical Forge sandbox map; its 15k tags
//! (extracted via Blamite/Assembly's CacheFileLoader) cover every object
//! that can appear in a Reach MCC mvar's palette. The mvar's quotas
//! reference these tags by a *palette slot index* (small int), which
//! resolves via the scnr's Map Variant Palettes block — a layout that
//! is NOT yet defined in Blamite's ReachMCC scnr XML and is the
//! remaining RE step for full friendly-name display.
//!
//! For now this module ships the tag table indexed by datum_index
//! (the 32-bit value the .map file actually stores in tag refs).
//! Once the scnr palette block is RE'd, the mvar's slot index can
//! be mapped to a datum_index via that table and then looked up here.

use std::collections::HashMap;

/// Pre-extracted forge_halo.map tag table (idx → friendly name).
/// Loaded lazily from the bundled JSONL file in `blf_lib/assets/`.
static FORGE_HALO_TAGS_JSONL: &str = include_str!("../../../../assets/forge_halo_tags.jsonl");

/// (datum_index, group_code) → friendly name.
/// Group code is the 4-char tag class (e.g. "vehi" for vehicles).
pub fn forge_halo_tag_table() -> HashMap<(u32, [u8; 4]), String> {
    let mut out = HashMap::new();
    for line in FORGE_HALO_TAGS_JSONL.lines() {
        if line.is_empty() {
            continue;
        }
        let idx = extract_u32(line, "\"idx\":");
        let grp = extract_str(line, "\"grp\":\"", "\"");
        let name = extract_str(line, "\"name\":\"", "\"}");
        if let (Some(idx), Some(grp), Some(name)) = (idx, grp, name) {
            if grp.len() == 4 {
                let g = [grp.as_bytes()[0], grp.as_bytes()[1], grp.as_bytes()[2], grp.as_bytes()[3]];
                out.insert((idx, g), unescape_path(name));
            }
        }
    }
    out
}

fn extract_u32(line: &str, key: &str) -> Option<u32> {
    let p = line.find(key)?;
    let rest = &line[p + key.len()..];
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    rest[..end].parse().ok()
}

fn extract_str<'a>(line: &'a str, prefix: &str, end_marker: &str) -> Option<&'a str> {
    let p = line.find(prefix)?;
    let rest = &line[p + prefix.len()..];
    let end = rest.find(end_marker)?;
    Some(&rest[..end])
}

fn unescape_path(s: &str) -> String {
    s.replace("\\\\", "\\").replace("\\\"", "\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanity_warthog_gauss_banshee_resolve() {
        let table = forge_halo_tag_table();
        assert_eq!(
            table.get(&(0xE943069D, *b"vehi")).map(String::as_str),
            Some("objects\\vehicles\\human\\warthog\\warthog"),
        );
        assert_eq!(
            table.get(&(0xEB6F083E, *b"vehi")).map(String::as_str),
            Some("objects\\vehicles\\human\\warthog\\weapons\\warthog_gauss\\warthog_gauss"),
        );
        assert_eq!(
            table.get(&(0xED6A09BF, *b"vehi")).map(String::as_str),
            Some("objects\\vehicles\\covenant\\banshee\\banshee"),
        );
    }

    #[test]
    fn table_has_thousands_of_entries() {
        let table = forge_halo_tag_table();
        assert!(table.len() > 15_000, "expected 15k+ tags, got {}", table.len());
    }
}
