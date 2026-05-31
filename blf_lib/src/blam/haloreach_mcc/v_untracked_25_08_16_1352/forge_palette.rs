//! Reach MCC Forge palette (slot index -> tag info).
//!
//! Bundles the parsed Map Variant Palettes block from forge_halo.map.
//! Each record is one (category, entry, variant) tuple with the resolved
//! tag class + datum index + variant-name stringID + max_allowed.
//!
//! Combined with `friendly_tags` (datum_index -> tag name) this gives
//! the slot_index -> friendly_name lookup needed to display mvar contents.
//!
//! Provenance: RE'd 2026-05-25. scnr+0x228 tag block has 16 categories
//! (20B each: name_id, pad, count, ptr, pad). Each category has N entries
//! (28B each: max_allowed, variants_count, variants_ptr, pad ×4). Each
//! entry has N variants (24B each: variant_name_sid, group_magic_be,
//! pad, datum_index_le, pad). Pointer expansion uses ReachMCC
//! expand_magic = 0x50000000 (from Blamite Engines.xml inheritance).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// haloreach.dll mvar_palette_table_post_process_unverified @ +0x70AF8 — palette table walker (scnr+0x228 in MCC)
static FORGE_HALO_PALETTE_JSONL: &str = include_str!("../../../../assets/forge_halo_palette.jsonl");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaletteRecord {
    pub cat: u32,
    pub cat_name_id: u32,
    pub entry: u32,
    pub variant: u32,
    pub variant_name_stringid: u32,
    pub tag_class: String,
    pub datum_index: u32,
    pub max_allowed: u32,
}

/// Returns ALL palette records (including all sub-variants), in JSONL
/// order. Use `forge_halo_quota_entries()` when you need the engine's
/// entry-level enumeration (variants collapsed) — that's what a wire
/// `variant_quota_index` indexes into.
pub fn forge_halo_palette() -> Vec<PaletteRecord> {
    let mut out = Vec::new();
    for line in FORGE_HALO_PALETTE_JSONL.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<PaletteRecord>(line) {
            Ok(rec) => out.push(rec),
            Err(_) => {}
        }
    }
    out
}

/// Returns one record per UNIQUE (cat, entry) — i.e. variants collapsed
/// to the variant=0 canonical record. This is the engine's load-time
/// enumeration of the Map Variant Palettes block: `variant_quota_index`
/// from a placed object indexes into THIS list. Sub-variants are then
/// selected by the variant_object's separate `variant_index` field.
/// haloreach.dll mvar_payload_decode_unverified @ +0x6D468 reads exactly
/// `palette_max` (== entries.len()) 3-byte quota rows into obj+0xD640.
pub fn forge_halo_quota_entries() -> Vec<PaletteRecord> {
    forge_halo_palette().into_iter().filter(|r| r.variant == 0).collect()
}

/// Build a (cat, entry, variant) -> record lookup map.
pub fn forge_halo_palette_by_tuple() -> HashMap<(u32, u32, u32), PaletteRecord> {
    forge_halo_palette()
        .into_iter()
        .map(|r| ((r.cat, r.entry, r.variant), r))
        .collect()
}

/// Resolve a wire `(variant_quota_index, variant_index)` pair to:
/// (tag_class, datum_index, variant_name_stringid).
/// `quota_index` indexes the entry-level enumeration; `variant_index`
/// picks the sub-variant within the entry (or 0 for entries with no
/// sub-variants). Returns None if quota_index is out of range.
pub fn resolve_forge_halo_quota(
    quota_index: u32,
    variant_index: u32,
) -> Option<(String, u32, u32)> {
    let entries = forge_halo_quota_entries();
    let base = entries.get(quota_index as usize)?;
    let all = forge_halo_palette();
    let variant_rec = all.iter().find(|r| {
        r.cat == base.cat && r.entry == base.entry && r.variant == variant_index
    });
    let rec = variant_rec.unwrap_or(base);
    Some((rec.tag_class.clone(), rec.datum_index, rec.variant_name_stringid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blam::haloreach_mcc::v_untracked_25_08_16_1352::friendly_tags::forge_halo_tag_table;

    #[test]
    fn user_known_mvar_items_resolve_via_palette_to_friendly_names() {
        let palette = forge_halo_palette();
        let tags = forge_halo_tag_table();
        let banshee = palette
            .iter()
            .find(|r| r.cat == 3 && r.entry == 0 && r.variant == 0)
            .expect("banshee palette entry not found");
        let warthog = palette
            .iter()
            .find(|r| r.cat == 3 && r.entry == 7 && r.variant == 0)
            .expect("warthog default palette entry not found");
        let warthog_gauss_slot = palette
            .iter()
            .find(|r| r.cat == 3 && r.entry == 7 && r.variant == 1)
            .expect("warthog gauss palette entry not found");

        let banshee_name = tags
            .get(&(banshee.datum_index, [b'v', b'e', b'h', b'i']))
            .expect("banshee datum should resolve");
        let warthog_name = tags
            .get(&(warthog.datum_index, [b'v', b'e', b'h', b'i']))
            .expect("warthog datum should resolve");
        let warthog_gauss_base = tags
            .get(&(warthog_gauss_slot.datum_index, [b'v', b'e', b'h', b'i']))
            .expect("warthog gauss base datum should resolve");

        assert_eq!(banshee_name, "objects\\vehicles\\covenant\\banshee\\banshee");
        assert_eq!(warthog_name, "objects\\vehicles\\human\\warthog\\warthog");
        assert_eq!(warthog_gauss_base, "objects\\vehicles\\human\\warthog\\warthog");
        assert_ne!(
            warthog.variant_name_stringid, warthog_gauss_slot.variant_name_stringid,
            "warthog and warthog_gauss should differ in their sub-variant stringID"
        );
    }

    #[test]
    fn palette_record_count() {
        let palette = forge_halo_palette();
        assert!(palette.len() > 100, "expected 100+ palette records, got {}", palette.len());
    }

    /// Pins the engine's load-time enumeration of forge_halo Map Variant
    /// Palettes (entry-level — variants collapsed). The wire
    /// `variant_quota_index` indexes into THIS list. Specific slots verified
    /// against the user-supplied "3 VEHICLES TEST MAP" mvar (f6a16be6) where:
    ///   q=31 placed = banshee
    ///   q=38 placed = warthog
    ///   q=54 placed = initial_spawn_point  (NOT ff_grav_lift)
    /// Also matches haloreach.dll mvar_payload_decode_unverified @ +0x6D468
    /// which reads palette_max (m_number_of_placeable_object_quotas, 9 bits)
    /// equal to entries.len() == 145.
    #[test]
    fn forge_halo_entry_level_indexing_matches_engine() {
        let entries = forge_halo_quota_entries();
        let tags = forge_halo_tag_table();
        assert_eq!(entries.len(), 145,
            "engine reads palette_max=145 quotas for forge_halo, got {}", entries.len());

        let resolve = |idx: u32, want: &str| {
            let r = entries.get(idx as usize)
                .unwrap_or_else(|| panic!("entries[{}] should exist", idx));
            let mut cls_bytes = [0u8; 4];
            let b = r.tag_class.as_bytes();
            for k in 0..b.len().min(4) { cls_bytes[k] = b[k]; }
            let name = tags.get(&(r.datum_index, cls_bytes))
                .unwrap_or_else(|| panic!("no tag name for entries[{}] datum 0x{:08X}", idx, r.datum_index));
            assert!(name.contains(want),
                "entries[{}] should resolve to a tag containing {:?}, got {:?}", idx, want, name);
        };
        resolve(31, "banshee");
        resolve(38, "warthog");
        resolve(54, "initial_spawn_point");
    }
}
