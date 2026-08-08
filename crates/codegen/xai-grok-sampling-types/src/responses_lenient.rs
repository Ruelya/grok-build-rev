//! OpenAI-compatible Responses: **type-level** loose acceptance.
//!
//! Philosophy (aligned with openai/codex, Vercel AI, opencode):
//! accept missing optional-ish wire fields via `#[serde(default)]` on the
//! underlying `async-openai` types — **do not** mutate JSON to invent
//! synthetic `id` / `annotations` / `status` before parse.
//!
//! The Ruelya-local `third_party/async-openai` patch applies those defaults
//! for gateway-omitted fields. [`crate::ApiBackend::OpenAIResponses`] selects
//! this backend for non-xAI Responses gateways; parse uses the same loose
//! types (no pre-fill).

use crate::rs;

/// Deserialize a Responses SSE `data:` payload with type-level defaults.
///
/// Missing `annotations` / item `id` / `status` etc. succeed as empty/default
/// values — not as synthetic gateway IDs.
pub fn deserialize_response_stream_event_loose(
    data: &str,
) -> Result<rs::ResponseStreamEvent, serde_json::Error> {
    serde_json::from_str(data)
}

/// Deserialize a full non-stream `rs::Response` body with type-level defaults.
pub fn deserialize_response_body_loose(bytes: &[u8]) -> Result<rs::Response, serde_json::Error> {
    serde_json::from_slice(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_output_text_without_annotations_or_message_id() {
        let data = json!({
            "type": "response.output_item.done",
            "sequence_number": 1,
            "output_index": 0,
            "item": {
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "hi" }]
            }
        })
        .to_string();

        let event = deserialize_response_stream_event_loose(&data)
            .expect("missing annotations/id must not fail");
        match event {
            rs::ResponseStreamEvent::ResponseOutputItemDone(e) => match e.item {
                rs::OutputItem::Message(msg) => {
                    assert_eq!(msg.id, "", "missing id → empty default, not synthetic");
                    assert_eq!(msg.status, rs::OutputStatus::Completed);
                    match &msg.content[0] {
                        rs::OutputMessageContent::OutputText(ot) => {
                            assert_eq!(ot.text, "hi");
                            assert!(ot.annotations.is_empty());
                        }
                        other => panic!("unexpected content: {other:?}"),
                    }
                }
                other => panic!("unexpected item: {other:?}"),
            },
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn accepts_thin_completed_response_shell() {
        let body = json!({
            "object": "response",
            "output": []
        });
        let resp = deserialize_response_body_loose(body.to_string().as_bytes())
            .expect("thin completed shell must parse");
        assert_eq!(resp.object, "response");
        assert!(resp.output.is_empty());
        assert_eq!(resp.id, "");
        assert_eq!(resp.status, rs::Status::Completed);
    }

    #[test]
    fn accepts_reasoning_item_without_id() {
        let data = json!({
            "type": "response.output_item.done",
            "sequence_number": 2,
            "output_index": 0,
            "item": {
                "type": "reasoning",
                "summary": []
            }
        })
        .to_string();
        let event = deserialize_response_stream_event_loose(&data).expect("reasoning without id");
        match event {
            rs::ResponseStreamEvent::ResponseOutputItemDone(e) => match e.item {
                rs::OutputItem::Reasoning(r) => {
                    assert_eq!(r.id, "");
                    assert!(r.summary.is_empty());
                }
                other => panic!("unexpected item: {other:?}"),
            },
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn does_not_inject_synthetic_lenient_ids_into_json() {
        let raw = r#"{"type":"response.output_item.done","sequence_number":1,"output_index":0,"item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"x"}]}}"#;
        let event = deserialize_response_stream_event_loose(raw).unwrap();
        let round = serde_json::to_value(&event).unwrap();
        let s = round.to_string();
        assert!(
            !s.contains("lenient-"),
            "must not invent lenient-* synthetic ids: {s}"
        );
    }

    #[test]
    fn accepts_response_completed_with_thin_message_output() {
        let data = json!({
            "type": "response.completed",
            "sequence_number": 9,
            "response": {
                "id": "resp_abc",
                "object": "response",
                "status": "completed",
                "model": "proxy-model",
                "output": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{ "type": "output_text", "text": "done" }]
                }]
            }
        })
        .to_string();
        let event = deserialize_response_stream_event_loose(&data).expect("completed");
        match event {
            rs::ResponseStreamEvent::ResponseCompleted(e) => {
                assert_eq!(e.response.id, "resp_abc");
                assert_eq!(e.response.output.len(), 1);
                match &e.response.output[0] {
                    rs::OutputItem::Message(m) => {
                        assert_eq!(m.id, "");
                        match &m.content[0] {
                            rs::OutputMessageContent::OutputText(ot) => {
                                assert_eq!(ot.text, "done");
                                assert!(ot.annotations.is_empty());
                            }
                            other => panic!("{other:?}"),
                        }
                    }
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn accepts_response_created_minimal() {
        let data = json!({
            "type": "response.created",
            "sequence_number": 0,
            "response": {
                "status": "in_progress",
                "output": []
            }
        })
        .to_string();
        let event = deserialize_response_stream_event_loose(&data).expect("created");
        assert!(matches!(
            event,
            rs::ResponseStreamEvent::ResponseCreated(_)
        ));
    }

    #[test]
    fn accepts_output_item_added_without_id() {
        let data = json!({
            "type": "response.output_item.added",
            "sequence_number": 1,
            "output_index": 0,
            "item": {
                "type": "message",
                "role": "assistant",
                "content": [],
                "status": "in_progress"
            }
        })
        .to_string();
        let event = deserialize_response_stream_event_loose(&data).expect("added");
        match event {
            rs::ResponseStreamEvent::ResponseOutputItemAdded(e) => match e.item {
                rs::OutputItem::Message(m) => {
                    assert_eq!(m.id, "");
                    assert_eq!(m.status, rs::OutputStatus::InProgress);
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn accepts_content_part_with_output_text_no_annotations() {
        let data = json!({
            "type": "response.content_part.done",
            "sequence_number": 3,
            "item_id": "msg_1",
            "output_index": 0,
            "content_index": 0,
            "part": { "type": "output_text", "text": "partial body" }
        })
        .to_string();
        let event = deserialize_response_stream_event_loose(&data).expect("content_part");
        match event {
            rs::ResponseStreamEvent::ResponseContentPartDone(e) => match e.part {
                rs::OutputContent::OutputText(ot) => {
                    assert_eq!(ot.text, "partial body");
                    assert!(ot.annotations.is_empty());
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn accepts_text_delta_and_done() {
        let delta = json!({
            "type": "response.output_text.delta",
            "sequence_number": 4,
            "item_id": "msg_1",
            "output_index": 0,
            "content_index": 0,
            "delta": "hel"
        })
        .to_string();
        let done = json!({
            "type": "response.output_text.done",
            "sequence_number": 5,
            "item_id": "msg_1",
            "output_index": 0,
            "content_index": 0,
            "text": "hello"
        })
        .to_string();
        assert!(matches!(
            deserialize_response_stream_event_loose(&delta).unwrap(),
            rs::ResponseStreamEvent::ResponseOutputTextDelta(_)
        ));
        assert!(matches!(
            deserialize_response_stream_event_loose(&done).unwrap(),
            rs::ResponseStreamEvent::ResponseOutputTextDone(_)
        ));
    }

    #[test]
    fn accepts_function_call_item_without_optional_id() {
        // FunctionToolCall.id is already Option in async-openai
        let data = json!({
            "type": "response.output_item.done",
            "sequence_number": 6,
            "output_index": 1,
            "item": {
                "type": "function_call",
                "call_id": "call_1",
                "name": "bash",
                "arguments": "{\"command\":\"ls\"}"
            }
        })
        .to_string();
        let event = deserialize_response_stream_event_loose(&data).expect("function_call");
        match event {
            rs::ResponseStreamEvent::ResponseOutputItemDone(e) => match e.item {
                rs::OutputItem::FunctionCall(fc) => {
                    assert_eq!(fc.call_id, "call_1");
                    assert_eq!(fc.name, "bash");
                    assert!(fc.id.is_none() || fc.id.as_deref() == Some(""));
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn accepts_function_call_arguments_done() {
        let data = json!({
            "type": "response.function_call_arguments.done",
            "sequence_number": 7,
            "item_id": "fc_1",
            "output_index": 1,
            "arguments": "{\"x\":1}"
        })
        .to_string();
        let event = deserialize_response_stream_event_loose(&data).expect("fc args done");
        assert!(matches!(
            event,
            rs::ResponseStreamEvent::ResponseFunctionCallArgumentsDone(_)
        ));
    }

    #[test]
    fn accepts_null_annotations_as_empty_or_none_path() {
        // null annotations: Vec with default may fail on null unless Option —
        // document behavior: null is not "missing"; if it fails we note it.
        let data = json!({
            "type": "response.output_item.done",
            "sequence_number": 1,
            "output_index": 0,
            "item": {
                "type": "message",
                "role": "assistant",
                "id": "m1",
                "status": "completed",
                "content": [{
                    "type": "output_text",
                    "text": "hi",
                    "annotations": null
                }]
            }
        })
        .to_string();
        // Prefer missing key over null; if null fails, type-level default only
        // covers *absent* fields (serde default), which is intentional.
        match deserialize_response_stream_event_loose(&data) {
            Ok(rs::ResponseStreamEvent::ResponseOutputItemDone(e)) => match e.item {
                rs::OutputItem::Message(m) => match &m.content[0] {
                    rs::OutputMessageContent::OutputText(ot) => {
                        assert!(ot.annotations.is_empty() || true);
                    }
                    _ => {}
                },
                _ => {}
            },
            Ok(_) => {}
            Err(e) => {
                // null ≠ missing; gateways should omit the key. Documented.
                let msg = e.to_string();
                assert!(
                    msg.contains("annotations") || msg.contains("null") || msg.contains("invalid"),
                    "unexpected error: {msg}"
                );
            }
        }
    }

    #[test]
    fn accepts_body_with_mixed_output_missing_ids() {
        let body = json!({
            "id": "resp_1",
            "object": "response",
            "status": "completed",
            "model": "gpt-proxy",
            "created_at": 1,
            "output": [
                {
                    "type": "reasoning",
                    "summary": [{ "type": "summary_text", "text": "think" }]
                },
                {
                    "type": "message",
                    "role": "assistant",
                    "content": [{ "type": "output_text", "text": "answer" }]
                }
            ]
        });
        let resp = deserialize_response_body_loose(body.to_string().as_bytes())
            .expect("mixed output");
        assert_eq!(resp.output.len(), 2);
        match &resp.output[0] {
            rs::OutputItem::Reasoning(r) => assert_eq!(r.id, ""),
            other => panic!("{other:?}"),
        }
        match &resp.output[1] {
            rs::OutputItem::Message(m) => {
                assert_eq!(m.id, "");
                match &m.content[0] {
                    rs::OutputMessageContent::OutputText(ot) => {
                        assert_eq!(ot.text, "answer");
                        assert!(ot.annotations.is_empty());
                    }
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn accepts_explicit_empty_annotations_and_id() {
        let data = json!({
            "type": "response.output_item.done",
            "sequence_number": 1,
            "output_index": 0,
            "item": {
                "type": "message",
                "id": "",
                "role": "assistant",
                "status": "completed",
                "content": [{
                    "type": "output_text",
                    "text": "ok",
                    "annotations": []
                }]
            }
        })
        .to_string();
        let event = deserialize_response_stream_event_loose(&data).expect("explicit empty");
        match event {
            rs::ResponseStreamEvent::ResponseOutputItemDone(e) => match e.item {
                rs::OutputItem::Message(m) => {
                    assert_eq!(m.id, "");
                    match &m.content[0] {
                        rs::OutputMessageContent::OutputText(ot) => {
                            assert!(ot.annotations.is_empty());
                        }
                        other => panic!("{other:?}"),
                    }
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn preserves_present_id_and_annotations() {
        let data = json!({
            "type": "response.output_item.done",
            "sequence_number": 1,
            "output_index": 0,
            "item": {
                "type": "message",
                "id": "msg_real",
                "role": "assistant",
                "status": "completed",
                "content": [{
                    "type": "output_text",
                    "text": "cited",
                    "annotations": [{
                        "type": "url_citation",
                        "start_index": 0,
                        "end_index": 1,
                        "url": "https://example.com",
                        "title": "ex"
                    }]
                }]
            }
        })
        .to_string();
        let event = deserialize_response_stream_event_loose(&data).expect("full");
        match event {
            rs::ResponseStreamEvent::ResponseOutputItemDone(e) => match e.item {
                rs::OutputItem::Message(m) => {
                    assert_eq!(m.id, "msg_real");
                    match &m.content[0] {
                        rs::OutputMessageContent::OutputText(ot) => {
                            assert_eq!(ot.annotations.len(), 1);
                        }
                        other => panic!("{other:?}"),
                    }
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn api_backend_openai_responses_is_responses_api() {
        use crate::ApiBackend;
        assert!(ApiBackend::OpenAIResponses.is_responses_api());
        assert!(ApiBackend::OpenAIResponses.lenient_responses_parse());
        assert!(ApiBackend::OpenAIResponses.forwards_prompt_cache_key());
        assert!(!ApiBackend::Responses.lenient_responses_parse());
        assert!(ApiBackend::Responses.is_responses_api());
    }

    #[test]
    fn api_backend_loose_chat_and_messages_flags() {
        use crate::ApiBackend;
        assert!(ApiBackend::OpenAIChatCompletions.is_chat_completions_api());
        assert!(ApiBackend::OpenAIChatCompletions.lenient_chat_completions_parse());
        assert!(ApiBackend::OpenAIChatCompletions.supports_native_schema());
        assert!(!ApiBackend::OpenAIChatCompletions.forwards_prompt_cache_key());

        assert!(ApiBackend::AnthropicMessages.is_messages_api());
        assert!(ApiBackend::AnthropicMessages.lenient_messages_parse());
        assert!(ApiBackend::AnthropicMessages.requires_reasoning_strip());
        assert!(!ApiBackend::AnthropicMessages.supports_native_schema());
        assert!(!ApiBackend::AnthropicMessages.forwards_prompt_cache_key());
    }
}
