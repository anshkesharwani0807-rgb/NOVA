//! Tests for the natural language query parser.

use nova_search::document::MatchMode;
use nova_search::parser::QueryParser;

#[test]
fn test_parse_simple_text() {
    let query = QueryParser::parse("hello world");
    assert_eq!(query.text, Some("hello world".to_string()));
    assert_eq!(query.mode, MatchMode::Partial);
}

#[test]
fn test_parse_with_tags() {
    let query = QueryParser::parse("work project tag:urgent tag:meeting");
    assert_eq!(query.text, Some("work project".to_string()));
    assert!(query.tags.contains(&"urgent".to_string()));
    assert!(query.tags.contains(&"meeting".to_string()));
}

#[test]
fn test_parse_with_category() {
    let query = QueryParser::parse("meeting notes cat:work");
    assert_eq!(query.text, Some("meeting notes".to_string()));
    assert_eq!(query.category, Some("work".to_string()));
}

#[test]
fn test_parse_with_source() {
    let query = QueryParser::parse("emails src:gmail");
    assert_eq!(query.text, Some("emails".to_string()));
    assert_eq!(query.source, Some("gmail".to_string()));
}

#[test]
fn test_parse_mixed() {
    let query = QueryParser::parse("urgent task tag:todo cat:work src:memory");
    assert_eq!(query.text, Some("urgent task".to_string()));
    assert!(query.tags.contains(&"todo".to_string()));
    assert_eq!(query.category, Some("work".to_string()));
    assert_eq!(query.source, Some("memory".to_string()));
}

#[test]
fn test_parse_only_filters() {
    let query = QueryParser::parse("tag:work cat:notes");
    assert_eq!(query.text, None);
    assert!(query.tags.contains(&"work".to_string()));
    assert_eq!(query.category, Some("notes".to_string()));
}

#[test]
fn test_parse_empty() {
    let query = QueryParser::parse("");
    assert_eq!(query.text, None);
    assert!(query.tags.is_empty());
}

#[test]
fn test_parse_unknown_key() {
    let query = QueryParser::parse("hello unknown:value world");
    assert_eq!(query.text, Some("hello unknown:value world".to_string()));
}
