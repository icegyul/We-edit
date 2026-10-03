//! Whisper's text decoder with a key/value cache for self-attention: each new token costs one
//! position instead of running the whole sequence again, and the encoder's keys and values are
//! computed once per 30-second window, not once per try.

use candle_core::{Device, Module, Result, Tensor};
use candle_nn::{
    Embedding, LayerNorm, Linear, VarBuilder, embedding, layer_norm, linear, linear_no_bias,
};
use candle_transformers::models::whisper::Config;

struct Attention {
    q: Linear,
    k: Linear,
    v: Linear,
    out: Linear,
    heads: usize,
    cache: Option<(Tensor, Tensor)>,
}

impl Attention {
    fn load(n: usize, heads: usize, vb: VarBuilder) -> Result<Attention> {
        Ok(Attention {
            q: linear(n, n, vb.pp("q_proj"))?,
            k: linear_no_bias(n, n, vb.pp("k_proj"))?,
            v: linear(n, n, vb.pp("v_proj"))?,
            out: linear(n, n, vb.pp("out_proj"))?,
            heads,
            cache: None,
        })
    }

    fn split(&self, x: &Tensor) -> Result<Tensor> {
        let (b, t, n) = x.dims3()?;
        x.reshape((b, t, self.heads, n / self.heads))?
            .transpose(1, 2)
    }

    fn attend(&self, q: &Tensor, k: &Tensor, v: &Tensor, mask: Option<&Tensor>) -> Result<Tensor> {
        let (_, _, n) = q.dims3()?;
        let scale = ((n / self.heads) as f64).powf(-0.25);
        let q = (self.split(q)? * scale)?;
        let k = (self.split(k)?.transpose(2, 3)? * scale)?;
        let v = self.split(v)?.contiguous()?;
        let mut w = q.matmul(&k)?;
        if let Some(m) = mask {
            w = w.broadcast_add(m)?;
        }
        let w = candle_nn::ops::softmax_last_dim(&w)?;
        w.matmul(&v)?.transpose(1, 2)?.flatten_from(2)
    }

    /// Attends over the cached positions and `x`, then keeps `x`'s keys and values.
    fn self_attend(&mut self, x: &Tensor, mask: Option<&Tensor>) -> Result<Tensor> {
        let q = self.q.forward(x)?;
        let mut k = self.k.forward(x)?;
        let mut v = self.v.forward(x)?;
        if let Some((ck, cv)) = &self.cache {
            k = Tensor::cat(&[ck, &k], 1)?;
            v = Tensor::cat(&[cv, &v], 1)?;
        }
        let out = self.out.forward(&self.attend(&q, &k, &v, mask)?)?;
        self.cache = Some((k, v));
        Ok(out)
    }

    /// Attends over the audio features; their keys and values are computed on first use.
    fn cross_attend(&mut self, x: &Tensor, audio: &Tensor) -> Result<Tensor> {
        let (k, v) = match &self.cache {
            Some(c) => c.clone(),
            None => {
                let c = (self.k.forward(audio)?, self.v.forward(audio)?);
                self.cache = Some(c.clone());
                c
            }
        };
        let q = self.q.forward(x)?;
        self.out.forward(&self.attend(&q, &k, &v, None)?)
    }
}

struct Block {
    attn: Attention,
    attn_ln: LayerNorm,
    cross: Attention,
    cross_ln: LayerNorm,
    fc1: Linear,
    fc2: Linear,
    mlp_ln: LayerNorm,
}

impl Block {
    fn load(n: usize, heads: usize, vb: VarBuilder) -> Result<Block> {
        Ok(Block {
            attn: Attention::load(n, heads, vb.pp("self_attn"))?,
            attn_ln: layer_norm(n, 1e-5, vb.pp("self_attn_layer_norm"))?,
            cross: Attention::load(n, heads, vb.pp("encoder_attn"))?,
            cross_ln: layer_norm(n, 1e-5, vb.pp("encoder_attn_layer_norm"))?,
            fc1: linear(n, n * 4, vb.pp("fc1"))?,
            fc2: linear(n * 4, n, vb.pp("fc2"))?,
            mlp_ln: layer_norm(n, 1e-5, vb.pp("final_layer_norm"))?,
        })
    }

    fn forward(&mut self, x: &Tensor, audio: &Tensor, mask: Option<&Tensor>) -> Result<Tensor> {
        let x = (x + self.attn.self_attend(&self.attn_ln.forward(x)?, mask)?)?;
        let x = (&x
            + self
                .cross
                .cross_attend(&self.cross_ln.forward(&x)?, audio)?)?;
        let mlp = self
            .fc2
            .forward(&self.fc1.forward(&self.mlp_ln.forward(&x)?)?.gelu()?)?;
        x + mlp
    }
}

pub struct Decoder {
    embed: Embedding,
    positions: Tensor,
    blocks: Vec<Block>,
    ln: LayerNorm,
    device: Device,
    /// Positions already in the self-attention caches.
    len: usize,
}

impl Decoder {
    pub fn load(vb: VarBuilder, cfg: &Config) -> Result<Decoder> {
        let n = cfg.d_model;
        let blocks = (0..cfg.decoder_layers)
            .map(|i| Block::load(n, cfg.decoder_attention_heads, vb.pp(format!("layers.{i}"))))
            .collect::<Result<Vec<_>>>()?;
        Ok(Decoder {
            embed: embedding(cfg.vocab_size, n, vb.pp("embed_tokens"))?,
            positions: vb.get((cfg.max_target_positions, n), "embed_positions.weight")?,
            blocks,
            ln: layer_norm(n, 1e-5, vb.pp("layer_norm"))?,
            device: vb.device().clone(),
            len: 0,
        })
    }

    /// Starts a new text for the same audio window.
    pub fn restart(&mut self) {
        for b in &mut self.blocks {
            b.attn.cache = None;
        }
        self.len = 0;
    }

    /// Starts a new audio window.
    pub fn reset(&mut self) {
        self.restart();
        for b in &mut self.blocks {
            b.cross.cache = None;
        }
    }

    /// Feeds tokens that follow the ones already fed; returns their hidden states (1, n, d).
    pub fn forward(&mut self, tokens: &[u32], audio: &Tensor) -> Result<Tensor> {
        let n = tokens.len();
        let x = Tensor::new(tokens, &self.device)?.unsqueeze(0)?;
        let mut x = self
            .embed
            .forward(&x)?
            .broadcast_add(&self.positions.narrow(0, self.len, n)?)?;
        // several new tokens may only see the ones before them
        let mask = if n > 1 {
            let total = self.len + n;
            let m: Vec<f32> = (0..n)
                .flat_map(|i| {
                    let limit = self.len + i;
                    (0..total).map(move |j| if j > limit { f32::NEG_INFINITY } else { 0.0 })
                })
                .collect();
            Some(Tensor::from_vec(m, (n, total), &self.device)?)
        } else {
            None
        };
        for b in &mut self.blocks {
            x = b.forward(&x, audio, mask.as_ref())?;
        }
        self.len += n;
        self.ln.forward(&x)
    }

    /// Token logits of hidden states (1, n, d) -> (1, n, vocab).
    pub fn logits(&self, x: &Tensor) -> Result<Tensor> {
        let w = self.embed.embeddings().broadcast_left(x.dim(0)?)?;
        x.matmul(&w.t()?)
    }

    pub fn capacity(&self) -> usize {
        self.positions.dim(0).unwrap_or(448)
    }
}
