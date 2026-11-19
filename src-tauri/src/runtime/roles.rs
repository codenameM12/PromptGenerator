use crate::commands::models::ModelInfo;

fn normalize(value: &str) -> String {
    value.to_ascii_lowercase()
}

fn levenshtein(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];

    for (i, left_char) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, right_char) in right.iter().enumerate() {
            let cost = usize::from(left_char != right_char);
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[right.len()]
}

/// Suggested model ids ordered by edit distance to the requested id.
pub fn closest_models(requested: &str, available: &[String], limit: usize) -> Vec<String> {
    let target = normalize(requested);
    let mut scored: Vec<(usize, String)> = available
        .iter()
        .map(|model| (levenshtein(&target, &normalize(model)), model.clone()))
        .collect();

    scored.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, model)| model)
        .collect()
}

fn describer_score(model: &ModelInfo) -> i64 {
    let vision = match model.vision {
        Some(true) => 10_000,
        None => 1_000,
        Some(false) => 0,
    };
    let loaded = if model.loaded { 100 } else { 0 };
    vision + loaded + i64::from(model.context_length.unwrap_or(0).min(1000))
}

/// Orders models for the describer role. Known vision-capable models come
/// first, unknown capability next, and text-only models last.
pub fn rank_describer(models: &[ModelInfo]) -> Vec<String> {
    let mut ranked = models.iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        describer_score(right)
            .cmp(&describer_score(left))
            .then_with(|| left.id.cmp(&right.id))
    });
    ranked.into_iter().map(|model| model.id.clone()).collect()
}

/// Orders models for the composer role by reported context, then loaded state.
pub fn rank_composer(models: &[ModelInfo]) -> Vec<String> {
    let mut ranked = models.iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        let score = |model: &ModelInfo| {
            i64::from(model.context_length.unwrap_or(0).min(1_000_000))
                + if model.loaded { 1_000_000 } else { 0 }
        };
        score(right)
            .cmp(&score(left))
            .then_with(|| left.id.cmp(&right.id))
    });
    ranked.into_iter().map(|model| model.id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::{closest_models, rank_composer, rank_describer};
    use crate::commands::models::ModelInfo;

    fn model(id: &str, vision: Option<bool>, loaded: bool, context: Option<u32>) -> ModelInfo {
        ModelInfo {
            id: id.into(),
            kind: "llm".into(),
            loaded,
            context_length: context,
            path: None,
            vision,
            reasoning: None,
            max_output: None,
            quantization: None,
        }
    }

    #[test]
    fn ranks_the_exact_model_first() {
        let available = vec![
            "lfm2-vl-1.6b".to_string(),
            "lfm3-vl-2b".to_string(),
            "gemma-4-12b".to_string(),
        ];
        let closest = closest_models("lfm3-vl-2b", &available, 3);
        assert_eq!(closest[0], "lfm3-vl-2b");
    }

    #[test]
    fn suggests_a_near_miss() {
        let available = vec!["gemma-4-12b".to_string(), "lfm3-vl-2b".to_string()];
        let closest = closest_models("lfm3-vl-2", &available, 1);
        assert_eq!(closest, vec!["lfm3-vl-2b"]);
    }

    #[test]
    fn ranks_known_vision_models_first_for_describing() {
        let models = vec![
            model("text-only", Some(false), true, Some(32768)),
            model("unknown", None, false, Some(8192)),
            model("vision", Some(true), false, Some(4096)),
        ];
        assert_eq!(
            rank_describer(&models),
            vec!["vision", "unknown", "text-only"]
        );
    }

    #[test]
    fn falls_back_to_name_order_without_capabilities() {
        let models = vec![
            model("b-model", None, false, None),
            model("a-model", None, false, None),
        ];
        assert_eq!(rank_describer(&models), vec!["a-model", "b-model"]);
    }

    #[test]
    fn ranks_composers_by_context_and_loaded_state() {
        let models = vec![
            model("small", Some(false), false, Some(8192)),
            model("large", Some(true), false, Some(32768)),
            model("loaded", Some(false), true, Some(4096)),
        ];
        assert_eq!(rank_composer(&models), vec!["loaded", "large", "small"]);
    }
}
