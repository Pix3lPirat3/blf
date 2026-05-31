use std::io::{Read, Seek, Write};
use binrw::{BinRead, BinResult, BinWrite, Endian};
use serde::{Deserialize, Serialize};
use blf_lib::blam::common::memory::secure_signature::s_network_http_request_hash;
use blf_lib::io::bitstream::{c_bitstream_reader, c_bitstream_writer, e_bitstream_byte_order};
use blf_lib_derivable::blf::chunks::BlfChunkHooks;
use blf_lib_derive::BlfChunk;
use crate::blam::haloreach_mcc::v_untracked_25_08_16_1352::game::game_variant::c_game_variant;
use crate::blf::get_buffer_hash;

/// Total mpvr chunk body size (hash/header + gametype slot).
pub const variant_storage_length: usize = 0x5000;

#[derive(BlfChunk,PartialEq,Debug,Clone,Serialize,Deserialize,Default)]
#[Header("mpvr", 54.1)]
pub struct s_blf_chunk_game_variant
{
    pub game_variant: c_game_variant,
    #[serde(skip)]
    pub raw_hash: Vec<u8>,
    #[serde(skip)]
    pub raw_unknown04: Option<i16>,
    #[serde(skip)]
    pub raw_unknown06: Option<u16>,
    #[serde(skip)]
    pub raw_variant_length_bits: Option<u32>,
    #[serde(skip)]
    pub raw_gametype_data: Vec<u8>,
    #[serde(skip)]
    pub raw_tail: Vec<u8>,
}

impl BinRead for s_blf_chunk_game_variant {
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(reader: &mut R, endian: Endian, args: Self::Args<'_>) -> BinResult<Self> {
        let mut data: Vec<u8> = Vec::new();
        reader.read_to_end(&mut data)?;

        let mut bitstream = c_bitstream_reader::new(data.as_slice(), e_bitstream_byte_order::_bitstream_byte_order_big_endian);
        bitstream.begin_reading();

        let hash: s_network_http_request_hash = bitstream.read_raw(0x14 * 8)?;
        let unknown04: i16 = bitstream.read_signed_integer("unknown04", 16)?;
        let unknown06: u16 = bitstream.read_integer("unknown06", 16)?;
        let variant_length: u32 = bitstream.read_integer("variant-length", 32)?;

        let payload_byte_start = 28usize; // 20B hash + 2B unk04 + 2B unk06 + 4B variant_length
        let nominal_payload_bytes = ((variant_length as usize).saturating_add(7)) / 8;
        let available_slot = data.len().saturating_sub(payload_byte_start)
            .min(variant_storage_length);
        let raw_payload_byte_len = nominal_payload_bytes.min(available_slot);
        let raw_gametype_data: Vec<u8> = if raw_payload_byte_len > 0
            && data.len() >= payload_byte_start + raw_payload_byte_len {
            data[payload_byte_start..payload_byte_start + raw_payload_byte_len].to_vec()
        } else { Vec::new() };

        let mut game_variant = c_game_variant::default();
        let _ = game_variant.decode(&mut bitstream);

        let tail_start = payload_byte_start + raw_payload_byte_len;
        let tail_end = payload_byte_start + variant_storage_length;
        let tail = if data.len() >= tail_end && tail_start <= tail_end {
            data[tail_start..tail_end].to_vec()
        } else if data.len() > tail_start {
            data[tail_start..].to_vec()
        } else { Vec::new() };

        Ok(Self {
            game_variant,
            raw_hash: hash.data.get().clone(),
            raw_unknown04: Some(unknown04),
            raw_unknown06: Some(unknown06),
            raw_variant_length_bits: Some(variant_length),
            raw_gametype_data,
            raw_tail: tail,
        })
    }
}

impl BinWrite for s_blf_chunk_game_variant {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(&self, writer: &mut W, endian: Endian, args: Self::Args<'_>) -> BinResult<()> {
        let (gametype_bytes, gametype_bit_length): (Vec<u8>, u32) = if self.raw_variant_length_bits.is_some() {
            (self.raw_gametype_data.clone(), self.raw_variant_length_bits.unwrap())
        } else {
            let mut bitstream_writer = c_bitstream_writer::new(variant_storage_length, e_bitstream_byte_order::_bitstream_byte_order_big_endian);
            bitstream_writer.begin_writing();
            self.game_variant.encode(&mut bitstream_writer)?;
            bitstream_writer.finish_writing();
            let data = bitstream_writer.get_data()?;
            let bit_len = (data.len() as u32) * 8;
            (data, bit_len)
        };

        let hash_bytes: [u8; 20] = if self.raw_hash.len() == 20 {
            let mut h = [0u8; 20];
            h.copy_from_slice(&self.raw_hash);
            h
        } else {
            let mut hashable_data: Vec<u8> = (gametype_bytes.len() as u32).to_be_bytes().to_vec();
            hashable_data.extend_from_slice(gametype_bytes.as_slice());
            let hash = get_buffer_hash(&hashable_data)?;
            hash.data.get().as_slice().try_into().unwrap_or([0u8; 20])
        };
        let unknown04: i16 = self.raw_unknown04.unwrap_or(-1);
        let unknown06: u16 = self.raw_unknown06.unwrap_or(0);

        writer.write_all(&hash_bytes)?;
        unknown04.write_options(writer, Endian::Big, args)?;
        unknown06.write_options(writer, Endian::Big, args)?;
        gametype_bit_length.write_options(writer, Endian::Big, args)?;
        gametype_bytes.write_options(writer, Endian::Big, args)?;
        let gametype_data = gametype_bytes;

        let pad_size = variant_storage_length.saturating_sub(gametype_data.len());
        if pad_size > 0 {
            if self.raw_tail.len() == pad_size {
                writer.write_all(&self.raw_tail)?;
            } else {
                writer.write_all(&vec![0u8; pad_size])?;
            }
        }

        Ok(())
    }
}

impl BlfChunkHooks for s_blf_chunk_game_variant {}

impl s_blf_chunk_game_variant {
    pub fn create(game_variant: c_game_variant) -> s_blf_chunk_game_variant {
        s_blf_chunk_game_variant {
            game_variant,
            ..Default::default()
        }
    }

    /// Drop captured wire-byte overrides so the next encode produces bytes
    /// from the (possibly edited) `game_variant` struct. Call this after
    /// mutating any field on `game_variant`.
    pub fn clear_raw_overrides(&mut self) {
        self.raw_hash.clear();
        self.raw_unknown04 = None;
        self.raw_unknown06 = None;
        self.raw_variant_length_bits = None;
        self.raw_gametype_data.clear();
        self.raw_tail.clear();
    }
}
