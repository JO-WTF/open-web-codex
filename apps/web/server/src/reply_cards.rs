#[derive(Debug, Clone, Copy)]
pub struct ReplyCardCapability {
    pub kind: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub input_schema: &'static str,
    pub artifact_policy: &'static str,
}

const REPLY_CARD_MARKERS: &[&str] = &[
    "```open-web-card ",
    "\"type\":\"reply_card.v1\"",
    "\"type\": \"reply_card.v1\"",
    "\"widget_type\":\"map\"",
    "\"widget_type\": \"map\"",
];

const REPLY_CARD_CAPABILITIES: &[ReplyCardCapability] = &[ReplyCardCapability {
    kind: "map.v1",
    display_name: "Map card",
    description: "Use for coordinates, location lookup, routes, distances, boundaries, points, lines, polygons, GeoJSON and other geospatial visualizations.",
    input_schema: r#"{"title":"string","intent":"coordinates|location|route|distance|boundary|visualization","input_ref":"optional tool-result or artifact reference","artifact_id":"optional existing artifact reference","fallback_text":"string"}"#,
    artifact_policy: "Never inline large GeoJSON. Use input_ref or artifact_id for server-generated geospatial payloads.",
}];

pub fn reply_card_capabilities() -> &'static [ReplyCardCapability] {
    REPLY_CARD_CAPABILITIES
}

pub fn has_reply_card_marker(text: &str) -> bool {
    REPLY_CARD_MARKERS
        .iter()
        .any(|marker| text.contains(marker))
}

pub fn build_reply_card_affordance_prompt(capabilities: &[ReplyCardCapability]) -> String {
    let mut prompt = String::from(
        r#"

<open-web-codex-reply-card-capabilities>
The Web platform can render structured reply cards. Decide whether a card would
make the answer clearer. Do not force a card when plain text is better.

If a card is useful, call the conceptual platform tool `create_reply_card` in
your reasoning, then place only the returned compact marker in the final answer
at the exact position where the card should appear. The compact marker format is:

```open-web-card <kind>
{
  "title": "Short card title",
  "input_ref": "optional-tool-result-or-artifact-reference",
  "artifact_id": "optional-existing-artifact-reference",
  "fallback_text": "One sentence to show if the card cannot render."
}
```

Available card kinds:
"#,
    );

    for capability in capabilities {
        prompt.push_str("- `");
        prompt.push_str(capability.kind);
        prompt.push_str("` (");
        prompt.push_str(capability.display_name);
        prompt.push_str("): ");
        prompt.push_str(capability.description);
        prompt.push_str(" Schema: ");
        prompt.push_str(capability.input_schema);
        prompt.push_str(" Policy: ");
        prompt.push_str(capability.artifact_policy);
        prompt.push('\n');
    }

    prompt.push_str("</open-web-codex-reply-card-capabilities>");
    prompt
}

pub fn prepare_message_text_for_reply_cards(text: &str) -> String {
    if has_reply_card_marker(text) {
        return text.to_string();
    }
    format!(
        "{text}{}",
        build_reply_card_affordance_prompt(reply_card_capabilities())
    )
}

#[cfg(test)]
mod tests {
    use super::{
        build_reply_card_affordance_prompt, has_reply_card_marker,
        prepare_message_text_for_reply_cards, reply_card_capabilities,
    };

    #[test]
    fn registry_exposes_map_as_one_reply_card_kind() {
        let capabilities = reply_card_capabilities();
        assert_eq!(capabilities.len(), 1);
        assert_eq!(capabilities[0].kind, "map.v1");
        assert!(capabilities[0].description.contains("GeoJSON"));
    }

    #[test]
    fn generated_prompt_is_registry_driven_and_not_keyword_classifier() {
        let prompt = build_reply_card_affordance_prompt(reply_card_capabilities());
        assert!(prompt.contains("create_reply_card"));
        assert!(prompt.contains("```open-web-card <kind>"));
        assert!(prompt.contains("`map.v1`"));
        assert!(prompt.contains("Never inline large GeoJSON"));
    }

    #[test]
    fn prepares_normal_messages_with_generic_card_affordance() {
        let prepared = prepare_message_text_for_reply_cards("请修复这个 React 组件的类型错误");
        assert!(prepared.starts_with("请修复这个 React 组件的类型错误"));
        assert!(prepared.contains("Decide whether a card would"));
    }

    #[test]
    fn does_not_duplicate_existing_card_markers() {
        let text = "```open-web-card map.v1\n{}\n```";
        assert!(has_reply_card_marker(text));
        assert_eq!(prepare_message_text_for_reply_cards(text), text);
    }
}
