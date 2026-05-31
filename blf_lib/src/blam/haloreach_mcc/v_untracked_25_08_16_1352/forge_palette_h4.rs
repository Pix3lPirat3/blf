//! Halo 4 Forge palette (per-map Map Variant Palettes block at scnr+0x2C4).
//!
//! Bundles records dumped from every shipped H4 .map cache. The block
//! layout matches the Reach hierarchy (categories -> entries -> variants)
//! and is verified against the Blamite Halo4MCC scnr XML at line 5790:
//! category tagblock at scnr+0x2C4 (count) / +0x2C8 (data ptr),
//! category stride 0x14, entry stride 0x1C, variant stride 0x48.
//! Pointer expansion uses H4 expand_magic 0x4FFF0000.
//!
//! Runtime resolution (RE'd against halo4.dll, see VoxClient docs/421):
//!   - halo4.dll H4_palette_resolve_quota_to_cat_and_entry @ +0xB3614
//!     walks scnr+0x2C4 categories; takes a flat entry-level
//!     variant_quota_index; outputs (cat_idx, entry_in_cat).
//!   - halo4.dll H4_palette_lookup_entry_then_variant_returns_datum @ +0xB3594
//!     dereferences the matched entry (5 ints, +8 = entries_count for the
//!     category, +0xC -> entries block); entry layout: +0 default datum,
//!     +4 variant_count, +8 variants tagblock.
//!   - halo4.dll H4_palette_lookup_quota_index_with_overriding_class @ +0xB3690
//!     reads quota_rec+0x2 (variant_quota_index, short) and quota_rec+0x32
//!     (variant_index, byte) and returns the resolved datum.
//!
//! Wire-side: variant_quota_index/variant_index are decoded by
//! halo4.dll H4_variant_object_decode_mvar @ +0xB7C9C (via
//! H4_bitstream_read_index_8bit_or_negone @ +0xB8A04 and
//! H4_bitstream_read_index_5bit_or_negone @ +0xB8CA4) and written to
//! scnr_runtime+0x14fc + quota_rec_idx*0x4c (+2 short, +0x32 byte).
//!
//! Each record carries the resolved tag-path (`tag`) so consumers don't
//! need a separate datum -> tag-name lookup.
//!
//! Schema (from `assets/maps/halo4/<map>.mvpal.jsonl`):
//! ```text
//! {"cat":0,"entry":0,"variant":0,"variant_name_stringid":0,
//!  "display_name_stringid":0,"class":"weap","datum":4254799026,
//!  "tag":"objects\\weapons\\pistol\\storm_magnum\\storm_magnum"}
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

static H4_PALETTES_JSONL: &str =
    include_str!("../../../../assets/maps/halo4_palettes_bundled.jsonl");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct H4PaletteRecord {
    /// Map cache name (e.g. `"ca_basin"`, `"ca_warhouse"`).
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

fn parse_all() -> Vec<H4PaletteRecord> {
    H4_PALETTES_JSONL
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<H4PaletteRecord>(l).ok())
        .collect()
}

/// All H4 palette records across every shipped map.
pub fn halo4_palettes() -> Vec<H4PaletteRecord> {
    parse_all()
}

/// `map_name -> list of palette records` for fast per-map lookup.
pub fn halo4_palettes_by_map() -> HashMap<String, Vec<H4PaletteRecord>> {
    let mut out: HashMap<String, Vec<H4PaletteRecord>> = HashMap::new();
    for r in parse_all() {
        out.entry(r.map.clone()).or_default().push(r);
    }
    out
}

/// Entry-level enumeration per map (variants collapsed to variant=0).
/// Matches the engine's load-time walk of the Map Variant Palettes block
/// at scnr+0x2C4 — wire `variant_quota_index` indexes into this list,
/// `variant_index` then picks the sub-variant within the entry.
///
/// JSONL is bundled in (cat, entry, variant) order, so filtering to
/// variant==0 produces the same sequence the engine generates by
/// walking categories in order, then entries within each category.
/// Confirmed against halo4.dll H4_palette_resolve_quota_to_cat_and_entry
/// @ +0xB3614 (the actual flat-to-(cat,entry) reducer).
pub fn halo4_quota_entries_by_map() -> HashMap<String, Vec<H4PaletteRecord>> {
    let mut out: HashMap<String, Vec<H4PaletteRecord>> = HashMap::new();
    for r in parse_all() {
        if r.variant != 0 { continue; }
        out.entry(r.map.clone()).or_default().push(r);
    }
    out
}

/// Resolve `(map, cat, entry, variant)` -> palette record.
pub fn h4_lookup(
    map_name: &str,
    cat: u32,
    entry: u32,
    variant: u32,
) -> Option<H4PaletteRecord> {
    parse_all().into_iter().find(|r| {
        r.map == map_name && r.cat == cat && r.entry == entry && r.variant == variant
    })
}

/// Resolve a wire `(variant_quota_index, variant_index)` pair to the
/// palette record (containing tag class, datum, and resolved tag path).
///
/// `quota_index` indexes the entry-level enumeration (variants collapsed).
/// `variant_index` picks the sub-variant within the entry (or 0 for entries
/// with no sub-variants). Returns `None` if `quota_index` is out of range
/// for the map.
///
/// This is the headless equivalent of the engine path:
///   halo4.dll H4_palette_lookup_entry_then_variant_returns_datum @ +0xB3594
///   (which calls H4_palette_resolve_quota_to_cat_and_entry @ +0xB3614 to
///   reduce the flat quota index, then dereferences entry/variant).
///
/// Variant fall-through: if `variant_index` is out of range for the entry
/// (or the entry has no sub-variants beyond the default), the variant=0
/// (default) record is returned. This mirrors the engine's behavior in
/// halo4.dll H4_palette_lookup_quota_index_with_overriding_class @ +0xB3690
/// where `piVar3[1] == 1` (variant_count==1) returns the entry's default
/// datum without indexing the variants block.
pub fn resolve_halo4_quota(
    map_name: &str,
    quota_index: u32,
    variant_index: u32,
) -> Option<H4PaletteRecord> {
    let all = parse_all();
    let entries: Vec<&H4PaletteRecord> = all
        .iter()
        .filter(|r| r.map == map_name && r.variant == 0)
        .collect();
    let base = entries.get(quota_index as usize)?;
    let cat = base.cat;
    let entry = base.entry;
    let variant_rec = all.iter().find(|r| {
        r.map == map_name && r.cat == cat && r.entry == entry && r.variant == variant_index
    });
    Some(variant_rec.cloned().unwrap_or_else(|| (*base).clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonempty() {
        let p = halo4_palettes();
        assert!(p.len() > 1000, "expected 1000+ records, got {}", p.len());
    }

    #[test]
    fn known_h4_weapon_resolves() {
        let r = h4_lookup("ca_basin", 0, 0, 0).expect("ca_basin (0,0,0)");
        assert_eq!(r.class, "weap");
        assert!(
            r.tag.contains("storm_magnum"),
            "expected storm_magnum, got {}",
            r.tag
        );
    }

    /// Pins the engine's load-time enumeration of ca_basin's Map Variant
    /// Palettes (entry-level — variants collapsed). The wire
    /// `variant_quota_index` indexes into THIS list.
    ///
    /// ca_basin has 12 categories with the following entry counts:
    ///   cat=0:13 (Weapons), cat=1:11, cat=2:7, cat=3:8, cat=4:9, cat=5:9,
    ///   cat=6:3, cat=7:5, cat=8:10, cat=9:1, cat=10:8, cat=11:10
    /// Total = 94 entry-level slots.
    ///
    /// Anchors (verified against bundled jsonl ordering, matches
    /// halo4.dll H4_palette_resolve_quota_to_cat_and_entry @ +0xB3614
    /// flat-walk semantics):
    ///   q=0  -> (cat=0, entry=0)  -> storm_magnum
    ///   q=13 -> (cat=1, entry=0)  -> storm_plasma_pistol (cat-boundary)
    ///   q=39 -> (cat=4, entry=0)  -> dlc_fusion_coil (first cat-4 entry)
    ///   q=39, variant=1 -> fw_land_mine (sub-variant via variant_index)
    ///   q=39, variant=2 -> fw_unsc_fuel_canister
    #[test]
    fn ca_basin_entry_level_indexing_matches_engine() {
        let by_map = halo4_quota_entries_by_map();
        let entries = by_map.get("ca_basin").expect("ca_basin entries");
        assert_eq!(
            entries.len(),
            94,
            "ca_basin should produce 94 entry-level quota slots, got {}",
            entries.len()
        );

        let q0 = &entries[0];
        assert_eq!((q0.cat, q0.entry), (0, 0));
        assert!(q0.tag.contains("storm_magnum"), "q=0 -> {}", q0.tag);

        let q13 = &entries[13];
        assert_eq!((q13.cat, q13.entry), (1, 0), "q=13 should jump to cat=1");
        assert!(
            q13.tag.contains("storm_plasma_pistol"),
            "q=13 -> {}",
            q13.tag
        );

        let q39 = &entries[39];
        assert_eq!((q39.cat, q39.entry), (4, 0), "q=39 should be cat=4 entry=0");
        assert!(q39.tag.contains("dlc_fusion_coil"), "q=39 -> {}", q39.tag);
    }

    /// Verify the full (quota_index, variant_index) -> tag resolver against
    /// a known cat=4 entry that has multiple sub-variants. This is the
    /// headless equivalent of halo4.dll
    /// H4_palette_lookup_entry_then_variant_returns_datum @ +0xB3594.
    #[test]
    fn ca_basin_variant_resolver_picks_sub_variant() {
        let default = resolve_halo4_quota("ca_basin", 39, 0).expect("q=39 v=0");
        let variant1 = resolve_halo4_quota("ca_basin", 39, 1).expect("q=39 v=1");
        let variant2 = resolve_halo4_quota("ca_basin", 39, 2).expect("q=39 v=2");

        assert!(
            default.tag.contains("dlc_fusion_coil"),
            "q=39 v=0 -> {}",
            default.tag
        );
        assert!(
            variant1.tag.contains("fw_land_mine"),
            "q=39 v=1 -> {}",
            variant1.tag
        );
        assert!(
            variant2.tag.contains("fw_unsc_fuel_canister"),
            "q=39 v=2 -> {}",
            variant2.tag
        );

        // Out-of-range variant_index falls through to the default record
        // (matches halo4.dll H4_palette_lookup_quota_index_with_overriding_class
        // behaviour where variant_count==1 short-circuits and out-of-range
        // bypasses the variant block lookup).
        let oob = resolve_halo4_quota("ca_basin", 0, 99).expect("q=0 v=99");
        assert!(
            oob.tag.contains("storm_magnum"),
            "out-of-range variant should fall through to default, got {}",
            oob.tag
        );

        // Out-of-range quota_index returns None.
        assert!(resolve_halo4_quota("ca_basin", 999, 0).is_none());
        assert!(resolve_halo4_quota("nonexistent_map", 0, 0).is_none());
    }
}
