//! Halo 4 (v50) c_content_item_metadata + c_map_variant override.
//!
//! Bit-level cross-reference against the user-supplied known-plaintext sample
//! (name + description both "This is Halo 4") proved the H4 content_metadata
//! schema differs from Reach v12065 in TWO ways:
//!
//!  1. extended_ascii max for creator_name/modifier_name = 15 (not 16)
//!  2. NO `modifier_is_online` bit between modifier_name and `name` wchar
//!
//! Verification: bit-trace placed `name` field at bit 838 for the H4 sample
//! (15 + 1 + 64 + 64 + 15 = 369 bits added to creator_xuid_end=469 = 838 ✓).
//! Same schema reproduces description@1078 = 838+240 (15 u16s of "This is Halo 4" + NUL).

use std::cmp::min;
use blf_lib::io::bitstream::{c_bitstream_reader, c_bitstream_writer, e_bitstream_byte_order};
use blf_lib::blam::haloreach::v12065_11_08_24_1738_tu1actual::saved_games::saved_game_files::c_content_item_metadata;
use blf_lib::blam::haloreach_mcc::v_untracked_25_08_16_1352::saved_games::scenario_map_variant::{c_map_variant, H4MetaSchemaTag, k_maximum_variant_objects, k_maximum_variant_quotas};
use blf_lib_derivable::result::BLFLibResult;
use crate::types::c_string::{StaticString, StaticWcharString};

/// H4/H2A `s_variant_multiplayer_object_properties_definition::decode` —
/// RE'd from groundhog.dll `H2A_mp_object_properties_decode` @ 0x1800B6348
/// (identical to halo4.dll's equivalent). Bit-stream pattern (verified via
/// static disasm):
///
/// ```
/// boundary_decode():
///   read(2) → shape
///   if shape == 1: 1 × read(16)        # sphere: size
///   if shape == 2: 3 × read(16)        # cylinder: size, +height, -height
///   if shape == 3: 4 × read(16)        # box: size, length, +height, -height
/// read(6)  → cached_type
/// read(11) → label_index
/// match cached_type:
///   0x20: read(4) + read(8) + read(16)             # 28 bits
///   0x21|0x22: 9 × read(16)                        # 144 bits
///   0x23: 10 × read(4)                             # 40 bits
///   else: 0 bits
/// ```
///
/// Total: 19 to 227 bits per variant_object (vs Reach's ~50-120). The bit
/// stream values are NOT stored into a struct here (we only need bitstream
/// alignment for downstream variant_objects to read correctly); a fully
/// typed Rust struct lives at struct +0x34 in the in-engine `variant_object`
/// layout but blf_lib's `c_map_variant` doesn't yet have an H4/H2A-specific
/// `s_variant_multiplayer_object_properties_definition` distinct from Reach's.
/// H4/H2A `simulation_read_position` — CRITICAL difference from Reach.
///
/// Reach's `simulation_read_position` errors when `in_bounds=0` (no BSP fallback).
/// H4/H2A `H2A_simulation_read_position` (FUN_1805176B8) takes the same per-axis
/// loop **regardless of in_bounds** when `world_bounds != NULL`. The in_bounds
/// bit only affects which BSP-cluster table is consulted (default vs current).
///
/// Per disasm:
/// ```
/// in_bounds = read(1)
/// if in_bounds=1 && world_bounds!=NULL: jump to LOOP
/// if in_bounds=1 && world_bounds==NULL: load defaults, return (no further reads)
/// if in_bounds=0 && world_bounds!=NULL: jump to LOOP  (NOT an error)
/// if in_bounds=0 && world_bounds==NULL: call FUN_1800C3744 helper (1-3 bits)
/// LOOP: compute per_axis_bit_counts(world_bounds, axis_encoding) → no bits
///       for i in 0..3: read per_axis[i] bits  → axis values
/// ```
///
/// For variant_object reads (world_bounds always provided), this means:
///   1 bit in_bounds + 3 × per_axis_bits (no BSP fallback path needed)
///
/// blf_lib's Reach decoder errors on in_bounds=0. We need to consume the per-axis
/// bits even when in_bounds=0 to stay aligned with H4/H2A's stream.
pub fn h4_h2a_simulation_read_position(
    bitstream: &mut c_bitstream_reader,
    position: &mut blf_lib::blam::common::math::real_math::real_point3d,
    axis_encoding_size_in_bits: usize,
    exact_midpoints: bool,
    exact_endpoints: bool,
    world_bounds: &blf_lib::blam::common::math::real_math::real_rectangle3d,
) -> BLFLibResult {
    use blf_lib::blam::common::math::integer_math::int32_point3d;
    use blf_lib::blam::common::simulation::simulation_encoding::adjust_axis_encoding_bit_count_to_match_error_goals;
    use blf_lib::blam::haloreach::v12065_11_08_24_1738_tu1actual::math::real_math::dequantize_real_point3d_per_axis;

    // groundhog.dll H2A_simulation_read_position @ +0x5176B8 — 1-bit in_bounds (informational)
    let _in_bounds: bool = bitstream.read_bool("point-in-initial-bounds")?;
    let mut per_axis_bit_counts = int32_point3d::default();
    // groundhog.dll H2A_position_adjust_axis_encoding @ +0x51B1D0 — pure math, no bit reads
    adjust_axis_encoding_bit_count_to_match_error_goals(
        axis_encoding_size_in_bits,
        world_bounds,
        26,
        &mut per_axis_bit_counts,
    );
    let mut quantized_point = int32_point3d::default();
    bitstream.read_point3d_efficient(&mut quantized_point, per_axis_bit_counts)?;
    dequantize_real_point3d_per_axis(
        &quantized_point,
        world_bounds,
        &per_axis_bit_counts,
        position,
        exact_midpoints,
        exact_endpoints,
    );
    Ok(())
}

/// H4/H2A position decoder companion: consumes the EXTRA bits H2A reads vs Reach.
///
/// Reach's `simulation_read_position` consumes: 1 (in_bounds) + per_axis_bits.
/// H2A's `simulation_read_position` (FUN_1805176B8) additionally calls
/// `H2A_position_in_bounds_helper` (FUN_1800C3744) AFTER the in_bounds bit.
///
/// `H2A_position_in_bounds_helper` reads:
///   - 1 bit (first_bit)
///   - if first_bit == 1: return -1 (consume 1 bit total, take OOB fallback)
///   - if first_bit == 0: read 2 more bits (consume 3 bits total, in-bounds path)
///
/// Since blf_lib's `simulation_read_position` is Reach's version (which only
/// reads 1+per_axis), we need to consume the EXTRA helper bits separately to
/// keep the cursor aligned with H2A's actual bit-stream.
///
/// Per disasm of H2A_simulation_read_position: the helper is called only when
/// the outer in_bounds bit is 1. We've already consumed that bit via the Reach
/// decoder. We just need the helper's extra 1-3 bits here.
pub fn h4_h2a_position_in_bounds_helper(bitstream: &mut c_bitstream_reader) -> BLFLibResult<i32> {
    // groundhog.dll H2A_position_in_bounds_helper @ +0xC3744 — 1 OR 3 bits
    let first_bit: bool = bitstream.read_unnamed_bool()?;
    if first_bit {
        Ok(-1)
    } else {
        let value: i32 = bitstream.read_unnamed_integer(2)?;
        Ok(value)
    }
}

/// H4/H2A `read_axes` (forward + up vectors). RE'd from groundhog.dll
/// `H2A_variant_object_read_axes` @ `0x1800B7D10`. Differs from Reach's
/// `read_axes<14, 20>` in TWO ways:
///   1. 1-bit "is_default" prefix flag. If set, the function loads constant
///      default axes and consumes only 1 bit.
///   2. Field widths are SWAPPED: H4/H2A reads `read(20)` for forward angle
///      then a quantized real with `size_in_bits=14` for up rotation (Reach
///      is `read(14)` + `read(20)`).
///
/// Total: 1 bit (default) or 1 + 20 + 14 = 35 bits (non-default).
/// Reach: unconditional 34 bits. So H4/H2A consumes 1-35 bits depending on flag.
pub fn h4_h2a_read_axes_consume_bits(bitstream: &mut c_bitstream_reader) -> BLFLibResult {
    // groundhog.dll H2A_variant_object_read_axes @ +0xB7D10 — 1 bit (default) OR 1 + 20 + 14 = 35 bits
    let is_default: bool = bitstream.read_unnamed_bool()?;
    if is_default { return Ok(()); }
    let _forward_angle: u32 = bitstream.read_unnamed_integer(20)?;
    let _up_rotation_quantized: u32 = bitstream.read_unnamed_integer(14)?;
    Ok(())
}

pub fn h4_h2a_mp_object_properties_consume_bits(bitstream: &mut c_bitstream_reader) -> BLFLibResult {
    // groundhog.dll H2A_mp_obj_boundary_decode @ +0x691FE8 — 2-bit shape selector
    let shape: u32 = bitstream.read_unnamed_integer(2)?;
    let boundary_field_count = match shape {
        1 => 1, // sphere
        2 => 3, // cylinder
        3 => 4, // box
        _ => 0, // unused (0) or unknown
    };
    for _ in 0..boundary_field_count {
        let _field: u32 = bitstream.read_unnamed_integer(16)?;
    }
    // groundhog.dll H2A_mp_object_properties_decode @ +0xB6348 — 6-bit cached_type + 11-bit label_index (H2A)
    let cached_type: u8 = bitstream.read_unnamed_integer(6)?;
    let _label_index: u32 = bitstream.read_unnamed_integer(11)?;
    match cached_type {
        0x20 => {
            let _: u32 = bitstream.read_unnamed_integer(4)?;
            let _: u32 = bitstream.read_unnamed_integer(8)?;
            let _: u32 = bitstream.read_unnamed_integer(16)?;
        }
        0x21 | 0x22 => {
            for _ in 0..9 {
                let _: u32 = bitstream.read_unnamed_integer(8)?;
            }
        }
        0x23 => {
            let _: u32 = bitstream.read_unnamed_integer(4)?;
            let _: u32 = bitstream.read_unnamed_integer(8)?;
            let _ix: i32 = bitstream.read_unnamed_index::<8>(3)?;
            for _ in 0..3 {
                let _: u32 = bitstream.read_unnamed_integer(8)?;
            }
            for _ in 0..4 {
                let _ix: i32 = bitstream.read_unnamed_index::<256>(8)?;
            }
            for _ in 0..7 {
                let _: u32 = bitstream.read_unnamed_integer(5)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Read a fixed-length byte string from the bitstream as a Latin1 String, no NUL
/// termination required. Used for H4 author-name fields where the bytes are
/// FIXED 15-bytes (no NUL check, unlike Reach's NUL-terminated ext_ascii(16)).
fn read_fixed_ascii(bitstream: &mut c_bitstream_reader, n: usize) -> BLFLibResult<String> {
    let mut bytes = vec![0u8; n];
    for i in 0..n {
        bytes[i] = bitstream.read_unnamed_integer(8)?;
    }
    Ok(bytes.into_iter().map(|b| b as char).collect())
}

/// Round-trip-fidelity variant of `read_fixed_ascii`. Returns the literal
/// on-wire bytes (no `b as char` re-encoding) so the encoder can emit them
/// verbatim. The returned Vec is always exactly `n` bytes long.
fn read_fixed_ascii_raw(bitstream: &mut c_bitstream_reader, n: usize) -> BLFLibResult<Vec<u8>> {
    let mut bytes = vec![0u8; n];
    for i in 0..n {
        bytes[i] = bitstream.read_unnamed_integer(8)?;
    }
    Ok(bytes)
}

fn write_fixed_ascii(bitstream: &mut c_bitstream_writer, s: &str, n: usize) -> BLFLibResult {
    let chars: Vec<u8> = s.chars().take(n).map(|c| c as u8).collect();
    for i in 0..n {
        let b = chars.get(i).copied().unwrap_or(0);
        bitstream.write_integer(b, 8)?;
    }
    Ok(())
}

/// Round-trip-fidelity variant of `write_fixed_ascii`. Emits up to `n`
/// raw bytes from the input slice, zero-padding the remainder. The input
/// is the literal on-wire byte sequence captured by `read_fixed_ascii_raw`.
fn write_fixed_ascii_raw(bitstream: &mut c_bitstream_writer, bytes: &[u8], n: usize) -> BLFLibResult {
    for i in 0..n {
        let b = bytes.get(i).copied().unwrap_or(0);
        bitstream.write_integer(b, 8)?;
    }
    Ok(())
}

/// Which on-wire metadata schema is in use for the current H4 mvar payload.
///
/// In the wild we've encountered TWO distinct schemas for the `c_content_item_metadata`
/// region in H4 (v50) mvars:
///
/// * `FixedFifteen` — author/modifier names are FIXED 15 bytes (NO NUL terminator
///   consumed from wire), and there is NO `modifier_is_online` bit between
///   modifier_name and the wchar name field. Discovered 2026-05-25 by bit-trace
///   against a user-supplied "This is Halo 4" sample. ~95% of H4 corpus files.
///
/// * `NulBoundedSixteen` — matches the halo4.dll engine asm
///   (`H4_c_content_item_metadata_decode` @ 0x6A43C →
///   `bitread_ascii_zstring` @ 0xF5EB8, r9d=0x10). Reads UP TO 16 bytes, stops
///   on NUL OR after exactly 16 bytes. Also consumes a `modifier_is_online`
///   1-bit between modifier_name and wchar name. ~5% of H4 corpus files
///   (the 8,425 files in `h4_v50_over_read_eos` that previously failed EOS).
///
/// The two schemas are wire-format incompatible — applying the wrong schema
/// shifts every subsequent bit by ~9-50 bits, cascading into garbage
/// `m_map_variant_version` / `m_number_of_placeable_object_quotas` values
/// and eventually an EOS while reading the string_table or beyond.
///
/// `decode_map_variant_h4` tries `FixedFifteen` first (majority case),
/// captures the post-decode cursor, and on body-level EOS rewinds + retries
/// with `NulBoundedSixteen`. The chosen variant is recorded on the metadata
/// struct (`raw_creator_name_bytes` length plus a flag) so the encoder can
/// emit the matching wire shape.
#[derive(Default, PartialEq, Debug, Clone, Copy)]
pub enum H4MetaSchema {
    /// Fixed 15-byte creator + 15-byte modifier + NO modifier_is_online.
    /// Uses 3-bit activity (legacy / self-consistent round-trip; NOT engine-correct).
    #[default]
    FixedFifteen,
    /// NUL-bounded max-16 creator + max-16 modifier + 1-bit modifier_is_online.
    /// Uses 3-bit activity (engine-incorrect; preserved for byte-identical RT of
    /// files originally decoded under this schema).
    NulBoundedSixteen,
    /// ENGINE-CORRECT schema (RE'd 2026-05-27 against halo4.dll
    /// `H4_c_content_item_metadata_decode` @ 0x18006A43C + groundhog.dll
    /// equivalent @ 0x18006A024). Differences vs Reach (and from S1/S2):
    ///   * `activity` field is **2 bits** (not 3) — `mov r12d, 2` at +0xd8
    ///     in the decoder.
    ///   * creator_name + modifier_name are **NUL-bounded up to 16 bytes**
    ///     (engine `bitread_ascii_zstring` with cap=16; same as S2).
    ///   * `creator_is_online` (1 bit) AND `modifier_is_online` (1 bit)
    ///     both present.
    ///
    /// Schema 3 unblocks the ~8,425 H4 files (and ~444 H2A files) that
    /// previously failed under both FixedFifteen and NulBoundedSixteen with
    /// "failed to fill whole buffer" EOS errors. The cascading EOS occurred
    /// because S1/S2's 3-bit activity stole 1 bit from game_mode, shifting
    /// the bitstream cursor +1 across all subsequent fields. On most files
    /// the shifted creator_name read still happens to find a NUL terminator
    /// within 16 bytes (preserving byte-identical RT by accident); on the
    /// ~7% schema-3 files no NUL is found within the cap, the reader
    /// consumes 16 bytes, and the modifier/wchar reads then cascade past
    /// end-of-stream.
    EngineActivity2Bit,
}

/// Decode a Halo 4 c_content_item_metadata using the specified schema variant.
///
/// See [`H4MetaSchema`] for the two known wire formats. Round-trip fidelity is
/// preserved by capturing the raw on-wire byte sequence — the encoder emits it
/// verbatim using the same schema variant.
pub fn decode_metadata_h4_variant(
    meta: &mut c_content_item_metadata,
    bitstream: &mut c_bitstream_reader,
    schema: H4MetaSchema,
) -> BLFLibResult {
    // halo4.dll H4_c_content_item_metadata_decode @ +0x6A43C — metadata prefix
    meta.general.file_type = bitstream.read_integer::<i8>("type", 4)? - 1;
    meta.general.size_in_bytes = bitstream.read_integer("file-size", 32)?;
    meta.general.unique_id = bitstream.read_qword(64)?;
    meta.general.parent_unique_id = bitstream.read_qword(64)?;
    meta.general.root_unique_id = bitstream.read_qword(64)?;
    meta.general.game_id = bitstream.read_qword(64)?;
    // halo4.dll +0x6A514 `mov r12d, 2` — activity is 2 bits (engine-correct); S1/S2 used 3 bits historically
    let activity_bits = match schema {
        H4MetaSchema::EngineActivity2Bit => 2,
        _ => 3,
    };
    meta.general.activity = bitstream.read_integer::<i8>("activity", activity_bits)? - 1;
    meta.general.game_mode = bitstream.read_integer("game-mode", 3)?;
    meta.general.game_engine_type = bitstream.read_integer("game-engine-type", 3)?;
    meta.general.map_id = bitstream.read_signed_integer("map-id", 32)?;
    meta.display.megalo_category_index = bitstream.read_signed_integer("megalo-category-index", 8)?;
    meta.creation_history.timestamp = bitstream.read_qword(64)?;
    meta.creation_history.xuid = bitstream.read_qword(64)?;
    match schema {
        H4MetaSchema::FixedFifteen => {
            let creator_bytes = read_fixed_ascii_raw(bitstream, 15)?;
            let creator_name: String = creator_bytes.iter().map(|&b| b as char).collect();
            meta.creation_history.name = StaticString::from_string_trimmed(creator_name);
            meta.raw_creator_name_bytes = Some(creator_bytes);
            meta.creation_history.is_online = bitstream.read_bool("author-flags")?;
            meta.modification_history.timestamp = bitstream.read_qword(64)?;
            meta.modification_history.xuid = bitstream.read_qword(64)?;
            let modifier_bytes = read_fixed_ascii_raw(bitstream, 15)?;
            let modifier_name: String = modifier_bytes.iter().map(|&b| b as char).collect();
            meta.modification_history.name = StaticString::from_string_trimmed(modifier_name);
            meta.raw_modifier_name_bytes = Some(modifier_bytes);
            meta.modification_history.is_online = false.into();
        }
        H4MetaSchema::NulBoundedSixteen | H4MetaSchema::EngineActivity2Bit => {
            // halo4.dll bitread_ascii_zstring helper @ +0xF5EB8 — engine cap=16 NUL-bounded
            let creator_bytes = bitstream.read_string_extended_ascii_raw_bounded(16)?;
            let creator_name: String = creator_bytes.iter().map(|&b| b as char).collect();
            meta.creation_history.name = StaticString::from_string_trimmed(creator_name);
            meta.raw_creator_name_bytes = Some(creator_bytes);
            meta.creation_history.is_online = bitstream.read_bool("author-flags")?;
            meta.modification_history.timestamp = bitstream.read_qword(64)?;
            meta.modification_history.xuid = bitstream.read_qword(64)?;
            let modifier_bytes = bitstream.read_string_extended_ascii_raw_bounded(16)?;
            let modifier_name: String = modifier_bytes.iter().map(|&b| b as char).collect();
            meta.modification_history.name = StaticString::from_string_trimmed(modifier_name);
            meta.raw_modifier_name_bytes = Some(modifier_bytes);
            meta.modification_history.is_online = bitstream.read_bool("author-flags")?;
        }
    }
    let raw_name_wchars = bitstream.read_string_wchar_raw(8192)?;
    let raw_desc_wchars = bitstream.read_string_wchar_raw(8192)?;
    let name_string = String::from_utf16_lossy(&raw_name_wchars);
    let desc_string = String::from_utf16_lossy(&raw_desc_wchars);
    meta.name = StaticWcharString::from_string_trimmed(name_string)?;
    meta.description = StaticWcharString::from_string_trimmed(desc_string)?;
    meta.raw_name_wchars = Some(raw_name_wchars);
    meta.raw_description_wchars = Some(raw_desc_wchars);
    Ok(())
}

/// Decode a Halo 4 c_content_item_metadata using the majority-case schema
/// (`FixedFifteen`). Kept as the public single-schema entry point for callers
/// that don't want the retry logic; `decode_map_variant_h4` uses
/// `decode_metadata_h4_variant` directly so it can retry on EOS.
pub fn decode_metadata_h4(meta: &mut c_content_item_metadata, bitstream: &mut c_bitstream_reader) -> BLFLibResult {
    decode_metadata_h4_variant(meta, bitstream, H4MetaSchema::FixedFifteen)
}

/// Encode a Halo 4 c_content_item_metadata using the specified schema variant.
/// `schema` MUST match the variant that produced the captured raw bytes (the
/// `decode_map_variant_h4` retry path stores its choice on
/// `c_map_variant.h4_meta_schema` so the encoder picks the same variant).
pub fn encode_metadata_h4_variant(
    meta: &c_content_item_metadata,
    bitstream: &mut c_bitstream_writer,
    schema: H4MetaSchema,
) -> BLFLibResult {
    bitstream.write_integer((meta.general.file_type + 1) as u32, 4)?;
    bitstream.write_integer(meta.general.size_in_bytes, 32)?;
    bitstream.write_qword(meta.general.unique_id, 64)?;
    bitstream.write_qword(meta.general.parent_unique_id, 64)?;
    bitstream.write_qword(meta.general.root_unique_id, 64)?;
    bitstream.write_qword(meta.general.game_id, 64)?;
    let activity_bits = match schema {
        H4MetaSchema::EngineActivity2Bit => 2,
        _ => 3,
    };
    bitstream.write_integer((meta.general.activity + 1) as u32, activity_bits)?;
    bitstream.write_integer(meta.general.game_mode, 3)?;
    bitstream.write_integer(meta.general.game_engine_type, 3)?;
    bitstream.write_signed_integer(meta.general.map_id, 32)?;
    bitstream.write_signed_integer(meta.display.megalo_category_index, 8)?;
    bitstream.write_qword(meta.creation_history.timestamp, 64)?;
    bitstream.write_qword(meta.creation_history.xuid, 64)?;
    match schema {
        H4MetaSchema::FixedFifteen => {
            if let Some(raw) = &meta.raw_creator_name_bytes {
                write_fixed_ascii_raw(bitstream, raw, 15)?;
            } else {
                write_fixed_ascii(bitstream, &meta.creation_history.name.get_string()?, 15)?;
            }
            bitstream.write_bool(meta.creation_history.is_online)?;
            bitstream.write_qword(meta.modification_history.timestamp, 64)?;
            bitstream.write_qword(meta.modification_history.xuid, 64)?;
            if let Some(raw) = &meta.raw_modifier_name_bytes {
                write_fixed_ascii_raw(bitstream, raw, 15)?;
            } else {
                write_fixed_ascii(bitstream, &meta.modification_history.name.get_string()?, 15)?;
            }
        }
        H4MetaSchema::NulBoundedSixteen | H4MetaSchema::EngineActivity2Bit => {
            if let Some(raw) = &meta.raw_creator_name_bytes {
                bitstream.write_string_extended_ascii_raw_bounded(raw, 16)?;
            } else {
                bitstream.write_string_extended_ascii(&meta.creation_history.name.get_string()?, 16)?;
            }
            bitstream.write_bool(meta.creation_history.is_online)?;
            bitstream.write_qword(meta.modification_history.timestamp, 64)?;
            bitstream.write_qword(meta.modification_history.xuid, 64)?;
            if let Some(raw) = &meta.raw_modifier_name_bytes {
                bitstream.write_string_extended_ascii_raw_bounded(raw, 16)?;
            } else {
                bitstream.write_string_extended_ascii(&meta.modification_history.name.get_string()?, 16)?;
            }
            bitstream.write_bool(meta.modification_history.is_online)?;
        }
    }
    if let Some(raw) = &meta.raw_name_wchars {
        bitstream.write_string_wchar_raw(raw)?;
    } else {
        bitstream.write_string_wchar(&meta.name.get_string(), 256)?;
    }
    if let Some(raw) = &meta.raw_description_wchars {
        bitstream.write_string_wchar_raw(raw)?;
    } else {
        bitstream.write_string_wchar(&meta.description.get_string(), 256)?;
    }
    Ok(())
}

/// Encode a Halo 4 c_content_item_metadata using the majority-case
/// (`FixedFifteen`) schema. Public single-schema entry point.
pub fn encode_metadata_h4(meta: &c_content_item_metadata, bitstream: &mut c_bitstream_writer) -> BLFLibResult {
    encode_metadata_h4_variant(meta, bitstream, H4MetaSchema::FixedFifteen)
}

/// Just the metadata, no body. Called by validators that only need name/desc/map_id.
pub fn decode_meta_only_h4(mv: &mut c_map_variant, bitstream: &mut c_bitstream_reader) -> BLFLibResult {
    decode_metadata_h4(&mut mv.m_metadata, bitstream)
}

/// Inner decoder: runs a single-pass H4 v50 decode using the specified
/// metadata schema variant. Returns BLFLibError on EOS or other decode failures.
/// `decode_map_variant_h4` wraps this with auto-detect retry (see that fn).
pub fn decode_map_variant_h4_with_schema(
    mv: &mut c_map_variant,
    bitstream: &mut c_bitstream_reader,
    schema: H4MetaSchema,
) -> BLFLibResult {
    decode_metadata_h4_variant(&mut mv.m_metadata, bitstream, schema)?;
    // halo4.dll H4_c_map_variant_body_decode @ +0xB3BD8 — body header reads
    mv.m_map_variant_version = bitstream.read_unnamed_integer(8)?;
    mv.m_original_map_rsa_signature_hash = bitstream.read_unnamed_integer(32)?;
    mv.m_scenario_palette_crc = bitstream.read_unnamed_integer(32)?;
    mv.m_number_of_placeable_object_quotas = bitstream.read_unnamed_integer(9)?;
    mv.m_map_id = bitstream.read_unnamed_integer(32)?;
    mv.m_built_in = bitstream.read_unnamed_bool()?;
    mv.m_built_from_xml = bitstream.read_unnamed_bool()?;
    mv.m_world_bounds = bitstream.read_raw(0xC0)?;
    mv.m_maximum_budget = bitstream.read_unnamed_integer(32)?;
    mv.m_spent_budget = bitstream.read_unnamed_integer(32)?;
    mv.m_string_table.decode(bitstream)?;
    if mv.m_map_variant_version >= 32 {
        // halo4.dll H4_c_map_variant_body_decode @ +0xB3BD8 — v32+ 128-bit target blob
        let raw_handle: u32 = bitstream.read_unnamed_integer(32)?;
        mv.m_first_target_scenario_handle = raw_handle.swap_bytes();
        mv.m_target_sentinel = bitstream.read_unnamed_integer(32)?;
        let pad_hi: u64 = bitstream.read_unnamed_integer::<u32>(32)? as u64;
        let pad_lo: u64 = bitstream.read_unnamed_integer::<u32>(32)? as u64;
        mv.m_target_padding = (pad_hi << 32) | pad_lo;
    }
    // halo4.dll +0xB7488 `cmp eax, 0xa; shl rax, 0xa` — H4 label_index width = 10 bits
    const H4_LABEL_BITS: u32 = 10;
    for i in 0..k_maximum_variant_objects {
        mv.m_variant_objects.get_mut()[i].decode_h4_h2a(bitstream, &mv.m_world_bounds, H4_LABEL_BITS)?;
    }
    for i in 0..min(k_maximum_variant_quotas, mv.m_number_of_placeable_object_quotas as usize) {
        mv.m_quotas.get_mut()[i].decode(bitstream)?;
    }
    capture_h4_h2a_trailing_bits(mv, bitstream)?;
    mv.h4_meta_schema = Some(match schema {
        H4MetaSchema::FixedFifteen => H4MetaSchemaTag::FixedFifteen,
        H4MetaSchema::NulBoundedSixteen => H4MetaSchemaTag::NulBoundedSixteen,
        H4MetaSchema::EngineActivity2Bit => H4MetaSchemaTag::EngineActivity2Bit,
    });
    Ok(())
}

/// Decode a Halo 4 c_map_variant: dispatches metadata to H4 decoder, then continues
/// with the c_map_variant body. Auto-detects which on-wire metadata schema is
/// in use (FixedFifteen vs NulBoundedSixteen — see [`H4MetaSchema`]) by trying
/// the majority case first and retrying with the engine-asm schema on body-level
/// EOS. The chunk wrapper calls this fn; passing a fresh bitstream each retry
/// is handled by `decode_map_variant_h4_from_payload`.
///
/// IMPORTANT: when called with a NON-rewindable bitstream (i.e. caller passes
/// in `&mut bs` directly), retry is NOT possible. The fn falls back to single-
/// schema (FixedFifteen) behaviour. Use [`decode_map_variant_h4_from_payload`]
/// for the retry-capable path.
pub fn decode_map_variant_h4(mv: &mut c_map_variant, bitstream: &mut c_bitstream_reader) -> BLFLibResult {
    decode_map_variant_h4_with_schema(mv, bitstream, H4MetaSchema::FixedFifteen)
}

/// Auto-detecting H4 decoder: takes the raw payload bytes + bitstream byte
/// order, tries the majority `FixedFifteen` schema first, and on body-level
/// EOS retries with `NulBoundedSixteen` on a fresh bitstream.
///
/// The chunk wrapper (`s_blf_chunk_halo4_map_variant::read_options`) calls
/// this fn so the 8,425 corpus files that use the engine-asm-derived
/// `NulBoundedSixteen` schema can decode without false EOS.
pub fn decode_map_variant_h4_from_payload(
    mv: &mut c_map_variant,
    payload: &[u8],
    byte_order: e_bitstream_byte_order,
) -> BLFLibResult {
    fn is_eos_like(msg: &str) -> bool {
        msg.contains("failed to fill whole buffer")
            || msg.contains("ran out of bytes")
            || msg.contains("Tried to read past end")
            || msg.contains("Tried to read zero bits")
            || msg.contains("Tried to read") // covers "Tried to read N bits but ..." + the byte-buffer variant
            || msg.contains("Exceeded hard cap")
            || msg.contains("axis_encoding_size_in_bits")
            || msg.contains("Bitstream overflow")
            || msg.contains("read past")
            || msg.contains("out of bounds")
            || msg.contains("invalid value")
            || msg.contains("TryFromInt")
    }

    let attempts = [
        H4MetaSchema::FixedFifteen,
        H4MetaSchema::NulBoundedSixteen,
        H4MetaSchema::EngineActivity2Bit,
    ];

    let mut err_strs: Vec<String> = Vec::new();
    for (i, &schema) in attempts.iter().enumerate() {
        if i > 0 {
            *mv = c_map_variant::default();
        }
        let mut bs = c_bitstream_reader::new(payload, byte_order);
        bs.begin_reading();
        match decode_map_variant_h4_with_schema(mv, &mut bs, schema) {
            Ok(()) => return Ok(()),
            Err(e) => err_strs.push(e.to_string()),
        }
    }
    if let Some(real) = err_strs.iter().find(|s| !is_eos_like(s)) {
        return Err(real.clone().into());
    }
    Err(err_strs.pop().unwrap_or_else(|| "all H4 schemas failed".to_string()).into())
}

/// Read all remaining bits from `bitstream` (from the current cursor up to
/// the reader's data_size) into `mv.h4_h2a_trailing_bits`. Safe no-op when
/// the cursor is already at the end. Captures bits as
/// (bytes, bit_count) — the bytes are in MSB-first wire order, ready to be
/// re-emitted via `write_raw_data`.
pub(crate) fn capture_h4_h2a_trailing_bits(
    mv: &mut blf_lib::blam::haloreach_mcc::v_untracked_25_08_16_1352::saved_games::scenario_map_variant::c_map_variant,
    bitstream: &mut blf_lib::io::bitstream::c_bitstream_reader,
) -> BLFLibResult {
    use blf_lib::blam::haloreach_mcc::v_untracked_25_08_16_1352::saved_games::scenario_map_variant::H4H2ATrailingBits;
    let (byte_pos, bit_pos) = bitstream.get_current_offset();
    let mut total_size: usize = 0;
    let _ = bitstream.get_data(&mut total_size)?; // we only need total_size
    let total_bits = total_size * 8;
    let cur_bits = byte_pos * 8 + bit_pos;
    if cur_bits >= total_bits {
        mv.h4_h2a_trailing_bits = None;
        return Ok(());
    }
    let remaining_bits = total_bits - cur_bits;
    let bytes = bitstream.read_raw_data(remaining_bits)?;
    mv.h4_h2a_trailing_bits = Some(H4H2ATrailingBits { bit_count: remaining_bits, bytes });
    Ok(())
}

/// Encode a Halo 4 c_map_variant. Emits the metadata using the schema variant
/// the decoder chose (stored in `mv.h4_meta_schema`); defaults to
/// `FixedFifteen` when the field is `None` (new mvars constructed from scratch).
pub fn encode_map_variant_h4(mv: &c_map_variant, bitstream: &mut c_bitstream_writer) -> BLFLibResult {
    let schema = match mv.h4_meta_schema {
        Some(H4MetaSchemaTag::NulBoundedSixteen) => H4MetaSchema::NulBoundedSixteen,
        Some(H4MetaSchemaTag::EngineActivity2Bit) => H4MetaSchema::EngineActivity2Bit,
        _ => H4MetaSchema::FixedFifteen,
    };
    encode_metadata_h4_variant(&mv.m_metadata, bitstream, schema)?;
    bitstream.write_integer(mv.m_map_variant_version, 8)?;
    bitstream.write_integer(mv.m_original_map_rsa_signature_hash, 32)?;
    bitstream.write_integer(mv.m_scenario_palette_crc, 32)?;
    bitstream.write_integer(mv.m_number_of_placeable_object_quotas, 9)?;
    bitstream.write_integer(mv.m_map_id, 32)?;
    bitstream.write_bool(mv.m_built_in)?;
    bitstream.write_bool(mv.m_built_from_xml)?;
    bitstream.write_raw(mv.m_world_bounds, 0xC0)?;
    bitstream.write_integer(mv.m_maximum_budget, 32)?;
    bitstream.write_integer(mv.m_spent_budget, 32)?;
    mv.m_string_table.encode(bitstream)?;
    if mv.m_map_variant_version >= 32 {
        bitstream.write_integer(mv.m_first_target_scenario_handle.swap_bytes(), 32)?;
        bitstream.write_integer(mv.m_target_sentinel, 32)?;
        bitstream.write_integer((mv.m_target_padding >> 32) as u32, 32)?;
        bitstream.write_integer((mv.m_target_padding & 0xFFFFFFFF) as u32, 32)?;
    }
    // halo4.dll +0xB6F43 — H4 encoder uses 10-bit label_index (matches decoder)
    const H4_LABEL_BITS: u32 = 10;
    for i in 0..k_maximum_variant_objects {
        mv.m_variant_objects[i].encode_h4_h2a(bitstream, &mv.m_world_bounds, H4_LABEL_BITS)?;
    }
    for i in 0..min(k_maximum_variant_quotas, mv.m_number_of_placeable_object_quotas as usize) {
        mv.m_quotas[i].encode(bitstream)?;
    }
    if let Some(t) = &mv.h4_h2a_trailing_bits {
        if t.bit_count > 0 {
            bitstream.write_raw_data(&t.bytes, t.bit_count)?;
        }
    }
    Ok(())
}
