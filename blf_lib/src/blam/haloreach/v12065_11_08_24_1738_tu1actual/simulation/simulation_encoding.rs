use blf_lib::blam::common::math::integer_math::int32_point3d;
use blf_lib::blam::common::math::real_math::{point_in_rectangle3d, real_point3d, real_rectangle3d};
use blf_lib::blam::common::simulation::simulation_encoding::adjust_axis_encoding_bit_count_to_match_error_goals;
use blf_lib::blam::haloreach::v12065_11_08_24_1738_tu1actual::math::real_math::{dequantize_real_point3d_per_axis, quantize_real_point3d_per_axis};
use blf_lib::io::bitstream::{c_bitstream_reader, c_bitstream_writer};
use blf_lib_derivable::result::{BLFLibError, BLFLibResult};

pub fn simulation_read_position(
    bitstream: &mut c_bitstream_reader,
    position: &mut real_point3d,
    axis_encoding_size_in_bits: usize,
    exact_midpoints: bool,
    exact_endpoints: bool,
    world_bounds: &real_rectangle3d
) -> BLFLibResult {
    // In blam, world_bounds should be optional. If not provided, we do some stuff with encoing globals.
    // We haven't needed to support that branch yet, it may be a runtime only branch.

    if bitstream.read_bool("point-in-initial-bounds")? {
        let mut per_axis_bit_counts = int32_point3d::default();
        adjust_axis_encoding_bit_count_to_match_error_goals(
            axis_encoding_size_in_bits,
            world_bounds,
            26,
            &mut per_axis_bit_counts,
        );

        let mut quantized_point = int32_point3d::default();
        bitstream.read_point3d_efficient(&mut quantized_point, per_axis_bit_counts)?;

        dequantize_real_point3d_per_axis(
            &quantized_point,
            world_bounds,
            &per_axis_bit_counts,
            position,
            exact_midpoints,
            exact_endpoints,
        );

        Ok(())
    }
    else {
        // This branch requires runtime game BSP data, we can't perform it.
        Err(BLFLibError::from("Tried to read a position outside of world bounds! Fallback behaviour is only supported in-engine."))
    }
}

pub fn simulation_write_position(
    bitstream: &mut c_bitstream_writer,
    position: &real_point3d,
    bits: usize,
    world_bounds: &real_rectangle3d,
) -> BLFLibResult {
    let mut per_axis_bit_counts = int32_point3d { x: bits as i32, y: bits as i32, z: bits as i32 };
    let mut quantized_point = int32_point3d::default();

    let in_bounds = point_in_rectangle3d(position, world_bounds);
    bitstream.write_bool(in_bounds)?;

    if !in_bounds {
        // This branch requires runtime game BSP data, we can't perform it.
        return Err(BLFLibError::from(
            format!("Tried to write a position {position:?} outside of world bounds {world_bounds:?}! Fallback behaviour is only supported in-engine.")
        ))
    }

    adjust_axis_encoding_bit_count_to_match_error_goals(bits, world_bounds, 26, &mut per_axis_bit_counts);

    quantize_real_point3d_per_axis(
        position,
        world_bounds,
        &per_axis_bit_counts,
        &mut quantized_point,
    );

    bitstream.write_point3d_efficient(
        &quantized_point,
        &per_axis_bit_counts,
    )?;

    Ok(())
}

/// Round-trip-fidelity variant of `simulation_read_position`. Same wire
/// consumption as the regular reader, but ALSO returns the raw
/// `int32_point3d` quantized point that was read. Caller stores this and
/// passes it to `simulation_write_position_with_raw` on encode to bypass
/// the non-injective dequantize/requantize round-trip — fixes the ~1,858
/// Reach v31 corpus files (0.44%) that decode + encode cleanly but produce
/// slightly different position bits due to quantization drift.
pub fn simulation_read_position_capture(
    bitstream: &mut c_bitstream_reader,
    position: &mut real_point3d,
    axis_encoding_size_in_bits: usize,
    exact_midpoints: bool,
    exact_endpoints: bool,
    world_bounds: &real_rectangle3d,
) -> BLFLibResult<int32_point3d> {
    if bitstream.read_bool("point-in-initial-bounds")? {
        let mut per_axis_bit_counts = int32_point3d::default();
        adjust_axis_encoding_bit_count_to_match_error_goals(
            axis_encoding_size_in_bits,
            world_bounds,
            26,
            &mut per_axis_bit_counts,
        );

        let mut quantized_point = int32_point3d::default();
        bitstream.read_point3d_efficient(&mut quantized_point, per_axis_bit_counts)?;

        dequantize_real_point3d_per_axis(
            &quantized_point,
            world_bounds,
            &per_axis_bit_counts,
            position,
            exact_midpoints,
            exact_endpoints,
        );

        Ok(quantized_point)
    } else {
        Err(BLFLibError::from("Tried to read a position outside of world bounds! Fallback behaviour is only supported in-engine."))
    }
}

/// Round-trip-fidelity variant of `simulation_write_position` that takes a
/// captured raw `int32_point3d` (from `simulation_read_position_capture`)
/// and writes it verbatim instead of re-quantizing from `position`.
pub fn simulation_write_position_with_raw(
    bitstream: &mut c_bitstream_writer,
    raw_quantized: &int32_point3d,
    bits: usize,
    world_bounds: &real_rectangle3d,
) -> BLFLibResult {
    let mut per_axis_bit_counts = int32_point3d::default();
    adjust_axis_encoding_bit_count_to_match_error_goals(bits, world_bounds, 26, &mut per_axis_bit_counts);
    bitstream.write_bool(true)?; // in-bounds
    bitstream.write_point3d_efficient(raw_quantized, &per_axis_bit_counts)?;
    Ok(())
}

