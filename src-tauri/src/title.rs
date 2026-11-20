//! Short library titles, KeyBERT style: candidate phrases from the prompt are
//! embedded with all-MiniLM-L6-v2 (ONNX, int8) and ranked by cosine similarity
//! to the whole prompt. Without the model files a plain heuristic is used.

use std::path::{Path, PathBuf};

use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::value::Tensor;
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

const MODEL_DIR: &str = "models/all-MiniLM-L6-v2";
const MODEL_FILE: &str = "model_quantized.onnx";
const TOKENIZER_FILE: &str = "tokenizer.json";
/// Target title length in words.
const TITLE_WORDS: usize = 5;
const MAX_CANDIDATES: usize = 48;
const MAX_TOKENS: usize = 128;

const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "of", "in", "on", "at", "to", "for", "with", "by", "from",
    "as", "is", "are", "was", "were", "be", "been", "it", "its", "this", "that", "these", "those",
    "into", "onto", "over", "under", "up", "down", "out", "off", "very", "more", "most", "some",
    "any", "each", "while", "then", "than", "so", "such", "their", "his", "her", "he", "she",
    "they", "them", "you", "your", "we", "our", "i", "my", "me", "there", "here", "which", "who",
    "whom", "what", "where", "when", "also", "both", "just", "only", "not", "no", "has", "have",
    "had", "do", "does", "did", "will", "would", "can", "could", "should", "about", "around",
    "through", "across", "between", "after", "before", "during", "above", "below", "near",
    "image", "picture", "photo", "shot", "frame", "style", "highly", "detailed",
];

pub struct TitleModel {
    session: Session,
    tokenizer: Tokenizer,
}

/// Folders searched for the model, mirroring how the bundled ffmpeg is found.
pub fn model_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        dirs.push(parent.join(MODEL_DIR));
        if let Some(grandparent) = parent.parent() {
            dirs.push(grandparent.join(MODEL_DIR));
        }
    }
    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(MODEL_DIR));
    dirs
}

impl TitleModel {
    pub fn load_from(dir: &Path) -> Result<Self, String> {
        let mut tokenizer = Tokenizer::from_file(dir.join(TOKENIZER_FILE))
            .map_err(|error| format!("tokenizer: {error}"))?;
        tokenizer.with_padding(Some(PaddingParams::default()));
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_TOKENS,
                ..Default::default()
            }))
            .map_err(|error| format!("tokenizer: {error}"))?;
        let session = Session::builder()
            .map_err(|error| error.to_string())?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|error| error.to_string())?
            .with_intra_threads(2)
            .map_err(|error| error.to_string())?
            .commit_from_file(dir.join(MODEL_FILE))
            .map_err(|error| error.to_string())?;
        Ok(Self { session, tokenizer })
    }

    /// Loads the model from the first folder that has it.
    pub fn load() -> Result<Self, String> {
        let mut last = String::from("model folder not found");
        for dir in model_dirs() {
            if dir.join(MODEL_FILE).exists() && dir.join(TOKENIZER_FILE).exists() {
                match Self::load_from(&dir) {
                    Ok(model) => return Ok(model),
                    Err(error) => last = error,
                }
            }
        }
        Err(last)
    }

    /// L2-normalised mean-pooled sentence embeddings, one per text.
    pub fn embed(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|error| error.to_string())?;
        let batch = encodings.len();
        let width = encodings.first().map_or(0, |item| item.get_ids().len());
        let ids: Vec<i64> = encodings
            .iter()
            .flat_map(|item| item.get_ids().iter().map(|id| i64::from(*id)))
            .collect();
        let mask: Vec<i64> = encodings
            .iter()
            .flat_map(|item| item.get_attention_mask().iter().map(|id| i64::from(*id)))
            .collect();
        let types = vec![0i64; batch * width];
        let make = |data: Vec<i64>| {
            Tensor::from_array(([batch, width], data)).map_err(|error| error.to_string())
        };
        let outputs = self
            .session
            .run(ort::inputs![
                "input_ids" => make(ids)?,
                "attention_mask" => make(mask.clone())?,
                "token_type_ids" => make(types)?,
            ])
            .map_err(|error| error.to_string())?;
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|error| error.to_string())?;
        let dim = *shape.last().ok_or("empty output")? as usize;
        let mut vectors = Vec::with_capacity(batch);
        for row in 0..batch {
            let mut sum = vec![0f32; dim];
            let mut count = 0f32;
            for token in 0..width {
                if mask[row * width + token] == 0 {
                    continue;
                }
                count += 1.0;
                let offset = (row * width + token) * dim;
                for (slot, value) in sum.iter_mut().zip(&data[offset..offset + dim]) {
                    *slot += value;
                }
            }
            let norm = sum
                .iter()
                .map(|value| (value / count.max(1.0)).powi(2))
                .sum::<f32>()
                .sqrt()
                .max(1e-12);
            vectors.push(
                sum.iter()
                    .map(|value| value / count.max(1.0) / norm)
                    .collect(),
            );
        }
        Ok(vectors)
    }

    /// A 4-5 word title for `text`. Falls back to the heuristic on any error.
    pub fn title(&mut self, text: &str) -> String {
        self.try_title(text)
            .unwrap_or_else(|_| fallback_title(text))
    }

    fn try_title(&mut self, text: &str) -> Result<String, String> {
        let words = words_of(text);
        let candidates = candidates(&words);
        if candidates.len() < 2 {
            return Ok(fallback_title(text));
        }
        // Separate batches: the prompt is long, the phrases are short, and a
        // shared batch would pad every phrase to the prompt's length.
        let document = self.embed(&[text.chars().take(1500).collect::<String>()])?;
        let phrases: Vec<String> = candidates.iter().map(Candidate::text).collect();
        let vectors = self.embed(&phrases)?;
        let scores: Vec<f32> = vectors
            .iter()
            .map(|vector| cosine(&document[0], vector))
            .collect();
        Ok(pick(&candidates, &scores))
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[derive(Debug, Clone)]
struct Candidate {
    words: Vec<String>,
    start: usize,
}

impl Candidate {
    fn text(&self) -> String {
        self.words.join(" ")
    }
}

/// Lower-cased words with a break marker (empty string) at punctuation.
fn words_of(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '\'' || ch == '-' {
            current.extend(ch.to_lowercase());
        } else {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            if matches!(ch, '.' | ',' | ';' | ':' | '!' | '?' | '\n' | '(' | ')' | '"') {
                words.push(String::new());
            }
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn is_content(word: &str) -> bool {
    !word.is_empty()
        && word.chars().any(char::is_alphabetic)
        && !STOP_WORDS.contains(&word)
        && word.chars().count() > 1
}

/// 1-3 word runs of content words that sit next to each other in the text.
fn candidates(words: &[String]) -> Vec<Candidate> {
    let mut found: Vec<Candidate> = Vec::new();
    for start in 0..words.len() {
        for size in 1..=3 {
            let Some(slice) = words.get(start..start + size) else {
                break;
            };
            if !slice.iter().all(|word| is_content(word)) {
                break;
            }
            if found
                .iter()
                .any(|existing| existing.words.as_slice() == slice)
            {
                continue;
            }
            found.push(Candidate {
                words: slice.to_vec(),
                start,
            });
        }
        if found.len() >= MAX_CANDIDATES {
            break;
        }
    }
    found
}

/// Best phrase, then the best non-overlapping phrase that fills the word
/// budget; shown in reading order.
fn pick(candidates: &[Candidate], scores: &[f32]) -> String {
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    order.sort_by(|a, b| scores[*b].total_cmp(&scores[*a]));
    // Prefer a 3-word phrase for the first slot when it scores near the best.
    let best = order[0];
    let first = order
        .iter()
        .copied()
        .find(|index| candidates[*index].words.len() == 3 && scores[*index] >= scores[best] - 0.05)
        .unwrap_or(best);
    let mut chosen = vec![first];
    let used = |chosen: &Vec<usize>, index: usize| {
        chosen.iter().any(|picked| {
            candidates[*picked]
                .words
                .iter()
                .any(|word| candidates[index].words.contains(word))
        })
    };
    let mut total = candidates[first].words.len();
    while total < TITLE_WORDS - 1 {
        let room = TITLE_WORDS - total;
        let Some(next) = order.iter().copied().find(|index| {
            candidates[*index].words.len() <= room && !used(&chosen, *index)
        }) else {
            break;
        };
        total += candidates[next].words.len();
        chosen.push(next);
    }
    chosen.sort_by_key(|index| candidates[*index].start);
    title_case(
        &chosen
            .iter()
            .flat_map(|index| candidates[*index].words.iter().cloned())
            .collect::<Vec<_>>(),
    )
}

fn title_case(words: &[String]) -> String {
    words
        .iter()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// First content words of the prompt; used when the model is unavailable.
pub fn fallback_title(text: &str) -> String {
    let picked: Vec<String> = words_of(text)
        .into_iter()
        .filter(|word| is_content(word))
        .take(TITLE_WORDS)
        .collect();
    if picked.is_empty() {
        text.split_whitespace()
            .take(TITLE_WORDS)
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        title_case(&picked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> Option<TitleModel> {
        TitleModel::load().ok()
    }

    #[test]
    fn fallback_takes_content_words() {
        assert_eq!(
            fallback_title("A cinematic portrait of an old fisherman at golden hour"),
            "Cinematic Portrait Old Fisherman Golden"
        );
        assert_eq!(fallback_title("the and of"), "the and of");
    }

    #[test]
    fn candidates_do_not_cross_punctuation() {
        let words = words_of("red fox, snowy field");
        let found: Vec<String> = candidates(&words).iter().map(Candidate::text).collect();
        assert!(found.contains(&"red fox".to_string()));
        assert!(found.contains(&"snowy field".to_string()));
        assert!(!found.iter().any(|text| text.contains("fox snowy")));
    }

    #[test]
    fn titles_are_four_to_five_words_and_on_topic() {
        let Some(mut model) = model() else {
            eprintln!("skipping: model files not present");
            return;
        };
        let cases = [
            (
                "A cinematic portrait of an old fisherman with weathered skin at golden hour, mending a net on a wooden pier",
                "fisherman",
            ),
            (
                "Poster for a jazz night titled JAZZ AT MIDNIGHT with a saxophonist silhouette lit by amber spotlight",
                "jazz",
            ),
            (
                "A red fox leaping through deep snow in a pine forest at dawn, mist between the trees",
                "fox",
            ),
        ];
        for (prompt, keyword) in cases {
            let title = model.title(prompt);
            eprintln!("title: {title}");
            let count = title.split_whitespace().count();
            assert!((3..=5).contains(&count), "{title:?} has {count} words");
            assert!(
                title.to_lowercase().contains(keyword),
                "{title:?} misses {keyword}"
            );
        }
    }

    #[test]
    fn warm_titles_are_fast() {
        let Some(mut model) = model() else {
            return;
        };
        let prompt = "A cinematic portrait of an old fisherman with weathered skin at golden hour, mending a net on a wooden pier, shallow depth of field, warm rim light, film grain";
        let _ = model.title(prompt);
        let mut times = Vec::new();
        for _ in 0..30 {
            let start = std::time::Instant::now();
            let _ = model.title(prompt);
            times.push(start.elapsed());
        }
        times.sort();
        let p95 = times[(times.len() * 95 / 100).min(times.len() - 1)];
        eprintln!("title p95 = {p95:?}");
        // Debug builds (unoptimised tokenizer) are not representative.
        if !cfg!(debug_assertions) {
            assert!(p95 < std::time::Duration::from_millis(50), "p95 {p95:?}");
        }
    }
}
