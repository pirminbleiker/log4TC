//! Message template formatting and parsing

use std::collections::HashMap;
use std::sync::OnceLock;
use regex::Regex;

fn placeholder_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{([^}]+)\}").expect("regex should compile"))
}

fn numeric_placeholder_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{(\d+)\}").expect("regex should compile"))
}

/// Message formatter for template-based message formatting
pub struct MessageFormatter;

impl MessageFormatter {
    pub fn format(template: &str, arguments: &HashMap<usize, serde_json::Value>) -> String {
        if !template.contains('{') {
            return template.to_string();
        }

        let re = numeric_placeholder_regex();
        let mut result = template.to_string();

        // Collect all (placeholder, replacement) pairs first to avoid repeated String::replace
        let mut replacements = Vec::with_capacity(arguments.len());
        for cap in re.captures_iter(template) {
            if let Ok(index) = cap[1].parse::<usize>() {
                if let Some(value) = arguments.get(&index) {
                    let placeholder = &cap[0];
                    let value_str = Self::value_to_string(value);
                    replacements.push((placeholder.to_string(), value_str));
                }
            }
        }

        // Apply replacements
        for (placeholder, replacement) in replacements {
            result = result.replace(&placeholder, &replacement);
        }

        result
    }

    /// Format a message with both positional and named arguments.
    /// Named placeholders like {time} are matched to arguments by order of appearance
    /// (Serilog/MessageTemplates style): first placeholder → arg[0], second → arg[1], etc.
    /// Numeric placeholders like {0}, {1} are matched by index directly.
    pub fn format_with_context(
        template: &str,
        arguments: &HashMap<usize, serde_json::Value>,
        context: &HashMap<String, serde_json::Value>,
    ) -> String {
        if !template.contains('{') {
            return template.to_string();
        }

        let re = placeholder_regex();
        let mut result = template.to_string();
        let mut replacements = Vec::new();
        let mut positional_index: usize = 0; // tracks which arg to use for named placeholders

        for cap in re.captures_iter(template) {
            let key = &cap[1];
            let placeholder = cap[0].to_string();

            if let Ok(index) = key.parse::<usize>() {
                // Numeric placeholder {0}, {1} → direct index lookup
                if let Some(value) = arguments.get(&index) {
                    replacements.push((placeholder, Self::value_to_string(value)));
                }
                positional_index += 1;
            } else {
                // Named placeholder {time}, {name} → match by order of appearance
                // PLC uses 1-based arg indices, so first named placeholder = arg[1]
                let arg_index = positional_index + 1;
                if let Some(value) = arguments.get(&arg_index) {
                    replacements.push((placeholder, Self::value_to_string(value)));
                } else if let Some(value) = context.get(key) {
                    // Fallback: try context by name
                    replacements.push((placeholder, Self::value_to_string(value)));
                }
                positional_index += 1;
            }
        }

        for (placeholder, replacement) in replacements {
            result = result.replace(&placeholder, &replacement);
        }

        result
    }

    /// Extract placeholders from a template
    pub fn extract_placeholders(template: &str) -> Vec<String> {
        let re = placeholder_regex();
        re.captures_iter(template)
            .map(|cap| cap[1].to_string())
            .collect()
    }

    /// Convert a JSON value to string representation
    fn value_to_string(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Null => "null".to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(arr) => {
                let items: Vec<String> = arr.iter().map(Self::value_to_string).collect();
                format!("[{}]", items.join(", "))
            }
            serde_json::Value::Object(obj) => {
                let items: Vec<String> = obj
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, Self::value_to_string(v)))
                    .collect();
                format!("{{{}}}", items.join(", "))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_positional_args() {
        let mut args = HashMap::new();
        args.insert(0, serde_json::json!("world"));
        args.insert(1, serde_json::json!(42));

        let result = MessageFormatter::format("Hello {0}, answer is {1}", &args);
        assert_eq!(result, "Hello world, answer is 42");
    }

    #[test]
    fn test_format_with_context() {
        let mut args = HashMap::new();
        args.insert(0, serde_json::json!("user123"));

        let mut context = HashMap::new();
        context.insert("action".to_string(), serde_json::json!("login"));

        let result = MessageFormatter::format_with_context(
            "User {0} performed {action}",
            &args,
            &context,
        );
        assert_eq!(result, "User user123 performed login");
    }

    #[test]
    fn test_extract_placeholders() {
        let template = "Hello {0}, action is {action}, count is {1}";
        let placeholders = MessageFormatter::extract_placeholders(template);

        assert_eq!(placeholders.len(), 3);
        assert!(placeholders.contains(&"0".to_string()));
        assert!(placeholders.contains(&"action".to_string()));
        assert!(placeholders.contains(&"1".to_string()));
    }

    #[test]
    fn test_value_formatting() {
        assert_eq!(MessageFormatter::value_to_string(&serde_json::json!(null)), "null");
        assert_eq!(MessageFormatter::value_to_string(&serde_json::json!(true)), "true");
        assert_eq!(MessageFormatter::value_to_string(&serde_json::json!(123)), "123");
        assert_eq!(MessageFormatter::value_to_string(&serde_json::json!("text")), "text");
    }

    #[test]
    fn test_format_partial_placeholders() {
        let args = HashMap::new();
        let result = MessageFormatter::format("Missing {0} here {1}", &args);
        assert_eq!(result, "Missing {0} here {1}");
    }

    #[test]
    fn test_format_no_args() {
        let args = HashMap::new();
        let result = MessageFormatter::format("Simple message without placeholders", &args);
        assert_eq!(result, "Simple message without placeholders");
    }

    #[test]
    fn test_format_extra_args() {
        let mut args = HashMap::new();
        args.insert(0, serde_json::json!("used"));
        args.insert(1, serde_json::json!("unused"));
        args.insert(2, serde_json::json!("also_unused"));

        let result = MessageFormatter::format("Only {0} is used", &args);
        assert_eq!(result, "Only used is used");
    }

    #[test]
    fn test_format_out_of_order_indices() {
        let mut args = HashMap::new();
        args.insert(2, serde_json::json!("third"));
        args.insert(0, serde_json::json!("first"));
        args.insert(1, serde_json::json!("second"));

        let result = MessageFormatter::format("Order: {0}, {1}, {2}", &args);
        assert_eq!(result, "Order: first, second, third");
    }

    #[test]
    fn test_format_repeated_placeholders() {
        let mut args = HashMap::new();
        args.insert(0, serde_json::json!("hello"));

        let result = MessageFormatter::format("{0} {0} {0}", &args);
        assert_eq!(result, "hello hello hello");
    }

    #[test]
    fn test_format_named_args_only() {
        let args = HashMap::new();
        let mut context = HashMap::new();
        context.insert("name".to_string(), serde_json::json!("Alice"));
        context.insert("action".to_string(), serde_json::json!("logged in"));

        let result = MessageFormatter::format_with_context(
            "{name} {action}",
            &args,
            &context,
        );
        assert_eq!(result, "Alice logged in");
    }

    #[test]
    fn test_format_arg_type_coercion() {
        let mut args = HashMap::new();
        args.insert(0, serde_json::json!(42));
        args.insert(1, serde_json::json!(3.14));
        args.insert(2, serde_json::json!(true));
        args.insert(3, serde_json::json!(null));

        let result = MessageFormatter::format("Int: {0}, Float: {1}, Bool: {2}, Null: {3}", &args);
        assert_eq!(result, "Int: 42, Float: 3.14, Bool: true, Null: null");
    }

    #[test]
    fn test_format_special_chars_in_args() {
        let mut args = HashMap::new();
        args.insert(0, serde_json::json!("line1\nline2"));
        args.insert(1, serde_json::json!("\"quoted\""));
        args.insert(2, serde_json::json!("path\\to\\file"));

        let result = MessageFormatter::format("Newline: {0}, Quote: {1}, Path: {2}", &args);
        assert!(result.contains("line1\nline2"));
        assert!(result.contains("\"quoted\""));
    }

    #[test]
    fn test_format_array_argument() {
        let mut args = HashMap::new();
        args.insert(0, serde_json::json!([1, 2, 3]));

        let result = MessageFormatter::format("Array: {0}", &args);
        assert!(result.contains("["));
        assert!(result.contains("]"));
        assert!(result.contains("1"));
        assert!(result.contains("2"));
        assert!(result.contains("3"));
    }

    #[test]
    fn test_format_object_argument() {
        let mut args = HashMap::new();
        args.insert(0, serde_json::json!({"key": "value"}));

        let result = MessageFormatter::format("Object: {0}", &args);
        assert!(result.contains("{"));
        assert!(result.contains("}"));
        assert!(result.contains("key"));
    }

    #[test]
    fn test_extract_mixed_placeholders() {
        let template = "User {0} performed {action} with {count}";
        let placeholders = MessageFormatter::extract_placeholders(template);

        assert_eq!(placeholders.len(), 3);
        assert!(placeholders.contains(&"0".to_string()));
        assert!(placeholders.contains(&"action".to_string()));
        assert!(placeholders.contains(&"count".to_string()));
    }

    #[test]
    fn test_extract_no_placeholders() {
        let template = "Just a plain message";
        let placeholders = MessageFormatter::extract_placeholders(template);

        assert_eq!(placeholders.len(), 0);
    }

    #[test]
    fn test_extract_numeric_placeholders() {
        let template = "{0} {1} {2} {3} {4}";
        let placeholders = MessageFormatter::extract_placeholders(template);

        assert_eq!(placeholders.len(), 5);
        for i in 0..5 {
            assert!(placeholders.contains(&i.to_string()));
        }
    }

    #[test]
    fn test_format_empty_template() {
        let args = HashMap::new();
        let result = MessageFormatter::format("", &args);
        assert_eq!(result, "");
    }

    #[test]
    fn test_format_context_overrides_positional() {
        // When a named placeholder appears, it should use context
        let mut args = HashMap::new();
        args.insert(0, serde_json::json!("positional"));

        let mut context = HashMap::new();
        context.insert("name".to_string(), serde_json::json!("context_value"));

        let result = MessageFormatter::format_with_context(
            "Positional: {0}, Named: {name}",
            &args,
            &context,
        );
        assert!(result.contains("Positional: positional"));
        assert!(result.contains("Named: context_value"));
    }

    #[test]
    fn test_format_special_placeholder_chars() {
        let mut args = HashMap::new();
        args.insert(0, serde_json::json!("value"));

        // Test that special regex chars in placeholders don't break things
        let result = MessageFormatter::format("Placeholder: {0}", &args);
        assert_eq!(result, "Placeholder: value");
    }

    #[test]
    fn test_value_to_string_number_precision() {
        let large_number = serde_json::json!(1234567890.123456789);
        let result = MessageFormatter::value_to_string(&large_number);
        assert!(result.contains("1234567890"));
    }

    #[test]
    fn test_format_very_long_message() {
        let mut args = HashMap::new();
        let long_arg = "x".repeat(10000);
        args.insert(0, serde_json::json!(&long_arg));

        let template = "Start {0} End";
        let result = MessageFormatter::format(template, &args);
        assert!(result.starts_with("Start x"));
        assert!(result.ends_with("x End"));
        assert_eq!(result.len(), 10000 + "Start  End".len());
    }

    #[test]
    fn test_extract_placeholder_with_spaces() {
        // Note: The current implementation doesn't support spaces in placeholders
        // but we should test the behavior
        let template = "Test { 0 } here";
        let placeholders = MessageFormatter::extract_placeholders(template);

        // The regex might or might not match " 0 " depending on implementation
        // This test documents the current behavior
        if !placeholders.is_empty() {
            // If it matches, it should capture the content with spaces
            assert!(placeholders[0].contains("0"));
        }
    }

    #[test]
    fn test_format_numeric_string_context() {
        let args = HashMap::new();
        let mut context = HashMap::new();
        context.insert("123".to_string(), serde_json::json!("numeric_key"));

        let result = MessageFormatter::format_with_context(
            "Value: {123}",
            &args,
            &context,
        );
        // Numeric placeholders like {123} are treated as positional arguments, not context keys
        assert_eq!(result, "Value: {123}");
    }

    #[test]
    fn test_format_with_empty_context_and_args() {
        let args = HashMap::new();
        let context = HashMap::new();

        let result = MessageFormatter::format_with_context(
            "No substitution here",
            &args,
            &context,
        );
        assert_eq!(result, "No substitution here");
    }
}
