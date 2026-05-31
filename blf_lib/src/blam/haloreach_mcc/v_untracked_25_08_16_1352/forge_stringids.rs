//! StringID -> string resolver for Reach MCC palette variants.
//!
//! The mvar pipeline resolves variant_name_stringids (e.g., 0x6BC, 0x6BD)
//! to human-friendly names ("warthog_default", "warthog_gauss") via the
//! cache's IndexedStringIDSource. Only the 282 stringIDs actually referenced
//! by the forge_halo.map palette are bundled here (10KB).

use std::collections::HashMap;

static FORGE_HALO_STRINGIDS_JSONL: &str = include_str!("../../../../assets/forge_halo_stringids.jsonl");

pub fn forge_halo_stringid_table() -> HashMap<u32, String> {
    let mut out = HashMap::new();
    for line in FORGE_HALO_STRINGIDS_JSONL.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let sid_start = line.find("\"sid\":").unwrap() + 6;
        let sid_end = line[sid_start..].find(',').unwrap();
        let sid: u32 = line[sid_start..sid_start + sid_end].parse().unwrap_or(0);
        let name_start = line.find("\"name\":\"").map(|p| p + 8);
        if let Some(s) = name_start {
            let rest = &line[s..];
            if let Some(end) = rest.find("\"}") {
                out.insert(sid, rest[..end].replace("\\\\", "\\").replace("\\\"", "\""));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warthog_variant_ids_resolve() {
        let table = forge_halo_stringid_table();
        assert_eq!(table.get(&0x356).map(String::as_str), Some("banshee"));
        assert_eq!(table.get(&0x6BC).map(String::as_str), Some("warthog_default"));
        assert_eq!(table.get(&0x6BD).map(String::as_str), Some("warthog_gauss"));
        assert_eq!(table.get(&0x6BF).map(String::as_str), Some("warthog_rocket"));
    }
}
