//! Halo 2 Anniversary Forge palette (per-map block at scnr+0x2C4).
//!
//! Bundles records dumped from every shipped H2A (groundhog) .map cache.
//! Structurally identical to the H4 Map Variant Palettes block — same
//! cat/entry/variant hierarchy, same `group_be(4) + pad(8) + datum(4)`
//! tag-ref shape — verified against the Halo2AMCC scnr XML mirror of
//! the Halo4MCC layout. Pointer expansion uses H2A expand_magic 0x7AC00000.
//!
//! H2A-specific in-memory strides (vs H4): entry record is 0x20 bytes
//! (H4: 0x1C) and variant record is 0x4C bytes (H4: 0x48). Category
//! record is 0x14 bytes in BOTH engines.
//!
//! Each record carries the resolved tag-path (`tag`) so consumers don't
//! need a separate datum -> tag-name lookup.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

static H2A_PALETTES_JSONL: &str =
    include_str!("../../../../assets/maps/groundhog_palettes_bundled.jsonl");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct H2APaletteRecord {
    /// Map cache name (e.g. `"ca_ascension"`, `"ca_sanctuary"`).
    pub map: String,
    /// Category index within the Map Variant Palettes block.
    pub cat: u32,
    /// Entry slot within the category.
    pub entry: u32,
    /// Variant slot within the entry.
    pub variant: u32,
    /// StringID labelling the variant (often 0 for default variants).
    #[serde(default)]
    pub variant_name_stringid: u32,
    /// StringID labelling the entry as displayed in Forge UI.
    #[serde(default)]
    pub display_name_stringid: u32,
    /// Tag class fourcc.
    pub class: String,
    /// Tag datum index from the .map's tag table.
    pub datum: u32,
    /// Resolved tag path.
    pub tag: String,
}

fn parse_all() -> Vec<H2APaletteRecord> {
    H2A_PALETTES_JSONL
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<H2APaletteRecord>(l).ok())
        .collect()
}

/// All H2A palette records across every shipped map.
pub fn halo2a_palettes() -> Vec<H2APaletteRecord> {
    parse_all()
}

/// `map_name -> list of palette records` for fast per-map lookup.
pub fn halo2a_palettes_by_map() -> HashMap<String, Vec<H2APaletteRecord>> {
    let mut out: HashMap<String, Vec<H2APaletteRecord>> = HashMap::new();
    for r in parse_all() {
        out.entry(r.map.clone()).or_default().push(r);
    }
    out
}

/// Entry-level enumeration per map (variants collapsed to variant=0).
/// Same engine semantics as H4 and Reach — wire `variant_quota_index`
/// indexes into this list, `variant_index` picks the sub-variant.
//
// groundhog.dll H2A_palette_resolve_quota_to_cat_and_entry @ +0xB23F0 is
// the load-time decomposer: it walks scnr+0x2C4 categories subtracting
// each cat.entry_count until the remainder fits in the current cat,
// yielding (cat, entry). Variants are NOT counted.
//
// groundhog.dll H2A_palette_lookup_entry_then_variant_returns_datum
// @ +0xB2374 then fetches the variant tag record at
// entries_ptr + entry*0x20 + 0x14 + variant_index*0x4C
// (H2A-specific strides; H4 uses 0x1C / 0x48).
//
// groundhog.dll H2A_quota_decode_3x10bits @ +0xB0AEC confirms quotas
// are 30 bits per slot (3 × u10), distinct from Reach/H4 which use 24
// bits (3 × u8).
pub fn halo2a_quota_entries_by_map() -> HashMap<String, Vec<H2APaletteRecord>> {
    let mut out: HashMap<String, Vec<H2APaletteRecord>> = HashMap::new();
    for r in parse_all() {
        if r.variant != 0 { continue; }
        out.entry(r.map.clone()).or_default().push(r);
    }
    out
}

/// Resolve `(map, cat, entry, variant)` -> palette record.
pub fn h2a_lookup(
    map_name: &str,
    cat: u32,
    entry: u32,
    variant: u32,
) -> Option<H2APaletteRecord> {
    parse_all().into_iter().find(|r| {
        r.map == map_name && r.cat == cat && r.entry == entry && r.variant == variant
    })
}

/// Resolve a wire `(map, variant_quota_index, variant_index)` triple to
/// a palette record using the engine's entry-level enumeration of
/// scnr+0x2C4. Returns None if `variant_quota_index` is out of range.
///
/// Sub-variant fallback: if the entry has no record at `variant_index`
/// (e.g. variant=0 only), returns the variant=0 record.
//
// groundhog.dll H2A_palette_resolve_quota_to_cat_and_entry @ +0xB23F0 +
// H2A_palette_lookup_entry_then_variant_returns_datum @ +0xB2374 —
// combined behavior.
pub fn resolve_h2a_quota(
    map_name: &str,
    variant_quota_index: u32,
    variant_index: u32,
) -> Option<H2APaletteRecord> {
    let entries = halo2a_quota_entries_by_map();
    let map_entries = entries.get(map_name)?;
    let base = map_entries.get(variant_quota_index as usize)?;
    if variant_index == 0 {
        return Some(base.clone());
    }
    h2a_lookup(map_name, base.cat, base.entry, variant_index)
        .or_else(|| Some(base.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonempty() {
        let p = halo2a_palettes();
        assert!(p.len() > 500, "expected 500+ records, got {}", p.len());
    }

    #[test]
    fn ascension_has_records() {
        let by_map = halo2a_palettes_by_map();
        let asc = by_map
            .get("ca_ascension")
            .expect("ca_ascension records should exist");
        assert!(!asc.is_empty());
    }

    /// Pins the engine's load-time entry-level enumeration of the H2A
    /// Map Variant Palettes block. The wire `variant_quota_index`
    /// indexes into THIS list (sub-variants collapsed).
    ///
    /// Verified against the cumulative-count walk of
    /// `ca_ascension.mvpal.jsonl`:
    ///   cat 0: 15 entries → abs_quota 0..14
    ///   cat 1: 13 entries → abs_quota 15..27
    ///   cat 2:  4 entries → abs_quota 28..31
    ///     - q=28 (2,0,0) = mongoose; (2,0,1) sub-variant = gungoose
    ///     - q=29 (2,1,0) = hornet     ← would mis-resolve under flat indexing
    ///     - q=31 (2,3,0) = banshee_mp; (2,3,1) sub-variant = banshee_heretic
    ///
    /// Matches groundhog.dll H2A_palette_resolve_quota_to_cat_and_entry
    /// @ +0xB23F0 which decomposes abs_quota_index by walking
    /// scnr+0x2C4 and subtracting per-category entry_count.
    #[test]
    fn ca_ascension_entry_level_indexing_matches_engine() {
        let entries = halo2a_quota_entries_by_map();
        let asc = entries
            .get("ca_ascension")
            .expect("ca_ascension entry-level enumeration should exist");

        let resolve = |abs_q: usize| -> &H2APaletteRecord {
            asc.get(abs_q).unwrap_or_else(|| {
                panic!("ca_ascension entries[{}] should exist (have {})", abs_q, asc.len())
            })
        };

        // First UNSC weapon at the head of the palette.
        assert!(resolve(0).tag.contains("h2a_magnum"));
        assert_eq!(resolve(0).cat, 0);
        assert_eq!(resolve(0).entry, 0);

        // Last entry of cat=0 (15th UNSC weapon, abs_quota=14).
        assert!(resolve(14).tag.contains("h2a_mounted_turret"));
        assert_eq!(resolve(14).cat, 0);

        // First entry of cat=1 (Covenant weapons start).
        assert!(resolve(15).tag.contains("h2a_plasma_pistol"));
        assert_eq!(resolve(15).cat, 1);
        assert_eq!(resolve(15).entry, 0);

        // Last entry of cat=1 (covenant_turret).
        assert!(resolve(27).tag.contains("h2a_covenant_turret"));
        assert_eq!(resolve(27).cat, 1);

        // Vehicle category starts here.
        assert!(resolve(28).tag.contains("h2a_mongoose"));
        assert_eq!(resolve(28).cat, 2);
        assert_eq!(resolve(28).entry, 0);

        // THE LOAD-BEARING ASSERTION:
        // q=29 must be hornet under entry-level enumeration. Under a
        // (broken) flat-list indexing that counted sub-variants, the
        // gungoose at JSONL line 30 would shift indices and q=29 would
        // mis-resolve. groundhog.dll engine does NOT count sub-variants.
        assert!(
            resolve(29).tag.contains("h2a_hornet"),
            "q=29 must be hornet under entry-level enumeration, got {:?}",
            resolve(29).tag
        );
        assert_eq!(resolve(29).cat, 2);
        assert_eq!(resolve(29).entry, 1);

        // Banshee at q=31, and its heretic sub-variant lives under the
        // SAME (cat,entry) — selected by variant_index=1 at runtime.
        assert!(resolve(31).tag.contains("h2a_banshee"));
        assert!(!resolve(31).tag.contains("heretic"));
        let heretic = h2a_lookup("ca_ascension", 2, 3, 1)
            .expect("heretic banshee sub-variant should exist");
        assert!(
            heretic.tag.contains("banshee_heretic"),
            "expected heretic banshee at (2,3,1), got {:?}",
            heretic.tag
        );

        // resolve_h2a_quota end-to-end: wire (q=31, variant=1) →
        // banshee_heretic.
        let r = resolve_h2a_quota("ca_ascension", 31, 1)
            .expect("wire (31, 1) should resolve");
        assert!(
            r.tag.contains("banshee_heretic"),
            "wire (q=31, v=1) should resolve to heretic banshee, got {:?}",
            r.tag
        );

        // resolve_h2a_quota fallback: wire (q=29, variant=1) — hornet
        // has no sub-variant, must fall back to variant=0 record.
        let r = resolve_h2a_quota("ca_ascension", 29, 1)
            .expect("wire (29, 1) should still resolve via fallback");
        assert!(r.tag.contains("h2a_hornet"));
    }
}
