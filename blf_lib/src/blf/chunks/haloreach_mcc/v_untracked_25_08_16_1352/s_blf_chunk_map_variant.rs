
use std::io::{Read, Seek, Write};
use binrw::{BinRead, BinResult, BinWrite, BinWriterExt, Endian};
use serde::{Deserialize, Serialize};
use blf_lib::blam::common::memory::secure_signature::s_network_http_request_hash;
use blf_lib::blam::haloreach_mcc::v_untracked_25_08_16_1352::saved_games::scenario_map_variant::c_map_variant;
use blf_lib::io::bitstream::{c_bitstream_reader, c_bitstream_writer, e_bitstream_byte_order};
use blf_lib_derivable::blf::chunks::BlfChunkHooks;
use blf_lib_derive::BlfChunk;
use sha1::{Digest, Sha1};

// haloreach.dll mvar_buffer_initialize_empty @ +0x67900 — hardcodes 0x7028 = chunk body+12-byte header
const MCC_MVAR_CHUNK_SIZE: usize = 0x7028;
const MCC_MVAR_CHUNK_BODY_SIZE: usize = MCC_MVAR_CHUNK_SIZE - 12; // 28700

// haloreach.dll mvar_buffer_initialize_empty @ +0x67900 — Reach mvar chunk magic + version 31.1
#[derive(BlfChunk, PartialEq, Debug, Clone, Serialize, Deserialize)]
#[Header("mvar", 31.1)]
#[derive(Default)]
pub struct s_blf_chunk_map_variant {
    pub map_variant: c_map_variant,
    #[serde(skip)]
    pub raw_hash: Vec<u8>,
    #[serde(skip)]
    pub raw_tail: Vec<u8>,
    #[serde(skip)]
    pub raw_body: Vec<u8>,
}

impl BlfChunkHooks for s_blf_chunk_map_variant {}

impl BinRead for s_blf_chunk_map_variant {
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(reader: &mut R, _endian: Endian, _args: Self::Args<'_>) -> BinResult<Self> {
        let mut body = Vec::<u8>::new();
        reader.read_to_end(&mut body)?;
        let mut packed_map_variant = Self::default();
        if body.len() < 24 {
            packed_map_variant.raw_body = body;
            return Ok(packed_map_variant);
        }
        // haloreach.dll mvar_serialize_and_sha1 @ +0x17AC38 — SHA-1 at chunk+0x0C (20 bytes)
        packed_map_variant.raw_hash = body[..20].to_vec();
        // haloreach.dll mvar_serialize_and_sha1 @ +0x17AC38 — BE u32 payload_size at chunk+0x20
        let packed_variant_length = u32::from_be_bytes([body[20], body[21], body[22], body[23]]) as usize;
        let payload_end = 24usize.saturating_add(packed_variant_length).min(body.len());
        if body.len() > payload_end {
            packed_map_variant.raw_tail = body[payload_end..].to_vec();
        }

        let bitstream_slice = &body[24..payload_end];
        let mut bitstream = c_bitstream_reader::new(bitstream_slice, e_bitstream_byte_order::_bitstream_byte_order_big_endian);
        bitstream.begin_reading();
        let _ = packed_map_variant.map_variant.decode(&mut bitstream);
        packed_map_variant.raw_body = body;
        Ok(packed_map_variant)
    }
}

impl BinWrite for s_blf_chunk_map_variant {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(&self, writer: &mut W, _endian: Endian, _args: Self::Args<'_>) -> BinResult<()> {
        if !self.raw_body.is_empty() {
            writer.write_all(&self.raw_body)?;
            return Ok(());
        }
        let mut bitstream = c_bitstream_writer::new(0xD9B0, e_bitstream_byte_order::_bitstream_byte_order_big_endian);
        bitstream.begin_writing();

        self.map_variant.encode(&mut bitstream)?;

        bitstream.finish_writing();
        let packed_data = bitstream.get_data()?;
        let packed_data_length = packed_data.len() as u32;

        let hash_bytes: [u8; 20] = if self.raw_hash.len() == 20 {
            let mut h = [0u8; 20];
            h.copy_from_slice(&self.raw_hash);
            h
        } else {
            // haloreach.dll mvar_serialize_and_sha1 @ +0x17AC38 — SHA1(LE size_u32 || payload)
            let mut hash_input = packed_data_length.to_le_bytes().to_vec();
            hash_input.extend_from_slice(&packed_data);
            let mut hasher = Sha1::new();
            Digest::update(&mut hasher, &hash_input);
            hasher.finalize().into()
        };

        writer.write_ne(&hash_bytes)?;
        packed_data_length.write_options(writer, Endian::Big, ())?;
        writer.write_ne(&bitstream.get_data()?)?;

        let written_body = 20usize + 4 + packed_data.len();
        let pad_size = MCC_MVAR_CHUNK_BODY_SIZE.saturating_sub(written_body);
        if pad_size > 0 {
            if self.raw_tail.len() == pad_size {
                writer.write_ne(&self.raw_tail)?;
            } else {
                let padding = vec![0u8; pad_size];
                writer.write_ne(&padding)?;
            }
        }
        Ok(())
    }
}

impl s_blf_chunk_map_variant {
    pub fn create(map_variant: c_map_variant) -> Self {
        Self { map_variant, ..Default::default() }
    }

    /// Drop captured raw bytes so the next encode reflects struct edits.
    pub fn clear_raw_overrides(&mut self) {
        self.raw_hash.clear();
        self.raw_tail.clear();
        self.raw_body.clear();
    }
}
