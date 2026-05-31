use std::cmp::min;
use binrw::{BinRead, BinWrite};
use num_derive::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};
use blf_lib::io::bitstream::{c_bitstream_reader, c_bitstream_writer};
use blf_lib::OPTION_TO_RESULT;
use crate::blam::common::math::integer_math::int32_point3d;
use crate::blam::common::math::real_math::{real_point3d, real_rectangle3d};
use blf_lib::types::array::StaticArray;
use crate::blam::common::math::real_math::real_vector3d;
use serde_hex::{SerHex,StrictCap};
use blf_lib::blam::haloreach::v12065_11_08_24_1738_tu1actual::memory::bitstream_writer::c_bitstream_writer_extensions;
use blf_lib::blam::haloreach::v12065_11_08_24_1738_tu1actual::saved_games::saved_game_files::c_content_item_metadata;
use blf_lib::blam::haloreach::v12065_11_08_24_1738_tu1actual::simulation::simulation_encoding::{simulation_read_position, simulation_write_position};
use blf_lib_derive::TestSize;
use blf_lib_derivable::result::BLFLibResult;
use crate::blam::haloreach::v12065_11_08_24_1738_tu1actual::game::string_table::c_single_language_string_table;
use crate::blam::haloreach::v12065_11_08_24_1738_tu1actual::memory::bitstream_reader::c_bitstream_reader_extensions;
use crate::types::bool::Bool;
use crate::types::numbers::Float32;

// haloreach.dll mvar_payload_decode_unverified @ +0x6D468 — 0x28B = 651 object slots
pub const k_maximum_variant_objects: usize = 651;
// haloreach.dll mvar_payload_decode_unverified @ +0x6D468 — palette_max u9 (capped 256)
pub const k_maximum_variant_quotas: usize = 256;

#[derive(Default, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct c_map_variant {
    pub m_metadata: c_content_item_metadata,
    pub m_map_variant_version: u16,
    pub m_number_of_placeable_object_quotas: u16,
    pub m_map_id: u32,
    pub m_world_bounds: real_rectangle3d,
    pub m_maximum_budget: u32,
    pub m_spent_budget: u32,
    pub m_helpers_enabled: Bool, // seems to still exist, though not packed.
    pub m_built_in: Bool,
    pub m_built_from_xml: Bool,
    #[serde(with = "SerHex::<StrictCap>")]
    pub m_original_map_rsa_signature_hash: u32,
    #[serde(with = "SerHex::<StrictCap>")]
    pub m_scenario_palette_crc: u32,
    pub m_string_table: c_single_language_string_table<256, 4096, 12, 13, 9>,
    // per level_id_to_map_short_name @ haloreach.dll+0x38E98).
    pub m_first_target_scenario_handle: u32, // wire u32_be; stored swapped so value matches level_id directly
    pub m_target_sentinel: u32,              // always 0x88880000 for v32 files
    pub m_target_padding: u64,               // always 0
    pub m_variant_objects: StaticArray<s_variant_object_datum, k_maximum_variant_objects>,
    pub m_quotas: StaticArray<s_variant_quota, k_maximum_variant_quotas>,
    /// Trailing bits captured by the H4/H2A decoder for byte-identical round-trip.
    /// Real H4/H2A MCC mvars contain ~6-13 KiB of additional structures (likely
    /// trait sets / game-engine-variant blob / forge palette mappings) past the
    /// quotas section. Until those sub-structures are RE'd, the H4/H2A path
    /// captures the raw tail bytes here and emits them verbatim on encode so
    /// `decode → encode` produces a byte-identical bitstream.
    ///
    /// `None` on Reach (Reach has no trailing data past quotas).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h4_h2a_trailing_bits: Option<H4H2ATrailingBits>,
    /// Reach soft-stop salvage: when the decoder hits an "outside of world
    /// bounds" position while decoding `m_variant_objects[N]`, the BSP-cluster
    /// fallback needed to decode subsequent objects is unavailable to blf_lib
    /// (it requires runtime BSP cluster data). To still produce a byte-identical
    /// round-trip for these files, the decoder captures all remaining bits
    /// (starting from variant_object[N]'s first bit, through the rest of the
    /// variant_objects loop AND all m_quotas) into this field and the encoder
    /// re-emits them verbatim instead of running the structural loops past `N`.
    /// `soft_stop_object_index` is the count of variant_objects that were
    /// successfully decoded (i.e. `N`); encode writes 0..N structurally then
    /// dumps the captured bits.
    ///
    /// `None` for cleanly-decoded files (the vast majority).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reach_soft_stop: Option<ReachSoftStopTail>,

    /// H4/H2A soft-stop tail: when the decoder runs out of bits during the
    /// variant_objects or variant_quotas loop (typical for "empty" Forge
    /// variant templates that have default-initialized metadata but no
    /// populated body), capture the count of successfully-decoded slots +
    /// any remaining bits. Encoder writes decoded slots structurally then
    /// dumps the captured tail. Empty/default mvars must decode cleanly
    /// so users can read AND edit them — per blf-focus feedback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h4_h2a_soft_stop: Option<H4H2ASoftStopTail>,

    /// Which H4 metadata schema variant was detected during decode.
    /// `None` on Reach / H2A (fixed schemas). Set by `decode_map_variant_h4`
    /// after its auto-detect retry; the encoder reads it to emit the matching
    /// wire shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h4_meta_schema: Option<H4MetaSchemaTag>,

    /// Which H2A metadata schema variant was detected during decode.
    /// `None` on Reach / H4 (fixed schemas). Set by `decode_map_variant_h2a`
    /// after its auto-detect retry; the encoder reads it to emit the matching
    /// wire shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h2a_meta_schema: Option<H2AMetaSchemaTag>,
}

/// Serializable tag corresponding to `scenario_map_variant_h4::H4MetaSchema`.
/// Lives on `c_map_variant` so the encoder can recall which variant decode
/// chose without forcing this module to import the enum.
#[derive(Default, PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
pub enum H4MetaSchemaTag {
    #[default]
    FixedFifteen,
    NulBoundedSixteen,
    /// Engine-correct schema: activity is 2 bits (not 3) per halo4.dll /
    /// groundhog.dll `c_content_item_metadata_decode` (`mov r12d, 2` at the
    /// activity-read site). Creator + modifier are NUL-bounded up-to-16-byte
    /// ASCII (same as NulBoundedSixteen). Both `is_online` flags present.
    /// Unblocks ~8,425 H4 + ~444 H2A files that previously failed under
    /// both legacy schemas with EOS errors.
    EngineActivity2Bit,
}

/// Serializable tag corresponding to `scenario_map_variant_h2a::H2AMetaSchema`.
/// Lives on `c_map_variant` so the encoder can recall which H2A variant decode
/// chose. Motivated by the same activity-bit-count fix as H4 (see
/// `groundhog.dll` `H2A_c_content_item_metadata_decode` `mov r12d, 2`).
#[derive(Default, PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
pub enum H2AMetaSchemaTag {
    /// Legacy 3-bit activity (engine-incorrect but self-consistent for
    /// round-trip). Used by ~96% of H2A corpus.
    #[default]
    LegacyActivity3Bit,
    /// Engine-correct 2-bit activity per groundhog.dll asm. Unblocks the
    /// ~444 H2A files that previously failed with EOS errors.
    EngineActivity2Bit,
}

/// Soft-stop tail captured when the H4/H2A decoder ran out of bits during
/// the variant_objects or variant_quotas loop. Typical for empty/default
/// Forge variant templates (file_type=5 + all-zero IDs, no populated body).
/// Encoder writes the structurally-decoded prefix then dumps the captured
/// raw bits so the file round-trips byte-identically.
#[derive(Default, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct H4H2ASoftStopTail {
    /// Which loop ran out of bits: `"variant_objects"`, `"quotas"`, or
    /// `"trailing"`. Encoder uses this to skip the matching tail write.
    pub stopped_in: String,
    /// Number of slots successfully decoded within `stopped_in` before EOS.
    /// Encoder writes 0..N structurally then dumps the captured bits.
    pub stopped_at_index: usize,
    /// Total bit count of the captured tail (may not be byte-aligned).
    pub bit_count: usize,
    /// Captured raw bytes in MSB-first wire order.
    pub bytes: Vec<u8>,
}

/// Raw payload tail captured when the Reach decoder soft-stopped on an
/// out-of-world-bounds position. See `c_map_variant::reach_soft_stop`.
#[derive(Default, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct ReachSoftStopTail {
    /// Index of the variant_object whose decode failed (= number of objects
    /// successfully decoded before the failure).
    pub soft_stop_object_index: usize,
    /// Total bit count of the captured tail (may not be a byte multiple).
    pub bit_count: usize,
    /// Bytes containing `bit_count` bits in the bitstream's native MSB-first
    /// layout (read via `read_raw_data` / written via `write_raw_data`).
    pub bytes: Vec<u8>,
}

/// Raw trailing bit-stream payload captured by the H4/H2A decoder.
#[derive(Default, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct H4H2ATrailingBits {
    /// Total bit count of the trailing payload (may not be a byte multiple).
    pub bit_count: usize,
    /// Bytes containing `bit_count` bits in the bitstream's native MSB-first
    /// layout (i.e. the bytes are written/read via `write_raw_data` /
    /// `read_raw_data`).
    pub bytes: Vec<u8>,
}

impl c_map_variant {
    pub fn encode(&self, bitstream: &mut c_bitstream_writer) -> BLFLibResult {
        self.m_metadata.encode(bitstream)?;
        // haloreach.dll mvar_payload_encode_unverified @ +0x6D04C — u8 version (Reach 31/32)
        bitstream.write_integer(self.m_map_variant_version, 8)?;
        bitstream.write_integer(self.m_original_map_rsa_signature_hash, 32)?;
        bitstream.write_integer(self.m_scenario_palette_crc, 32)?;
        // haloreach.dll mvar_payload_encode_unverified @ +0x6D04C — palette_count u9
        bitstream.write_integer(self.m_number_of_placeable_object_quotas, 9)?;
        bitstream.write_integer(self.m_map_id, 32)?;
        bitstream.write_bool(self.m_built_in)?;
        bitstream.write_bool(self.m_built_from_xml)?;
        // haloreach.dll mvar_payload_decode_unverified @ +0x6D468 — 192-bit raw m_world_bounds (6 × f32)
        bitstream.write_raw(self.m_world_bounds, 0xC0)?;
        bitstream.write_integer(self.m_maximum_budget, 32)?;
        bitstream.write_integer(self.m_spent_budget, 32)?;
        self.m_string_table.encode(bitstream)?;

        if self.m_map_variant_version >= 32 {
            // haloreach.dll bitread_guid_be @ +0xDD58C — v32 128-bit tag-handle blob (BE byte-swapped)
            bitstream.write_integer(self.m_first_target_scenario_handle.swap_bytes(), 32)?;
            // haloreach.dll tag_handle_synthesize_or_default @ +0x3C5FC — sentinel 0x88880000
            bitstream.write_integer(self.m_target_sentinel, 32)?;
            bitstream.write_integer((self.m_target_padding >> 32) as u32, 32)?;
            bitstream.write_integer((self.m_target_padding & 0xFFFFFFFF) as u32, 32)?;
        }

        if let Some(ss) = &self.reach_soft_stop {
            let n = ss.soft_stop_object_index.min(k_maximum_variant_objects);
            for i in 0..n {
                self.m_variant_objects[i].encode(bitstream, &self.m_world_bounds)?;
            }
            bitstream.write_raw_data(&ss.bytes, ss.bit_count)?;
            return Ok(());
        }

        for i in 0..k_maximum_variant_objects {
            self.m_variant_objects[i].encode(bitstream, &self.m_world_bounds)?;
        }

        for i in 0..min(k_maximum_variant_quotas, self.m_number_of_placeable_object_quotas as usize) {
            self.m_quotas[i].encode(bitstream)?;
        }

        Ok(())
    }

    pub fn decode(&mut self, bitstream: &mut c_bitstream_reader) -> BLFLibResult {
        self.m_metadata.decode(bitstream)?;
        // haloreach.dll mvar_payload_decode_unverified @ +0x6D468 — u8 version into obj+0x2B0
        self.m_map_variant_version = bitstream.read_unnamed_integer(8)?;
        self.m_original_map_rsa_signature_hash = bitstream.read_unnamed_integer(32)?;
        self.m_scenario_palette_crc = bitstream.read_unnamed_integer(32)?;
        // haloreach.dll mvar_payload_decode_unverified @ +0x6D468 — palette_count u9 (obj+0x14F8)
        self.m_number_of_placeable_object_quotas = bitstream.read_unnamed_integer(9)?;
        self.m_map_id = bitstream.read_unnamed_integer(32)?;
        self.m_built_in = bitstream.read_unnamed_bool()?;
        self.m_built_from_xml = bitstream.read_unnamed_bool()?;
        // haloreach.dll mvar_payload_decode_unverified @ +0x6D468 — 192-bit raw world_bounds (6 × f32)
        self.m_world_bounds = bitstream.read_raw(0xC0)?;
        self.m_maximum_budget = bitstream.read_unnamed_integer(32)?;
        self.m_spent_budget = bitstream.read_unnamed_integer(32)?;
        self.m_string_table.decode(bitstream)?;

        if self.m_map_variant_version >= 32 {
            // haloreach.dll bitread_guid_be @ +0xDD58C — v32 path: 128-bit tag-handle blob into obj+0x2B8
            let raw_handle: u32 = bitstream.read_unnamed_integer(32)?;
            self.m_first_target_scenario_handle = raw_handle.swap_bytes(); // wire BE → semantic LE (= level_id)
            // haloreach.dll tag_handle_synthesize_or_default @ +0x3C5FC — sentinel 0x88880000
            self.m_target_sentinel = bitstream.read_unnamed_integer(32)?;
            let pad_hi: u64 = bitstream.read_unnamed_integer::<u32>(32)? as u64;
            let pad_lo: u64 = bitstream.read_unnamed_integer::<u32>(32)? as u64;
            self.m_target_padding = (pad_hi << 32) | pad_lo;
        }

        // variant_object loop. Per haloreach.dll RE (bitread_packed_xyz_quantized @ 0x3C6884),
        let mut soft_stop_at: Option<(usize, (usize, usize))> = None;
        for i in 0..k_maximum_variant_objects {
            let before = bitstream.get_current_offset();
            if let Err(e) = self.m_variant_objects.get_mut()[i].decode(bitstream, &self.m_world_bounds) {
                let s = e.to_string();
                if s.contains("outside of world bounds") {
                    soft_stop_at = Some((i, before));
                    break;
                }
                return Err(e);
            }
        }
        if let Some((idx, (byte_pos, bit_pos))) = soft_stop_at {
            let mut tail_len_bytes: usize = 0;
            let _ = bitstream.get_data(&mut tail_len_bytes)?;
            let total_bits = tail_len_bytes * 8;
            let consumed_bits = byte_pos * 8 + bit_pos;
            if total_bits > consumed_bits {
                let remaining_bits = total_bits - consumed_bits;
                bitstream.seek_bit(byte_pos, bit_pos)?;
                let bytes = bitstream.read_raw_data(remaining_bits)?;
                self.reach_soft_stop = Some(ReachSoftStopTail {
                    soft_stop_object_index: idx,
                    bit_count: remaining_bits,
                    bytes,
                });
            } else {
                self.reach_soft_stop = Some(ReachSoftStopTail {
                    soft_stop_object_index: idx,
                    bit_count: 0,
                    bytes: Vec::new(),
                });
            }
            return Ok(());
        }

        for i in 0..min(k_maximum_variant_quotas, self.m_number_of_placeable_object_quotas as usize) {
            &mut self.m_quotas.get_mut()[i].decode(bitstream)?;
        }

        Ok(())
    }
}

#[derive(Default, PartialEq, Debug, Clone, Copy, Serialize, Deserialize, BinRead, BinWrite)]
pub struct s_variant_quota {
    // (0..1023) wire ranges. Per groundhog.dll+0xB0AEC.
    pub minimum_count: u16,
    pub maximum_count: u16,
    pub placed_on_map: u16,
}

impl s_variant_quota {
    /// Reach and H4 use 8-bit fields per quota.
    pub fn decode(&mut self, bitstream: &mut c_bitstream_reader) -> BLFLibResult {
        // haloreach.dll mvar_quota_entry_write @ +0x6BC20 — 3 × u8 (Reach); halo4.dll H4_quota_decode_3x8bits @ +0xB1DBC
        self.minimum_count = bitstream.read_unnamed_integer::<u16>(8)?;
        self.maximum_count = bitstream.read_unnamed_integer::<u16>(8)?;
        self.placed_on_map = bitstream.read_unnamed_integer::<u16>(8)?;
        Ok(())
    }

    pub fn encode(&self, bitstream: &mut c_bitstream_writer) -> BLFLibResult {
        // haloreach.dll mvar_quota_entry_write @ +0x6BC20 — 3 × u8
        bitstream.write_integer((self.minimum_count & 0xFF) as u8, 8)?;
        bitstream.write_integer((self.maximum_count & 0xFF) as u8, 8)?;
        bitstream.write_integer((self.placed_on_map & 0xFF) as u8, 8)?;
        Ok(())
    }

    /// H2A — 10 bits per field. 30 bits per quota total.
    /// Verified groundhog.dll+0xB0AEC disasm.
    pub fn decode_h2a(&mut self, bitstream: &mut c_bitstream_reader) -> BLFLibResult {
        // groundhog.dll H2A_variant_quota_decode_3x10bit @ +0xB0AEC — 3 × u10 (H2A only)
        self.minimum_count = bitstream.read_unnamed_integer::<u16>(10)?;
        self.maximum_count = bitstream.read_unnamed_integer::<u16>(10)?;
        self.placed_on_map = bitstream.read_unnamed_integer::<u16>(10)?;
        Ok(())
    }

    pub fn encode_h2a(&self, bitstream: &mut c_bitstream_writer) -> BLFLibResult {
        // groundhog.dll H2A_variant_quota_decode_3x10bit @ +0xB0AEC — 3 × u10
        bitstream.write_integer(self.minimum_count, 10)?;
        bitstream.write_integer(self.maximum_count, 10)?;
        bitstream.write_integer(self.placed_on_map, 10)?;
        Ok(())
    }
}

#[derive(BinRead, BinWrite, Serialize, Deserialize, Default, PartialEq, Debug, Copy, Clone, FromPrimitive, ToPrimitive)]
#[repr(u8)]
#[brw(repr = u8)]
pub enum e_boundary_shape {
    #[default]
    unused = 0,
    sphere = 1,
    cylinder = 2,
    r#box = 3,
}

#[derive(Default, PartialEq, Debug, Clone, Copy, Serialize, Deserialize, BinRead, BinWrite)]
pub struct s_multiplayer_object_boundary {
    pub shape: e_boundary_shape,
    pub size: Float32,
    pub box_length: Float32,
    pub positive_height: Float32,
    pub negative_height: Float32,
    /// Raw 11-bit quantized values captured by `decode` for round-trip
    /// fidelity. Each Some(u32) holds the on-wire integer that was
    /// dequantized into the corresponding float field. Encoder prefers
    /// these raws when present, falls back to `write_quantized_real` for
    /// newly-constructed boundaries. None on the BinRead/BinWrite path
    /// (used only by the bitstream encode/decode).
    #[br(ignore)]
    #[bw(ignore)]
    #[serde(default, skip_serializing_if = "Option::is_none", skip_deserializing)]
    pub raw_size: Option<u32>,
    #[br(ignore)]
    #[bw(ignore)]
    #[serde(default, skip_serializing_if = "Option::is_none", skip_deserializing)]
    pub raw_box_length: Option<u32>,
    #[br(ignore)]
    #[bw(ignore)]
    #[serde(default, skip_serializing_if = "Option::is_none", skip_deserializing)]
    pub raw_positive_height: Option<u32>,
    #[br(ignore)]
    #[bw(ignore)]
    #[serde(default, skip_serializing_if = "Option::is_none", skip_deserializing)]
    pub raw_negative_height: Option<u32>,
}

impl s_multiplayer_object_boundary {
    pub fn decode(bitstream: &mut c_bitstream_reader) -> BLFLibResult<Option<s_multiplayer_object_boundary>> {
        let mut boundary = Self::default();
        // groundhog.dll H2A_mp_obj_boundary_decode @ +0x691FE8 — 2-bit shape selector
        boundary.shape = bitstream.read_unnamed_enum(2)?;

        match boundary.shape {
            e_boundary_shape::unused => return Ok(None),
            e_boundary_shape::sphere => {
                // groundhog.dll H2A_bitstream_read_quantized_16bit_field @ +0x6922C8 — sphere: 1 × q16
                let (v, raw) = bitstream.read_quantized_real_capture(0f32, 200.0f32, 11, false, true)?;
                boundary.size = v; boundary.raw_size = Some(raw);
            }
            e_boundary_shape::cylinder => {
                // groundhog.dll H2A_mp_obj_boundary_decode @ +0x691FE8 — cylinder: 3 × q16 (size, ±height)
                let (v, raw) = bitstream.read_quantized_real_capture(0f32, 200.0f32, 11, false, true)?;
                boundary.size = v; boundary.raw_size = Some(raw);
                let (v, raw) = bitstream.read_quantized_real_capture(0f32, 200.0f32, 11, false, true)?;
                boundary.positive_height = v; boundary.raw_positive_height = Some(raw);
                let (v, raw) = bitstream.read_quantized_real_capture(0f32, 200.0f32, 11, false, true)?;
                boundary.negative_height = v; boundary.raw_negative_height = Some(raw);
            }
            e_boundary_shape::r#box => {
                // groundhog.dll H2A_mp_obj_boundary_decode @ +0x691FE8 — box: 4 × q16 (size, length, ±height)
                let (v, raw) = bitstream.read_quantized_real_capture(0f32, 200.0f32, 11, false, true)?;
                boundary.size = v; boundary.raw_size = Some(raw);
                let (v, raw) = bitstream.read_quantized_real_capture(0f32, 200.0f32, 11, false, true)?;
                boundary.box_length = v; boundary.raw_box_length = Some(raw);
                let (v, raw) = bitstream.read_quantized_real_capture(0f32, 200.0f32, 11, false, true)?;
                boundary.positive_height = v; boundary.raw_positive_height = Some(raw);
                let (v, raw) = bitstream.read_quantized_real_capture(0f32, 200.0f32, 11, false, true)?;
                boundary.negative_height = v; boundary.raw_negative_height = Some(raw);
            }
        };

        Ok(Some(boundary))
    }

    pub fn encode(&self, mut bitstream: &mut c_bitstream_writer) -> BLFLibResult {
        bitstream.write_enum(self.shape, 2)?;

        match self.shape {
            e_boundary_shape::unused => {}
            e_boundary_shape::sphere => {
                emit_q(&mut bitstream, self.raw_size, self.size)?;
            }
            e_boundary_shape::cylinder => {
                emit_q(&mut bitstream, self.raw_size, self.size)?;
                emit_q(&mut bitstream, self.raw_positive_height, self.positive_height)?;
                emit_q(&mut bitstream, self.raw_negative_height, self.negative_height)?;
            }
            e_boundary_shape::r#box => {
                emit_q(&mut bitstream, self.raw_size, self.size)?;
                emit_q(&mut bitstream, self.raw_box_length, self.box_length)?;
                emit_q(&mut bitstream, self.raw_positive_height, self.positive_height)?;
                emit_q(&mut bitstream, self.raw_negative_height, self.negative_height)?;
            }
        }

        Ok(())
    }
}

/// Emit a boundary's 11-bit quantized field, preferring the raw-captured
/// int when available. Falls back to re-quantizing the float for newly-
/// constructed boundaries. See `s_multiplayer_object_boundary::encode`.
fn emit_q(mut bitstream: &mut c_bitstream_writer, raw: Option<u32>, val: Float32) -> BLFLibResult {
    if let Some(r) = raw {
        bitstream.write_integer(r, 11)?;
    } else {
        bitstream.write_quantized_real(val, 0f32, 200f32, 11, false, true)?;
    }
    Ok(())
}

#[derive(Default, PartialEq, Debug, Clone, Copy, Serialize, Deserialize, BinRead, BinWrite)]
pub struct s_variant_multiplayer_object_properties_definition_location_data {
    pub location_name_index: i8, // 8
}

#[derive(Default, PartialEq, Debug, Clone, Copy, Serialize, Deserialize, BinRead, BinWrite)]
pub struct s_variant_multiplayer_object_properties_definition_teleporter_data {
    pub channel: u8, // 5
    pub passability: u8, // 5
}

#[derive(Default, PartialEq, Debug, Clone, Copy, Serialize, Deserialize, BinRead, BinWrite)]
pub struct s_variant_multiplayer_object_properties_definition_weapon_data {
    pub spare_clips: u8, // 8
}

#[derive(Default, PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct s_variant_multiplayer_object_properties_definition {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boundary: Option<s_multiplayer_object_boundary>,
    pub game_engine_flags: u16,
    pub user_data: u8,
    pub spawn_time: u8,
    pub cached_type: u8,
    pub label_index: i8,
    pub placement_flags: u8,
    pub team: i8,
    pub primary_change_color_index: i8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_data: Option<s_variant_multiplayer_object_properties_definition_location_data>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teleporter_data: Option<s_variant_multiplayer_object_properties_definition_teleporter_data>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weapon_data: Option<s_variant_multiplayer_object_properties_definition_weapon_data>,
}

impl s_variant_multiplayer_object_properties_definition {
    pub fn decode(&mut self, bitstream: &mut c_bitstream_reader) -> BLFLibResult {
        // haloreach.dll bitread_object_metadata_typed @ +0x6FA9C — Reach Forge mp_props (cached_type 5-bit)
        self.boundary = s_multiplayer_object_boundary::decode(bitstream)?;
        self.user_data = bitstream.read_unnamed_integer(8)?;
        self.spawn_time = bitstream.read_unnamed_integer(8)?;
        // haloreach.dll bitread_object_metadata_typed @ +0x6FA9C — Reach cached_type is 5 bits (vs H4/H2A 6 bits)
        self.cached_type = bitstream.read_unnamed_integer(5)?;
        self.label_index = bitstream.read_unnamed_index::<256>(8)? as i8;
        self.placement_flags = bitstream.read_unnamed_integer(8)?;
        self.team = bitstream.read_unnamed_integer::<i8>(4)? - 1;
        self.primary_change_color_index = bitstream.read_unnamed_index::<8>(3)? as i8;

        match self.cached_type {
            1 => {
                self.weapon_data = Some(s_variant_multiplayer_object_properties_definition_weapon_data {
                    spare_clips: bitstream.read_unnamed_integer(8)?,
                })
            }
            12 | 13 | 14 => {
                self.teleporter_data = Some(s_variant_multiplayer_object_properties_definition_teleporter_data {
                    channel: bitstream.read_unnamed_integer(5)?,
                    passability: bitstream.read_unnamed_integer(5)?,
                })
            }
            19 => {
                self.location_data = Some(s_variant_multiplayer_object_properties_definition_location_data {
                    location_name_index: bitstream.read_unnamed_index::<255>(8)? as i8,
                })
            }
            _ => {}
        }

        Ok(())
    }

    pub fn encode(&self, mut bitstream: &mut c_bitstream_writer) -> BLFLibResult {
        self.boundary.unwrap_or_default().encode(&mut bitstream)?;
        bitstream.write_integer(self.user_data, 8)?;
        bitstream.write_integer(self.spawn_time, 8)?;
        bitstream.write_integer(self.cached_type, 5)?;
        bitstream.write_index::<256>(self.label_index, 8)?;
        bitstream.write_integer(self.placement_flags, 8)?;
        bitstream.write_integer((self.team + 1) as u32, 4)?;
        bitstream.write_index::<8>(self.primary_change_color_index, 3)?;

        match self.cached_type {
            1 => {
                let weapon_data = OPTION_TO_RESULT!(
                    self.weapon_data,
                    "Tried to encode a weapon with no weapon data provided."
                )?;

                bitstream.write_integer(weapon_data.spare_clips, 8)?;
            }
            12 | 13 | 14 => {
                let teleporter_data = OPTION_TO_RESULT!(
                    self.teleporter_data,
                    "Tried to encode a teleporter with no teleporter data provided."
                )?;

                bitstream.write_integer(teleporter_data.channel, 5)?;
                bitstream.write_integer(teleporter_data.passability, 5)?;
            }
            19 => {
                let location_data = OPTION_TO_RESULT!(
                    self.location_data,
                    "Tried to encode a location name with no name provided."
                )?;

                bitstream.write_index::<255>(location_data.location_name_index, 8)?;
            }
            _ => {}
        }

        Ok(())
    }
}

#[derive(Default, PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct s_variant_object_datum {
    pub flags: u16,
    pub reuse_timeout: u16,
    pub object_datum_index: i32,
    pub editor_object_index: i32,
    pub variant_quota_index: i32,
    pub variant_index: i32,  // not sure on unpacked layout
    pub position: real_point3d,
    pub forward: real_vector3d,
    pub up: real_vector3d,
    pub spawn_relative_to: i32, // not sure on unpacked layout
    pub multiplayer_game_object_properties: s_variant_multiplayer_object_properties_definition,

    /// Position decoder's `point-in-initial-bounds` bit (raw). H4/H2A consumes
    /// per-axis bits regardless of this flag's value; we capture it so the
    /// encoder writes the same wire bit back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h4_h2a_position_in_bounds: Option<bool>,

    /// Raw per-axis quantized position values (the ints that go on the wire
    /// AFTER the in_bounds bit). Stored for round-trip fidelity — re-quantizing
    /// `self.position` would drift for out-of-bounds samples because
    /// `quantize_real_fast_guts` clamps OOB inputs differently than the engine
    /// did originally. The integer triple is the only safe representation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h4_h2a_position_raw: Option<int32_point3d>,

    /// Axes decoder state. None = Reach-style (`forward`/`up` populated via
    /// the Reach `read_axes` path); Some = H4/H2A-style (raw axis bits).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h4_h2a_axes: Option<H4H2AAxes>,

    /// `+0x2C` field — `read_quantized_real(0.0, 10.0, 6, false, true)`. We
    /// store the raw 6-bit integer to avoid quantize/dequantize round-trip
    /// drift; a typed-f32 accessor can derive from it on demand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h4_h2a_extra_quantized_raw: Option<u8>,

    /// `+0x33` field — 1-bit bool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h4_h2a_extra_bool: Option<bool>,

    /// H4/H2A `mp_object_properties` extras (different schema from Reach).
    /// `Some` exactly when this variant_object exists on the wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h4_h2a_mp_props: Option<s_h4_h2a_mp_props>,

    /// Raw per-axis quantized position values from the Reach v31 decoder.
    /// None on H4/H2A path and for newly-constructed mvars; Some when the
    /// Reach decoder captured the on-wire integers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reach_position_raw: Option<int32_point3d>,

    /// Raw axes bit fields from the Reach v31 decoder. Triple of
    /// (up_is_global, raw_up_quantization, raw_forward_angle).
    /// Up bit width = 20, forward bit width = 14 for Reach v31's
    /// `read_axes::<14, 20>` call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reach_axes_raw: Option<ReachAxesRaw>,

    /// Tracks whether THIS slot was on-wire (`variant_object_exists==true`)
    /// when the Reach decoder read it. Used by the encoder to bypass the
    /// historical `flags & 0x3FF == 0` heuristic for the existence bit —
    /// modded files in the corpus sometimes have `exists=true + flags=0`,
    /// which the heuristic mis-encodes as `exists=false` and cascades
    /// all subsequent slots into byte_diff. Set to `Some(true)` when decode
    /// reads a populated slot; `None` for empty slots or newly-constructed
    /// mvars (encoder falls back to the heuristic in that case).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reach_decoded_exists: Option<bool>,
}

/// Raw axes wire fields captured by the Reach v31 decoder for round-trip
/// fidelity. See `s_variant_object_datum::reach_axes_raw`.
#[derive(Default, PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ReachAxesRaw {
    /// 1-bit "up is global up3d" flag.
    pub up_is_global: bool,
    /// Raw 20-bit up-axis quantization (only meaningful when
    /// `up_is_global == false`).
    pub raw_up_quantization: u32,
    /// Raw 14-bit forward-angle quantized value.
    pub raw_forward_angle: u32,
}

/// H4/H2A `read_axes` representation. Captures either the 1-bit "is_default"
/// shortcut or the raw 20+14 bit forward-angle + up-rotation pair (no Reach
/// `up-is-global-up3d` short-circuit on H4/H2A).
#[derive(Default, PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct H4H2AAxes {
    /// If true, only 1 bit was on the wire; engine-default axes are loaded.
    pub is_default: bool,
    /// Raw 20-bit forward angle (only valid when `is_default == false`).
    pub forward_angle_raw: u32,
    /// Raw 14-bit up rotation quantized value (only valid when `is_default == false`).
    pub up_rotation_raw: u32,
}

/// H4/H2A `mp_object_properties` (FUN_1800B6348 in groundhog.dll /
/// FUN_1800B7C9C in halo4.dll). Schema differs from Reach's
/// `s_variant_multiplayer_object_properties_definition`. Stored separately
/// so the existing Reach struct can stay untouched.
///
/// On-wire layout (verified via static x64 disasm — counts NOT guesses):
/// ```
/// boundary_decode():          # see s_multiplayer_object_boundary::decode
///   read(2)  → shape
///   shape==1: 1 × read(16) → size
///   shape==2: 3 × read(16) → size + ±height
///   shape==3: 4 × read(16) → size + length + ±height
/// read(6)               → cached_type (0..63)
/// read(H4: 10, H2A: 11) → label_index (H4 0..1023; H2A 0..2047)
/// match cached_type:
///   0x20:        read(4)-1 + read(8) + read(16)                     = 28 bits   (Type20Advanced)
///   0x21:        8 × read(8)                                        = 64 bits   (Type21Teleporter1)
///   0x22:        9 × read(8)                                        = 72 bits   (Type22Teleporter2)
///   0x23:        0 bits                                             = 0 bits    (None — "advanced" marker)
///   0..0x1F:     common 60 bits + cached_type-specific sub-dispatch = 60..70 bits (Default)
///                  Common: read(4)-1 + read(8) + read_index<8>(3)
///                        + read(8) + read(8) + 4 × read_index<256>(8)
///                  Sub-dispatch:
///                    cached_type==1  (weapon):                 read(8)              [+8 = 68 bits total]
///                    cached_type==12 (teleporter basic):       read(8)              [+8 = 68 bits total]
///                    cached_type==20 (location_name):          read(8)-1            [+8 = 68 bits total]
///                    cached_type==31 (trait_zone):             read(5)              [+5 = 65 bits total]
///                    else:                                     read(5) + read(5)    [+10 = 70 bits total]
/// ```
///
/// RE'd from halo4.dll+0xB7410 / groundhog.dll+0xB6348 + per-engine label_index
/// disasm at halo4+0xB7488 (10 bits) vs groundhog+0xB63C5 (11 bits). Field
/// semantics from `forge_object_properties_*` string set at halo4+0xCAA8B0.
#[derive(Default, PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct s_h4_h2a_mp_props {
    pub boundary: Option<s_multiplayer_object_boundary>,
    /// 6-bit cached type (0..63). Primary discriminator for the tail.
    pub cached_type: u8,
    /// Label index. H4 reads/writes as 10 bits (0..1023); H2A as 11 bits (0..2047).
    pub label_index: u16,
    pub tail: s_h4_h2a_mp_props_tail,
}

/// Cached-type-dependent tail of `s_h4_h2a_mp_props`. Variants match the
/// halo4.dll dispatch order. See doc 411 for asm evidence.
#[derive(PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum s_h4_h2a_mp_props_tail {
    /// `cached_type == 0x23` — zero tail bits. The "advanced" marker (per
    /// `forge_object_properties_advanced` string anchor); carries only
    /// boundary + cached_type + label_index, no body.
    None,

    /// `cached_type == 0x20` — 28 bits. Likely the H4 *dispenser* extension
    /// (only `forge_object_properties_dispenser_*` string in halo4.dll is
    /// `_use_cooldown`).
    Type20Advanced {
        /// 4-bit raw value MINUS 1 (range -1..14). Possibly `dispenser_channel`.
        field_4bit_signed: i8,
        /// 8-bit u8. Possibly `dispenser_use_cooldown` (seconds).
        field_8bit: u8,
        /// 16-bit u16. Possibly `dispenser_spawn_object_palette_index`.
        field_16bit: u16,
    },

    /// `cached_type == 0x21` — 64 bits (8 × 8). Likely H4 *teleporter one-way*.
    /// 8 individual property bytes (channel + 7 boolean/mask flags).
    Type21Teleporter1 {
        teleporter_field_bytes: [u8; 8],
    },

    /// `cached_type == 0x22` — 72 bits (9 × 8). Likely H4 *teleporter two-way*
    /// (or receiver). 1 extra byte vs 0x21 — possibly direction/target byte.
    Type22Teleporter2 {
        teleporter_field_bytes: [u8; 9],
    },

    /// `cached_type ∈ {0x00..0x1F}` (excluding 0x20-0x23) — common 60-bit prefix
    /// + sub-dispatch tail. Inherits/extends Reach's flat decoder layout.
    Default {
        /// 4 bits raw - 1 (range -1..14). `primary_change_color` per Reach analog.
        primary_change_color: i8,
        /// 8 bits. `spawn_time` seconds (0..255).
        spawn_time: u8,
        /// 3 bits indexed-with-none. `team` (-1 = neutral, 0..7 = team).
        team: i8,
        /// 8 bits. Low byte of `placement_flags` / `game_engine_flags`.
        flags_lo: u8,
        /// 8 bits sign-extended. High byte of placement_flags
        /// (hide / spawn_at_start / is_shortcut / symmetry / physics bits).
        flags_hi_signed: i8,
        /// 4 × 8-bit indexed-with-none. `user_data` / `user_data2` /
        /// `min_count` / `max_count` per `forge_object_properties_*` strings.
        user_data_block: [i32; 4],
        /// Cached_type-specific tail (1-2 bytes depending on sub-case).
        sub: DefaultSubcase,
    },
}

/// Sub-case for the default-branch tail. Discriminated by cached_type itself.
#[derive(PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DefaultSubcase {
    /// `cached_type == 1` — weapon. `spare_clips` (8 bits, 0..255).
    Weapon { spare_clips: u8 },
    /// `cached_type == 12` — teleporter (basic). 8-bit channel or passability mask.
    Teleporter12 { channel_or_mask: u8 },
    /// `cached_type == 20` (0x14) — location name. 8-bit-1 (signed -1..254).
    LocationName { location_name_index: i32 },
    /// `cached_type == 31` (0x1F) — trait zone. 5-bit channel (0..31).
    TraitZone { trait_zone_channel: u8 },
    /// All other `cached_type ∈ {0, 2..10, 11, 13..19, 21..30}`. 5+5 bits.
    /// Likely `min_count_quota` + `max_count_quota` per Forge string anchors.
    QuotaOther { min_count: u8, max_count: u8 },
}

impl Default for s_h4_h2a_mp_props_tail {
    fn default() -> Self { Self::None }
}

impl Default for DefaultSubcase {
    fn default() -> Self { Self::QuotaOther { min_count: 0, max_count: 0 } }
}

impl s_variant_object_datum {
    pub fn decode(&mut self, bitstream: &mut c_bitstream_reader, world_bounds: &real_rectangle3d) -> BLFLibResult {
        use blf_lib::blam::haloreach::v12065_11_08_24_1738_tu1actual::simulation::simulation_encoding::simulation_read_position_capture;
        if bitstream.read_bool("variant_object_exists")? {
            self.reach_decoded_exists = Some(true);
            // haloreach.dll mvar_payload_decode_unverified @ +0x6D468 — slot: 1 exists + 2 flags + 8 quota_idx + 5 variant_idx
            self.flags = bitstream.read_unnamed_integer(2)?;
            self.variant_quota_index = bitstream.read_unnamed_index::<k_maximum_variant_quotas>(8)?;
            self.variant_index = bitstream.read_unnamed_index::<32>(5)?;
            // haloreach.dll bitread_packed_xyz_quantized @ +0x3C6884 — POSITION via scale_idx=21
            let raw_pos = simulation_read_position_capture(bitstream, &mut self.position, 21, false, true, &world_bounds)?;
            self.reach_position_raw = Some(raw_pos);
            // haloreach.dll bitread_object_orientation @ +0x708A4 — Reach axes: 14 forward + 20 up
            let (up_is_global, raw_up_q, raw_forward_angle) = bitstream.read_axes_capture::<14, 20>(&mut self.forward, &mut self.up)?;
            self.reach_axes_raw = Some(ReachAxesRaw { up_is_global, raw_up_quantization: raw_up_q, raw_forward_angle });
            // haloreach.dll mvar_payload_decode_unverified @ +0x6D468 — spawn_relative_to u10 (stored value-1)
            self.spawn_relative_to = bitstream.read_unnamed_integer::<i32>(10)? - 1;
            // haloreach.dll bitread_object_metadata_typed @ +0x6FA9C — mp_object_properties
            self.multiplayer_game_object_properties.decode(bitstream)?;
        }

        Ok(())
    }

    pub fn encode(&self, mut bitstream: &mut c_bitstream_writer, world_bounds: &real_rectangle3d) -> BLFLibResult {
        use blf_lib::blam::haloreach::v12065_11_08_24_1738_tu1actual::simulation::simulation_encoding::simulation_write_position_with_raw;
        let exists = match self.reach_decoded_exists {
            Some(v) => v,
            None => (self.flags & 0x3FF) != 0,
        };
        if !exists {
            bitstream.write_bool(false)?;
            return Ok(());
        }

        bitstream.write_bool(true)?;
        bitstream.write_integer(self.flags, 2)?;
        bitstream.write_index::<k_maximum_variant_quotas>(self.variant_quota_index, 8)?;
        bitstream.write_index::<32>(self.variant_index, 5)?;
        if let Some(raw) = &self.reach_position_raw {
            simulation_write_position_with_raw(bitstream, raw, 21, world_bounds)?;
        } else {
            simulation_write_position(bitstream, &self.position, 21, world_bounds)?;
        }
        if let Some(axes) = &self.reach_axes_raw {
            bitstream.write_axes_with_raw::<14, 20>(axes.up_is_global, axes.raw_up_quantization, axes.raw_forward_angle)?;
        } else {
            bitstream.write_axes::<14, 20>(&self.forward, &self.up)?;
        }
        bitstream.write_integer((self.spawn_relative_to + 1) as u32, 10)?;
        self.multiplayer_game_object_properties.encode(bitstream)?;

        Ok(())
    }

    /// Decode H4/H2A variant_object. The only per-engine wire difference plumbed
    /// here is `label_bits` for mp_object_properties.label_index (H4=10, H2A=11
    /// per halo4+0xB7488 vs groundhog+0xB63C5).
    pub fn decode_h4_h2a(
        &mut self,
        bitstream: &mut c_bitstream_reader,
        world_bounds: &real_rectangle3d,
        label_bits: u32,
    ) -> BLFLibResult {
        use blf_lib::blam::common::math::integer_math::int32_point3d;
        use blf_lib::blam::common::simulation::simulation_encoding::adjust_axis_encoding_bit_count_to_match_error_goals;
        use blf_lib::blam::haloreach::v12065_11_08_24_1738_tu1actual::math::real_math::dequantize_real_point3d_per_axis;

        // halo4.dll H4_variant_object_decode_mvar @ +0xB7C9C / groundhog.dll H2A_variant_object_decode @ +0xB6C78
        let exists: bool = bitstream.read_bool("variant_object_exists")?;
        if !exists {
            return Ok(());
        }

        self.flags = bitstream.read_unnamed_integer(2)?;
        self.variant_quota_index = bitstream.read_unnamed_index::<k_maximum_variant_quotas>(8)?;
        self.variant_index = bitstream.read_unnamed_index::<32>(5)?;

        // groundhog.dll H2A_simulation_read_position @ +0x5176B8 — in_bounds bit doesn't gate per-axis reads
        let in_bounds: bool = bitstream.read_bool("point-in-initial-bounds")?;
        self.h4_h2a_position_in_bounds = Some(in_bounds);

        let mut per_axis_bit_counts = int32_point3d::default();
        // groundhog.dll H2A_position_adjust_axis_encoding @ +0x51B1D0 — per-axis bit-counts (no bit reads)
        adjust_axis_encoding_bit_count_to_match_error_goals(21, world_bounds, 26, &mut per_axis_bit_counts);
        let mut quantized_point = int32_point3d::default();
        bitstream.read_point3d_efficient(&mut quantized_point, per_axis_bit_counts)?;
        self.h4_h2a_position_raw = Some(quantized_point);
        dequantize_real_point3d_per_axis(
            &quantized_point,
            world_bounds,
            &per_axis_bit_counts,
            &mut self.position,
            false,
            true,
        );

        // halo4.dll axes decoder @ +0xB8D38 — up-rotation q14 ALWAYS read; forward angle 20-bit only if !is_default
        let is_default: bool = bitstream.read_unnamed_bool()?;
        let up_rotation_raw: u32 = bitstream.read_unnamed_integer(14)?;
        let forward_angle_raw: u32 = if is_default {
            0u32
        } else {
            bitstream.read_unnamed_integer(20)?
        };
        self.h4_h2a_axes = Some(H4H2AAxes { is_default, forward_angle_raw, up_rotation_raw });

        self.spawn_relative_to = bitstream.read_unnamed_integer::<i32>(10)? - 1;

        // halo4.dll H4_variant_object_decode_mvar @ +0xB7C9C — +0x2C f32 via read_quantized_real(0,10,6,0,1)
        self.h4_h2a_extra_quantized_raw = Some(bitstream.read_unnamed_integer::<u32>(6)? as u8);
        // halo4.dll H4_variant_object_iter_consume_extra_bool_at_0x33 @ +0xC0F6C — +0x33 u8 bool, 1 bit
        self.h4_h2a_extra_bool = Some(bitstream.read_unnamed_bool()?);

        self.h4_h2a_mp_props = Some(s_h4_h2a_mp_props::decode(bitstream, label_bits)?);
        Ok(())
    }

    pub fn encode_h4_h2a(
        &self,
        bitstream: &mut c_bitstream_writer,
        world_bounds: &real_rectangle3d,
        label_bits: u32,
    ) -> BLFLibResult {
        use blf_lib::blam::common::math::integer_math::int32_point3d;
        use blf_lib::blam::common::simulation::simulation_encoding::adjust_axis_encoding_bit_count_to_match_error_goals;
        use blf_lib::blam::haloreach::v12065_11_08_24_1738_tu1actual::math::real_math::quantize_real_point3d_per_axis;

        let mp = match &self.h4_h2a_mp_props {
            None => {
                bitstream.write_bool(false)?;
                return Ok(());
            }
            Some(mp) => mp,
        };
        bitstream.write_bool(true)?;

        bitstream.write_integer(self.flags, 2)?;
        bitstream.write_index::<k_maximum_variant_quotas>(self.variant_quota_index, 8)?;
        bitstream.write_index::<32>(self.variant_index, 5)?;

        let in_bounds = self.h4_h2a_position_in_bounds.unwrap_or(true);
        bitstream.write_bool(in_bounds)?;
        let mut per_axis_bit_counts = int32_point3d::default();
        adjust_axis_encoding_bit_count_to_match_error_goals(21, world_bounds, 26, &mut per_axis_bit_counts);
        let quantized_point = match self.h4_h2a_position_raw {
            Some(q) => q,
            None => {
                let mut q = int32_point3d::default();
                quantize_real_point3d_per_axis(
                    &self.position,
                    world_bounds,
                    &per_axis_bit_counts,
                    &mut q,
                );
                q
            }
        };
        bitstream.write_point3d_efficient(&quantized_point, &per_axis_bit_counts)?;

        let axes = self.h4_h2a_axes.unwrap_or_default();
        bitstream.write_bool(axes.is_default)?;
        bitstream.write_integer(axes.up_rotation_raw, 14)?;
        if !axes.is_default {
            bitstream.write_integer(axes.forward_angle_raw, 20)?;
        }

        bitstream.write_integer((self.spawn_relative_to + 1) as u32, 10)?;

        let extra_q: u8 = self.h4_h2a_extra_quantized_raw.unwrap_or(0);
        bitstream.write_integer(extra_q as u32, 6)?;
        let extra_b: bool = self.h4_h2a_extra_bool.unwrap_or(false);
        bitstream.write_bool(extra_b)?;

        mp.encode(bitstream, label_bits)?;
        Ok(())
    }
}

impl s_h4_h2a_mp_props {
    /// Decode an H4/H2A mp_object_properties from the bitstream. The only
    /// per-engine wire difference is `label_index` bit count: H4 = 10 bits,
    /// H2A = 11 bits (verified via halo4.dll+0xB7488 vs groundhog.dll+0xB63C5).
    pub fn decode(bitstream: &mut c_bitstream_reader, label_bits: u32) -> BLFLibResult<Self> {
        let boundary = s_multiplayer_object_boundary::decode(bitstream)?;
        // halo4.dll H4_mp_object_properties_decode @ +0xB7410 — 6-bit cached_type (0..63)
        let cached_type: u8 = bitstream.read_unnamed_integer(6)?;
        // halo4.dll +0xB7488 (10 bits) / groundhog.dll +0xB63C5 (11 bits) — label_index width per engine
        let label_index: u16 = bitstream.read_unnamed_integer(label_bits as usize)?;

        let tail = match cached_type {
            0x20 => {
                // halo4.dll H4_mp_props_decode_branch_cached_type_0x20 @ +0xB74D4 — 4+8+16 = 28 bits
                let raw4: u8 = bitstream.read_unnamed_integer::<u32>(4)? as u8;
                let field_4bit_signed: i8 = (raw4 as i8) - 1;
                let field_8bit: u8 = bitstream.read_unnamed_integer(8)?;
                let field_16bit: u16 = bitstream.read_unnamed_integer(16)?;
                s_h4_h2a_mp_props_tail::Type20Advanced { field_4bit_signed, field_8bit, field_16bit }
            }
            0x21 => {
                // halo4.dll H4_mp_props_decode_branch_cached_type_0x21_or_0x22 @ +0xB7599 — 8 × u8
                let mut teleporter_field_bytes = [0u8; 8];
                for i in 0..8 {
                    teleporter_field_bytes[i] = bitstream.read_unnamed_integer(8)?;
                }
                s_h4_h2a_mp_props_tail::Type21Teleporter1 { teleporter_field_bytes }
            }
            0x22 => {
                // halo4.dll H4_mp_props_decode_branch_cached_type_0x21_or_0x22 @ +0xB7599 — 9 × u8
                let mut teleporter_field_bytes = [0u8; 9];
                for i in 0..9 {
                    teleporter_field_bytes[i] = bitstream.read_unnamed_integer(8)?;
                }
                s_h4_h2a_mp_props_tail::Type22Teleporter2 { teleporter_field_bytes }
            }
            0x23 => {
                // halo4.dll H4_mp_props_decode_branch_cached_type_default_or_0x23 @ +0xB7794 — 0 tail bits
                s_h4_h2a_mp_props_tail::None
            }
            _ => {
                // halo4.dll H4_mp_props_decode_branch_cached_type_default_or_0x23 @ +0xB7794 — common 60-bit prefix
                let raw_pcc: u8 = bitstream.read_unnamed_integer::<u32>(4)? as u8;
                let primary_change_color: i8 = (raw_pcc as i8) - 1;
                let spawn_time: u8 = bitstream.read_unnamed_integer(8)?;
                let team: i8 = bitstream.read_unnamed_index::<8>(3)? as i8;
                let flags_lo: u8 = bitstream.read_unnamed_integer(8)?;
                let flags_hi_raw: u8 = bitstream.read_unnamed_integer(8)?;
                let flags_hi_signed: i8 = flags_hi_raw as i8;
                let mut user_data_block = [0i32; 4];
                for i in 0..4 {
                    user_data_block[i] = bitstream.read_unnamed_index::<256>(8)?;
                }
                let sub: DefaultSubcase = match cached_type {
                    // halo4.dll H4_mp_props_decode_subcase_weapon_or_teleporter @ +0xB79B6 — 8-bit spare_clips / channel
                    1 => DefaultSubcase::Weapon { spare_clips: bitstream.read_unnamed_integer(8)? },
                    12 => DefaultSubcase::Teleporter12 { channel_or_mask: bitstream.read_unnamed_integer(8)? },
                    20 => {
                        // halo4.dll H4_mp_props_decode_subcase_location_name @ +0xB7983 — 8-bit-1
                        let raw: u8 = bitstream.read_unnamed_integer(8)?;
                        DefaultSubcase::LocationName { location_name_index: (raw as i32) - 1 }
                    }
                    // halo4.dll H4_mp_props_decode_subcase_trait_zone @ +0xB7963 — 5-bit channel
                    31 => DefaultSubcase::TraitZone { trait_zone_channel: bitstream.read_unnamed_integer::<u32>(5)? as u8 },
                    _ => {
                        let min_count: u8 = bitstream.read_unnamed_integer::<u32>(5)? as u8;
                        let max_count: u8 = bitstream.read_unnamed_integer::<u32>(5)? as u8;
                        DefaultSubcase::QuotaOther { min_count, max_count }
                    }
                };
                s_h4_h2a_mp_props_tail::Default {
                    primary_change_color, spawn_time, team, flags_lo, flags_hi_signed,
                    user_data_block, sub,
                }
            }
        };

        Ok(Self { boundary, cached_type, label_index, tail })
    }

    pub fn encode(&self, bitstream: &mut c_bitstream_writer, label_bits: u32) -> BLFLibResult {
        match self.boundary {
            Some(b) => b.encode(bitstream)?,
            None => {
                s_multiplayer_object_boundary::default().encode(bitstream)?;
            }
        }
        bitstream.write_integer(self.cached_type as u32, 6)?;
        bitstream.write_integer(self.label_index as u32, label_bits as usize)?;

        match self.tail {
            s_h4_h2a_mp_props_tail::None => { /* 0 bits */ }
            s_h4_h2a_mp_props_tail::Type20Advanced { field_4bit_signed, field_8bit, field_16bit } => {
                let raw4: u32 = ((field_4bit_signed + 1) as u32) & 0xF;
                bitstream.write_integer(raw4, 4)?;
                bitstream.write_integer(field_8bit as u32, 8)?;
                bitstream.write_integer(field_16bit as u32, 16)?;
            }
            s_h4_h2a_mp_props_tail::Type21Teleporter1 { teleporter_field_bytes } => {
                for b in teleporter_field_bytes.iter() {
                    bitstream.write_integer(*b as u32, 8)?;
                }
            }
            s_h4_h2a_mp_props_tail::Type22Teleporter2 { teleporter_field_bytes } => {
                for b in teleporter_field_bytes.iter() {
                    bitstream.write_integer(*b as u32, 8)?;
                }
            }
            s_h4_h2a_mp_props_tail::Default {
                primary_change_color, spawn_time, team, flags_lo, flags_hi_signed,
                user_data_block, sub,
            } => {
                bitstream.write_integer(((primary_change_color + 1) as u32) & 0xF, 4)?;
                bitstream.write_integer(spawn_time as u32, 8)?;
                bitstream.write_index::<8>(team as i32, 3)?;
                bitstream.write_integer(flags_lo as u32, 8)?;
                bitstream.write_integer((flags_hi_signed as u8) as u32, 8)?;
                for ix in user_data_block.iter() {
                    bitstream.write_index::<256>(*ix, 8)?;
                }
                match sub {
                    DefaultSubcase::Weapon { spare_clips } => bitstream.write_integer(spare_clips as u32, 8)?,
                    DefaultSubcase::Teleporter12 { channel_or_mask } => bitstream.write_integer(channel_or_mask as u32, 8)?,
                    DefaultSubcase::LocationName { location_name_index } => {
                        bitstream.write_integer(((location_name_index + 1) as u32) & 0xFF, 8)?;
                    }
                    DefaultSubcase::TraitZone { trait_zone_channel } => bitstream.write_integer(trait_zone_channel as u32, 5)?,
                    DefaultSubcase::QuotaOther { min_count, max_count } => {
                        bitstream.write_integer(min_count as u32, 5)?;
                        bitstream.write_integer(max_count as u32, 5)?;
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Default, PartialEq, Debug, Clone, Copy, Serialize, Deserialize, BinRead, BinWrite, TestSize)]
#[Size(0x8)]
pub struct c_object_identifier {
    m_unique_id: i32,
    m_origin_bsp_index: i16,
    m_type: i8,
    m_source: i8,
}
