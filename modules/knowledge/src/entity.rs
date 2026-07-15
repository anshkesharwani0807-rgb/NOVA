use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum EntityType {
    Person,
    Place,
    Organization,
    Device,
    Document,
    Website,
    Event,
    File,
    Image,
    Topic,
    Custom(String),
}

impl EntityType {
    pub fn as_str(&self) -> &str {
        match self {
            EntityType::Person => "person",
            EntityType::Place => "place",
            EntityType::Organization => "organization",
            EntityType::Device => "device",
            EntityType::Document => "document",
            EntityType::Website => "website",
            EntityType::Event => "event",
            EntityType::File => "file",
            EntityType::Image => "image",
            EntityType::Topic => "topic",
            EntityType::Custom(s) => s,
        }
    }
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntity {
    pub id: String,
    pub name: String,
    pub entity_type: EntityType,
    pub description: String,
    pub aliases: Vec<String>,
    pub first_seen: i64,
    pub last_seen: i64,
    pub mention_count: u32,
    pub confidence: f64,
    pub source: EntitySource,
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EntitySource {
    Memory,
    Note,
    Ocr,
    Screenshot,
    Conversation,
    Automation,
    Plugin,
    Vision,
    Manual,
    Import,
}

impl std::fmt::Display for EntitySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntitySource::Memory => write!(f, "memory"),
            EntitySource::Note => write!(f, "note"),
            EntitySource::Ocr => write!(f, "ocr"),
            EntitySource::Screenshot => write!(f, "screenshot"),
            EntitySource::Conversation => write!(f, "conversation"),
            EntitySource::Automation => write!(f, "automation"),
            EntitySource::Plugin => write!(f, "plugin"),
            EntitySource::Vision => write!(f, "vision"),
            EntitySource::Manual => write!(f, "manual"),
            EntitySource::Import => write!(f, "import"),
        }
    }
}

pub struct EntityExtractor {
    known_names: HashSet<String>,
    _max_entities: usize,
}

impl Default for EntityExtractor {
    fn default() -> Self {
        Self::new(100000)
    }
}

impl EntityExtractor {
    pub fn new(max_entities: usize) -> Self {
        let mut known_names = HashSet::new();
        for name in &[
            "alice", "bob", "charlie", "dave", "eve", "frank", "grace", "henry", "iris", "jack",
            "kate", "leo", "mia", "noah", "olivia", "peter", "quinn", "rose", "sam", "tina", "uma",
            "victor", "wendy", "xander", "yara", "zack",
        ] {
            known_names.insert(name.to_string());
        }
        Self {
            known_names,
            _max_entities: max_entities,
        }
    }

    pub fn extract_from_text(
        &self,
        text: &str,
        title: &str,
        source: EntitySource,
    ) -> Vec<KnowledgeEntity> {
        let mut entities = Vec::new();
        let combined = format!("{} {}", title, text);
        let lower = combined.to_lowercase();
        let now = chrono::Utc::now().timestamp_millis();

        entities.extend(self.extract_persons(&combined, &lower, now, &source));
        if let Some(e) = self.extract_place(&lower, now, &source) {
            entities.push(e);
        }
        if let Some(e) = self.extract_organization(&lower, now, &source) {
            entities.push(e);
        }
        entities.extend(self.extract_topics(&lower, now, &source));
        if let Some(e) = self.extract_document(&lower, now, &source) {
            entities.push(e);
        }
        if let Some(e) = self.extract_website(&lower, now, &source) {
            entities.push(e);
        }
        if let Some(e) = self.extract_event(&lower, now, &source) {
            entities.push(e);
        }
        if let Some(e) = self.extract_device(&lower, now, &source) {
            entities.push(e);
        }

        entities
    }

    pub fn extract_from_memory(&self, content: &str, title: &str) -> Vec<KnowledgeEntity> {
        self.extract_from_text(content, title, EntitySource::Memory)
    }

    pub fn extract_from_ocr(&self, text: &str) -> Vec<KnowledgeEntity> {
        let mut entities = self.extract_from_text(text, "", EntitySource::Ocr);
        let lower = text.to_lowercase();
        let now = chrono::Utc::now().timestamp_millis();
        if lower.contains("http") || lower.contains("www.") {
            entities.push(KnowledgeEntity {
                id: Uuid::new_v4().to_string(),
                name: "Website link".to_string(),
                entity_type: EntityType::Website,
                description: format!("URL found in OCR text: {}", &text[..text.len().min(100)]),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.6,
                source: EntitySource::Ocr,
                metadata: std::collections::HashMap::new(),
            });
        }
        entities
    }

    pub fn extract_from_screenshot_text(&self, text_parts: &[&str]) -> Vec<KnowledgeEntity> {
        let mut entities = Vec::new();
        let now = chrono::Utc::now().timestamp_millis();
        if !text_parts.is_empty() {
            let combined = text_parts.join(" ");
            entities.push(KnowledgeEntity {
                id: Uuid::new_v4().to_string(),
                name: format!("Screenshot: {}", &combined[..combined.len().min(80)]),
                entity_type: EntityType::Document,
                description: "Text extracted from screenshot analysis".to_string(),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.5,
                source: EntitySource::Screenshot,
                metadata: std::collections::HashMap::new(),
            });
            entities.extend(self.extract_from_text(&combined, "", EntitySource::Screenshot));
        }
        entities
    }

    pub fn extract_from_conversation(
        &self,
        text: &str,
        speaker: Option<&str>,
    ) -> Vec<KnowledgeEntity> {
        let mut entities = Vec::new();
        let now = chrono::Utc::now().timestamp_millis();
        if let Some(s) = speaker {
            entities.push(KnowledgeEntity {
                id: Uuid::new_v4().to_string(),
                name: s.to_string(),
                entity_type: EntityType::Person,
                description: "Speaker in conversation".to_string(),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.7,
                source: EntitySource::Conversation,
                metadata: std::collections::HashMap::new(),
            });
        }
        entities.extend(self.extract_from_text(text, "", EntitySource::Conversation));
        entities
    }

    pub fn extract_from_automation(
        &self,
        workflow_name: &str,
        action_type: &str,
    ) -> Vec<KnowledgeEntity> {
        let now = chrono::Utc::now().timestamp_millis();
        vec![KnowledgeEntity {
            id: Uuid::new_v4().to_string(),
            name: workflow_name.to_string(),
            entity_type: EntityType::Event,
            description: format!("Automation workflow of type: {}", action_type),
            aliases: vec![],
            first_seen: now,
            last_seen: now,
            mention_count: 1,
            confidence: 0.9,
            source: EntitySource::Automation,
            metadata: std::collections::HashMap::new(),
        }]
    }

    pub fn extract_from_plugin(&self, plugin_id: &str, plugin_name: &str) -> Vec<KnowledgeEntity> {
        let now = chrono::Utc::now().timestamp_millis();
        vec![KnowledgeEntity {
            id: Uuid::new_v4().to_string(),
            name: plugin_name.to_string(),
            entity_type: EntityType::Custom("plugin".to_string()),
            description: format!("Plugin: {} ({})", plugin_name, plugin_id),
            aliases: vec![plugin_id.to_string()],
            first_seen: now,
            last_seen: now,
            mention_count: 1,
            confidence: 0.9,
            source: EntitySource::Plugin,
            metadata: std::collections::HashMap::new(),
        }]
    }

    pub fn deduplicate(&self, entities: &[KnowledgeEntity]) -> Vec<KnowledgeEntity> {
        let mut seen = HashSet::new();
        let mut deduped = Vec::new();
        for e in entities {
            let key = (e.name.to_lowercase(), e.entity_type.as_str().to_string());
            if seen.insert(key) {
                deduped.push(e.clone());
            }
        }
        deduped
    }

    pub fn merge_entity(existing: &mut KnowledgeEntity, new: &KnowledgeEntity) {
        Self::merge_entity_impl(existing, new);
    }

    pub fn merge_entity_from_graph(
        existing: &mut crate::graph::GraphEntity,
        new: &KnowledgeEntity,
    ) {
        if new.last_seen > existing.last_seen {
            existing.last_seen = new.last_seen;
        }
        existing.mention_count = existing.mention_count.saturating_add(new.mention_count);
        existing.confidence = (existing.confidence + new.confidence) / 2.0;
        if !new.aliases.is_empty() {
            for alias in &new.aliases {
                if !existing.aliases.contains(alias) {
                    existing.aliases.push(alias.clone());
                }
            }
        }
        if new.description.len() > existing.description.len() {
            existing.description = new.description.clone();
        }
        if existing.first_seen > new.first_seen {
            existing.first_seen = new.first_seen;
        }
    }

    fn merge_entity_impl(existing: &mut KnowledgeEntity, new: &KnowledgeEntity) {
        if new.last_seen > existing.last_seen {
            existing.last_seen = new.last_seen;
        }
        existing.mention_count = existing.mention_count.saturating_add(new.mention_count);
        existing.confidence = (existing.confidence + new.confidence) / 2.0;
        if !new.aliases.is_empty() {
            for alias in &new.aliases {
                if !existing.aliases.contains(alias) {
                    existing.aliases.push(alias.clone());
                }
            }
        }
        if new.description.len() > existing.description.len() {
            existing.description = new.description.clone();
        }
        if existing.first_seen > new.first_seen {
            existing.first_seen = new.first_seen;
        }
    }

    fn extract_persons(
        &self,
        _text: &str,
        lower: &str,
        now: i64,
        source: &EntitySource,
    ) -> Vec<KnowledgeEntity> {
        let mut results = Vec::new();
        // Self-identification pattern: "my name is X" or "I'm X"
        if lower.contains("i'm") || lower.contains("my name") {
            let name = if let Some(pos) = lower.find("my name is ") {
                let rest = &lower[pos + 11..];
                rest.split_whitespace().next().unwrap_or("User").to_string()
            } else {
                "User".to_string()
            };
            results.push(KnowledgeEntity {
                id: Uuid::new_v4().to_string(),
                name,
                entity_type: EntityType::Person,
                description: "User identified from conversation".to_string(),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.8,
                source: source.clone(),
                metadata: std::collections::HashMap::new(),
            });
        }
        // Known names lookup: match any name in the known_names set
        for name in &self.known_names {
            let search = format!(" {} ", name);
            if (lower.contains(&search) || lower.ends_with(name) || lower.starts_with(name))
                && !results
                    .iter()
                    .any(|e: &KnowledgeEntity| e.name.to_lowercase() == *name)
            {
                results.push(KnowledgeEntity {
                    id: Uuid::new_v4().to_string(),
                    name: name[..1].to_uppercase() + &name[1..],
                    entity_type: EntityType::Person,
                    description: format!("Person mentioned: {}", name),
                    aliases: vec![name.clone()],
                    first_seen: now,
                    last_seen: now,
                    mention_count: 1,
                    confidence: 0.7,
                    source: source.clone(),
                    metadata: std::collections::HashMap::new(),
                });
            }
        }
        results
    }

    fn extract_place(
        &self,
        lower: &str,
        now: i64,
        source: &EntitySource,
    ) -> Option<KnowledgeEntity> {
        let place_names = [
            "home", "office", "school", "work", "gym", "store", "park", "city", "beach",
        ];
        for pn in &place_names {
            let search = &format!(" {}", pn);
            if lower.contains(search) {
                return Some(KnowledgeEntity {
                    id: Uuid::new_v4().to_string(),
                    name: pn.to_string(),
                    entity_type: EntityType::Place,
                    description: format!("Place mentioned: {}", pn),
                    aliases: vec![],
                    first_seen: now,
                    last_seen: now,
                    mention_count: 1,
                    confidence: 0.6,
                    source: source.clone(),
                    metadata: std::collections::HashMap::new(),
                });
            }
        }
        None
    }

    fn extract_organization(
        &self,
        lower: &str,
        now: i64,
        source: &EntitySource,
    ) -> Option<KnowledgeEntity> {
        let org_names = [
            "microsoft",
            "google",
            "apple",
            "amazon",
            "meta",
            "openai",
            "github",
            "tesla",
        ];
        for org in &org_names {
            if lower.contains(org) {
                return Some(KnowledgeEntity {
                    id: Uuid::new_v4().to_string(),
                    name: org.to_string(),
                    entity_type: EntityType::Organization,
                    description: format!("Organization mentioned: {}", org),
                    aliases: vec![],
                    first_seen: now,
                    last_seen: now,
                    mention_count: 1,
                    confidence: 0.8,
                    source: source.clone(),
                    metadata: std::collections::HashMap::new(),
                });
            }
        }
        None
    }

    fn extract_topics(&self, lower: &str, now: i64, source: &EntitySource) -> Vec<KnowledgeEntity> {
        let topic_map = [
            ("rust", "Rust"),
            ("python", "Python"),
            ("project", "Project"),
            ("idea", "Idea"),
            ("feature", "Feature"),
            ("design", "Design"),
            ("research", "Research"),
            ("learning", "Learning"),
            ("ai", "AI"),
            ("ml", "ML"),
            ("data", "Data"),
            ("security", "Security"),
            ("privacy", "Privacy"),
            ("performance", "Performance"),
        ];
        let mut entities = Vec::new();
        for (keyword, name) in &topic_map {
            if lower.contains(keyword) {
                entities.push(KnowledgeEntity {
                    id: Uuid::new_v4().to_string(),
                    name: name.to_string(),
                    entity_type: EntityType::Topic,
                    description: format!("Topic mentioned: {}", name),
                    aliases: vec![keyword.to_string()],
                    first_seen: now,
                    last_seen: now,
                    mention_count: 1,
                    confidence: 0.7,
                    source: source.clone(),
                    metadata: std::collections::HashMap::new(),
                });
            }
        }
        entities
    }

    fn extract_document(
        &self,
        lower: &str,
        now: i64,
        source: &EntitySource,
    ) -> Option<KnowledgeEntity> {
        if lower.contains("document") || lower.contains("file:") || lower.contains("note:") {
            Some(KnowledgeEntity {
                id: Uuid::new_v4().to_string(),
                name: "Document".to_string(),
                entity_type: EntityType::Document,
                description: "Document or file referenced".to_string(),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.6,
                source: source.clone(),
                metadata: std::collections::HashMap::new(),
            })
        } else {
            None
        }
    }

    fn extract_website(
        &self,
        lower: &str,
        now: i64,
        source: &EntitySource,
    ) -> Option<KnowledgeEntity> {
        if lower.contains("http") || lower.contains(".com") || lower.contains(".org") {
            Some(KnowledgeEntity {
                id: Uuid::new_v4().to_string(),
                name: "Website".to_string(),
                entity_type: EntityType::Website,
                description: "Website or URL referenced".to_string(),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.5,
                source: source.clone(),
                metadata: std::collections::HashMap::new(),
            })
        } else {
            None
        }
    }

    fn extract_event(
        &self,
        lower: &str,
        now: i64,
        source: &EntitySource,
    ) -> Option<KnowledgeEntity> {
        let event_keywords = [
            "meeting",
            "call",
            "appointment",
            "deadline",
            "birthday",
            "anniversary",
            "holiday",
            "conference",
            "event",
            "party",
        ];
        for ev in &event_keywords {
            if lower.contains(ev) {
                return Some(KnowledgeEntity {
                    id: Uuid::new_v4().to_string(),
                    name: ev.to_string(),
                    entity_type: EntityType::Event,
                    description: format!("Event mentioned: {}", ev),
                    aliases: vec![],
                    first_seen: now,
                    last_seen: now,
                    mention_count: 1,
                    confidence: 0.7,
                    source: source.clone(),
                    metadata: std::collections::HashMap::new(),
                });
            }
        }
        None
    }

    fn extract_device(
        &self,
        lower: &str,
        now: i64,
        source: &EntitySource,
    ) -> Option<KnowledgeEntity> {
        let device_keywords = [
            "phone", "laptop", "desktop", "tablet", "server", "watch", "tv", "speaker", "camera",
            "printer", "router",
        ];
        for d in &device_keywords {
            if lower.contains(d) {
                return Some(KnowledgeEntity {
                    id: Uuid::new_v4().to_string(),
                    name: d.to_string(),
                    entity_type: EntityType::Device,
                    description: format!("Device mentioned: {}", d),
                    aliases: vec![],
                    first_seen: now,
                    last_seen: now,
                    mention_count: 1,
                    confidence: 0.7,
                    source: source.clone(),
                    metadata: std::collections::HashMap::new(),
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_type_display() {
        assert_eq!(EntityType::Person.to_string(), "person");
        assert_eq!(EntityType::Place.to_string(), "place");
        assert_eq!(EntityType::Custom("test".into()).to_string(), "test");
    }

    #[test]
    fn test_extract_from_memory_basic() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_text(
            "Working on a Rust project at home",
            "Weekend work",
            EntitySource::Memory,
        );
        assert!(entities.iter().any(|e| e.name == "Rust"));
        assert!(entities.iter().any(|e| e.name == "home"));
        assert!(entities.iter().any(|e| e.name == "Project"));
    }

    #[test]
    fn test_extract_from_ocr_with_url() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_ocr("Visit https://example.com for info");
        assert!(entities
            .iter()
            .any(|e| e.entity_type == EntityType::Website));
    }

    #[test]
    fn test_deduplicate() {
        let extractor = EntityExtractor::new(100);
        let now = chrono::Utc::now().timestamp_millis();
        let e1 = KnowledgeEntity {
            id: "1".into(),
            name: "Rust".into(),
            entity_type: EntityType::Topic,
            description: "".into(),
            aliases: vec![],
            first_seen: now,
            last_seen: now,
            mention_count: 1,
            confidence: 0.8,
            source: EntitySource::Memory,
            metadata: std::collections::HashMap::new(),
        };
        let e2 = KnowledgeEntity {
            id: "2".into(),
            name: "Rust".into(),
            entity_type: EntityType::Topic,
            description: "".into(),
            aliases: vec![],
            first_seen: now,
            last_seen: now,
            mention_count: 1,
            confidence: 0.8,
            source: EntitySource::Memory,
            metadata: std::collections::HashMap::new(),
        };
        let deduped = extractor.deduplicate(&[e1, e2]);
        assert_eq!(deduped.len(), 1);
    }

    #[test]
    fn test_merge_entity() {
        let now = chrono::Utc::now().timestamp_millis();
        let mut existing = KnowledgeEntity {
            id: "1".into(),
            name: "Rust".into(),
            entity_type: EntityType::Topic,
            description: "lang".into(),
            aliases: vec!["rs".into()],
            first_seen: now,
            last_seen: now,
            mention_count: 1,
            confidence: 0.8,
            source: EntitySource::Memory,
            metadata: std::collections::HashMap::new(),
        };
        let new = KnowledgeEntity {
            id: "2".into(),
            name: "Rust".into(),
            entity_type: EntityType::Topic,
            description: "Rust programming language".into(),
            aliases: vec!["rust-lang".into()],
            first_seen: now,
            last_seen: now + 1000,
            mention_count: 3,
            confidence: 0.9,
            source: EntitySource::Memory,
            metadata: std::collections::HashMap::new(),
        };
        EntityExtractor::merge_entity(&mut existing, &new);
        assert_eq!(existing.mention_count, 4);
        assert!((existing.confidence - 0.85).abs() < 0.01);
        assert!(existing.aliases.contains(&"rust-lang".to_string()));
        assert_eq!(existing.last_seen, now + 1000);
    }

    #[test]
    fn test_extract_no_entities_from_empty() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_memory("", "");
        assert!(entities.is_empty());
    }

    #[test]
    fn test_extract_organization() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_memory("Works at Google on AI", "");
        assert!(entities.iter().any(|e| e.name.to_lowercase() == "google"));
    }

    #[test]
    fn test_extract_device() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_memory("My phone battery died", "");
        assert!(entities.iter().any(|e| e.name.to_lowercase() == "phone"));
    }

    #[test]
    fn test_entity_source_display() {
        assert_eq!(EntitySource::Memory.to_string(), "memory");
        assert_eq!(EntitySource::Plugin.to_string(), "plugin");
    }

    #[test]
    fn test_extract_from_conversation_with_speaker() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_conversation("Hello there", Some("Alice"));
        assert!(entities.iter().any(|e| e.name == "Alice"));
    }

    #[test]
    fn test_extract_from_automation() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_automation("BackupWorkflow", "scheduled");
        assert!(entities.iter().any(|e| e.name == "BackupWorkflow"));
    }

    #[test]
    fn test_extract_from_plugin() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_plugin("mem_reader", "Memory Reader");
        assert!(entities.iter().any(|e| e.name == "Memory Reader"));
    }

    #[test]
    fn test_extract_from_screenshot_text() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_screenshot_text(&["Submit", "Welcome", "Login"]);
        assert!(entities
            .iter()
            .any(|e| e.source == EntitySource::Screenshot));
    }

    #[test]
    fn test_extract_event() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_memory("Meeting at 3pm tomorrow", "");
        assert!(entities.iter().any(|e| e.name == "meeting"));
    }

    #[test]
    fn test_extract_person_from_text() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_memory("my name is Alice and I am here", "");
        assert!(entities.iter().any(|e| e.entity_type == EntityType::Person));
    }

    #[test]
    fn test_entity_type_as_str() {
        assert_eq!(EntityType::File.as_str(), "file");
        assert_eq!(EntityType::Image.as_str(), "image");
        assert_eq!(EntityType::Website.as_str(), "website");
    }

    #[test]
    fn test_extract_from_ocr_empty() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_ocr("");
        assert!(entities.is_empty());
    }

    #[test]
    fn test_extract_from_screenshot_empty() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_screenshot_text(&[]);
        assert!(entities.is_empty());
    }

    #[test]
    fn test_extract_from_conversation_no_speaker() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_conversation("Hello world", None);
        assert!(entities
            .iter()
            .all(|e| e.source != EntitySource::Conversation));
    }

    #[test]
    fn test_extract_device_laptop() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_memory("My laptop is slow today", "");
        assert!(entities.iter().any(|e| e.name == "laptop"));
    }

    #[test]
    fn test_extract_no_duplicate_persons() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_memory("alice and bob met with alice again", "");
        let alice_count = entities.iter().filter(|e| e.name == "Alice").count();
        assert!(alice_count <= 1);
    }

    #[test]
    fn test_extract_place_home() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_memory("At home today", "");
        assert!(entities.iter().any(|e| e.name == "home"));
    }

    #[test]
    fn test_extract_organization_google() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_memory("Google products are great", "");
        assert!(entities.iter().any(|e| e.name.to_lowercase() == "google"));
    }

    #[test]
    fn test_entity_source_from_vision() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_text("test", "test", EntitySource::Vision);
        assert!(entities.iter().all(|e| e.source == EntitySource::Vision));
    }

    #[test]
    fn test_extract_topic_privacy() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_memory("privacy is important", "");
        assert!(entities.iter().any(|e| e.name == "Privacy"));
    }

    #[test]
    fn test_extract_topic_security() {
        let extractor = EntityExtractor::new(100);
        let entities = extractor.extract_from_memory("security audit needed", "");
        assert!(entities.iter().any(|e| e.name == "Security"));
    }

    #[test]
    fn test_entity_deduplicate_multiple() {
        let extractor = EntityExtractor::new(100);
        let now = chrono::Utc::now().timestamp_millis();
        let entities = vec![
            KnowledgeEntity {
                id: "1".into(),
                name: "Rust".into(),
                entity_type: EntityType::Topic,
                description: "".into(),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.8,
                source: EntitySource::Memory,
                metadata: std::collections::HashMap::new(),
            },
            KnowledgeEntity {
                id: "2".into(),
                name: "Rust".into(),
                entity_type: EntityType::Topic,
                description: "".into(),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.8,
                source: EntitySource::Memory,
                metadata: std::collections::HashMap::new(),
            },
            KnowledgeEntity {
                id: "3".into(),
                name: "Python".into(),
                entity_type: EntityType::Topic,
                description: "".into(),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.8,
                source: EntitySource::Memory,
                metadata: std::collections::HashMap::new(),
            },
            KnowledgeEntity {
                id: "4".into(),
                name: "Python".into(),
                entity_type: EntityType::Topic,
                description: "".into(),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.8,
                source: EntitySource::Memory,
                metadata: std::collections::HashMap::new(),
            },
        ];
        let deduped = extractor.deduplicate(&entities);
        assert_eq!(deduped.len(), 2);
    }

    #[test]
    fn test_merge_entity_from_graph() {
        let now = chrono::Utc::now().timestamp_millis();
        let mut existing = crate::graph::GraphEntity {
            id: "1".into(),
            name: "Rust".into(),
            entity_type: EntityType::Topic,
            description: "lang".into(),
            aliases: vec!["rs".into()],
            first_seen: now,
            last_seen: now,
            mention_count: 1,
            confidence: 0.8,
            metadata: std::collections::HashMap::new(),
        };
        let new = KnowledgeEntity {
            id: "2".into(),
            name: "Rust".into(),
            entity_type: EntityType::Topic,
            description: "Rust programming language".into(),
            aliases: vec!["rust-lang".into()],
            first_seen: now,
            last_seen: now + 1000,
            mention_count: 3,
            confidence: 0.9,
            source: EntitySource::Memory,
            metadata: std::collections::HashMap::new(),
        };
        EntityExtractor::merge_entity_from_graph(&mut existing, &new);
        assert_eq!(existing.mention_count, 4);
        assert!(existing.aliases.contains(&"rust-lang".to_string()));
    }
}
