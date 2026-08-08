# fork fork notes (async-openai)

Based on our-forks/async-openai @ 95b52eb (0.33.1).

## Loose Responses deserialize (type-level accept)

Gateways / OpenAI-compatible proxies often omit fields that stock OpenAPI
codegen marks required (`output_text.annotations`, message `id`/`status`, …).

Industry approach (openai/codex, Vercel AI, opencode): **accept via types** —
`#[serde(default)]` / `Option` — not “fill synthetic JSON then strict parse”.

This tree adds `#[serde(default)]` (and `Default` on status enums) for those
fields so `ApiBackend::OpenAIResponses` can parse incomplete wire payloads
without inventing `lenient-*` ids.

Do **not** reintroduce pre-parse JSON mutation for this purpose.
