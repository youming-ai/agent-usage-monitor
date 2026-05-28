/// Pricing entry: model pattern + USD per 1M tokens
struct PricingEntry {
    pattern: &'static str,
    input: f64,
    output: f64,
    cache_read: f64,
}

const ANTHROPIC_PRICING: &[PricingEntry] = &[
    PricingEntry { pattern: "claude-opus-4", input: 15.0, output: 75.0, cache_read: 1.50 },
    PricingEntry { pattern: "claude-sonnet-4", input: 3.0, output: 15.0, cache_read: 0.30 },
    PricingEntry { pattern: "claude-haiku-4", input: 1.0, output: 5.0, cache_read: 0.10 },
    PricingEntry { pattern: "claude-opus-3", input: 15.0, output: 75.0, cache_read: 1.50 },
    PricingEntry { pattern: "claude-sonnet-3", input: 3.0, output: 15.0, cache_read: 0.30 },
    PricingEntry { pattern: "claude-haiku-3", input: 0.25, output: 1.25, cache_read: 0.03 },
];

const OPENAI_PRICING: &[PricingEntry] = &[
    PricingEntry { pattern: "gpt-5.5", input: 1.25, output: 5.00, cache_read: 0.125 },
    PricingEntry { pattern: "gpt-5.4", input: 0.15, output: 0.60, cache_read: 0.015 },
    PricingEntry { pattern: "gpt-5.3-codex", input: 2.50, output: 10.0, cache_read: 0.25 },
    PricingEntry { pattern: "gpt-5.4-mini", input: 0.15, output: 0.60, cache_read: 0.015 },
    PricingEntry { pattern: "gpt-4.1", input: 2.00, output: 8.00, cache_read: 0.20 },
    PricingEntry { pattern: "gpt-4.1-mini", input: 0.40, output: 1.60, cache_read: 0.04 },
    PricingEntry { pattern: "kimi-k2", input: 0.60, output: 3.00, cache_read: 0.06 },
];

fn find_price<'a>(model: &str, table: &'a [PricingEntry]) -> Option<&'a PricingEntry> {
    table.iter().find(|e| model.contains(e.pattern))
}

/// Calculate cost in USD for a single request.
pub fn calculate_cost(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    _cache_creation_tokens: u64,
) -> f64 {
    let entry = find_price(model, ANTHROPIC_PRICING)
        .or_else(|| find_price(model, OPENAI_PRICING));

    let Some(e) = entry else {
        return 0.0;
    };

    let input_cost = (input_tokens as f64 / 1_000_000.0) * e.input;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * e.output;
    let cache_cost = (cache_read_tokens as f64 / 1_000_000.0) * e.cache_read;

    input_cost + output_cost + cache_cost
}
