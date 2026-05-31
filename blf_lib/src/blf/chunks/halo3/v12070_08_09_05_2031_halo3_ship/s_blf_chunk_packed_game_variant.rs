use std::io::{Read, Seek, Write};
use binrw::{BinRead, BinResult, BinWrite, BinWriterExt, Endian};
use serde::{Deserialize, Serialize};
use blf_lib::blam::halo3::v12070_08_09_05_2031_halo3_ship::game::game_engine_variant::c_game_variant;
use blf_lib::io::bitstream::{c_bitstream_reader, c_bitstream_writer, e_bitstream_byte_order};
use blf_lib_derivable::blf::chunks::BlfChunkHooks;
use blf_lib_derive::BlfChunk;

#[derive(BlfChunk,Default,PartialEq,Debug,Clone,Serialize,Deserialize)]
#[Header("gvar", 10.1)]
pub struct s_blf_chunk_packed_game_variant
{
    pub game_variant: c_game_variant,
    #[serde(skip)]
    pub raw_body: Vec<u8>,
}

impl BlfChunkHooks for s_blf_chunk_packed_game_variant {}

impl s_blf_chunk_packed_game_variant {
    pub fn create(game_variant: c_game_variant) -> Self {
        Self {
            game_variant,
            raw_body: Vec::new(),
        }
    }

    /// Drop captured raw bytes so the next encode reflects struct edits.
    pub fn clear_raw_overrides(&mut self) {
        self.raw_body.clear();
    }
}

impl BinRead for s_blf_chunk_packed_game_variant {
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(reader: &mut R, _endian: Endian, _args: Self::Args<'_>) -> BinResult<Self> {
        let mut buffer = Vec::<u8>::new();
        reader.read_to_end(&mut buffer)?;

        let mut bitstream = c_bitstream_reader::new(buffer.as_slice(), e_bitstream_byte_order::_bitstream_byte_order_big_endian);
        bitstream.begin_reading();

        let mut packed_game_variant = Self::default();
        let _ = packed_game_variant.game_variant.decode(&mut bitstream);
        packed_game_variant.raw_body = buffer;

        Ok(packed_game_variant)
    }
}

impl BinWrite for s_blf_chunk_packed_game_variant {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(&self, writer: &mut W, _endian: Endian, _args: Self::Args<'_>) -> BinResult<()> {
        if !self.raw_body.is_empty() {
            writer.write_all(&self.raw_body)?;
            return Ok(());
        }
        let mut bitstream = c_bitstream_writer::new(0x264, e_bitstream_byte_order::_bitstream_byte_order_big_endian);
        bitstream.begin_writing();
        self.game_variant.encode(&mut bitstream)?;
        bitstream.finish_writing();
        writer.write_ne(&bitstream.get_data()?)
    }
}
