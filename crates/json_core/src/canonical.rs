use crate::json_value::JSONValue;

pub struct CanonicalSerializer;

impl CanonicalSerializer {
    pub fn compact(value: &JSONValue) -> String {
        render(value, None, 0)
    }

    pub fn pretty(value: &JSONValue) -> String {
        render(value, Some("  "), 0)
    }

    pub fn pretty_with_indent(value: &JSONValue, indent: &str) -> String {
        render(value, Some(indent), 0)
    }
}

fn render(value: &JSONValue, indent: Option<&str>, level: usize) -> String {
    match value {
        JSONValue::Object(members) => {
            if members.is_empty() {
                return "{}".to_string();
            }
            let rendered: Vec<String> = members
                .iter()
                .map(|member| {
                    let key = escape(&member.key);
                    let value = render(&member.value, indent, level + 1);
                    if indent.is_none() {
                        format!("\"{key}\":{value}")
                    } else {
                        format!("\"{key}\": {value}")
                    }
                })
                .collect();
            render_container("{", "}", &rendered, indent, level)
        }
        JSONValue::Array(values) => {
            if values.is_empty() {
                return "[]".to_string();
            }
            let rendered: Vec<String> = values
                .iter()
                .map(|value| render(value, indent, level + 1))
                .collect();
            render_container("[", "]", &rendered, indent, level)
        }
        JSONValue::String(value) => format!("\"{}\"", escape(value)),
        JSONValue::Number { raw } => raw.clone(),
        JSONValue::Bool(true) => "true".to_string(),
        JSONValue::Bool(false) => "false".to_string(),
        JSONValue::Null => "null".to_string(),
    }
}

fn render_container(
    opening: &str,
    closing: &str,
    values: &[String],
    indent: Option<&str>,
    level: usize,
) -> String {
    match indent {
        None => format!("{opening}{}{closing}", values.join(",")),
        Some(indent) => {
            let padding = indent.repeat(level + 1);
            let closing_padding = indent.repeat(level);
            format!(
                "{opening}\n{padding}{}\n{closing_padding}{closing}",
                values.join(&format!(",\n{padding}"))
            )
        }
    }
}

fn escape(value: &str) -> String {
    let mut output = String::new();
    for scalar in value.chars() {
        let code = scalar as u32;
        match code {
            0x08 => output.push_str("\\b"),
            0x0C => output.push_str("\\f"),
            0x0A => output.push_str("\\n"),
            0x0D => output.push_str("\\r"),
            0x09 => output.push_str("\\t"),
            0x00..=0x1F => {
                output.push_str("\\u");
                output.push_str(&hexadecimal(code, 4));
            }
            0x22 => output.push_str("\\\""),
            0x5C => output.push_str("\\\\"),
            _ => output.push(scalar),
        }
    }
    output
}

fn hexadecimal(value: u32, width: usize) -> String {
    let digits = b"0123456789abcdef";
    let mut result = String::new();
    let mut remaining = value;
    for _ in 0..width {
        result.push(digits[(remaining & 0xF) as usize] as char);
        remaining >>= 4;
    }
    result.chars().rev().collect()
}
