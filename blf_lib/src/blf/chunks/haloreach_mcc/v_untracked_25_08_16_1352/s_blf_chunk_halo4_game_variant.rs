
use std::io::{Read, Seek, Write};
use binrw::{BinRead, BinResult, BinWrite, Endian};
use serde::{Deserialize, Serialize};
use blf_lib::blam::common::memory::secure_signature::s_network_http_request_hash;
use blf_lib::blam::haloreach_mcc::v_untracked_25_08_16_1352::game::game_variant::c_game_variant;
use blf_lib_derivable::blf::chunks::BlfChunkHooks;
use blf_lib_derive::BlfChunk;

/// Total mpvr chunk-body capacity for H4 (28-byte header + 0x7C00 slot).
pub const h4_variant_storage_length: usize = 0x7C00;

#[derive(BlfChunk, PartialEq, Debug, Clone, Serialize, Deserialize, Default)]
#[Header("mpvr", 132.1)]
pub struct s_blf_chunk_halo4_game_variant
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

impl BinRead for s_blf_chunk_halo4_game_variant {
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(reader: &mut R, _endian: Endian, _args: Self::Args<'_>) -> BinResult<Self> {
        let mut data: Vec<u8> = Vec::new();
        reader.read_to_end(&mut data)?;
        if data.len() < 28 {
            return Ok(Self::default());
        }

        let hash: [u8; 20] = data[0..20].try_into().unwrap_or([0u8; 20]);
        let unknown04 = i16::from_be_bytes([data[20], data[21]]);
        let unknown06 = u16::from_be_bytes([data[22], data[23]]);
        let variant_length = u32::from_be_bytes([data[24], data[25], data[26], data[27]]);

        let payload_byte_start = 28usize;
        let nominal_payload_bytes = ((variant_length as usize).saturating_add(7)) / 8;
        let available_slot = data.len().saturating_sub(payload_byte_start)
            .min(h4_variant_storage_length);
        let raw_payload_byte_len = nominal_payload_bytes.min(available_slot);
        let raw_gametype_data: Vec<u8> = if raw_payload_byte_len > 0
            && data.len() >= payload_byte_start + raw_payload_byte_len {
            data[payload_byte_start..payload_byte_start + raw_payload_byte_len].to_vec()
        } else { Vec::new() };

        let tail_start = payload_byte_start + raw_payload_byte_len;
        let tail_end = payload_byte_start + h4_variant_storage_length;
        let tail = if data.len() >= tail_end && tail_start <= tail_end {
            data[tail_start..tail_end].to_vec()
        } else if data.len() > tail_start {
            data[tail_start..].to_vec()
        } else { Vec::new() };

        Ok(Self {
            game_variant: c_game_variant::default(),
            raw_hash: hash.to_vec(),
            raw_unknown04: Some(unknown04),
            raw_unknown06: Some(unknown06),
            raw_variant_length_bits: Some(variant_length),
            raw_gametype_data,
            raw_tail: tail,
        })
    }
}

impl BinWrite for s_blf_chunk_halo4_game_variant {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(&self, writer: &mut W, _endian: Endian, _args: Self::Args<'_>) -> BinResult<()> {
        let mut hash_bytes = [0u8; 20];
        if self.raw_hash.len() == 20 {
            hash_bytes.copy_from_slice(&self.raw_hash);
        }
        let unknown04 = self.raw_unknown04.unwrap_or(-1);
        let unknown06 = self.raw_unknown06.unwrap_or(0);
        let variant_length = self.raw_variant_length_bits.unwrap_or(0);

        writer.write_all(&hash_bytes)?;
        writer.write_all(&unknown04.to_be_bytes())?;
        writer.write_all(&unknown06.to_be_bytes())?;
        writer.write_all(&variant_length.to_be_bytes())?;
        writer.write_all(&self.raw_gametype_data)?;

        let pad_size = h4_variant_storage_length.saturating_sub(self.raw_gametype_data.len());
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

impl BlfChunkHooks for s_blf_chunk_halo4_game_variant {}

impl s_blf_chunk_halo4_game_variant {
    pub fn create(game_variant: c_game_variant) -> s_blf_chunk_halo4_game_variant {
        s_blf_chunk_halo4_game_variant {
            game_variant,
            ..Default::default()
        }
    }

    /// Drop captured wire-byte overrides; next encode will rebuild from
    /// the (possibly edited) `game_variant`. NOTE: H4 c_game_variant
    /// re-encoding is not yet implemented; clearing overrides on a
    /// decoded-only struct will currently produce an empty payload.
    pub fn clear_raw_overrides(&mut self) {
        self.raw_hash.clear();
        self.raw_unknown04 = None;
        self.raw_unknown06 = None;
        self.raw_variant_length_bits = None;
        self.raw_gametype_data.clear();
        self.raw_tail.clear();
    }
}
