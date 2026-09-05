use super::event::{kind_of, EmitRequest, Fragment, Money, Payload, Source};
use serde_json::Value;
use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::Path;

pub fn load_events(inject: Option<&Path>) -> Result<Vec<EmitRequest>, String> {
    if let Some(path) = inject {
        let text = fs::read_to_string(path).map_err(|err| format!("inject: {err}"))?;
        return parse_inject(&text);
    }
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return Ok(default_fixture());
    }
    let mut text = String::new();
    stdin
        .lock()
        .read_to_string(&mut text)
        .map_err(|err| format!("stdin: {err}"))?;
    parse_inject(&text)
}

pub fn parse_inject(text: &str) -> Result<Vec<EmitRequest>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(default_fixture());
    }
    if trimmed.starts_with('[') {
        let values: Vec<Value> =
            serde_json::from_str(trimmed).map_err(|err| format!("inject: {err}"))?;
        return values.into_iter().map(parse_one).collect();
    }
    if trimmed.starts_with('{') {
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            return Ok(vec![parse_one(value)?]);
        }
    }
    let mut out = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: Value =
            serde_json::from_str(line).map_err(|err| format!("inject: {err}"))?;
        out.push(parse_one(value)?);
    }
    if out.is_empty() {
        return Err("inject: пустой JSON".into());
    }
    Ok(out)
}

pub fn default_fixture() -> Vec<EmitRequest> {
    vec![
        host_event(
            Payload::Message {
                user_id: "fixture".into(),
                display_name: "fixture".into(),
                fragments: vec![Fragment::Text {
                    text: "fixture hello".into(),
                }],
                name_color: None,
                message_id: None,
                rewarded: false,
            },
        ),
        host_event(Payload::Donation {
            user_id: "fixture".into(),
            display_name: "fixture".into(),
            money: Money {
                amount: 5.0,
                currency: "USD".into(),
            },
            fragments: vec![],
        }),
        host_event(Payload::Reward {
            user_id: "fixture".into(),
            display_name: "fixture".into(),
            reward_id: "reward-1".into(),
            title: "Highlight".into(),
            cost: 100,
            fragments: vec![Fragment::Text {
                text: "hello".into(),
            }],
            image_url: None,
        }),
    ]
}

fn host_event(payload: Payload) -> EmitRequest {
    EmitRequest {
        source: Source {
            plugin_id: "fixture".into(),
            platform: "fixture".into(),
            channel: "dev".into(),
        },
        kind: kind_of(&payload),
        payload,
        opaque: None,
    }
}

fn parse_one(mut value: Value) -> Result<EmitRequest, String> {
    let channel = take_str(&mut value, &["channel"]).unwrap_or_else(|| "dev".into());
    let plugin_id =
        take_str(&mut value, &["plugin_id", "pluginId"]).unwrap_or_else(|| "fixture".into());
    let platform = take_str(&mut value, &["platform"]).unwrap_or_else(|| "fixture".into());
    let payload_value = value
        .get("payload")
        .cloned()
        .unwrap_or(value);
    let payload: Payload =
        serde_json::from_value(payload_value).map_err(|err| format!("inject: {err}"))?;
    Ok(EmitRequest {
        source: Source {
            plugin_id,
            platform,
            channel,
        },
        kind: kind_of(&payload),
        payload,
        opaque: None,
    })
}

fn take_str(value: &mut Value, keys: &[&str]) -> Option<String> {
    let obj = value.as_object_mut()?;
    for key in keys {
        if let Some(v) = obj.remove(*key) {
            if let Some(s) = v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                return Some(s.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_core_message_json() {
        let events = parse_inject(
            r#"{"type":"message","user_id":"1","display_name":"Nick","fragments":[{"type":"text","text":"hi"}]}"#,
        )
        .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source.channel, "dev");
        match &events[0].payload {
            Payload::Message {
                display_name,
                fragments,
                ..
            } => {
                assert_eq!(display_name, "Nick");
                assert!(matches!(
                    &fragments[0],
                    Fragment::Text { text } if text == "hi"
                ));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn empty_is_fixture_hello() {
        let events = parse_inject("").unwrap();
        assert!(payload_has_hello(&events));
    }

    fn payload_has_hello(events: &[EmitRequest]) -> bool {
        events.iter().any(|event| {
            matches!(
                &event.payload,
                Payload::Message { fragments, .. }
                    if fragments.iter().any(|f| matches!(f, Fragment::Text { text } if text == "fixture hello"))
            )
        })
    }
}
