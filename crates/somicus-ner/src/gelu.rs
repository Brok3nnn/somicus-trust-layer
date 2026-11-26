//! The exact GELU activation, `x · Phi(x)` with the error function, computed with a rational approximation of `erf` that
//! the compiler vectorizes. candle's own `gelu_erf` calls a scalar `erf` per element, which took about 17 ms per
//! feed-forward layer of a dense screen, more than all the matrix products together.

use candle_core::{CpuStorage, CustomOp1, Layout, Shape, Tensor};

/// `erf(x)` for 32-bit floats: the odd/even rational approximation used by Eigen and XLA for single precision, on
/// `[-4, 4]` (outside it `erf` is ±1 in single precision). Its error against an exact `erf` is about 1e-7.
#[allow(
    clippy::excessive_precision,
    clippy::unreadable_literal,
    reason = "published coefficients, kept digit for digit"
)]
fn erf(x: f32) -> f32 {
    let x = x.clamp(-4.0, 4.0);
    let x2 = x * x;
    let mut p = x2.mul_add(-2.726_142_25e-10, 2.770_681_42e-08);
    p = x2.mul_add(p, -2.101_024_02e-06);
    p = x2.mul_add(p, -5.692_506_39e-05);
    p = x2.mul_add(p, -7.349_906_30e-04);
    p = x2.mul_add(p, -2.954_599_80e-03);
    p = x2.mul_add(p, -1.609_603_33e-02);
    let mut q = x2.mul_add(-1.456_607_18e-05, -2.133_740_55e-04);
    q = x2.mul_add(q, -1.682_826_97e-03);
    q = x2.mul_add(q, -7.373_329_17e-03);
    q = x2.mul_add(q, -1.426_473_90e-02);
    x * p / q
}

/// `GELU(x) = x/2 · (1 + erf(x/√2))`.
fn gelu(x: f32) -> f32 {
    0.5 * x * (1.0 + erf(x * std::f32::consts::FRAC_1_SQRT_2))
}

/// The most threads one activation is spread over.
const MAX_THREADS: usize = 8;
/// The fewest values a thread computes: below that, starting it costs more than it saves.
const MIN_CHUNK: usize = 1 << 15;

/// The exact GELU over a contiguous 32-bit float tensor, spread over the CPU's cores.
struct Gelu;

impl CustomOp1 for Gelu {
    fn name(&self) -> &'static str {
        "somicus_gelu"
    }

    fn cpu_fwd(
        &self,
        storage: &CpuStorage,
        layout: &Layout,
    ) -> candle_core::Result<(CpuStorage, Shape)> {
        let (CpuStorage::F32(values), Some((start, end))) = (storage, layout.contiguous_offsets())
        else {
            return Err(candle_core::Error::Msg(
                "GELU needs a contiguous 32-bit float tensor".to_owned(),
            ));
        };
        let input = values.get(start..end).ok_or_else(|| {
            candle_core::Error::Msg("GELU's input lies outside its storage".to_owned())
        })?;
        let mut output = vec![0.0f32; input.len()];
        let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        let chunk = input
            .len()
            .div_ceil(threads.clamp(1, MAX_THREADS))
            .max(MIN_CHUNK);
        std::thread::scope(|scope| {
            for (source, target) in input.chunks(chunk).zip(output.chunks_mut(chunk)) {
                scope.spawn(move || {
                    for (value, result) in source.iter().zip(target) {
                        *result = gelu(*value);
                    }
                });
            }
        });
        Ok((CpuStorage::F32(output), layout.shape().clone()))
    }
}

/// The exact GELU of `tensor`, which must be a contiguous 32-bit float tensor on the CPU.
pub(crate) fn exact_gelu(tensor: &Tensor) -> candle_core::Result<Tensor> {
    tensor.apply_op1_no_bwd(&Gelu)
}

#[cfg(test)]
mod tests {
    use candle_core::{Device, Tensor};

    use super::*;

    #[test]
    fn the_approximation_matches_an_exact_erf() {
        let mut worst = 0.0f64;
        for step in -60_000..=60_000 {
            let x = f64::from(step) / 10_000.0;
            #[allow(
                clippy::cast_possible_truncation,
                reason = "the test's inputs lie in [-6, 6]"
            )]
            let approximate = f64::from(erf(x as f32));
            let exact = candle_core::cpu::erf::erf_f64(x);
            worst = worst.max((approximate - exact).abs());
        }
        assert!(worst < 1e-6, "largest error {worst}");
    }

    #[test]
    fn the_custom_op_matches_candles_exact_gelu() -> candle_core::Result<()> {
        let input = Tensor::arange(-600i64, 600, &Device::Cpu)?
            .to_dtype(candle_core::DType::F32)?
            .affine(0.01, 0.0)?
            .reshape((40, 30))?;
        let ours = exact_gelu(&input)?.flatten_all()?.to_vec1::<f32>()?;
        let candle = input.gelu_erf()?.flatten_all()?.to_vec1::<f32>()?;
        for (left, right) in ours.iter().zip(&candle) {
            assert!((left - right).abs() < 1e-6, "{left} against {right}");
        }
        Ok(())
    }

    #[test]
    fn a_tensor_that_is_not_contiguous_is_refused() -> candle_core::Result<()> {
        let input = Tensor::zeros((4, 6), candle_core::DType::F32, &Device::Cpu)?.t()?;
        assert!(exact_gelu(&input).is_err());
        Ok(())
    }
}
