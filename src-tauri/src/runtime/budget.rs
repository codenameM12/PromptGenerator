use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub name: String,
    pub text: String,
    /// Higher values are trimmed first. Priority 0 is never trimmed.
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrimmedSection {
    pub name: String,
    pub chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrimReport {
    pub removed: Vec<TrimmedSection>,
    pub estimated_tokens: usize,
}

pub fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() / 4).max(1)
}

/// Removes the highest-priority-number sections until the estimate fits.
pub fn fit_sections(sections: &mut Vec<Section>, max_tokens: usize) -> TrimReport {
    let mut removed = Vec::new();

    loop {
        let total: usize = sections
            .iter()
            .map(|section| estimate_tokens(&section.text))
            .sum();
        if total <= max_tokens || sections.len() <= 1 {
            break;
        }

        let mut index = 0;
        for candidate in 1..sections.len() {
            if sections[candidate].priority >= sections[index].priority {
                index = candidate;
            }
        }

        if sections[index].priority == 0 {
            break;
        }

        let section = sections.remove(index);
        removed.push(TrimmedSection {
            name: section.name,
            chars: section.text.chars().count(),
        });
    }

    let estimated_tokens = sections
        .iter()
        .map(|section| estimate_tokens(&section.text))
        .sum();

    TrimReport {
        removed,
        estimated_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::{Section, fit_sections};

    #[test]
    fn trims_low_priority_sections_first() {
        let mut sections = vec![
            Section {
                name: "core".into(),
                text: "a".repeat(400),
                priority: 0,
            },
            Section {
                name: "style".into(),
                text: "b".repeat(400),
                priority: 2,
            },
            Section {
                name: "detail".into(),
                text: "c".repeat(400),
                priority: 1,
            },
        ];

        let report = fit_sections(&mut sections, 150);
        assert_eq!(report.removed.len(), 2);
        assert_eq!(report.removed[0].name, "style");
        assert_eq!(report.removed[1].name, "detail");
        assert_eq!(report.removed[0].chars, 400);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].name, "core");
        assert!(report.estimated_tokens <= 150);
    }

    #[test]
    fn keeps_priority_zero_even_when_over_budget() {
        let mut sections = vec![Section {
            name: "core".into(),
            text: "x".repeat(4000),
            priority: 0,
        }];
        let report = fit_sections(&mut sections, 10);
        assert!(report.removed.is_empty());
        assert_eq!(sections.len(), 1);
    }
}
