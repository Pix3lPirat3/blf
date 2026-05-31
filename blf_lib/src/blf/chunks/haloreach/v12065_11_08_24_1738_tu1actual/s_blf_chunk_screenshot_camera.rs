use binrw::binrw;
use serde::{Deserialize, Serialize};
use blf_lib::types::array::StaticArray;
use blf_lib_derivable::blf::chunks::BlfChunkHooks;
use blf_lib_derive::BlfChunk;
#[cfg(feature = "napi")]
use napi_derive::napi;

#[binrw]
#[cfg_attr(feature = "napi", napi(object, namespace = "haloreach_12065_11_08_24_1738_tu1actual"))]
#[derive(BlfChunk,Default,PartialEq,Debug,Clone,Serialize,Deserialize)]
#[Header("scnc", 4.1)]
#[brw(big)]
// This is a stubbed chunk, i have no idea what it contains.
pub struct s_blf_chunk_screenshot_camera
{
    // includes s_screenshot_player_info
    pub data: StaticArray<u8, 4964>,
}

impl BlfChunkHooks for s_blf_chunk_screenshot_camera {}
