use binrw::binrw;
#[cfg(feature = "napi")]
use napi_derive::napi;
use serde::{Deserialize, Serialize};
use blf_lib::types::time::time64_t;
use blf_lib_derivable::blf::chunks::BlfChunkHooks;
use blf_lib_derive::BlfChunk;

#[binrw]
#[derive(BlfChunk,PartialEq,Debug,Clone,Serialize,Deserialize,Default)]
#[Header("umsg", 1.0)]
#[brw(big)]
#[Size(0xC)]
#[cfg_attr(feature = "napi", napi(object, namespace = "haloreach_12065_11_08_24_1738_tu1actual"))]
pub struct s_blf_chunk_user_messaging_data {
    pub message_index: u32,
    pub expires_at: time64_t,
}

impl BlfChunkHooks for s_blf_chunk_user_messaging_data {}