use std::io::{Read, Seek, Write};
use binrw::{BinRead, BinResult, BinWrite, Endian};
#[cfg(feature = "napi")]
use napi_derive::napi;
use serde::{Deserialize, Serialize};
use blf_lib::blam::halo3::v12070_08_09_05_2031_halo3_ship::saved_games::saved_game_files::s_content_item_metadata;
use blf_lib_derivable::blf::chunks::BlfChunkHooks;
use blf_lib_derive::BlfChunk;

#[cfg_attr(feature = "napi", napi(object, namespace = "halo3_10015_07_05_14_2217_delta"))]
#[derive(BlfChunk, Default, PartialEq, Debug, Clone, Serialize, Deserialize)]
#[Header("chdr", 9.1)]
pub struct s_blf_chunk_content_header
{
    pub build_number: u16,
    pub build_sequence_number: u16,
    pub metadata: s_content_item_metadata,
    #[serde(skip)]
    pub raw_body: Vec<u8>,
}

impl BinRead for s_blf_chunk_content_header {
    type Args<'a> = ();
    fn read_options<R: Read + Seek>(reader: &mut R, _endian: Endian, _args: Self::Args<'_>) -> BinResult<Self> {
        let mut buf = Vec::<u8>::new();
        reader.read_to_end(&mut buf)?;
        let mut cur = std::io::Cursor::new(&buf);
        let build_number: u16 = BinRead::read_options(&mut cur, Endian::Big, ()).unwrap_or(0);
        let build_sequence_number: u16 = BinRead::read_options(&mut cur, Endian::Big, ()).unwrap_or(0);
        let metadata: s_content_item_metadata = BinRead::read_options(&mut cur, Endian::Big, ()).unwrap_or_default();
        Ok(Self { build_number, build_sequence_number, metadata, raw_body: buf })
    }
}

impl BinWrite for s_blf_chunk_content_header {
    type Args<'a> = ();
    fn write_options<W: Write + Seek>(&self, writer: &mut W, _endian: Endian, _args: Self::Args<'_>) -> BinResult<()> {
        if !self.raw_body.is_empty() {
            writer.write_all(&self.raw_body)?;
            return Ok(());
        }
        self.build_number.write_options(writer, Endian::Big, ())?;
        self.build_sequence_number.write_options(writer, Endian::Big, ())?;
        self.metadata.write_options(writer, Endian::Big, ())?;
        Ok(())
    }
}

impl BlfChunkHooks for s_blf_chunk_content_header {}

impl s_blf_chunk_content_header {
    /// Drop captured raw bytes so the next encode reflects struct edits.
    pub fn clear_raw_overrides(&mut self) {
        self.raw_body.clear();
    }
}
