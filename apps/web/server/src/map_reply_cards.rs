const MAP_CARD_FORMAT_INSTRUCTION: &str = r#"

<open-web-codex-map-card-instructions>
The Web platform can render map reply cards when the answer is geographic.
Use a map card when the answer contains coordinates, a location lookup, a route,
a distance/path, an administrative boundary, or geospatial data visualization.

When a map is useful, include a compact card marker at the exact position where
the map should appear. Do not inline large GeoJSON. Use input_ref or artifact_id
for any server-generated GeoJSON/tool result.

```open-web-card map.v1
{
  "title": "Short map title",
  "intent": "coordinates|location|route|distance|boundary|visualization",
  "input_ref": "optional-tool-or-artifact-reference",
  "fallback_text": "One sentence to show if the map cannot render."
}
```
</open-web-codex-map-card-instructions>"#;

const GEO_KEYWORDS: &[&str] = &[
    "经纬度",
    "纬度",
    "经度",
    "地理",
    "地理位置",
    "位置",
    "坐标",
    "路线",
    "路程",
    "距离",
    "导航",
    "边界",
    "行政区",
    "地图",
    "geojson",
    "GeoJSON",
    "可视化",
    "多边形",
    "点位",
    "coordinate",
    "coordinates",
    "latitude",
    "longitude",
    "location",
    "geocode",
    "reverse geocode",
    "route",
    "directions",
    "distance",
    "boundary",
    "polygon",
    "map",
    "geospatial",
    "geojson",
    "visualize",
];

const GEO_PATTERNS: &[&str] = &["到", "from", "to", "between"];

pub fn has_map_card_marker(text: &str) -> bool {
    text.contains("```open-web-card map.v1")
        || text.contains("\"widget_type\":\"map\"")
        || text.contains("\"widget_type\": \"map\"")
}

pub fn looks_geographic(text: &str) -> bool {
    let normalized = text.trim();
    if normalized.is_empty() {
        return false;
    }
    let lower = normalized.to_lowercase();
    let keyword_hit = GEO_KEYWORDS
        .iter()
        .any(|keyword| lower.contains(&keyword.to_lowercase()));
    if keyword_hit {
        return true;
    }

    // A lightweight route heuristic for prompts such as "北京到上海怎么走" without
    // requiring an LLM-side classifier before we can tell Codex about cards.
    let has_route_connector = GEO_PATTERNS.iter().any(|pattern| lower.contains(pattern));
    has_route_connector
        && (lower.contains("公里")
            || lower.contains("km")
            || lower.contains("路线")
            || lower.contains("route"))
}

pub fn prepare_message_text_for_map_cards(text: &str) -> String {
    if has_map_card_marker(text) || !looks_geographic(text) {
        return text.to_string();
    }
    format!("{text}{MAP_CARD_FORMAT_INSTRUCTION}")
}

#[cfg(test)]
mod tests {
    use super::{has_map_card_marker, looks_geographic, prepare_message_text_for_map_cards};

    #[test]
    fn detects_chinese_geographic_intents() {
        assert!(looks_geographic("查询北京的经纬度并显示地图"));
        assert!(looks_geographic("计算北京到上海的路线和距离"));
        assert!(looks_geographic("展示广东省行政区边界"));
    }

    #[test]
    fn does_not_inject_for_normal_coding_prompts() {
        let text = "请修复这个 React 组件的类型错误";
        assert_eq!(prepare_message_text_for_map_cards(text), text);
    }

    #[test]
    fn appends_compact_marker_instructions_for_geographic_prompts() {
        let prepared = prepare_message_text_for_map_cards("计算北京到上海的路线和距离");
        assert!(prepared.contains("```open-web-card map.v1"));
        assert!(prepared.contains("Do not inline large GeoJSON"));
        assert!(prepared.contains("input_ref"));
    }

    #[test]
    fn does_not_duplicate_existing_card_markers() {
        let text = "```open-web-card map.v1\n{}\n```";
        assert!(has_map_card_marker(text));
        assert_eq!(prepare_message_text_for_map_cards(text), text);
    }
}
