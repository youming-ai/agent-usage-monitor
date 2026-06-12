/// Pricing entry: model pattern + USD per 1M tokens
struct PricingEntry {
    pattern: &'static str,
    input: f64,
    output: f64,
    cache_read: f64,
    cache_creation: f64,
}

const ANTHROPIC_PRICING: &[PricingEntry] = &[
    PricingEntry { pattern: "claude-opus-4",   input: 15.0, output: 75.0, cache_read: 1.50, cache_creation: 15.00 },
    PricingEntry { pattern: "claude-sonnet-4",  input:  3.0, output: 15.0, cache_read: 0.30, cache_creation:  3.00 },
    PricingEntry { pattern: "claude-haiku-4",   input:  1.0, output:  5.0, cache_read: 0.10, cache_creation:  1.00 },
    PricingEntry { pattern: "claude-opus-3",    input: 15.0, output: 75.0, cache_read: 1.50, cache_creation: 15.00 },
    PricingEntry { pattern: "claude-sonnet-3",  input:  3.0, output: 15.0, cache_read: 0.30, cache_creation:  3.00 },
    PricingEntry { pattern: "claude-haiku-3",   input: 0.25, output: 1.25, cache_read: 0.03, cache_creation:  0.25 },
];

const KIMI_PRICING: &[PricingEntry] = &[
    PricingEntry { pattern: "mimo-v2.5-pro", input: 0.60, output: 3.00, cache_read: 0.06, cache_creation: 0.60 },
    PricingEntry { pattern: "mimo-v2-pro",   input: 0.60, output: 3.00, cache_read: 0.06, cache_creation: 0.60 },
    PricingEntry { pattern: "mimo-v2",       input: 0.60, output: 3.00, cache_read: 0.06, cache_creation: 0.60 },
    PricingEntry { pattern: "kimi-k2",       input: 0.60, output: 3.00, cache_read: 0.06, cache_creation: 0.60 },
];

const CURSOR_PRICING: &[PricingEntry] = &[
    PricingEntry { pattern: "cursor-auto",     input: 1.25, output: 6.00, cache_read: 0.25,  cache_creation: 1.25 },
    PricingEntry { pattern: "composer-2.5",    input: 1.25, output: 6.00, cache_read: 0.25,  cache_creation: 1.25 },
    PricingEntry { pattern: "composer-1",      input: 1.25, output: 6.00, cache_read: 0.25,  cache_creation: 1.25 },
    PricingEntry { pattern: "claude-4.6-sonnet", input: 3.0, output: 15.0, cache_read: 0.30, cache_creation: 3.0  },
    PricingEntry { pattern: "claude-opus-4",   input: 15.0, output: 75.0, cache_read: 1.50,  cache_creation: 15.0 },
    PricingEntry { pattern: "claude-sonnet-4", input: 3.0,  output: 15.0, cache_read: 0.30,  cache_creation: 3.0  },
    PricingEntry { pattern: "gpt-5.5",          input: 1.25, output: 5.00, cache_read: 0.125, cache_creation: 1.25 },
    PricingEntry { pattern: "gpt-5.3-codex",    input: 2.50, output: 10.0, cache_read: 0.25,  cache_creation: 2.50 },
    PricingEntry { pattern: "gemini-3",         input: 1.25, output: 5.00, cache_read: 0.125, cache_creation: 1.25 },
    PricingEntry { pattern: "grok-build",       input: 0.60, output: 3.00, cache_read: 0.06,  cache_creation: 0.60 },
];

const GITHUB_PRICING: &[PricingEntry] = &[
    PricingEntry { pattern: "gpt-4.1",      input: 2.00, output: 8.00, cache_read: 0.20, cache_creation: 2.00 },
    PricingEntry { pattern: "gpt-4.1-mini",  input: 0.40, output: 1.60, cache_read: 0.04, cache_creation: 0.40 },
    PricingEntry { pattern: "claude-sonnet-4", input: 3.0, output: 15.0, cache_read: 0.30, cache_creation: 3.0 },
    PricingEntry { pattern: "claude-opus-4", input: 15.0, output: 75.0, cache_read: 1.50, cache_creation: 15.0 },
    PricingEntry { pattern: "gemini-3",      input: 1.25, output: 5.00, cache_read: 0.125, cache_creation: 1.25 },
];

const GOOGLE_PRICING: &[PricingEntry] = &[
    PricingEntry { pattern: "gemini-3.5-flash", input: 0.15, output: 0.60, cache_read: 0.015, cache_creation: 0.15 },
    PricingEntry { pattern: "gemini-3.1-pro",   input: 1.25, output: 5.00, cache_read: 0.125, cache_creation: 1.25 },
    PricingEntry { pattern: "gemini-3",         input: 1.25, output: 5.00, cache_read: 0.125, cache_creation: 1.25 },
    PricingEntry { pattern: "claude-sonnet-4",  input: 3.0,  output: 15.0, cache_read: 0.30,  cache_creation: 3.0  },
    PricingEntry { pattern: "claude-opus-4",    input: 15.0, output: 75.0, cache_read: 1.50,  cache_creation: 15.0 },
    PricingEntry { pattern: "gpt-4.1",          input: 2.00, output: 8.00, cache_read: 0.20,  cache_creation: 2.00 },
];

const OPENAI_PRICING: &[PricingEntry] = &[
    PricingEntry { pattern: "gpt-5.5",        input: 1.25, output: 5.00, cache_read: 0.125, cache_creation: 1.25 },
    PricingEntry { pattern: "gpt-5.4",        input: 0.15, output: 0.60, cache_read: 0.015, cache_creation: 0.15 },
    PricingEntry { pattern: "gpt-5.3-codex",  input: 2.50, output: 10.0, cache_read: 0.25,  cache_creation: 2.50 },
    PricingEntry { pattern: "gpt-5.4-mini",   input: 0.15, output: 0.60, cache_read: 0.015, cache_creation: 0.15 },
    PricingEntry { pattern: "gpt-4.1",        input: 2.00, output: 8.00, cache_read: 0.20,  cache_creation: 2.00 },
    PricingEntry { pattern: "gpt-4.1-mini",   input: 0.40, output: 1.60, cache_read: 0.04,  cache_creation: 0.40 },
    PricingEntry { pattern: "kimi-k2",        input: 0.60, output: 3.00, cache_read: 0.06,  cache_creation: 0.60 },
];

fn find_price<'a>(model: &str, table: &'a [PricingEntry]) -> Option<&'a PricingEntry> {
    // Pick the most specific match: a model like "gpt-4.1-mini" contains both
    // "gpt-4.1" and "gpt-4.1-mini", and the longer (more specific) pattern wins.
    // To avoid false positives (e.g. "my-gpt-4.1" matching "gpt-4.1"), require
    // the pattern to appear at a word boundary: preceded by a separator (-, /, :)
    // or at the start of the model string.
    table
        .iter()
        .filter(|e| {
            model.contains(e.pattern) && {
                let idx = model.find(e.pattern).unwrap_or(0);
                idx == 0 || model.as_bytes().get(idx - 1).is_some_and(|b| matches!(b, b'-' | b'/' | b':'))
            }
        })
        .max_by_key(|e| e.pattern.len())
}

/// Calculate cost in USD for a single request.
pub fn calculate_cost(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
) -> f64 {
    let entry = find_price(model, ANTHROPIC_PRICING)
        .or_else(|| find_price(model, KIMI_PRICING))
        .or_else(|| find_price(model, OPENAI_PRICING))
        .or_else(|| find_price(model, CURSOR_PRICING))
        .or_else(|| find_price(model, GITHUB_PRICING))
        .or_else(|| find_price(model, GOOGLE_PRICING));

    let Some(e) = entry else {
        return 0.0;
    };

    let input_cost = (input_tokens as f64 / 1_000_000.0) * e.input;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * e.output;
    let cache_read_cost = (cache_read_tokens as f64 / 1_000_000.0) * e.cache_read;
    let cache_creation_cost = (cache_creation_tokens as f64 / 1_000_000.0) * e.cache_creation;

    input_cost + output_cost + cache_read_cost + cache_creation_cost
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mini_models_are_not_matched_by_their_base_pattern() {
        // gpt-4.1-mini must use mini pricing (0.40 + 1.60), not gpt-4.1 (2.00 + 8.00).
        let mini = calculate_cost("gpt-4.1-mini", 1_000_000, 1_000_000, 0, 0);
        assert!(
            (mini - 2.00).abs() < 1e-9,
            "gpt-4.1-mini priced as {mini}, expected 2.00"
        );

        let base = calculate_cost("gpt-4.1", 1_000_000, 1_000_000, 0, 0);
        assert!((base - 10.00).abs() < 1e-9, "gpt-4.1 priced as {base}, expected 10.00");
    }

    #[test]
    fn unknown_model_is_free() {
        assert_eq!(calculate_cost("totally-unknown", 1_000_000, 1_000_000, 0, 0), 0.0);
    }

    #[test]
    fn kimi_models_use_kimi_pricing() {
        let cost = calculate_cost("xiaomi-token-plan-cn/mimo-v2.5-pro", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 3.60).abs() < 1e-9, "mimo priced as {cost}, expected 3.60");
    }
}
