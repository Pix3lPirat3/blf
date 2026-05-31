
use std::io::{Read, Seek, Write};
use binrw::{BinRead, BinResult, BinWrite, BinWriterExt, Endian};
use serde::{Deserialize, Serialize};
use blf_lib::blam::common::memory::secure_signature::s_network_http_request_hash;
use blf_lib::blam::haloreach_mcc::v_untracked_25_08_16_1352::saved_games::scenario_map_variant::c_map_variant;
use blf_lib::blam::haloreach_mcc::v_untracked_25_08_16_1352::saved_games::scenario_map_variant_h2a::{decode_map_variant_h2a_from_payload, encode_map_variant_h2a};
use blf_lib::io::bitstream::{c_bitstream_writer, e_bitstream_byte_order};
use blf_lib_derivable::blf::chunks::BlfChunkHooks;
use blf_lib_derive::BlfChunk;
use sha1::{Digest, Sha1};

// groundhog.dll H2A_mvar_chunk_validator @ +0xAEE78 — validates H2A mvar version 52, cap 0x7428
#[derive(BlfChunk, PartialEq, Debug, Clone, Serialize, Deserialize)]
#[Header("mvar", 52.1)]
#[derive(Default)]
pub struct s_blf_chunk_halo2a_map_variant {
    pub map_variant: c_map_variant,
    #[serde(skip)]
    pub raw_hash: Vec<u8>,
    #[serde(skip)]
    pub raw_tail: Vec<u8>,
}

impl BlfChunkHooks for s_blf_chunk_halo2a_map_variant {}

impl BinRead for s_blf_chunk_halo2a_map_variant {
    type Args<'a> = ();
    fn read_options<R: Read + Seek>(reader: &mut R, endian: Endian, _: Self::Args<'_>) -> BinResult<Self> {
        let mut out = Self::default();
        let hash: s_network_http_request_hash = s_network_http_request_hash::read_options(reader, endian, ())?;
        out.raw_hash = hash.data.get().clone();
        let len = u32::read_options(reader, Endian::Big, ())? as usize;
        let mut buf = Vec::<u8>::with_capacity(len);
        reader.read_to_end(&mut buf)?;
        if buf.len() > len {
            out.raw_tail = buf[len..].to_vec();
        }
        let payload = &buf[..len.min(buf.len())];
        decode_map_variant_h2a_from_payload(
            &mut out.map_variant,
            payload,
            e_bitstream_byte_order::_bitstream_byte_order_big_endian,
        )?;
        Ok(out)
    }
}

impl BinWrite for s_blf_chunk_halo2a_map_variant {
    type Args<'a> = ();
    fn write_options<W: Write + Seek>(&self, writer: &mut W, _e: Endian, _: Self::Args<'_>) -> BinResult<()> {
        let mut bs = c_bitstream_writer::new(0xD9B0, e_bitstream_byte_order::_bitstream_byte_order_big_endian);
        bs.begin_writing();
        encode_map_variant_h2a(&self.map_variant, &mut bs)?;
        bs.finish_writing();
        let packed = bs.get_data()?;
        let packed_len = packed.len() as u32;
        let hash_bytes: [u8; 20] = if self.raw_hash.len() == 20 {
            let mut h = [0u8; 20];
            h.copy_from_slice(&self.raw_hash);
            h
        } else {
            // groundhog.dll H2A_c_map_variant_hash_verify_and_decode @ +0x237390 — SHA1(LE size_u32 || payload)
            let mut hash_in = packed_len.to_le_bytes().to_vec();
            hash_in.extend_from_slice(&packed);
            let mut hasher = Sha1::new();
            Digest::update(&mut hasher, &hash_in);
            hasher.finalize().into()
        };
        writer.write_ne(&hash_bytes)?;
        packed_len.write_options(writer, Endian::Big, ())?;
        writer.write_ne(&bs.get_data()?)?;
        if !self.raw_tail.is_empty() {
            writer.write_ne(&self.raw_tail)?;
        }
        Ok(())
    }
}

impl s_blf_chunk_halo2a_map_variant {
    pub fn create(map_variant: c_map_variant) -> Self { Self { map_variant, raw_hash: Vec::new(), raw_tail: Vec::new() } }
}
