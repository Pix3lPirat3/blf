use std::io::{Read, Seek, Write};
use binrw::{BinRead, BinResult, BinWrite, Endian};
use serde::{Deserialize, Serialize};
use blf_lib_derivable::blf::chunks::BlfChunkHooks;
use crate::blam::halo3::v12070_08_09_05_2031_halo3_ship::saved_games::scenario_map_variant::c_map_variant;
use blf_lib_derive::BlfChunk;

// halo3.dll mapv chunk handler @ +0x10D2A0 — H3 retail mapv version 12.1
#[derive(BlfChunk, Default, PartialEq, Debug, Clone, Serialize, Deserialize)]
#[Header("mapv", 12.1)]
pub struct s_blf_chunk_map_variant
{
    pub map_variant: c_map_variant,
    #[serde(skip)]
    pub raw_lead: [u8; 4],
    #[serde(skip)]
    pub raw_body: Vec<u8>,
}

impl BinRead for s_blf_chunk_map_variant {
    type Args<'a> = ();
    fn read_options<R: Read + Seek>(reader: &mut R, _endian: Endian, _args: Self::Args<'_>) -> BinResult<Self> {
        let mut body = Vec::<u8>::new();
        reader.read_to_end(&mut body)?;
        let mut out = Self::default();
        if body.len() < 4 {
            out.raw_body = body;
            return Ok(out);
        }
        out.raw_lead.copy_from_slice(&body[..4]);
        let mut cur = std::io::Cursor::new(&body[4..]);
        out.map_variant = <c_map_variant as BinRead>::read_options(&mut cur, Endian::Big, ())
            .unwrap_or_default();
        out.raw_body = body;
        Ok(out)
    }
}

impl BinWrite for s_blf_chunk_map_variant {
    type Args<'a> = ();
    fn write_options<W: Write + Seek>(&self, writer: &mut W, _endian: Endian, _args: Self::Args<'_>) -> BinResult<()> {
        if !self.raw_body.is_empty() {
            writer.write_all(&self.raw_body)?;
            return Ok(());
        }
        writer.write_all(&self.raw_lead)?;
        self.map_variant.write_options(writer, Endian::Big, ())?;
        Ok(())
    }
}

impl BlfChunkHooks for s_blf_chunk_map_variant {}

impl s_blf_chunk_map_variant {
    pub fn create(map_variant: c_map_variant) -> Self {
        Self {
            map_variant,
            ..Default::default()
        }
    }

    /// Drop captured raw bytes so the next encode reflects struct edits.
    pub fn clear_raw_overrides(&mut self) {
        self.raw_body.clear();
    }
}
