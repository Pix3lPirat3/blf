
use std::io::Cursor;

/// Unpack the 1-byte packed LZMA dict-size encoding used by MCC cache files.
/// Returns the dict-size as a u32 little-endian-encoded 4 bytes (for splicing
/// into the standard 5-byte LZMA1 properties prefix).
pub fn unpack_lzma_dict_size(packed: u8) -> u32 {
    let shift = (((packed >> 1) & 0x1F) as u32) + 10;
    let base_val: u32 = if (packed & 0x4) != 0 { 5 } else { 1 };
    base_val << shift
}

/// Decompress a single LZMA1 section as found in an MCC cache file.
///
/// `compressed_chunk` is the raw bytes starting at the section's file
/// offset (i.e. including the 1-byte skip + 1-byte packed header + payload).
/// `expected_decompressed_size` is read from the cache header's `sections`
/// array for that section.
pub fn decompress_mcc_lzma_section(
    compressed_chunk: &[u8],
    expected_decompressed_size: usize,
) -> Result<Vec<u8>, String> {
    if compressed_chunk.len() < 3 {
        return Err(format!("compressed section too small: {} bytes", compressed_chunk.len()));
    }
    let packed = compressed_chunk[1];
    let dict_size = unpack_lzma_dict_size(packed);
    let dict_size_bytes = dict_size.to_le_bytes();

    let mut lzma1_input = Vec::with_capacity(5 + 8 + compressed_chunk.len() - 2);
    lzma1_input.push(0x5D);
    lzma1_input.extend_from_slice(&dict_size_bytes);

    lzma1_input.extend_from_slice(&(expected_decompressed_size as u64).to_le_bytes());

    lzma1_input.extend_from_slice(&compressed_chunk[2..]);

    let mut decompressed = Vec::with_capacity(expected_decompressed_size);
    let mut input_cursor = Cursor::new(lzma1_input);
    lzma_rs::lzma_decompress(&mut input_cursor, &mut decompressed)
        .map_err(|e| format!("lzma_decompress failed: {:?}", e))?;
    Ok(decompressed)
}

/// Section-table entry parsed from the MCC cache file header.
#[derive(Debug, Clone)]
pub struct MccCacheSection {
    pub index: usize,
    pub virtual_address: u32,
    pub decompressed_size: u32,
    pub compressed_offset: u32,
    pub compressed_size: u32,
    pub compression_type: u8,
}

/// Decompress an entire MCC cache file. Header is preserved verbatim; sections
/// are decompressed and placed at their virtual addresses (aligned to `align`,
/// which is the engine's SegmentAlignment, typically 0x1000).
///
/// The decompressed file is suitable for parsing with the normal cache header
/// + tag-data section layout used by uncompressed MCC maps. Returns the
/// decompressed bytes and the array of new offset masks that should overwrite
/// `offset masks` in the header.
pub fn decompress_mcc_cache_file(
    file_bytes: &[u8],
    header_size: usize,
    sections: &[MccCacheSection], // ordered 0,1,2,3 (NOT load order)
    align: u32,
) -> Result<(Vec<u8>, [u32; 4]), String> {
    if file_bytes.len() < header_size {
        return Err("file shorter than header_size".into());
    }
    let mut out = Vec::with_capacity(file_bytes.len() * 3);
    out.extend_from_slice(&file_bytes[..header_size]);

    let order = [0, 1, 3, 2];
    let mut new_masks = [0u32; 4];

    for &ind in &order {
        let sec = sections.get(ind).ok_or_else(|| format!("missing section {}", ind))?;

        let pad_to = align_up(out.len() as u32, align);
        out.resize(pad_to as usize, 0);
        new_masks[ind] = pad_to.wrapping_sub(sec.virtual_address);

        let start = sec.compressed_offset as usize;
        let end = start.checked_add(sec.compressed_size as usize)
            .ok_or("compressed_offset+size overflow")?;
        if end > file_bytes.len() {
            return Err(format!("section {} compressed range {}..{} > file len {}", ind, start, end, file_bytes.len()));
        }
        let compressed_chunk = &file_bytes[start..end];

        match sec.compression_type {
            0 => {
                if compressed_chunk.len() != sec.decompressed_size as usize {
                    return Err(format!(
                        "section {} type=0 size mismatch: compressed_size={} decompressed_size={}",
                        ind, compressed_chunk.len(), sec.decompressed_size
                    ));
                }
                out.extend_from_slice(compressed_chunk);
            }
            3 => {
                let decompressed = decompress_mcc_lzma_section(compressed_chunk, sec.decompressed_size as usize)?;
                if decompressed.len() != sec.decompressed_size as usize {
                    return Err(format!(
                        "section {} LZMA decompressed size mismatch: got {} expected {}",
                        ind, decompressed.len(), sec.decompressed_size
                    ));
                }
                out.extend_from_slice(&decompressed);
            }
            other => return Err(format!("unknown MCC compression type {} for section {}", other, ind)),
        }
    }

    Ok((out, new_masks))
}

fn align_up(value: u32, alignment: u32) -> u32 {
    (value + alignment - 1) & !(alignment - 1)
}
