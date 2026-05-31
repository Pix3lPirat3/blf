use blf_lib::assert_ok;
use blf_lib::blam::common::math::real_math::{global_up3d, k_pi, real_vector3d};
use blf_lib::blam::haloreach::v12065_11_08_24_1738_tu1actual::math::real_math::dequantize_unit_vector3d;
use blf_lib::io::bitstream::c_bitstream_reader;
use blf_lib::types::numbers::Float32;
use blf_lib_derivable::result::BLFLibResult;
use crate::blam::haloreach::v12065_11_08_24_1738_tu1actual::math::real_math::dequantize_real;

pub trait c_bitstream_reader_extensions<'a> {
    fn bitstream_reader(&mut self) -> &mut c_bitstream_reader<'a>;

    fn read_quantized_real(&mut self, min_value: f32, max_value: f32, size_in_bits: usize, exact_midpoint: bool, exact_endpoints: bool) -> BLFLibResult<Float32> {
        let reader = self.bitstream_reader();

        assert_ok!(reader.reading());
        let value: i32 = reader.read_unnamed_integer(size_in_bits)?;
        Ok(Float32(dequantize_real(value, min_value, max_value, 1 << size_in_bits, exact_midpoint, exact_endpoints)))
    }

    /// Round-trip-fidelity variant of `read_quantized_real`. Same wire
    /// consumption but ALSO returns the raw `size_in_bits` integer value.
    /// Caller stores the raw int and passes it to `write_integer(raw,
    /// size_in_bits)` on encode to bypass the non-injective quantize/
    /// dequantize round-trip. Used by `s_multiplayer_object_boundary`'s
    /// size/box_length/positive_height/negative_height fields to fix
    /// Reach v31 byte_diff cases where the boundary's float fields drift.
    fn read_quantized_real_capture(
        &mut self,
        min_value: f32,
        max_value: f32,
        size_in_bits: usize,
        exact_midpoint: bool,
        exact_endpoints: bool,
    ) -> BLFLibResult<(Float32, u32)> {
        let reader = self.bitstream_reader();
        assert_ok!(reader.reading());
        let value: u32 = reader.read_unnamed_integer(size_in_bits)?;
        let f = Float32(dequantize_real(value as i32, min_value, max_value, 1 << size_in_bits, exact_midpoint, exact_endpoints));
        Ok((f, value))
    }

    fn read_axes<const forward_bits: usize, const up_bits: usize>(&mut self, forward: &mut real_vector3d, up: &mut real_vector3d) -> BLFLibResult {
        let reader = self.bitstream_reader();

        if reader.read_bool("up-is-global-up3d")? {
            up.clone_from(&global_up3d);
        }
        else {
            let quantized = reader.read_integer("up-quantization", up_bits)?;
            dequantize_unit_vector3d(quantized, up, up_bits)?;
        }

        let forward_angle = reader.read_quantized_real(-k_pi, k_pi, forward_bits, false, false)?;
        c_bitstream_reader::angle_to_axes_internal(up, forward_angle, forward)?;

        Ok(())
    }

    /// Round-trip-fidelity variant of `read_axes`. Same wire consumption but
    /// ALSO returns the raw bit fields the wire used:
    ///   `(up_is_global, raw_up_quantization, raw_forward_angle)`
    /// Caller stores these and passes them to `write_axes_with_raw` on encode
    /// to bypass the non-injective quantize/dequantize round-trip — fixes the
    /// ~1,858 Reach v31 corpus files that have axes-drift byte_diff.
    fn read_axes_capture<const forward_bits: usize, const up_bits: usize>(
        &mut self,
        forward: &mut real_vector3d,
        up: &mut real_vector3d,
    ) -> BLFLibResult<(bool, u32, u32)> {
        let reader = self.bitstream_reader();
        let up_is_global = reader.read_bool("up-is-global-up3d")?;
        let raw_up_q: u32 = if up_is_global {
            up.clone_from(&global_up3d);
            0
        } else {
            let q: u32 = reader.read_integer("up-quantization", up_bits)?;
            dequantize_unit_vector3d(q as i32, up, up_bits)?;
            q
        };
        let raw_forward_angle: u32 = reader.read_unnamed_integer(forward_bits)?;
        let max = 1u64 << forward_bits;
        let forward_angle = Float32(dequantize_real(raw_forward_angle as i32, -k_pi, k_pi, max as usize, false, false));
        c_bitstream_reader::angle_to_axes_internal(up, forward_angle, forward)?;
        Ok((up_is_global, raw_up_q, raw_forward_angle))
    }
}

impl<'a> c_bitstream_reader_extensions<'a> for c_bitstream_reader<'a> {
    fn bitstream_reader(&mut self) -> &mut c_bitstream_reader<'a> {
        self
    }
}
