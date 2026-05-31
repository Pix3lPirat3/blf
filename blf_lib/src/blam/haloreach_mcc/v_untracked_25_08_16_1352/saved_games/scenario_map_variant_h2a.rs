//! Halo 2 Anniversary (v52) c_content_item_metadata + c_map_variant override.
//!
//!   - creator_name: ext_ascii(16) NUL-terminated (like Reach, NOT fixed
//!     like H4)
//!   - creator_is_online: 1 bit (same as Reach/H4)
//!   - mod_time, mod_xuid: 64+64 (same)
//!   - **NO modifier_name field** (vs Reach has ext_ascii(16) and H4 has
//!     fixed 15 bytes)
//!   - **NO modifier_is_online bit** (vs Reach has one)
//!   - name + description: wchar NUL-terminated (same encoding as Reach)
//!

use std::cmp::min;
use blf_lib::io::bitstream::{c_bitstream_reader, c_bitstream_writer, e_bitstream_byte_order};
use blf_lib::blam::haloreach::v12065_11_08_24_1738_tu1actual::saved_games::saved_game_files::c_content_item_metadata;
use blf_lib::blam::haloreach_mcc::v_untracked_25_08_16_1352::saved_games::scenario_map_variant::{c_map_variant, k_maximum_variant_objects, k_maximum_variant_quotas, H2AMetaSchemaTag};
use blf_lib_derivable::result::BLFLibResult;
use crate::types::c_string::{StaticString, StaticWcharString};

/// Which on-wire H2A metadata schema is in use for the current payload.
///
/// `Legacy3BitActivity`: engine-incorrect 3-bit activity read. Used historically by
/// blf_lib for H2A; preserved for byte-identical round-trip of the ~96% of H2A
/// corpus files that already decode under this schema.
///
/// `EngineActivity2Bit`: engine-correct 2-bit activity per groundhog.dll
/// `H2A_c_content_item_metadata_decode` (`mov r12d, 2` @ ~+0xd8 from fn entry).
/// Same root-cause analysis as H4.
#[derive(Default, PartialEq, Debug, Clone, Copy)]
pub enum H2AMetaSchema {
    #[default]
    Legacy3BitActivity,
    EngineActivity2Bit,
}

pub fn decode_metadata_h2a(meta: &mut c_content_item_metadata, bitstream: &mut c_bitstream_reader) -> BLFLibResult {
    decode_metadata_h2a_variant(meta, bitstream, H2AMetaSchema::Legacy3BitActivity)
}

/// H2A metadata decoder with schema dispatch. Legacy3BitActivity matches the
/// historical blf_lib behaviour (3-bit activity). EngineActivity2Bit applies
/// the same engine-asm-correct 2-bit-activity fix used for H4 schema 3.
pub fn decode_metadata_h2a_variant(meta: &mut c_content_item_metadata, bitstream: &mut c_bitstream_reader, schema: H2AMetaSchema) -> BLFLibResult {
    // groundhog.dll H2A_c_content_item_metadata_decode @ +0x6A024 — metadata prefix
    meta.general.file_type = bitstream.read_integer::<i8>("type", 4)? - 1;
    meta.general.size_in_bytes = bitstream.read_integer("file-size", 32)?;
    meta.general.unique_id = bitstream.read_qword(64)?;
    meta.general.parent_unique_id = bitstream.read_qword(64)?;
    meta.general.root_unique_id = bitstream.read_qword(64)?;
    meta.general.game_id = bitstream.read_qword(64)?;
    // groundhog.dll +0x6A024 `mov r12d, 2` — activity is 2 bits (engine); legacy schema reads 3 bits historically
    let activity_bits = match schema {
        H2AMetaSchema::EngineActivity2Bit => 2,
        _ => 3,
    };
    meta.general.activity = bitstream.read_integer::<i8>("activity", activity_bits)? - 1;
    meta.general.game_mode = bitstream.read_integer("game-mode", 3)?;
    meta.general.game_engine_type = bitstream.read_integer("game-engine-type", 3)?;
    meta.general.map_id = bitstream.read_signed_integer("map-id", 32)?;
    meta.display.megalo_category_index = bitstream.read_signed_integer("megalo-category-index", 8)?;
    meta.creation_history.timestamp = bitstream.read_qword(64)?;
    meta.creation_history.xuid = bitstream.read_qword(64)?;
    let raw_creator_bytes = bitstream.read_string_extended_ascii_raw(64).unwrap_or_default();
    let raw_creator_string: String = raw_creator_bytes.iter().map(|&b| b as char).collect();
    meta.creation_history.name = StaticString::from_string_trimmed(raw_creator_string);
    meta.raw_creator_name_bytes = Some(raw_creator_bytes);
    meta.creation_history.is_online = bitstream.read_bool("author-flags")?;
    meta.modification_history.timestamp = bitstream.read_qword(64)?;
    meta.modification_history.xuid = bitstream.read_qword(64)?;
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

pub fn encode_metadata_h2a(meta: &c_content_item_metadata, bitstream: &mut c_bitstream_writer) -> BLFLibResult {
    encode_metadata_h2a_variant(meta, bitstream, H2AMetaSchema::Legacy3BitActivity)
}

/// H2A metadata encoder with schema dispatch (mirrors `decode_metadata_h2a_variant`).
pub fn encode_metadata_h2a_variant(meta: &c_content_item_metadata, bitstream: &mut c_bitstream_writer, schema: H2AMetaSchema) -> BLFLibResult {
    bitstream.write_integer((meta.general.file_type + 1) as u32, 4)?;
    bitstream.write_integer(meta.general.size_in_bytes, 32)?;
    bitstream.write_qword(meta.general.unique_id, 64)?;
    bitstream.write_qword(meta.general.parent_unique_id, 64)?;
    bitstream.write_qword(meta.general.root_unique_id, 64)?;
    bitstream.write_qword(meta.general.game_id, 64)?;
    let activity_bits = match schema {
        H2AMetaSchema::EngineActivity2Bit => 2,
        _ => 3,
    };
    bitstream.write_integer((meta.general.activity + 1) as u32, activity_bits)?;
    bitstream.write_integer(meta.general.game_mode, 3)?;
    bitstream.write_integer(meta.general.game_engine_type, 3)?;
    bitstream.write_signed_integer(meta.general.map_id, 32)?;
    bitstream.write_signed_integer(meta.display.megalo_category_index, 8)?;
    bitstream.write_qword(meta.creation_history.timestamp, 64)?;
    bitstream.write_qword(meta.creation_history.xuid, 64)?;
    if let Some(raw) = &meta.raw_creator_name_bytes {
        bitstream.write_string_extended_ascii_raw(raw)?;
    } else {
        bitstream.write_string_extended_ascii(&meta.creation_history.name.get_string()?, 16)?;
    }
    bitstream.write_bool(meta.creation_history.is_online)?;
    bitstream.write_qword(meta.modification_history.timestamp, 64)?;
    bitstream.write_qword(meta.modification_history.xuid, 64)?;
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

pub fn decode_map_variant_h2a(mv: &mut c_map_variant, bitstream: &mut c_bitstream_reader) -> BLFLibResult {
    decode_map_variant_h2a_with_schema(mv, bitstream, H2AMetaSchema::Legacy3BitActivity)
}

/// Inner: H2A body decode under a specific metadata schema. Records the chosen
/// schema on `mv.h2a_meta_schema` so the encoder picks the same variant.
pub fn decode_map_variant_h2a_with_schema(mv: &mut c_map_variant, bitstream: &mut c_bitstream_reader, schema: H2AMetaSchema) -> BLFLibResult {
    decode_metadata_h2a_variant(&mut mv.m_metadata, bitstream, schema)?;

    fn soft_stop_here(
        mv: &mut c_map_variant,
        bitstream: &mut c_bitstream_reader,
        phase: &str,
        idx: usize,
        before: (usize, usize),
    ) -> BLFLibResult<()> {
        use blf_lib::blam::haloreach_mcc::v_untracked_25_08_16_1352::saved_games::scenario_map_variant::H4H2ASoftStopTail;
        let mut tail_len_bytes: usize = 0;
        let _ = bitstream.get_data(&mut tail_len_bytes)?;
        let total_bits = tail_len_bytes * 8;
        let consumed_bits = before.0 * 8 + before.1;
        let (bit_count, bytes) = if total_bits > consumed_bits {
            bitstream.seek_bit(before.0, before.1)?;
            let remaining_bits = total_bits - consumed_bits;
            (remaining_bits, bitstream.read_raw_data(remaining_bits)?)
        } else {
            (0, Vec::new())
        };
        mv.h4_h2a_soft_stop = Some(H4H2ASoftStopTail {
            stopped_in: phase.to_string(),
            stopped_at_index: idx,
            bit_count,
            bytes,
        });
        Ok(())
    }

    macro_rules! try_or_stop {
        ($expr:expr, $phase:expr, $idx:expr, $before:expr) => {
            match $expr {
                Ok(v) => v,
                Err(_) => {
                    soft_stop_here(mv, bitstream, $phase, $idx, $before)?;
                    return finalize(mv, schema);
                }
            }
        };
    }

    fn finalize(mv: &mut c_map_variant, schema: H2AMetaSchema) -> BLFLibResult {
        mv.h2a_meta_schema = Some(match schema {
            H2AMetaSchema::Legacy3BitActivity => H2AMetaSchemaTag::LegacyActivity3Bit,
            H2AMetaSchema::EngineActivity2Bit => H2AMetaSchemaTag::EngineActivity2Bit,
        });
        Ok(())
    }

    let pos = bitstream.get_current_offset();
    mv.m_map_variant_version = try_or_stop!(bitstream.read_unnamed_integer(8), "body_header", 0, pos);
    let pos = bitstream.get_current_offset();
    mv.m_original_map_rsa_signature_hash = try_or_stop!(bitstream.read_unnamed_integer(32), "body_header", 1, pos);
    let pos = bitstream.get_current_offset();
    mv.m_scenario_palette_crc = try_or_stop!(bitstream.read_unnamed_integer(32), "body_header", 2, pos);
    let pos = bitstream.get_current_offset();
    mv.m_number_of_placeable_object_quotas = try_or_stop!(bitstream.read_unnamed_integer(9), "body_header", 3, pos);
    let pos = bitstream.get_current_offset();
    mv.m_map_id = try_or_stop!(bitstream.read_unnamed_integer(32), "body_header", 4, pos);
    let pos = bitstream.get_current_offset();
    mv.m_built_in = try_or_stop!(bitstream.read_unnamed_bool(), "body_header", 5, pos);
    let pos = bitstream.get_current_offset();
    mv.m_built_from_xml = try_or_stop!(bitstream.read_unnamed_bool(), "body_header", 6, pos);
    let pos = bitstream.get_current_offset();
    mv.m_world_bounds = try_or_stop!(bitstream.read_raw(0xC0), "body_header", 7, pos);
    let pos = bitstream.get_current_offset();
    mv.m_maximum_budget = try_or_stop!(bitstream.read_unnamed_integer(32), "body_header", 8, pos);
    let pos = bitstream.get_current_offset();
    mv.m_spent_budget = try_or_stop!(bitstream.read_unnamed_integer(32), "body_header", 9, pos);
    let pos = bitstream.get_current_offset();
    try_or_stop!(mv.m_string_table.decode(bitstream), "string_table", 0, pos);
    if mv.m_map_variant_version >= 32 {
        let pos = bitstream.get_current_offset();
        let raw_handle: u32 = try_or_stop!(bitstream.read_unnamed_integer(32), "v32_blob", 0, pos);
        mv.m_first_target_scenario_handle = raw_handle.swap_bytes();
        let pos = bitstream.get_current_offset();
        mv.m_target_sentinel = try_or_stop!(bitstream.read_unnamed_integer(32), "v32_blob", 1, pos);
        let pos = bitstream.get_current_offset();
        let pad_hi: u64 = try_or_stop!(bitstream.read_unnamed_integer::<u32>(32), "v32_blob", 2, pos) as u64;
        let pos = bitstream.get_current_offset();
        let pad_lo: u64 = try_or_stop!(bitstream.read_unnamed_integer::<u32>(32), "v32_blob", 3, pos) as u64;
        mv.m_target_padding = (pad_hi << 32) | pad_lo;
    }
    // groundhog.dll +0xB63C5 `cmp eax, 0xb; shl rax, 0xb` — H2A label_index width = 11 bits
    const H2A_LABEL_BITS: u32 = 11;
    let stopped: Option<(&'static str, usize, (usize, usize))> = (|| -> Option<(&'static str, usize, (usize, usize))> {
        for i in 0..k_maximum_variant_objects {
            let before = bitstream.get_current_offset();
            if mv.m_variant_objects.get_mut()[i].decode_h4_h2a(bitstream, &mv.m_world_bounds, H2A_LABEL_BITS).is_err() {
                return Some(("variant_objects", i, before));
            }
        }
        // groundhog.dll H2A_variant_quota_decode_3x10bit @ +0xB0AEC — 30-bit quotas
        let max_q = min(k_maximum_variant_quotas, mv.m_number_of_placeable_object_quotas as usize);
        for i in 0..max_q {
            let before = bitstream.get_current_offset();
            if mv.m_quotas.get_mut()[i].decode_h2a(bitstream).is_err() {
                return Some(("quotas", i, before));
            }
        }
        None
    })();

    if let Some((phase, idx, (byte_pos, bit_pos))) = stopped {
        use blf_lib::blam::haloreach_mcc::v_untracked_25_08_16_1352::saved_games::scenario_map_variant::H4H2ASoftStopTail;
        let mut tail_len_bytes: usize = 0;
        let _ = bitstream.get_data(&mut tail_len_bytes)?;
        let total_bits = tail_len_bytes * 8;
        let consumed_bits = byte_pos * 8 + bit_pos;
        let (bit_count, bytes) = if total_bits > consumed_bits {
            bitstream.seek_bit(byte_pos, bit_pos)?;
            let remaining_bits = total_bits - consumed_bits;
            (remaining_bits, bitstream.read_raw_data(remaining_bits)?)
        } else {
            (0, Vec::new())
        };
        mv.h4_h2a_soft_stop = Some(H4H2ASoftStopTail {
            stopped_in: phase.to_string(),
            stopped_at_index: idx,
            bit_count,
            bytes,
        });
    } else {
        crate::blam::haloreach_mcc::v_untracked_25_08_16_1352::saved_games::scenario_map_variant_h4::capture_h4_h2a_trailing_bits(mv, bitstream)?;
    }
    mv.h2a_meta_schema = Some(match schema {
        H2AMetaSchema::Legacy3BitActivity => H2AMetaSchemaTag::LegacyActivity3Bit,
        H2AMetaSchema::EngineActivity2Bit => H2AMetaSchemaTag::EngineActivity2Bit,
    });
    Ok(())
}

/// Auto-detecting H2A decoder: tries `Legacy3BitActivity` first (majority case
/// — ~96% of corpus), retries with `EngineActivity2Bit` on EOS-like errors.
/// Mirrors the H4 retry path; same root-cause analysis applies (engine asm at
/// groundhog.dll H2A_c_content_item_metadata_decode reads activity as 2 bits).
pub fn decode_map_variant_h2a_from_payload(
    mv: &mut c_map_variant,
    payload: &[u8],
    byte_order: e_bitstream_byte_order,
) -> BLFLibResult {
    fn is_eos_like(msg: &str) -> bool {
        msg.contains("failed to fill whole buffer")
            || msg.contains("ran out of bytes")
            || msg.contains("Tried to read past end")
            || msg.contains("Tried to read zero bits")
            || msg.contains("Tried to read")
            || msg.contains("Exceeded hard cap")
            || msg.contains("axis_encoding_size_in_bits")
            || msg.contains("Bitstream overflow")
            || msg.contains("read past")
            || msg.contains("out of bounds")
            || msg.contains("invalid value")
            || msg.contains("TryFromInt")
    }

    let attempts = [H2AMetaSchema::Legacy3BitActivity, H2AMetaSchema::EngineActivity2Bit];
    let mut err_strs: Vec<String> = Vec::new();
    for (i, &schema) in attempts.iter().enumerate() {
        if i > 0 {
            *mv = c_map_variant::default();
        }
        let mut bs = c_bitstream_reader::new(payload, byte_order);
        bs.begin_reading();
        match decode_map_variant_h2a_with_schema(mv, &mut bs, schema) {
            Ok(()) => return Ok(()),
            Err(e) => err_strs.push(e.to_string()),
        }
    }
    if let Some(real) = err_strs.iter().find(|s| !is_eos_like(s)) {
        return Err(real.clone().into());
    }
    Err(err_strs.pop().unwrap_or_else(|| "all H2A schemas failed".to_string()).into())
}

pub fn encode_map_variant_h2a(mv: &c_map_variant, bitstream: &mut c_bitstream_writer) -> BLFLibResult {
    let schema = match mv.h2a_meta_schema {
        Some(H2AMetaSchemaTag::EngineActivity2Bit) => H2AMetaSchema::EngineActivity2Bit,
        _ => H2AMetaSchema::Legacy3BitActivity,
    };
    encode_metadata_h2a_variant(&mv.m_metadata, bitstream, schema)?;

    if let Some(ss) = &mv.h4_h2a_soft_stop {
        let stopped_in = ss.stopped_in.as_str();
        let header_fields: [(&str, fn(&c_map_variant, &mut c_bitstream_writer) -> BLFLibResult); 10] = [
            ("ver",      |m, b| b.write_integer(m.m_map_variant_version, 8)),
            ("rsa",      |m, b| b.write_integer(m.m_original_map_rsa_signature_hash, 32)),
            ("palette",  |m, b| b.write_integer(m.m_scenario_palette_crc, 32)),
            ("nquotas",  |m, b| b.write_integer(m.m_number_of_placeable_object_quotas, 9)),
            ("mapid",    |m, b| b.write_integer(m.m_map_id, 32)),
            ("built_in", |m, b| b.write_bool(m.m_built_in)),
            ("from_xml", |m, b| b.write_bool(m.m_built_from_xml)),
            ("bounds",   |m, b| b.write_raw(m.m_world_bounds, 0xC0)),
            ("maxbud",   |m, b| b.write_integer(m.m_maximum_budget, 32)),
            ("spntbud",  |m, b| b.write_integer(m.m_spent_budget, 32)),
        ];
        if stopped_in == "body_header" {
            for i in 0..ss.stopped_at_index.min(header_fields.len()) {
                (header_fields[i].1)(mv, bitstream)?;
            }
            if ss.bit_count > 0 { bitstream.write_raw_data(&ss.bytes, ss.bit_count)?; }
            return Ok(());
        }
        for (_, f) in &header_fields { f(mv, bitstream)?; }
        if stopped_in == "string_table" {
            if ss.bit_count > 0 { bitstream.write_raw_data(&ss.bytes, ss.bit_count)?; }
            return Ok(());
        }
        mv.m_string_table.encode(bitstream)?;
        if stopped_in == "v32_blob" {
            let v32_fields: [(&str, fn(&c_map_variant, &mut c_bitstream_writer) -> BLFLibResult); 4] = [
                ("handle",   |m, b| b.write_integer(m.m_first_target_scenario_handle.swap_bytes(), 32)),
                ("sentinel", |m, b| b.write_integer(m.m_target_sentinel, 32)),
                ("pad_hi",   |m, b| b.write_integer((m.m_target_padding >> 32) as u32, 32)),
                ("pad_lo",   |m, b| b.write_integer((m.m_target_padding & 0xFFFFFFFF) as u32, 32)),
            ];
            for i in 0..ss.stopped_at_index.min(v32_fields.len()) {
                (v32_fields[i].1)(mv, bitstream)?;
            }
            if ss.bit_count > 0 { bitstream.write_raw_data(&ss.bytes, ss.bit_count)?; }
            return Ok(());
        }
        if mv.m_map_variant_version >= 32 {
            bitstream.write_integer(mv.m_first_target_scenario_handle.swap_bytes(), 32)?;
            bitstream.write_integer(mv.m_target_sentinel, 32)?;
            bitstream.write_integer((mv.m_target_padding >> 32) as u32, 32)?;
            bitstream.write_integer((mv.m_target_padding & 0xFFFFFFFF) as u32, 32)?;
        }
    } else {
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
    }
    // groundhog.dll +0xB63C5 — H2A encoder uses 11-bit label_index (matches decoder)
    const H2A_LABEL_BITS: u32 = 11;

    if let Some(ss) = &mv.h4_h2a_soft_stop {
        match ss.stopped_in.as_str() {
            "variant_objects" => {
                for i in 0..ss.stopped_at_index {
                    mv.m_variant_objects[i].encode_h4_h2a(bitstream, &mv.m_world_bounds, H2A_LABEL_BITS)?;
                }
                if ss.bit_count > 0 {
                    bitstream.write_raw_data(&ss.bytes, ss.bit_count)?;
                }
            }
            "quotas" => {
                for i in 0..k_maximum_variant_objects {
                    mv.m_variant_objects[i].encode_h4_h2a(bitstream, &mv.m_world_bounds, H2A_LABEL_BITS)?;
                }
                for i in 0..ss.stopped_at_index {
                    mv.m_quotas[i].encode_h2a(bitstream)?;
                }
                if ss.bit_count > 0 {
                    bitstream.write_raw_data(&ss.bytes, ss.bit_count)?;
                }
            }
            _ => {
                for i in 0..k_maximum_variant_objects {
                    mv.m_variant_objects[i].encode_h4_h2a(bitstream, &mv.m_world_bounds, H2A_LABEL_BITS)?;
                }
                for i in 0..min(k_maximum_variant_quotas, mv.m_number_of_placeable_object_quotas as usize) {
                    mv.m_quotas[i].encode_h2a(bitstream)?;
                }
            }
        }
        return Ok(());
    }

    for i in 0..k_maximum_variant_objects {
        mv.m_variant_objects[i].encode_h4_h2a(bitstream, &mv.m_world_bounds, H2A_LABEL_BITS)?;
    }
    for i in 0..min(k_maximum_variant_quotas, mv.m_number_of_placeable_object_quotas as usize) {
        mv.m_quotas[i].encode_h2a(bitstream)?;
    }
    if let Some(t) = &mv.h4_h2a_trailing_bits {
        if t.bit_count > 0 {
            bitstream.write_raw_data(&t.bytes, t.bit_count)?;
        }
    }
    Ok(())
}
