//! Halo 3 retail Forge palette (per-map slot -> tag info) + wire-quota resolver.
//!
//! Bundles the parsed per-class palette blocks from every shipped H3 .map.
//! Per-class blocks live at engine-specific scnr offsets reached via a
//! 12-entry kind switch (RE'd in halo3.dll H3_mvar_palette_kind_switch
//! @ +0xB7308) — Scenery@scnr+0xC0, Vehicle@+0xF0, Equipment@+0x108,
//! Weapon@+0x120, MapVariantVehicle@+0x1E0, ..., Crates@+0x5C8.
//!
//! ## Wire-quota encoding
//!
//! Each `s_variant_quota.object_definition_index` is a packed u32:
//!   * `palette_kind = (idx >> 16) & 0xFFFF`  — engine enum 1..12
//!   * `entry_index  = (int16_t)(idx & 0xFFFF)` — signed; -1 = no entry
//!
//! Two-level wire lookup from a placed object:
//!   1. `m_variant_objects[i].variant_quota_index` -> `m_quotas[i]` (0..255)
//!   2. `m_quotas[i].object_definition_index`      -> `(kind, entry)` split
//!   3. `kind` -> scnr palette block (via H3_mvar_palette_kind_switch)
//!   4. `entry` indexes that block, stride 0x1C for MapVariant* (kind 1..7)
//!      or 0x10 for engine palettes (kind 8..12); datum lives at +0xC.
//!
//! See `H3_mvar_quota_resolve` @ halo3.dll+0xB7598 for the runtime impl
//! and `H3_mvar_quota_index_by_datum_linear` @ halo3.dll+0xB8D6C for the
//! reverse lookup (linear m_quotas scan by tag datum).
//!
//! ## JSONL schema
//!
//! ```text
//! {"pal":0,"name":"Scenery","entry":0,"class":"scen","datum":3793354916,
//!  "tag":"objects\\multi\\spawning\\respawn_point","map":"sandbox"}
//! ```
//!
//! The JSONL `pal` field uses a HISTORICAL "Nth scnr palette in Assembly
//! Halo3MCC scnr.xml" numbering (0=Scenery, 1=Biped, 2=Vehicle, 3=Equipment,
//! 4=Weapon, ..., 12=MapVariantVehicle, ..., 25=Crates). The engine's mvar
//! palette_kind (1..12) only ever indexes a SUBSET of these — the other
//! pals (1,5..11,19..24,26,27) appear in the JSONL for scnr-inventory
//! completeness but are never reachable from an mvar quota. Use
//! `h3_palette_kind_to_jsonl_pal` to map between the two numberings.
//!

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// halo3.dll H3_mvar_palette_kind_switch @ +0xB7308 — 12-entry kind->scnr-offset switch
// halo3.dll H3_mvar_quota_resolve @ +0xB7598 — splits object_definition_index, returns datum
// halo3.dll H3_mvar_quota_index_by_datum_linear @ +0xB8D6C — reverse: datum -> quota slot
// halo3.dll H3_mvar_quota_budget_recompute_on_change @ +0xBA3F0 — runtime budget walker
// halo3.dll H3MCC_cmapvariant_encode_decode_loop_BodyChunk @ +0xB8E9C — bitstream codec
static HALO3_PALETTES_JSONL: &str =
    include_str!("../../../../assets/maps/halo3_palettes_bundled.jsonl");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct H3PaletteRecord {
    /// Map cache name (e.g. `"sandbox"`, `"high_ground"`).
    pub map: String,
    /// JSONL palette category index (legacy "Nth scnr palette" numbering,
    /// 0..27). To go from a wire palette_kind (1..12), use
    /// [`h3_palette_kind_to_jsonl_pal`].
    pub pal: u32,
    /// Category label (`"Scenery"`, `"Vehicle"`, `"MapVariantWeapon"`, ...).
    pub name: String,
    /// Entry slot within the category — directly comparable to the
    /// (signed) low16 of `object_definition_index`.
    pub entry: u32,
    /// Tag class fourcc (`"scen"`, `"weap"`, `"vehi"`, ...).
    pub class: String,
    /// Tag datum index from the .map's tag table.
    pub datum: u32,
    /// Resolved tag path. Tag names already came out of Blamite at extraction
    /// time, so no separate datum->name lookup is needed.
    pub tag: String,
}

fn parse_all() -> Vec<H3PaletteRecord> {
    HALO3_PALETTES_JSONL
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<H3PaletteRecord>(l).ok())
        .collect()
}

/// All H3 retail palette records across every shipped map.
pub fn halo3_palettes() -> Vec<H3PaletteRecord> {
    parse_all()
}

/// `map_name -> list of palette records` for fast per-map lookup.
pub fn halo3_palettes_by_map() -> HashMap<String, Vec<H3PaletteRecord>> {
    let mut out: HashMap<String, Vec<H3PaletteRecord>> = HashMap::new();
    for r in parse_all() {
        out.entry(r.map.clone()).or_default().push(r);
    }
    out
}

/// Resolve `(map, pal, entry)` -> palette record. `pal` here is the JSONL
/// numbering (NOT the wire palette_kind — see
/// [`h3_palette_kind_to_jsonl_pal`]).
pub fn h3_lookup(
    map_name: &str,
    pal: u32,
    entry: u32,
) -> Option<H3PaletteRecord> {
    parse_all()
        .into_iter()
        .find(|r| r.map == map_name && r.pal == pal && r.entry == entry)
}

/// Map an engine wire `palette_kind` (1..12 from the high16 of
/// `s_variant_quota.object_definition_index`) to the JSONL `pal` numbering.
///
/// Returns `None` for kind 0 (unused/null quota) and any kind outside
/// 1..=12 (no scnr palette assigned). Mapping is the exact switch table
/// in halo3.dll `H3_mvar_palette_kind_switch` @ +0xB7308.
///
/// | kind | scnr off | name                | jsonl pal |
/// |------|----------|---------------------|-----------|
/// |  1   | 0x1E0    | MapVariantVehicle   | 12        |
/// |  2   | 0x1EC    | MapVariantWeapon    | 13        |
/// |  3   | 0x1F8    | MapVariantEquipment | 14        |
/// |  4   | 0x204    | MapVariantScenery   | 15        |
/// |  5   | 0x210    | MapVariantTeleporter| 16        |
/// |  6   | 0x21C    | MapVariantGoal      | 17        |
/// |  7   | 0x228    | MapVariantSpawner   | 18        |
/// |  8   | 0x0C0    | Scenery             |  0        |
/// |  9   | 0x0F0    | Vehicle             |  2        |
/// | 10   | 0x120    | Weapon              |  4        |
/// | 11   | 0x108    | Equipment           |  3        |
/// | 12   | 0x5C8    | Crates              | 25        |
pub fn h3_palette_kind_to_jsonl_pal(kind: u32) -> Option<u32> {
    Some(match kind {
        1 => 12,
        2 => 13,
        3 => 14,
        4 => 15,
        5 => 16,
        6 => 17,
        7 => 18,
        8 => 0,
        9 => 2,
        10 => 4,
        11 => 3,
        12 => 25,
        _ => return None,
    })
}

/// Split a wire `s_variant_quota.object_definition_index` u32 into
/// `(palette_kind, entry_index)`.
///
/// * `palette_kind`  = `(idx >> 16) & 0xFFFF` — engine enum, valid range 1..12.
/// * `entry_index`   = `(int16_t)(idx & 0xFFFF)` — SIGNED 16-bit; -1 is a
///                     sentinel for "no entry" (yields no palette lookup).
///
/// halo3.dll `H3_mvar_quota_resolve` @ +0xB7598 does the same split at
/// 1800b75e5..1800b75ee (movsx ax for the signed cast).
pub fn h3_split_object_definition_index(idx: u32) -> (u32, i16) {
    let kind = (idx >> 16) & 0xFFFF;
    let entry = (idx & 0xFFFF) as i16;
    (kind, entry)
}

/// Resolve a wire `object_definition_index` u32 directly to a palette record.
///
/// This is the headless equivalent of halo3.dll `H3_mvar_quota_resolve`
/// @ +0xB7598. Steps:
///   1. Split `idx` into `(palette_kind, entry_index)` via
///      [`h3_split_object_definition_index`].
///   2. Map `palette_kind` -> jsonl `pal` via
///      [`h3_palette_kind_to_jsonl_pal`].
///   3. Look up `(map, pal, entry)` in the bundled per-map palette.
///
/// Returns `None` if the kind is out of 1..=12, the entry is negative, or
/// the slot isn't present in this map's palette JSONL.
pub fn h3_resolve_object_definition_index(
    map_name: &str,
    object_definition_index: u32,
) -> Option<H3PaletteRecord> {
    let (kind, entry) = h3_split_object_definition_index(object_definition_index);
    let pal = h3_palette_kind_to_jsonl_pal(kind)?;
    if entry < 0 {
        return None;
    }
    h3_lookup(map_name, pal, entry as u32)
}

/// Resolve a wire `(variant_quota_index, m_quotas[])` pair from a placed
/// variant object to a palette record.
///
/// The two-level indirection in the wire format:
///   * `m_variant_objects[i].variant_quota_index: i32` indexes `m_quotas`.
///   * `m_quotas[variant_quota_index].object_definition_index: u32` is the
///     packed `(palette_kind, entry)` resolved above.
///
/// `quotas` is the in-order slice of `(object_definition_index, placed)`
/// or just `object_definition_index` values — pass `&[u32]` and let the
/// caller pluck it out of `c_map_variant.m_quotas`.
///
/// Returns `None` for out-of-range `variant_quota_index`, kind 0/-1
/// sentinels, or any failed jsonl lookup.
pub fn h3_resolve_quota_index(
    map_name: &str,
    variant_quota_index: i32,
    quotas_object_def_indices: &[u32],
) -> Option<H3PaletteRecord> {
    if variant_quota_index < 0 {
        return None;
    }
    let i = variant_quota_index as usize;
    let odi = *quotas_object_def_indices.get(i)?;
    h3_resolve_object_definition_index(map_name, odi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonempty() {
        let p = halo3_palettes();
        assert!(p.len() > 1000, "expected 1000+ records, got {}", p.len());
    }

    #[test]
    fn sandbox_resolves_known_items() {
        let r = h3_lookup("sandbox", 0, 2).expect("sandbox pal=0 entry=2 should exist");
        assert_eq!(r.class, "scen");
        assert!(
            r.tag.contains("spawn_point"),
            "expected spawn_point tag, got {}",
            r.tag
        );
    }

    /// Pin the engine palette_kind -> jsonl pal map. Exact mirror of the
    /// switch table in halo3.dll H3_mvar_palette_kind_switch @ +0xB7308.
    #[test]
    fn kind_to_pal_full_table() {
        assert_eq!(h3_palette_kind_to_jsonl_pal(0), None, "kind 0 = null/unused");
        assert_eq!(h3_palette_kind_to_jsonl_pal(13), None, "kind 13 = out of range");
        assert_eq!(h3_palette_kind_to_jsonl_pal(0xFFFF), None);

        let expect = [
            ( 1, 12), // MapVariantVehicle
            ( 2, 13), // MapVariantWeapon
            ( 3, 14), // MapVariantEquipment
            ( 4, 15), // MapVariantScenery
            ( 5, 16), // MapVariantTeleporter
            ( 6, 17), // MapVariantGoal
            ( 7, 18), // MapVariantSpawner
            ( 8,  0), // Scenery
            ( 9,  2), // Vehicle
            (10,  4), // Weapon
            (11,  3), // Equipment
            (12, 25), // Crates
        ];
        for (kind, pal) in expect {
            assert_eq!(
                h3_palette_kind_to_jsonl_pal(kind),
                Some(pal),
                "kind={} should map to pal={}",
                kind, pal
            );
        }
    }

    /// Pin the bit-level split of object_definition_index. Sentinels matter
    /// because halo3.dll's H3_mvar_quota_resolve early-outs at -1 BEFORE the
    /// kind switch — but only the FULL -1 case (idx == 0xFFFFFFFF) triggers
    /// the early-out; individual high16==0xFFFF / low16==0xFFFF aren't
    /// treated as universal sentinels here. We expose only the split.
    #[test]
    fn split_object_definition_index() {
        // sandbox Scenery entry 2 (oddball_initial_spawn_point per the JSONL):
        // kind=8 (Scenery), entry=2 -> packed 0x00080002
        assert_eq!(h3_split_object_definition_index(0x00080002), (8, 2));
        // Crates entry 0: kind=12, entry=0 -> 0x000C0000
        assert_eq!(h3_split_object_definition_index(0x000C0000), (12, 0));
        // -1 entry sentinel: kind=8, entry=-1 -> 0x0008FFFF
        assert_eq!(h3_split_object_definition_index(0x0008FFFF), (8, -1));
        // High16==0 means "no palette" per the kind switch (returns null)
        assert_eq!(h3_split_object_definition_index(0x00000005), (0, 5));
    }

    /// End-to-end: pack a known sandbox slot via the engine's encoding,
    /// run it through h3_resolve_object_definition_index, expect the same
    /// record back. This is the inverse of the wire decoder.
    #[test]
    fn sandbox_wire_quota_resolves() {
        // sandbox kind=8 (Scenery) entry=2 -> oddball_initial_spawn_point
        let r = h3_resolve_object_definition_index("sandbox", 0x00080002)
            .expect("sandbox wire-quota 0x00080002 should resolve");
        assert_eq!(r.pal, 0, "Scenery jsonl pal=0");
        assert_eq!(r.entry, 2);
        assert_eq!(r.class, "scen");
        assert!(
            r.tag.contains("oddball_initial_spawn_point"),
            "expected oddball_initial_spawn_point, got {}",
            r.tag
        );
    }

    /// Verify a MapVariant* palette (kind 1..7 = 28B stride in the engine
    /// but unchanged in JSONL): MapVariantVehicle entry 0 on sandbox.
    #[test]
    fn sandbox_mapvariant_vehicle_entry0_resolves() {
        // kind=1 (MapVariantVehicle), entry=0 -> packed 0x00010000
        let r = h3_resolve_object_definition_index("sandbox", 0x00010000)
            .expect("sandbox kind=1 entry=0 should resolve");
        assert_eq!(r.pal, 12, "MapVariantVehicle jsonl pal=12");
        assert_eq!(r.entry, 0);
        assert_eq!(r.class, "vehi");
        assert!(!r.tag.is_empty(), "tag should be resolved");
    }

    /// Kind 0 (null), kind > 12 (invalid), and negative entry all yield
    /// None. Mirrors the early-outs in H3_mvar_quota_resolve.
    #[test]
    fn invalid_wire_quotas_yield_none() {
        assert!(h3_resolve_object_definition_index("sandbox", 0xFFFFFFFF).is_none());
        assert!(h3_resolve_object_definition_index("sandbox", 0x00000000).is_none(), "kind=0 -> null");
        assert!(h3_resolve_object_definition_index("sandbox", 0x000D0000).is_none(), "kind=13 -> oob");
        assert!(h3_resolve_object_definition_index("sandbox", 0x0008FFFF).is_none(), "entry=-1 -> reject");
    }

    /// Two-level wire lookup: variant_object.variant_quota_index ->
    /// m_quotas[variant_quota_index].object_definition_index -> resolve.
    /// Simulates a real wire decode where the placed-object record points
    /// into the per-map quotas table.
    #[test]
    fn two_level_quota_index_resolves() {
        // Simulate sandbox's m_quotas where slot 7 is Scenery entry 2.
        let quotas: Vec<u32> = vec![
            0x00080000, // slot 0: Scenery entry 0
            0x00080001,
            0x00080003,
            0x000C0000, // slot 3: Crates entry 0
            0x00010000, // slot 4: MapVariantVehicle entry 0
            0xFFFFFFFF, // slot 5: empty
            0x00000000, // slot 6: kind=0 null
            0x00080002, // slot 7: Scenery entry 2 (oddball_initial_spawn_point)
        ];

        let r = h3_resolve_quota_index("sandbox", 7, &quotas)
            .expect("variant_quota_index=7 should resolve via two-level lookup");
        assert!(
            r.tag.contains("oddball_initial_spawn_point"),
            "got {}",
            r.tag
        );

        // Out-of-range and sentinel slot 5/6 yield None.
        assert!(h3_resolve_quota_index("sandbox", -1, &quotas).is_none());
        assert!(h3_resolve_quota_index("sandbox", 99, &quotas).is_none());
        assert!(h3_resolve_quota_index("sandbox", 5, &quotas).is_none());
        assert!(h3_resolve_quota_index("sandbox", 6, &quotas).is_none());
    }
}
