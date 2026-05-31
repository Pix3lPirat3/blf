use std::io::{Read, Seek, Write};
use binrw::{BinRead, BinResult, BinWrite, BinWriterExt, Endian};
use serde::{Deserialize, Serialize};
use crate::io::bitstream::{e_bitstream_byte_order};
use crate::blam::halo3::v12070_08_09_05_2031_halo3_ship::saved_games::scenario_map_variant::c_map_variant;
use blf_lib::io::bitstream::{c_bitstream_reader, c_bitstream_writer};
use blf_lib_derivable::blf::chunks::BlfChunkHooks;
use blf_lib_derive::BlfChunk;

// halo3.dll mapv body decoder candidate @ +0xBF698 — H3 mvar version 12.1 (packed variant)
#[derive(BlfChunk, Default, PartialEq, Debug, Clone, Serialize, Deserialize)]
#[Header("mvar", 12.1)]
pub struct s_blf_chunk_packed_map_variant
{
    pub map_variant: c_map_variant,
    #[serde(skip)]
    pub raw_body: Vec<u8>,
}

impl BlfChunkHooks for s_blf_chunk_packed_map_variant {}

impl BinRead for s_blf_chunk_packed_map_variant {
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(reader: &mut R, _endian: Endian, _args: Self::Args<'_>) -> BinResult<Self> {
        let mut buffer = Vec::<u8>::new();
        reader.read_to_end(&mut buffer)?;

        let mut bitstream = c_bitstream_reader::new(buffer.as_slice(), e_bitstream_byte_order::_bitstream_byte_order_big_endian);
        bitstream.begin_reading();

        let mut packed_map_variant = Self::default();
        let _ = packed_map_variant.map_variant.decode(&mut bitstream);
        packed_map_variant.raw_body = buffer;

        Ok(packed_map_variant)
    }
}

impl BinWrite for s_blf_chunk_packed_map_variant {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(&self, writer: &mut W, _endian: Endian, _args: Self::Args<'_>) -> BinResult<()> {
        if !self.raw_body.is_empty() {
            writer.write_all(&self.raw_body)?;
            return Ok(());
        }
        let mut bitstream = c_bitstream_writer::new(0xE0A0, e_bitstream_byte_order::_bitstream_byte_order_big_endian);
        bitstream.begin_writing();
        self.map_variant.encode(&mut bitstream)?;
        bitstream.finish_writing();
        writer.write_ne(&bitstream.get_data()?)
    }
}

impl s_blf_chunk_packed_map_variant {
    /// Drop captured raw bytes so the next encode reflects struct edits.
    pub fn clear_raw_overrides(&mut self) {
        self.raw_body.clear();
    }
}
