use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use binrw::{BinRead, BinResult, BinWrite, Endian};
#[cfg(feature = "napi")]
use napi_derive::napi;
use serde::{Deserialize, Serialize};
use blf_lib::blam::halo3::v12070_08_09_05_2031_halo3_ship::game::game_engine_variant::c_game_variant;
use blf_lib_derivable::blf::chunks::BlfChunkHooks;
use blf_lib_derive::BlfChunk;

const H3_RETAIL_GAME_VARIANT_SIZE: usize = 0x264;

// halo3.dll mpvr chunk handler — H3 retail game variant mpvr version 3.1
#[derive(BlfChunk, Default, PartialEq, Debug, Clone, Serialize, Deserialize)]
#[Header("mpvr", 3.1)]
#[cfg_attr(feature = "napi", napi(object, namespace = "halo3_12070_08_09_05_2031_halo3_ship"))]
pub struct s_blf_chunk_game_variant
{
    pub game_variant: c_game_variant,
    #[serde(skip)]
    pub raw_body: Vec<u8>,
    #[serde(skip)]
    pub raw_was_little_endian: bool,
}

impl BinRead for s_blf_chunk_game_variant {
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(reader: &mut R, _endian: Endian, _args: Self::Args<'_>) -> BinResult<Self> {
        let mut body = vec![0u8; H3_RETAIL_GAME_VARIANT_SIZE];
        let mut filled = 0usize;
        while filled < H3_RETAIL_GAME_VARIANT_SIZE {
            match reader.read(&mut body[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) => return Err(binrw::Error::Io(e)),
            }
        }
        body.truncate(filled.max(4));

        let raw_was_little_endian = if filled >= 4 {
            let be = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
            let le = u32::from_le_bytes([body[0], body[1], body[2], body[3]]);
            if be <= 10 { false }
            else if le <= 10 { true }
            else { false } // best-effort default
        } else { false };

        let chosen_endian = if raw_was_little_endian { Endian::Little } else { Endian::Big };
        let mut cur = Cursor::new(&body);
        let game_variant: c_game_variant = match BinRead::read_options(&mut cur, chosen_endian, ()) {
            Ok(v) => v,
            Err(_) => {
                let other = if raw_was_little_endian { Endian::Big } else { Endian::Little };
                let mut cur2 = Cursor::new(&body);
                BinRead::read_options(&mut cur2, other, ()).unwrap_or_default()
            }
        };

        Ok(Self {
            game_variant,
            raw_body: body,
            raw_was_little_endian,
        })
    }
}

impl BinWrite for s_blf_chunk_game_variant {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(&self, writer: &mut W, _endian: Endian, _args: Self::Args<'_>) -> BinResult<()> {
        if self.raw_body.len() == H3_RETAIL_GAME_VARIANT_SIZE {
            writer.write_all(&self.raw_body)?;
            return Ok(());
        }
        let chosen_endian = if self.raw_was_little_endian { Endian::Little } else { Endian::Big };
        let start = writer.stream_position()?;
        self.game_variant.write_options(writer, chosen_endian, ())?;
        let end = writer.stream_position()?;
        let written = (end - start) as usize;
        if written < H3_RETAIL_GAME_VARIANT_SIZE {
            writer.write_all(&vec![0u8; H3_RETAIL_GAME_VARIANT_SIZE - written])?;
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

    /// Drop captured raw bytes so the next encode reflects struct edits.
    pub fn clear_raw_overrides(&mut self) {
        self.raw_body.clear();
    }
}
