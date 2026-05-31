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
#[Header("ssig", 1.1)]
#[brw(big)]
// name is a guess
pub struct s_blf_chunk_server_signature
{
    pub data: StaticArray<u8, 44>,
}

impl BlfChunkHooks for s_blf_chunk_server_signature {}
