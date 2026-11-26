//! The reproducible conversion of the upstream 32-bit weights to 16-bit ones (ADR-0042 decision 2): every tensor keeps
//! its name and shape, each value is rounded to the nearest 16-bit float (ties to even), the tensors are written in
//! name order with the upstream metadata, so the same input always gives the same bytes.

use std::collections::HashMap;

use half::f16;
use safetensors::tensor::TensorView;
use safetensors::{Dtype, SafeTensors};

use crate::error::NerError;
use crate::load::verify;
use crate::pins::UPSTREAM_MODEL;

fn conversion(error: impl std::fmt::Display) -> NerError {
    NerError::Conversion(error.to_string())
}

/// Converts the bytes of the upstream `model.safetensors` to 16-bit floats.
///
/// # Errors
/// Returns [`NerError::Verification`] unless `upstream` is the pinned upstream file, and [`NerError::Conversion`] when
/// it holds a tensor that is not 32-bit floats.
pub fn to_half_precision(upstream: &[u8]) -> Result<Vec<u8>, NerError> {
    verify(upstream, &UPSTREAM_MODEL)?;
    convert_unverified(upstream)
}

/// The conversion itself, for any safetensors file of 32-bit floats.
pub(crate) fn convert_unverified(upstream: &[u8]) -> Result<Vec<u8>, NerError> {
    let (_, metadata) = SafeTensors::read_metadata(upstream).map_err(conversion)?;
    let tensors = SafeTensors::deserialize(upstream).map_err(conversion)?;
    let mut converted: Vec<(String, Vec<usize>, Vec<u8>)> = Vec::new();
    for (name, view) in tensors.tensors() {
        if view.dtype() != Dtype::F32 {
            return Err(NerError::Conversion(format!(
                "{name} is {:?}, not 32-bit floats",
                view.dtype()
            )));
        }
        let halves: Vec<u8> = view
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|bytes| f16::from_f32(f32::from_le_bytes(*bytes)).to_le_bytes())
            .collect();
        converted.push((name, view.shape().to_vec(), halves));
    }
    converted.sort_by(|left, right| left.0.cmp(&right.0));
    let views = converted
        .iter()
        .map(|(name, shape, data)| {
            TensorView::new(Dtype::F16, shape.clone(), data)
                .map(|view| (name.clone(), view))
                .map_err(conversion)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let info: Option<HashMap<String, String>> = metadata.metadata().clone();
    safetensors::serialize(views, info).map_err(conversion)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn float_file(tensors: &[(&str, Vec<f32>)]) -> Vec<u8> {
        let data: Vec<(String, Vec<usize>, Vec<u8>)> = tensors
            .iter()
            .map(|(name, values)| {
                (
                    (*name).to_owned(),
                    vec![values.len()],
                    values
                        .iter()
                        .flat_map(|value| value.to_le_bytes())
                        .collect(),
                )
            })
            .collect();
        let views: Vec<(String, TensorView<'_>)> = data
            .iter()
            .map(|(name, shape, bytes)| {
                (
                    name.clone(),
                    TensorView::new(Dtype::F32, shape.clone(), bytes)
                        .expect("a valid synthetic fixture"),
                )
            })
            .collect();
        let metadata = HashMap::from([("format".to_owned(), "pt".to_owned())]);
        safetensors::serialize(views, Some(metadata)).expect("a valid synthetic fixture")
    }

    #[test]
    fn the_conversion_keeps_names_shapes_and_rounds_each_value() -> Result<(), NerError> {
        let upstream = float_file(&[
            ("b.weight", vec![1.0, -2.5, 65_520.0]),
            ("a.bias", vec![0.1, 1e-8]),
        ]);
        let converted = convert_unverified(&upstream)?;
        let tensors = SafeTensors::deserialize(&converted).map_err(conversion)?;
        assert_eq!(tensors.names().len(), 2);
        let bias = tensors.tensor("a.bias").map_err(conversion)?;
        assert_eq!(bias.dtype(), Dtype::F16);
        assert_eq!(bias.shape(), &[2]);
        let values: Vec<f32> = bias
            .data()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|bytes| f16::from_le_bytes(*bytes).to_f32())
            .collect();
        assert!((values[0] - 0.1).abs() < 1e-4);
        assert!(values[1].abs() < 1e-7);
        let weight = tensors.tensor("b.weight").map_err(conversion)?;
        let overflow = f16::from_le_bytes([weight.data()[4], weight.data()[5]]);
        // 65,520 rounds up past the largest half-precision value.
        assert!(overflow.is_infinite());
        Ok(())
    }

    #[test]
    fn the_same_input_always_gives_the_same_bytes() -> Result<(), NerError> {
        let upstream = float_file(&[("z", vec![3.0]), ("a", vec![1.0, 2.0])]);
        assert_eq!(
            convert_unverified(&upstream)?,
            convert_unverified(&upstream)?
        );
        Ok(())
    }

    #[test]
    fn only_the_pinned_upstream_file_is_converted() {
        let upstream = float_file(&[("a", vec![1.0])]);
        assert!(matches!(
            to_half_precision(&upstream),
            Err(NerError::Verification)
        ));
    }
}
