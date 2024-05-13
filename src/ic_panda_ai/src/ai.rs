use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::qwen2::{Config as ConfigBase, Model};
use tokenizers::Tokenizer;

pub struct TextGeneration {
    model: Model,
    device: Device,
    tokenizer: Tokenizer,
    logits_processor: LogitsProcessor,
    repeat_penalty: f32,
    repeat_last_n: usize,
}

pub struct Args {
    /// The temperature used to generate samples.
    pub temperature: Option<f64>,
    /// Nucleus sampling probability cutoff.
    pub top_p: Option<f64>,
    /// The seed to use when generating random samples, default to 299792458.
    pub seed: u64,
    /// The length of the sample to generate (in tokens), default to 10000.
    pub sample_len: usize,
    /// Penalty to be applied for repeating tokens, 1. means no penalty, default to 1.1.
    pub repeat_penalty: f32,
    /// The context size to consider for the repeat penalty, default to 64.
    pub repeat_last_n: usize,
}

impl TextGeneration {
    #[allow(clippy::too_many_arguments)]
    fn new(
        model: Model,
        tokenizer: Tokenizer,
        seed: u64,
        temp: Option<f64>,
        top_p: Option<f64>,
        repeat_penalty: f32,
        repeat_last_n: usize,
        device: Device,
    ) -> Self {
        let logits_processor = LogitsProcessor::new(seed, temp, top_p);
        Self {
            model,
            tokenizer,
            logits_processor,
            repeat_penalty,
            repeat_last_n,
            device,
        }
    }

    pub fn load(
        args: &Args,
        config_json: &[u8],
        tokenizer_json: &[u8],
        model_safetensors: Vec<u8>,
    ) -> anyhow::Result<Self> {
        let tokenizer = Tokenizer::from_bytes(tokenizer_json).map_err(anyhow::Error::msg)?;

        let config: ConfigBase = serde_json::from_slice(config_json)?;
        let device = Device::Cpu;
        let dtype = if device.is_cuda() {
            DType::BF16
        } else {
            DType::F32
        };

        let vb = VarBuilder::from_buffered_safetensors(model_safetensors, dtype, &device)?;
        let model = Model::new(&config, vb)?;

        Ok(TextGeneration::new(
            model,
            tokenizer,
            args.seed,
            args.temperature,
            args.top_p,
            args.repeat_penalty,
            args.repeat_last_n,
            device,
        ))
    }

    pub fn run<W>(&mut self, prompt: &str, sample_len: usize, w: &mut W) -> anyhow::Result<u32>
    where
        W: std::io::Write,
    {
        let mut token_stream = TokenOutputStream::new();
        let mut tokens = self
            .tokenizer
            .encode(prompt, true)
            .map_err(anyhow::Error::msg)?
            .get_ids()
            .to_vec();
        // for &t in tokens.iter() {
        //     if let Some(t) = token_stream.next_token(&self.tokenizer, t)? {
        //         w.write_all(t.as_bytes())?;
        //     }
        // }
        // w.flush()?;

        let mut generated_tokens = 0u32;
        let eos_token = match self.tokenizer.get_vocab(true).get("<|endoftext|>").copied() {
            Some(token) => token,
            None => anyhow::bail!("cannot find the <|endoftext|> token"),
        };

        for index in 0..sample_len {
            let context_size = if index > 0 { 1 } else { tokens.len() };
            let start_pos = tokens.len().saturating_sub(context_size);
            let ctxt = &tokens[start_pos..];
            let input = Tensor::new(ctxt, &self.device)?.unsqueeze(0)?;
            let logits = self.model.forward(&input, start_pos)?;
            let logits = logits.squeeze(0)?.squeeze(0)?.to_dtype(DType::F32)?;
            let logits = if self.repeat_penalty == 1. {
                logits
            } else {
                let start_at = tokens.len().saturating_sub(self.repeat_last_n);
                candle_transformers::utils::apply_repeat_penalty(
                    &logits,
                    self.repeat_penalty,
                    &tokens[start_at..],
                )?
            };

            let next_token = self.logits_processor.sample(&logits)?;
            tokens.push(next_token);
            generated_tokens += 1;
            if next_token == eos_token {
                break;
            }
            if let Some(t) = token_stream.next_token(&self.tokenizer, next_token)? {
                w.write_all(t.as_bytes())?;
                w.flush()?;
            }
        }

        if let Some(t) = token_stream.decode_rest(&self.tokenizer)? {
            w.write_all(t.as_bytes())?;
        }
        w.flush()?;
        Ok(generated_tokens)
    }
}

pub struct TokenOutputStream {
    tokens: Vec<u32>,
    prev_index: usize,
    current_index: usize,
}

impl TokenOutputStream {
    pub fn new() -> Self {
        Self {
            tokens: Vec::new(),
            prev_index: 0,
            current_index: 0,
        }
    }

    pub fn next_token(
        &mut self,
        tokenizer: &tokenizers::Tokenizer,
        token: u32,
    ) -> anyhow::Result<Option<String>> {
        let prev_text = if self.tokens.is_empty() {
            String::new()
        } else {
            let tokens = &self.tokens[self.prev_index..self.current_index];
            tokenizer.decode(tokens, true).map_err(anyhow::Error::msg)?
        };
        self.tokens.push(token);
        let text = tokenizer
            .decode(&self.tokens[self.prev_index..], true)
            .map_err(anyhow::Error::msg)?;
        if text.len() > prev_text.len() && text.chars().last().unwrap().is_alphanumeric() {
            let text = text.split_at(prev_text.len());
            self.prev_index = self.current_index;
            self.current_index = self.tokens.len();
            Ok(Some(text.1.to_string()))
        } else {
            Ok(None)
        }
    }

    pub fn decode_rest(&self, tokenizer: &tokenizers::Tokenizer) -> anyhow::Result<Option<String>> {
        let prev_text = if self.tokens.is_empty() {
            String::new()
        } else {
            let tokens = &self.tokens[self.prev_index..self.current_index];
            tokenizer.decode(tokens, true).map_err(anyhow::Error::msg)?
        };
        let text = tokenizer
            .decode(&self.tokens[self.prev_index..], true)
            .map_err(anyhow::Error::msg)?;
        if text.len() > prev_text.len() {
            let text = text.split_at(prev_text.len());
            Ok(Some(text.1.to_string()))
        } else {
            Ok(None)
        }
    }

    pub fn decode_all(&self, tokenizer: &tokenizers::Tokenizer) -> anyhow::Result<String> {
        tokenizer
            .decode(&self.tokens, true)
            .map_err(anyhow::Error::msg)
    }
}
