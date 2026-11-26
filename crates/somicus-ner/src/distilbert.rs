//! A `DistilBERT` token classifier (`DistilBertForTokenClassification`): six encoder layers over word and position
//! embeddings, then a linear layer that gives each word piece a score per label. Inference only, on the CPU.
//!
//! The input is packed: each [`Row`] holds several sequences one after another, each with its own positions from 0, and
//! an attention mask lets each sequence attend only to itself. That is the same computation as one padded sequence per
//! text, without the padding, and with a few large attention products instead of many tiny ones.

use candle_core::{DType, Device, Module as _, Tensor};
use candle_nn::{Embedding, LayerNorm, VarBuilder};

use crate::error::NerError;

/// The shape of `dslim/distilbert-NER` (its `config.json`, which is the `distilbert-base-cased` configuration).
pub(crate) const VOCABULARY: usize = 28_996;
/// Positions the model has embeddings for.
pub(crate) const MAX_POSITIONS: usize = 512;
const HIDDEN: usize = 768;
const HEADS: usize = 12;
const HEAD_SIZE: usize = HIDDEN / HEADS;
const FEED_FORWARD: usize = 3_072;
const LAYERS: usize = 6;
/// The labels the classifier scores, in the order of its rows.
pub(crate) const LABELS: usize = 9;
const LAYER_NORM_EPSILON: f64 = 1e-12;
/// Added to the attention score between word pieces of different sequences, so softmax gives it no weight.
const MASKED: f32 = -1e9;
/// `1/√64`, by which attention scores are scaled; a power of two, so folding it into the query weights is exact.
const SCORE_SCALE: f64 = 0.125;
/// The segment of the padding at the end of a row: it attends only to itself, so no row of the mask is all masked.
const PADDING_SEGMENT: u32 = u32::MAX;

fn build<T>(result: candle_core::Result<T>) -> Result<T, NerError> {
    result.map_err(|error| NerError::Model(error.to_string()))
}

/// One row of a model input: sequences (`[CLS] … [SEP]` each) one after another.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    /// The word-piece ids.
    pub ids: Vec<u32>,
    /// Each piece's position inside its own sequence, from 0.
    pub positions: Vec<u32>,
    /// Which sequence of the row each piece belongs to.
    pub segments: Vec<u32>,
}

/// A fully connected layer whose weight is stored transposed (`[in, out]`, contiguous) at load. Apple's Accelerate
/// multiplies a few rows by a transposed weight more than ten times slower than by a contiguous one, and every word
/// piece of a screen passes through these layers.
struct Linear {
    weight: Tensor,
    bias: Tensor,
}

impl Linear {
    fn new(input: usize, output: usize, weights: &VarBuilder<'_>) -> candle_core::Result<Self> {
        Self::from_parts(
            &weights.get((output, input), "weight")?,
            weights.get(output, "bias")?,
        )
    }

    /// From a weight in the file's layout (`[out, in]`) and a bias.
    fn from_parts(weight: &Tensor, bias: Tensor) -> candle_core::Result<Self> {
        Ok(Self {
            weight: weight.t()?.contiguous()?,
            bias,
        })
    }

    /// `input` (`[rows, length, in]`, contiguous) times the weight, plus the bias.
    fn forward(&self, input: &Tensor) -> candle_core::Result<Tensor> {
        let (rows, length, width) = input.dims3()?;
        input
            .reshape((rows * length, width))?
            .matmul(&self.weight)?
            .broadcast_add(&self.bias)?
            .reshape((rows, length, ()))
    }
}

struct Layer {
    /// The query, key and value projections as one, the query's part already scaled by [`SCORE_SCALE`].
    attention_input: Linear,
    output: Linear,
    attention_norm: LayerNorm,
    expand: Linear,
    contract: Linear,
    output_norm: LayerNorm,
}

impl Layer {
    fn new(weights: &VarBuilder<'_>) -> candle_core::Result<Self> {
        let attention = weights.pp("attention");
        let feed_forward = weights.pp("ffn");
        let part = |name: &str| -> candle_core::Result<(Tensor, Tensor)> {
            let projection = attention.pp(name);
            Ok((
                projection.get((HIDDEN, HIDDEN), "weight")?,
                projection.get(HIDDEN, "bias")?,
            ))
        };
        let (query_weight, query_bias) = part("q_lin")?;
        let (key_weight, key_bias) = part("k_lin")?;
        let (value_weight, value_bias) = part("v_lin")?;
        let attention_input = Linear::from_parts(
            &Tensor::cat(
                &[&(query_weight * SCORE_SCALE)?, &key_weight, &value_weight],
                0,
            )?,
            Tensor::cat(&[&(query_bias * SCORE_SCALE)?, &key_bias, &value_bias], 0)?,
        )?;
        Ok(Self {
            attention_input,
            output: Linear::new(HIDDEN, HIDDEN, &attention.pp("out_lin"))?,
            attention_norm: candle_nn::layer_norm(
                HIDDEN,
                LAYER_NORM_EPSILON,
                weights.pp("sa_layer_norm"),
            )?,
            expand: Linear::new(HIDDEN, FEED_FORWARD, &feed_forward.pp("lin1"))?,
            contract: Linear::new(FEED_FORWARD, HIDDEN, &feed_forward.pp("lin2"))?,
            output_norm: candle_nn::layer_norm(
                HIDDEN,
                LAYER_NORM_EPSILON,
                weights.pp("output_layer_norm"),
            )?,
        })
    }

    /// One encoder layer over `hidden` (`[rows, length, 768]`), with `mask_bias` (`[rows, heads, length, length]`,
    /// contiguous) added to the attention scores.
    fn forward(&self, hidden: &Tensor, mask_bias: &Tensor) -> candle_core::Result<Tensor> {
        let (rows, length, _) = hidden.dims3()?;
        let projected = self
            .attention_input
            .forward(hidden)?
            .reshape((rows, length, 3, HEADS, HEAD_SIZE))?
            .permute((2, 0, 3, 1, 4))?
            .contiguous()?;
        let (query, key, value) = (projected.get(0)?, projected.get(1)?, projected.get(2)?);
        let scores = (query.matmul(&key.t()?)? + mask_bias)?;
        let weights = candle_nn::ops::softmax_last_dim(&scores)?;
        let context = weights
            .matmul(&value)?
            .transpose(1, 2)?
            .reshape((rows, length, HIDDEN))?;
        let attended = self
            .attention_norm
            .forward(&(self.output.forward(&context)? + hidden)?)?;
        let expanded = crate::gelu::exact_gelu(&self.expand.forward(&attended)?)?;
        self.output_norm
            .forward(&(self.contract.forward(&expanded)? + attended)?)
    }
}

/// The token classifier with its weights in memory, as 32-bit floats.
pub(crate) struct DistilBert {
    words: Embedding,
    positions: Embedding,
    embedding_norm: LayerNorm,
    layers: Vec<Layer>,
    classifier: Linear,
    device: Device,
}

impl DistilBert {
    /// Builds the model from safetensors `bytes`, widening every tensor to 32-bit floats. Each tensor must have the
    /// name and shape of `dslim/distilbert-NER`.
    ///
    /// # Errors
    /// Returns [`NerError::Model`] when a tensor is missing, has another shape or the bytes are not safetensors.
    pub(crate) fn from_safetensors(bytes: &[u8]) -> Result<Self, NerError> {
        let device = Device::Cpu;
        let weights = build(VarBuilder::from_slice_safetensors(
            bytes,
            DType::F32,
            &device,
        ))?;
        let encoder = weights.pp("distilbert");
        let embeddings = encoder.pp("embeddings");
        let layers = encoder.pp("transformer").pp("layer");
        Ok(Self {
            words: build(candle_nn::embedding(
                VOCABULARY,
                HIDDEN,
                embeddings.pp("word_embeddings"),
            ))?,
            positions: build(candle_nn::embedding(
                MAX_POSITIONS,
                HIDDEN,
                embeddings.pp("position_embeddings"),
            ))?,
            embedding_norm: build(candle_nn::layer_norm(
                HIDDEN,
                LAYER_NORM_EPSILON,
                embeddings.pp("LayerNorm"),
            ))?,
            layers: (0..LAYERS)
                .map(|index| build(Layer::new(&layers.pp(index.to_string()))))
                .collect::<Result<_, _>>()?,
            classifier: build(Linear::new(HIDDEN, LABELS, &weights.pp("classifier")))?,
            device,
        })
    }

    /// The label scores (`[LABELS]` logits) of every word piece of every row, padding excluded.
    ///
    /// # Errors
    /// Returns [`NerError::Model`] when a row is malformed (its parts of different lengths, a position past
    /// [`MAX_POSITIONS`]) or the computation fails.
    pub(crate) fn logits(&self, rows: &[Row]) -> Result<Vec<Vec<Vec<f32>>>, NerError> {
        build(self.logits_unchecked(rows))
    }

    fn logits_unchecked(&self, rows: &[Row]) -> candle_core::Result<Vec<Vec<Vec<f32>>>> {
        let length = rows.iter().map(|row| row.ids.len()).max().unwrap_or(0);
        let malformed = rows.iter().any(|row| {
            row.positions.len() != row.ids.len()
                || row.segments.len() != row.ids.len()
                || row
                    .positions
                    .iter()
                    .any(|position| *position as usize >= MAX_POSITIONS)
        });
        if rows.is_empty() || length == 0 || length > MAX_POSITIONS || malformed {
            return Err(candle_core::Error::Msg(
                "a model input with an impossible shape".to_owned(),
            ));
        }
        let padded = |values: &dyn Fn(&Row) -> &[u32], fill: u32| -> Vec<u32> {
            rows.iter()
                .flat_map(|row| {
                    let real = values(row);
                    real.iter()
                        .copied()
                        .chain(std::iter::repeat_n(fill, length - real.len()))
                })
                .collect()
        };
        let ids = padded(&|row| &row.ids, 0);
        let positions = padded(&|row| &row.positions, 0);
        let segments = padded(&|row| &row.segments, PADDING_SEGMENT);
        let mut mask = Vec::with_capacity(rows.len() * length * length);
        for row in segments.chunks_exact(length) {
            for from in row {
                mask.extend(row.iter().map(|to| if to == from { 0.0 } else { MASKED }));
            }
        }
        let shape = (rows.len(), length);
        // Expanded to every head once per pass: a broadcast addition in every layer costs more than the attention.
        let mask_bias = Tensor::from_vec(mask, (rows.len(), 1, length, length), &self.device)?
            .broadcast_as((rows.len(), HEADS, length, length))?
            .contiguous()?;
        let embedded = (self
            .words
            .forward(&Tensor::from_vec(ids, shape, &self.device)?)?
            + self
                .positions
                .forward(&Tensor::from_vec(positions, shape, &self.device)?)?)?;
        let mut hidden = self.embedding_norm.forward(&embedded)?;
        for layer in &self.layers {
            hidden = layer.forward(&hidden, &mask_bias)?;
        }
        let mut logits = self.classifier.forward(&hidden)?.to_vec3::<f32>()?;
        for (row, scores) in rows.iter().zip(&mut logits) {
            scores.truncate(row.ids.len());
        }
        Ok(logits)
    }
}
