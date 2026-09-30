use serde_json::{json, Map, Value};

/// Convert an OpenAI Responses request (from Codex) into a standard OpenAI Chat Completions request.
pub fn responses_to_chat_request(responses_body: &Value) -> Result<Value, String> {
    let mut chat = Map::new();

    // Model
    if let Some(model) = responses_body.get("model").and_then(Value::as_str) {
        chat.insert("model".to_string(), Value::String(model.to_string()));
    } else {
        return Err("Missing model in Responses request".to_string());
    }

    // Stream
    if let Some(stream) = responses_body.get("stream").and_then(Value::as_bool) {
        chat.insert("stream".to_string(), Value::Bool(stream));
    }

    // Temperature
    if let Some(temp) = responses_body.get("temperature").and_then(Value::as_f64) {
        chat.insert("temperature".to_string(), json!(temp));
    }

    // Reasoning effort mapping
    if let Some(reasoning) = responses_body.get("reasoning") {
        if let Some(effort) = reasoning.get("effort").and_then(Value::as_str) {
            chat.insert("reasoning_effort".to_string(), Value::String(effort.to_string()));
        }
    }

    let mut messages: Vec<Value> = Vec::new();

    // Responses uses `input` array containing items
    if let Some(input_items) = responses_body.get("input").and_then(Value::as_array) {
        for item in input_items {
            let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
            match item_type {
                "message" => {
                    let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
                    let mut text_buf = String::new();

                    if let Some(content_arr) = item.get("content").and_then(Value::as_array) {
                        for c in content_arr {
                            let c_type = c.get("type").and_then(Value::as_str).unwrap_or("");
                            if (c_type == "input_text" || c_type == "text")
                                && let Some(t) = c.get("text").and_then(Value::as_str)
                            {
                                text_buf.push_str(t);
                            }
                        }
                    } else if let Some(t) = item.get("content").and_then(Value::as_str) {
                        text_buf.push_str(t);
                    }

                    messages.push(json!({
                        "role": role,
                        "content": text_buf
                    }));
                }
                "function_call" => {
                    let call_id = item.get("call_id").and_then(Value::as_str).unwrap_or("");
                    let name = item.get("name").and_then(Value::as_str).unwrap_or("");
                    let arguments = item.get("arguments").and_then(Value::as_str).unwrap_or("{}");

                    messages.push(json!({
                        "role": "assistant",
                        "tool_calls": [{
                            "id": call_id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": arguments
                            }
                        }]
                    }));
                }
                "function_call_output" => {
                    let call_id = item.get("call_id").and_then(Value::as_str).unwrap_or("");
                    let output = item.get("output").and_then(Value::as_str).unwrap_or("");

                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": output
                    }));
                }
                _ => {}
            }
        }
    }

    chat.insert("messages".to_string(), Value::Array(messages));

    // Tools
    if let Some(tools) = responses_body.get("tools").and_then(Value::as_array) {
        let mut chat_tools = Vec::new();
        for tool in tools {
            let tool_type = tool.get("type").and_then(Value::as_str).unwrap_or("");
            if tool_type == "function" {
                let name = tool.get("name").and_then(Value::as_str).unwrap_or("");
                let desc = tool.get("description").and_then(Value::as_str);
                let params = tool.get("parameters").unwrap_or(&Value::Null);

                let mut func_obj = json!({
                    "name": name,
                    "parameters": params
                });
                if let Some(d) = desc {
                    func_obj["description"] = Value::String(d.to_string());
                }

                chat_tools.push(json!({
                    "type": "function",
                    "function": func_obj
                }));
            }
        }
        if !chat_tools.is_empty() {
            chat.insert("tools".to_string(), Value::Array(chat_tools));
        }
    }

    Ok(Value::Object(chat))
}

/// Convert a Chat Completions JSON response into an OpenAI Responses JSON response for Codex.
pub fn chat_response_to_responses(chat_json: &Value) -> Value {
    let id = chat_json.get("id").and_then(Value::as_str).unwrap_or("resp_gw_translated");
    let model = chat_json.get("model").and_then(Value::as_str).unwrap_or("gpt-5");

    let choice = chat_json
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first());

    let mut output_items = Vec::new();
    let mut status = "completed";

    if let Some(choice) = choice {
        let message = choice.get("message");
        if let Some(message) = message {
            // Text content
            if let Some(text) = message.get("content").and_then(Value::as_str) {
                if !text.is_empty() {
                    output_items.push(json!({
                        "type": "message",
                        "id": format!("msg_{id}"),
                        "role": "assistant",
                        "content": [{
                            "type": "text",
                            "text": text
                        }]
                    }));
                }
            }

            // Tool calls
            if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
                for tc in tool_calls {
                    let tc_id = tc.get("id").and_then(Value::as_str).unwrap_or("");
                    let func = tc.get("function");
                    let name = func.and_then(|f| f.get("name")).and_then(Value::as_str).unwrap_or("");
                    let args = func.and_then(|f| f.get("arguments")).and_then(Value::as_str).unwrap_or("{}");

                    output_items.push(json!({
                        "type": "function_call",
                        "id": format!("fc_{tc_id}"),
                        "call_id": tc_id,
                        "name": name,
                        "arguments": args
                    }));
                }
            }
        }

        if let Some(finish) = choice.get("finish_reason").and_then(Value::as_str) {
            match finish {
                "stop" | "tool_calls" => status = "completed",
                "length" => status = "incomplete",
                _ => status = "completed",
            }
        }
    }

    let input_tokens = chat_json
        .get("usage")
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = chat_json
        .get("usage")
        .and_then(|u| u.get("completion_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);

    json!({
        "id": id,
        "object": "response",
        "created_at": chrono::Utc::now().timestamp(),
        "status": status,
        "model": model,
        "output": output_items,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
            "total_tokens": input_tokens + output_tokens
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_responses_to_chat_request_conversion() {
        let responses_req = json!({
            "model": "gpt-5-codex",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "Fix this bug" }]
                }
            ],
            "reasoning": { "effort": "high" },
            "stream": true
        });

        let chat_req = responses_to_chat_request(&responses_req).unwrap();
        assert_eq!(chat_req["model"], "gpt-5-codex");
        assert_eq!(chat_req["stream"], true);
        assert_eq!(chat_req["reasoning_effort"], "high");
        let msgs = chat_req["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"], "Fix this bug");
    }
}
