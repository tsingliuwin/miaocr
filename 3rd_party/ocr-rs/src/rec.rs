//! Text Recognition Model
//!
//! Provides text recognition functionality based on PaddleOCR recognition models

use image::DynamicImage;
use ndarray::ArrayD;
use std::path::Path;

use crate::error::{OcrError, OcrResult};
use crate::mnn::{InferenceConfig, InferenceEngine};
use crate::preprocess::{preprocess_for_rec, NormalizeParams};

/// Recognition result
#[derive(Debug, Clone)]
pub struct RecognitionResult {
    /// Recognized text
    pub text: String,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Confidence score for each character
    pub char_scores: Vec<(char, f32)>,
}

impl RecognitionResult {
    /// Create a new recognition result
    pub fn new(text: String, confidence: f32, char_scores: Vec<(char, f32)>) -> Self {
        Self {
            text,
            confidence,
            char_scores,
        }
    }

    /// Check if the result is valid (confidence above threshold)
    pub fn is_valid(&self, threshold: f32) -> bool {
        self.confidence >= threshold
    }
}

/// Recognition options
#[derive(Debug, Clone)]
pub struct RecOptions {
    /// Target height (recognition model input height)
    pub target_height: u32,
    /// Minimum confidence threshold (characters below this value will be filtered)
    pub min_score: f32,
    /// Minimum confidence threshold for punctuation
    pub punct_min_score: f32,
    /// Batch size
    pub batch_size: usize,
    /// Whether to enable batch processing
    pub enable_batch: bool,
}

impl Default for RecOptions {
    fn default() -> Self {
        Self {
            target_height: 48,
            min_score: 0.3, // Lower threshold, model output is raw logit
            punct_min_score: 0.1,
            batch_size: 8,
            enable_batch: true,
        }
    }
}

impl RecOptions {
    /// Create new recognition options
    pub fn new() -> Self {
        Self::default()
    }

    /// Set target height
    pub fn with_target_height(mut self, height: u32) -> Self {
        self.target_height = height;
        self
    }

    /// Set minimum confidence
    pub fn with_min_score(mut self, score: f32) -> Self {
        self.min_score = score;
        self
    }

    /// Set punctuation minimum confidence
    pub fn with_punct_min_score(mut self, score: f32) -> Self {
        self.punct_min_score = score;
        self
    }

    /// Set batch size
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Enable/disable batch processing
    pub fn with_batch(mut self, enable: bool) -> Self {
        self.enable_batch = enable;
        self
    }
}

/// Text recognition model
pub struct RecModel {
    engine: InferenceEngine,
    /// Character set (index to character mapping)
    charset: Vec<char>,
    options: RecOptions,
    normalize_params: NormalizeParams,
}

/// Common punctuation marks
const PUNCTUATIONS: [char; 49] = [
    ',', '.', '!', '?', ';', ':', '"', '\'', '(', ')', '[', ']', '{', '}', '-', '_', '/', '\\',
    '|', '@', '#', '$', '%', '&', '*', '+', '=', '~', '，', '。', '！', '？', '；', '：', '、',
    '「', '」', '『', '』', '（', '）', '【', '】', '《', '》', '—', '…', '·', '～',
];

impl RecModel {
    /// Create recognizer from model file and charset file
    ///
    /// # Parameters
    /// - `model_path`: Model file path (.mnn format)
    /// - `charset_path`: Charset file path (one character per line)
    /// - `config`: Optional inference config
    pub fn from_file(
        model_path: impl AsRef<Path>,
        charset_path: impl AsRef<Path>,
        config: Option<InferenceConfig>,
    ) -> OcrResult<Self> {
        let engine = InferenceEngine::from_file(model_path, config)?;
        let charset = Self::load_charset_from_file(charset_path)?;

        Ok(Self {
            engine,
            charset,
            options: RecOptions::default(),
            normalize_params: NormalizeParams::paddle_rec(),
        })
    }

    /// Create recognizer from model bytes and charset file
    pub fn from_bytes(
        model_bytes: &[u8],
        charset_path: impl AsRef<Path>,
        config: Option<InferenceConfig>,
    ) -> OcrResult<Self> {
        let engine = InferenceEngine::from_buffer(model_bytes, config)?;
        let charset = Self::load_charset_from_file(charset_path)?;

        Ok(Self {
            engine,
            charset,
            options: RecOptions::default(),
            normalize_params: NormalizeParams::paddle_rec(),
        })
    }

    /// Create recognizer from model bytes and charset bytes
    pub fn from_bytes_with_charset(
        model_bytes: &[u8],
        charset_bytes: &[u8],
        config: Option<InferenceConfig>,
    ) -> OcrResult<Self> {
        let engine = InferenceEngine::from_buffer(model_bytes, config)?;
        let charset = Self::parse_charset(charset_bytes)?;

        Ok(Self {
            engine,
            charset,
            options: RecOptions::default(),
            normalize_params: NormalizeParams::paddle_rec(),
        })
    }

    /// Load charset from file
    fn load_charset_from_file(path: impl AsRef<Path>) -> OcrResult<Vec<char>> {
        let content = std::fs::read_to_string(path)?;
        Self::parse_charset(content.as_bytes())
    }

    /// Parse charset data
    fn parse_charset(data: &[u8]) -> OcrResult<Vec<char>> {
        let content = std::str::from_utf8(data)
            .map_err(|e| OcrError::CharsetError(format!("UTF-8 decode error: {}", e)))?;

        // Charset format: one character per line
        // Add space at beginning and end as blank and padding
        let mut charset: Vec<char> = vec![' ']; // blank token at start

        for ch in content.chars() {
            if ch != '\n' && ch != '\r' {
                charset.push(ch);
            }
        }

        charset.push(' '); // padding token at end

        if charset.len() < 3 {
            return Err(OcrError::CharsetError("Charset too small".to_string()));
        }

        Ok(charset)
    }

    /// Set recognition options
    pub fn with_options(mut self, options: RecOptions) -> Self {
        self.options = options;
        self
    }

    /// Get current recognition options
    pub fn options(&self) -> &RecOptions {
        &self.options
    }

    /// Modify recognition options
    pub fn options_mut(&mut self) -> &mut RecOptions {
        &mut self.options
    }

    /// Get charset size
    pub fn charset_size(&self) -> usize {
        self.charset.len()
    }

    /// Recognize a single image
    ///
    /// # Parameters
    /// - `image`: Input image (text line image)
    ///
    /// # Returns
    /// Recognition result
    pub fn recognize(&self, image: &DynamicImage) -> OcrResult<RecognitionResult> {
        // Preprocess
        let input = preprocess_for_rec(image, self.options.target_height, &self.normalize_params)?;

        // Inference (using dynamic shape)
        let output = self.engine.run_dynamic(input.view().into_dyn())?;

        // Decode
        self.decode_output(&output)
    }

    /// Recognize a single image, return text only
    pub fn recognize_text(&self, image: &DynamicImage) -> OcrResult<String> {
        let result = self.recognize(image)?;
        Ok(result.text)
    }

    /// Batch recognize images
    ///
    /// # Parameters
    /// - `images`: List of input images
    ///
    /// # Returns
    /// List of recognition results
    pub fn recognize_batch(&self, images: &[DynamicImage]) -> OcrResult<Vec<RecognitionResult>> {
        if images.is_empty() {
            return Ok(Vec::new());
        }

        // For small number of images, process individually
        if images.len() <= 2 || !self.options.enable_batch {
            return images.iter().map(|img| self.recognize(img)).collect();
        }

        // Batch processing
        let mut results = Vec::with_capacity(images.len());

        for chunk in images.chunks(self.options.batch_size) {
            let batch_results = self.recognize_batch_internal(chunk)?;
            results.extend(batch_results);
        }

        Ok(results)
    }

    /// Batch recognize images (borrowed version, avoid cloning)
    ///
    /// # Parameters
    /// - `images`: List of input image references
    ///
    /// # Returns
    /// List of recognition results
    pub fn recognize_batch_ref(
        &self,
        images: &[&DynamicImage],
    ) -> OcrResult<Vec<RecognitionResult>> {
        if images.is_empty() {
            return Ok(Vec::new());
        }

        // For small number of images, process individually
        if images.len() <= 2 || !self.options.enable_batch {
            return images.iter().map(|img| self.recognize(img)).collect();
        }

        // Batch processing
        let mut results = Vec::with_capacity(images.len());

        for chunk in images.chunks(self.options.batch_size) {
            // Dereference and convert to Vec<DynamicImage>
            let chunk_owned: Vec<DynamicImage> = chunk.iter().map(|img| (*img).clone()).collect();
            let batch_results = self.recognize_batch_internal(&chunk_owned)?;
            results.extend(batch_results);
        }

        Ok(results)
    }

    /// Internal batch recognition
    fn recognize_batch_internal(
        &self,
        images: &[DynamicImage],
    ) -> OcrResult<Vec<RecognitionResult>> {
        if images.is_empty() {
            return Ok(Vec::new());
        }

        // If only one image, process individually
        if images.len() == 1 {
            return Ok(vec![self.recognize(&images[0])?]);
        }

        // Batch preprocessing
        let batch_input = crate::preprocess::preprocess_batch_for_rec(
            images,
            self.options.target_height,
            &self.normalize_params,
        )?;

        // Batch inference
        let batch_output = self.engine.run_dynamic(batch_input.view().into_dyn())?;

        // Decode output for each sample
        let shape = batch_output.shape();
        if shape.len() != 3 {
            return Err(OcrError::PostprocessError(format!(
                "Batch inference output shape error: {:?}",
                shape
            )));
        }

        let batch_size = shape[0];
        let mut results = Vec::with_capacity(batch_size);

        for i in 0..batch_size {
            // Extract output for single sample
            let sample_output = batch_output.slice(ndarray::s![i, .., ..]).to_owned();
            let sample_output_dyn = sample_output.into_dyn();
            let result = self.decode_output(&sample_output_dyn)?;
            results.push(result);
        }

        Ok(results)
    }

    /// Decode model output
    fn decode_output(&self, output: &ArrayD<f32>) -> OcrResult<RecognitionResult> {
        let shape = output.shape();

        // Output shape should be [batch, seq_len, num_classes] or [seq_len, num_classes]
        let (seq_len, num_classes) = if shape.len() == 3 {
            (shape[1], shape[2])
        } else if shape.len() == 2 {
            (shape[0], shape[1])
        } else {
            return Err(OcrError::PostprocessError(format!(
                "Invalid output shape: {:?}",
                shape
            )));
        };

        let output_data: Vec<f32> = output.iter().cloned().collect();

        // CTC decoding
        let mut char_scores = Vec::new();
        let mut prev_idx = 0usize;

        for t in 0..seq_len {
            // Find character with maximum probability at current time step
            let start = t * num_classes;
            let end = start + num_classes;
            let probs = &output_data[start..end];

            let (max_idx, &max_prob) = probs
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .ok_or_else(|| {
                    OcrError::PostprocessError("Empty probability slice in CTC decoding".into())
                })?;

            // CTC decoding rule: skip blank (index 0) and duplicate characters
            if max_idx != 0 && max_idx != prev_idx {
                if max_idx < self.charset.len() {
                    let ch = self.charset[max_idx];

                    // Use raw logit value as confidence (model output is already softmax probability)
                    // For large character sets, softmax scores can be very small, so use max_prob directly
                    let score = max_prob;

                    // Only filter out very low confidence characters
                    let threshold = if Self::is_punctuation(ch) {
                        self.options.punct_min_score
                    } else {
                        self.options.min_score
                    };

                    if score >= threshold {
                        char_scores.push((ch, score));
                    }
                }
            }

            prev_idx = max_idx;
        }

        // Calculate average confidence
        let confidence = if char_scores.is_empty() {
            0.0
        } else {
            char_scores.iter().map(|(_, s)| s).sum::<f32>() / char_scores.len() as f32
        };

        // Extract text
        let text: String = char_scores.iter().map(|(ch, _)| ch).collect();

        Ok(RecognitionResult::new(text, confidence, char_scores))
    }

    /// Check if character is punctuation
    fn is_punctuation(ch: char) -> bool {
        PUNCTUATIONS.contains(&ch)
    }
}

/// Low-level recognition API
impl RecModel {
    /// Raw inference interface
    ///
    /// Execute model inference directly without preprocessing and postprocessing
    ///
    /// # Parameters
    /// - `input`: Preprocessed input tensor [1, 3, H, W]
    ///
    /// # Returns
    /// Model raw output
    pub fn run_raw(&self, input: ndarray::ArrayViewD<f32>) -> OcrResult<ArrayD<f32>> {
        Ok(self.engine.run_dynamic(input)?)
    }

    /// Get model input shape
    pub fn input_shape(&self) -> &[usize] {
        self.engine.input_shape()
    }

    /// Get model output shape
    pub fn output_shape(&self) -> &[usize] {
        self.engine.output_shape()
    }

    /// Get charset
    pub fn charset(&self) -> &[char] {
        &self.charset
    }

    /// Get character by index
    pub fn get_char(&self, index: usize) -> Option<char> {
        self.charset.get(index).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rec_options_default() {
        let opts = RecOptions::default();
        assert_eq!(opts.target_height, 48);
        assert_eq!(opts.min_score, 0.3);
        assert_eq!(opts.punct_min_score, 0.1);
        assert_eq!(opts.batch_size, 8);
        assert!(opts.enable_batch);
    }

    #[test]
    fn test_rec_options_builder() {
        let opts = RecOptions::new()
            .with_target_height(32)
            .with_min_score(0.6)
            .with_punct_min_score(0.2)
            .with_batch_size(16)
            .with_batch(false);

        assert_eq!(opts.target_height, 32);
        assert_eq!(opts.min_score, 0.6);
        assert_eq!(opts.punct_min_score, 0.2);
        assert_eq!(opts.batch_size, 16);
        assert!(!opts.enable_batch);
    }

    #[test]
    fn test_recognition_result_new() {
        let char_scores = vec![
            ('H', 0.99),
            ('e', 0.94),
            ('l', 0.93),
            ('l', 0.95),
            ('o', 0.94),
        ];
        let result = RecognitionResult::new("Hello".to_string(), 0.95, char_scores.clone());

        assert_eq!(result.text, "Hello");
        assert_eq!(result.confidence, 0.95);
        assert_eq!(result.char_scores.len(), 5);
        assert_eq!(result.char_scores[0].0, 'H');
        assert_eq!(result.char_scores[0].1, 0.99);
    }

    #[test]
    fn test_recognition_result_is_valid() {
        let result = RecognitionResult::new(
            "Hello".to_string(),
            0.95,
            vec![
                ('H', 0.99),
                ('e', 0.94),
                ('l', 0.93),
                ('l', 0.95),
                ('o', 0.94),
            ],
        );

        assert!(result.is_valid(0.9));
        assert!(result.is_valid(0.95));
        assert!(!result.is_valid(0.96));
        assert!(!result.is_valid(0.99));
    }

    #[test]
    fn test_recognition_result_empty() {
        let result = RecognitionResult::new(String::new(), 0.0, vec![]);

        assert!(result.text.is_empty());
        assert_eq!(result.confidence, 0.0);
        assert!(!result.is_valid(0.1));
    }

    #[test]
    fn test_is_punctuation_common() {
        // English punctuation
        assert!(RecModel::is_punctuation(','));
        assert!(RecModel::is_punctuation('.'));
        assert!(RecModel::is_punctuation('!'));
        assert!(RecModel::is_punctuation('?'));
        assert!(RecModel::is_punctuation(';'));
        assert!(RecModel::is_punctuation(':'));
        assert!(RecModel::is_punctuation('"'));
        assert!(RecModel::is_punctuation('\''));
    }

    #[test]
    fn test_is_punctuation_chinese() {
        // Chinese punctuation
        assert!(RecModel::is_punctuation('，'));
        assert!(RecModel::is_punctuation('。'));
        assert!(RecModel::is_punctuation('！'));
        assert!(RecModel::is_punctuation('？'));
        assert!(RecModel::is_punctuation('；'));
        assert!(RecModel::is_punctuation('：'));
        assert!(RecModel::is_punctuation('、'));
        assert!(RecModel::is_punctuation('—'));
        assert!(RecModel::is_punctuation('…'));
    }

    #[test]
    fn test_is_punctuation_brackets() {
        assert!(RecModel::is_punctuation('('));
        assert!(RecModel::is_punctuation(')'));
        assert!(RecModel::is_punctuation('['));
        assert!(RecModel::is_punctuation(']'));
        assert!(RecModel::is_punctuation('{'));
        assert!(RecModel::is_punctuation('}'));
        assert!(RecModel::is_punctuation('「'));
        assert!(RecModel::is_punctuation('」'));
        assert!(RecModel::is_punctuation('《'));
        assert!(RecModel::is_punctuation('》'));
    }

    #[test]
    fn test_is_punctuation_false() {
        // Non-punctuation characters
        assert!(!RecModel::is_punctuation('A'));
        assert!(!RecModel::is_punctuation('z'));
        assert!(!RecModel::is_punctuation('0'));
        assert!(!RecModel::is_punctuation('中'));
        assert!(!RecModel::is_punctuation('文'));
        assert!(!RecModel::is_punctuation(' '));
    }
}
