
use std::io::{Read, Seek, Write};
use binrw::{BinRead, BinResult, BinWrite, BinWriterExt, Endian};
use serde::{Deserialize, Serialize};
use blf_lib::blam::common::memory::secure_signature::s_network_http_request_hash;
use blf_lib::blam::haloreach_mcc::v_untracked_25_08_16_1352::saved_games::scenario_map_variant::c_map_variant;
use blf_lib::blam::haloreach_mcc::v_untracked_25_08_16_1352::saved_games::scenario_map_variant_h4::{decode_map_variant_h4_from_payload, encode_map_variant_h4};
use blf_lib::io::bitstream::{c_bitstream_writer, e_bitstream_byte_order};
use blf_lib_derivable::blf::chunks::BlfChunkHooks;
use blf_lib_derive::BlfChunk;
use sha1::{Digest, Sha1};

// halo4.dll H4_mvar_chunk_validator @ +0x2383A4 — size cap 0x7028 (shared with Reach)
const MCC_MVAR_CHUNK_SIZE: usize = 0x7028;
const MCC_MVAR_CHUNK_BODY_SIZE: usize = MCC_MVAR_CHUNK_SIZE - 12;

// halo4.dll H4_mvar_chunk_validator @ +0x2383A4 — H4 mvar version 50.1
#[derive(BlfChunk, PartialEq, Debug, Clone, Serialize, Deserialize)]
#[Header("mvar", 50.1)]
#[derive(Default)]
pub struct s_blf_chunk_halo4_map_variant {
    pub map_variant: c_map_variant,
    #[serde(skip)]
    pub raw_hash: Vec<u8>,
    #[serde(skip)]
    pub raw_tail: Vec<u8>,
    #[serde(skip)]
    pub raw_body: Vec<u8>,
}

impl BlfChunkHooks for s_blf_chunk_halo4_map_variant {}

impl BinRead for s_blf_chunk_halo4_map_variant {
    type Args<'a> = ();
    fn read_options<R: Read + Seek>(reader: &mut R, endian: Endian, _: Self::Args<'_>) -> BinResult<Self> {
        let mut body = Vec::<u8>::new();
        reader.read_to_end(&mut body)?;
        let mut out = Self::default();
        if body.len() < 24 {
            out.raw_body = body;
            return Ok(out);
        }
        // halo4.dll H4_c_map_variant_hash_verify_and_decode @ +0x238258 — 20-byte SHA-1 at +0x00
        out.raw_hash = body[..20].to_vec();
        // halo4.dll H4_c_map_variant_hash_verify_and_decode @ +0x238258 — BE u32 payload_size at +0x14
        let len = u32::from_be_bytes([body[20], body[21], body[22], body[23]]) as usize;
        let payload_end = 24usize.saturating_add(len).min(body.len());
        if body.len() > payload_end {
            out.raw_tail = body[payload_end..].to_vec();
        }
        let payload = &body[24..payload_end];
        let _ = decode_map_variant_h4_from_payload(
            &mut out.map_variant,
            payload,
            e_bitstream_byte_order::_bitstream_byte_order_big_endian,
        );
        out.raw_body = body;
        Ok(out)
    }
}

impl BinWrite for s_blf_chunk_halo4_map_variant {
    type Args<'a> = ();
    fn write_options<W: Write + Seek>(&self, writer: &mut W, _e: Endian, _: Self::Args<'_>) -> BinResult<()> {
        if !self.raw_body.is_empty() {
            writer.write_all(&self.raw_body)?;
            return Ok(());
        }
        let mut bs = c_bitstream_writer::new(0xD9B0, e_bitstream_byte_order::_bitstream_byte_order_big_endian);
        bs.begin_writing();
        encode_map_variant_h4(&self.map_variant, &mut bs)?;
        bs.finish_writing();
        let packed = bs.get_data()?;
        let packed_len = packed.len() as u32;
        let hash_bytes: [u8; 20] = if self.raw_hash.len() == 20 {
            let mut h = [0u8; 20];
            h.copy_from_slice(&self.raw_hash);
            h
        } else {
            // halo4.dll H4_c_map_variant_hash_verify_and_decode @ +0x238258 — SHA1(LE size_u32 || payload)
            let mut hash_in = packed_len.to_le_bytes().to_vec();
            hash_in.extend_from_slice(&packed);
            let mut hasher = Sha1::new();
            Digest::update(&mut hasher, &hash_in);
            hasher.finalize().into()
        };
        writer.write_ne(&hash_bytes)?;
        packed_len.write_options(writer, Endian::Big, ())?;
        writer.write_ne(&bs.get_data()?)?;
        let written = 20 + 4 + packed.len();
        let pad_size = MCC_MVAR_CHUNK_BODY_SIZE.saturating_sub(written);
        if pad_size > 0 {
            if self.raw_tail.len() == pad_size {
                writer.write_ne(&self.raw_tail)?;
            } else {
                let pad = vec![0u8; pad_size];
                writer.write_ne(&pad)?;
            }
        }
        Ok(())
    }
}

impl s_blf_chunk_halo4_map_variant {
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
