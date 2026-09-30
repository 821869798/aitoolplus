use serde_json::{json, Map, Value};

/// State for tracking Anthropic SSE event sequence when streaming from OpenAI chunks.
#[derive(Debug, Default)]
pub struct AnthropicSseState {
    pub message_started: bool,
    pub current_block_index: usize,
    pub in_text_block: bool,
    pub in_tool_block: bool,
    pub current_tool_id: String,
    pub current_tool_name: String,
}

/// Convert an Anthropic `/v1/messages` request JSON into an OpenAI `/v1/chat/completions` request JSON.
pub fn anthropic_to_openai_request(anthropic_body: &Value) -> Result<Value, String> {
    let mut openai = Map::new();

    // Model
    if let Some(model) = anthropic_body.get("model").and_then(Value::as_str) {
        openai.insert("model".to_string(), Value::String(model.to_string()));
    } else {
        return Err("Missing model in Anthropic request".to_string());
    }

    // Stream
    if let Some(stream) = anthropic_body.get("stream").and_then(Value::as_bool) {
        openai.insert("stream".to_string(), Value::Bool(stream));
    }

    // Max tokens
    if let Some(max_tokens) = anthropic_body.get("max_tokens").and_then(Value::as_u64) {
        openai.insert("max_tokens".to_string(), json!(max_tokens));
    }

    // Temperature
    if let Some(temp) = anthropic_body.get("temperature").and_then(Value::as_f64) {
        openai.insert("temperature".to_string(), json!(temp));
    }

    let mut messages: Vec<Value> = Vec::new();

    // System prompt in Anthropic can be a string or an array of blocks
    if let Some(system) = anthropic_body.get("system") {
        let system_text = if let Some(s) = system.as_str() {
            s.to_string()
        } else if let Some(arr) = system.as_array() {
            let mut buf = String::new();
            for item in arr {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    buf.push_str(text);
                }
            }
            buf
        } else {
            String::new()
        };

        if !system_text.is_empty() {
            messages.push(json!({
                "role": "system",
                "content": system_text
            }));
        }
    }

    // Messages
    if let Some(anthropic_messages) = anthropic_body.get("messages").and_then(Value::as_array) {
        for msg in anthropic_messages {
            let role = msg.get("role").and_then(Value::as_str).unwrap_or("user");
            let content = msg.get("content");

            if let Some(text) = content.and_then(Value::as_str) {
                messages.push(json!({
                    "role": role,
                    "content": text
                }));
            } else if let Some(blocks) = content.and_then(Value::as_array) {
                let mut text_buf = String::new();
                let mut tool_calls = Vec::new();

                for block in blocks {
                    let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
                    match block_type {
                        "text" => {
                            if let Some(t) = block.get("text").and_then(Value::as_str) {
                                text_buf.push_str(t);
                            }
                        }
                        "tool_use" => {
                            let id = block.get("id").and_then(Value::as_str).unwrap_or("");
                            let name = block.get("name").and_then(Value::as_str).unwrap_or("");
                            let input = block.get("input").unwrap_or(&Value::Null);
                            let args_str = serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string());

                            tool_calls.push(json!({
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": args_str
                                }
                            }));
                        }
                        "tool_result" => {
                            let tool_use_id = block.get("tool_use_id").and_then(Value::as_str).unwrap_or("");
                            let res_content = if let Some(t) = block.get("content").and_then(Value::as_str) {
                                t.to_string()
                            } else if let Some(sub_blocks) = block.get("content").and_then(Value::as_array) {
                                let mut s = String::new();
                                for b in sub_blocks {
                                    if let Some(t) = b.get("text").and_then(Value::as_str) {
                                        s.push_str(t);
                                    }
                                }
                                s
                            } else {
                                String::new()
                            };

                            messages.push(json!({
                                "role": "tool",
                                "tool_call_id": tool_use_id,
                                "content": res_content
                            }));
                        }
                        _ => {}
                    }
                }

                if !tool_calls.is_empty() {
                    let mut msg_obj = json!({
                        "role": "assistant",
                        "tool_calls": tool_calls
                    });
                    if !text_buf.is_empty() {
                        msg_obj["content"] = Value::String(text_buf);
                    }
                    messages.push(msg_obj);
                } else if !text_buf.is_empty() {
                    messages.push(json!({
                        "role": role,
                        "content": text_buf
                    }));
                }
            }
        }
    }

    openai.insert("messages".to_string(), Value::Array(messages));

    // Tools
    if let Some(anthropic_tools) = anthropic_body.get("tools").and_then(Value::as_array) {
        let mut openai_tools = Vec::new();
        for tool in anthropic_tools {
            let name = tool.get("name").and_then(Value::as_str).unwrap_or("");
            let desc = tool.get("description").and_then(Value::as_str);
            let schema = tool.get("input_schema").unwrap_or(&Value::Null);

            let mut func_obj = json!({
                "name": name,
                "parameters": schema
            });
            if let Some(d) = desc {
                func_obj["description"] = Value::String(d.to_string());
            }

            openai_tools.push(json!({
                "type": "function",
                "function": func_obj
            }));
        }
        if !openai_tools.is_empty() {
            openai.insert("tools".to_string(), Value::Array(openai_tools));
        }
    }

    // Thinking budget (for DeepSeek / OpenAI reasoning support)
    if let Some(thinking) = anthropic_body.get("thinking") {
        if thinking.get("type").and_then(Value::as_str) == Some("enabled") {
            if let Some(budget) = thinking.get("budget_tokens").and_then(Value::as_u64) {
                // Set reasoning effort or thinking param
                openai.insert("reasoning_effort".to_string(), Value::String(if budget > 20000 { "high" } else if budget > 8000 { "medium" } else { "low" }.to_string()));
            }
        }
    }

    Ok(Value::Object(openai))
}

/// Convert an OpenAI Chat Completion response JSON into Anthropic `/v1/messages` format.
pub fn openai_response_to_anthropic(openai_json: &Value) -> Value {
    let id = openai_json
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("msg_gw_translated");
    let model = openai_json
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("claude-sonnet-5");

    let choice = openai_json
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first());

    let mut content = Vec::new();
    let mut stop_reason = "end_turn";

    if let Some(choice) = choice {
        let message = choice.get("message");
        if let Some(message) = message {
            // Text content
            if let Some(text) = message.get("content").and_then(Value::as_str) {
                if !text.is_empty() {
                    content.push(json!({
                        "type": "text",
                        "text": text
                    }));
                }
            }

            // Tool calls
            if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
                for tc in tool_calls {
                    let tc_id = tc.get("id").and_then(Value::as_str).unwrap_or("");
                    let func = tc.get("function");
                    let name = func.and_then(|f| f.get("name")).and_then(Value::as_str).unwrap_or("");
                    let args_str = func.and_then(|f| f.get("arguments")).and_then(Value::as_str).unwrap_or("{}");
                    let input_val: Value = serde_json::from_str(args_str).unwrap_or_else(|_| json!({}));

                    content.push(json!({
                        "type": "tool_use",
                        "id": tc_id,
                        "name": name,
                        "input": input_val
                    }));
                }
                stop_reason = "tool_use";
            }
        }

        if let Some(finish) = choice.get("finish_reason").and_then(Value::as_str) {
            match finish {
                "stop" => stop_reason = "end_turn",
                "length" => stop_reason = "max_tokens",
                "tool_calls" => stop_reason = "tool_use",
                _ => {}
            }
        }
    }

    let input_tokens = openai_json
        .get("usage")
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = openai_json
        .get("usage")
        .and_then(|u| u.get("completion_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);

    json!({
        "id": id,
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens
        }
    })
}

/// Convert an incoming OpenAI SSE chunk line into Anthropic SSE event strings.
pub fn transform_openai_chunk_to_anthropic_sse(
    line: &str,
    state: &mut AnthropicSseState,
) -> Vec<String> {
    let mut events = Vec::new();
    let trimmed = line.trim();
    if !trimmed.starts_with("data:") {
        return events;
    }
    let data_str = trimmed["data:".len()..].trim();
    if data_str == "[DONE]" {
        if state.in_text_block || state.in_tool_block {
            events.push(format!(
                "event: content_block_stop\ndata: {{\"type\":\"content_block_stop\",\"index\":{}}}\n\n",
                state.current_block_index
            ));
            state.in_text_block = false;
            state.in_tool_block = false;
        }
        events.push("event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":0}}\n\n".to_string());
        events.push("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_string());
        return events;
    }

    let Ok(chunk_json) = serde_json::from_str::<Value>(data_str) else {
        return events;
    };

    let choice = chunk_json
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first());

    let delta = match choice.and_then(|c| c.get("delta")) {
        Some(d) => d,
        None => return events,
    };

    // First chunk: emit message_start
    if !state.message_started {
        let id = chunk_json
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("msg_gw_stream");
        let model = chunk_json
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("claude-sonnet-5");

        events.push(format!(
            "event: message_start\ndata: {{\"type\":\"message_start\",\"message\":{{\"id\":\"{}\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"{}\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{{\"input_tokens\":0,\"output_tokens\":0}}}}}}\n\n",
            id, model
        ));
        state.message_started = true;
    }

    // Text delta
    if let Some(text) = delta.get("content").and_then(Value::as_str) {
        if !text.is_empty() {
            if !state.in_text_block {
                if state.in_tool_block {
                    events.push(format!(
                        "event: content_block_stop\ndata: {{\"type\":\"content_block_stop\",\"index\":{}}}\n\n",
                        state.current_block_index
                    ));
                    state.current_block_index += 1;
                    state.in_tool_block = false;
                }
                events.push(format!(
                    "event: content_block_start\ndata: {{\"type\":\"content_block_start\",\"index\":{},\"content_block\":{{\"type\":\"text\",\"text\":\"\"}}}}\n\n",
                    state.current_block_index
                ));
                state.in_text_block = true;
            }

            let escaped_text = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string());
            events.push(format!(
                "event: content_block_delta\ndata: {{\"type\":\"content_block_delta\",\"index\":{},\"delta\":{{\"type\":\"text_delta\",\"text\":{}}}}}\n\n",
                state.current_block_index, escaped_text
            ));
        }
    }

    // Tool call delta
    if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
        for tc in tool_calls {
            let tc_id = tc.get("id").and_then(Value::as_str).unwrap_or("");
            let func = tc.get("function");
            let name = func.and_then(|f| f.get("name")).and_then(Value::as_str).unwrap_or("");
            let args_chunk = func.and_then(|f| f.get("arguments")).and_then(Value::as_str).unwrap_or("");

            if !tc_id.is_empty() || (!name.is_empty() && !state.in_tool_block) {
                // New tool block
                if state.in_text_block || state.in_tool_block {
                    events.push(format!(
                        "event: content_block_stop\ndata: {{\"type\":\"content_block_stop\",\"index\":{}}}\n\n",
                        state.current_block_index
                    ));
                    state.current_block_index += 1;
                    state.in_text_block = false;
                }
                state.current_tool_id = tc_id.to_string();
                state.current_tool_name = name.to_string();
                events.push(format!(
                    "event: content_block_start\ndata: {{\"type\":\"content_block_start\",\"index\":{},\"content_block\":{{\"type\":\"tool_use\",\"id\":\"{}\",\"name\":\"{}\",\"input\":{{}}}}}}\n\n",
                    state.current_block_index, tc_id, name
                ));
                state.in_tool_block = true;
            }

            if !args_chunk.is_empty() {
                let escaped_args = serde_json::to_string(args_chunk).unwrap_or_else(|_| "\"\"".to_string());
                events.push(format!(
                    "event: content_block_delta\ndata: {{\"type\":\"content_block_delta\",\"index\":{},\"delta\":{{\"type\":\"input_json_delta\",\"partial_json\":{}}}}}\n\n",
                    state.current_block_index, escaped_args
                ));
            }
        }
    }

    // Finish reason
    if let Some(finish) = choice.and_then(|c| c.get("finish_reason")).and_then(Value::as_str) {
        let stop_reason = match finish {
            "stop" => "end_turn",
            "length" => "max_tokens",
            "tool_calls" => "tool_use",
            _ => "end_turn",
        };
        if state.in_text_block || state.in_tool_block {
            events.push(format!(
                "event: content_block_stop\ndata: {{\"type\":\"content_block_stop\",\"index\":{}}}\n\n",
                state.current_block_index
            ));
            state.in_text_block = false;
            state.in_tool_block = false;
        }
        events.push(format!(
            "event: message_delta\ndata: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"{}\",\"stop_sequence\":null}},\"usage\":{{\"output_tokens\":0}}}}\n\n",
            stop_reason
        ));
        events.push("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_string());
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_to_openai_request_basic() {
        let req = json!({
            "model": "claude-sonnet-5",
            "system": "You are a helpful coding assistant.",
            "messages": [
                { "role": "user", "content": "Hello!" }
            ],
            "max_tokens": 4096,
            "temperature": 0.7
        });

        let openai = anthropic_to_openai_request(&req).unwrap();
        assert_eq!(openai["model"], "claude-sonnet-5");
        assert_eq!(openai["max_tokens"], 4096);
        let msgs = openai["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "You are a helpful coding assistant.");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"], "Hello!");
    }

    #[test]
    fn test_anthropic_to_openai_tools_and_results() {
        let req = json!({
            "model": "claude-sonnet-5",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        { "type": "text", "text": "Run command ls" }
                    ]
                },
                {
                    "role": "assistant",
                    "content": [
                        {
                            "type": "tool_use",
                            "id": "tool_123",
                            "name": "bash",
                            "input": { "cmd": "ls -la" }
                        }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "tool_result",
                            "tool_use_id": "tool_123",
                            "content": "file1.txt\nfile2.txt"
                        }
                    ]
                }
            ],
            "tools": [
                {
                    "name": "bash",
                    "description": "Execute bash command",
                    "input_schema": {
                        "type": "object",
                        "properties": { "cmd": { "type": "string" } }
                    }
                }
            ]
        });

        let openai = anthropic_to_openai_request(&req).unwrap();
        let msgs = openai["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[1]["role"], "assistant");
        assert_eq!(msgs[1]["tool_calls"][0]["function"]["name"], "bash");
        assert_eq!(msgs[2]["role"], "tool");
        assert_eq!(msgs[2]["tool_call_id"], "tool_123");
        assert_eq!(msgs[2]["content"], "file1.txt\nfile2.txt");

        let tools = openai["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["function"]["name"], "bash");
    }

    #[test]
    fn test_openai_response_to_anthropic_tools() {
        let openai_res = json!({
            "id": "chatcmpl-123",
            "model": "deepseek-chat",
            "choices": [
                {
                    "finish_reason": "tool_calls",
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call_abc",
                                "type": "function",
                                "function": {
                                    "name": "read_file",
                                    "arguments": "{\"path\":\"main.rs\"}"
                                }
                            }
                        ]
                    }
                }
            ],
            "usage": {
                "prompt_tokens": 120,
                "completion_tokens": 45
            }
        });

        let anthropic = openai_response_to_anthropic(&openai_res);
        assert_eq!(anthropic["stop_reason"], "tool_use");
        let content = anthropic["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "tool_use");
        assert_eq!(content[0]["name"], "read_file");
        assert_eq!(content[0]["input"]["path"], "main.rs");
        assert_eq!(anthropic["usage"]["input_tokens"], 120);
        assert_eq!(anthropic["usage"]["output_tokens"], 45);
    }
}
