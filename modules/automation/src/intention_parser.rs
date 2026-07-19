use std::collections::HashMap;

/// The type of intent parsed from natural language.
#[derive(Debug, Clone, PartialEq)]
pub enum IntentType {
    OpenApplication,
    CloseApplication,
    Search,
    Navigate,
    Click,
    Type,
    Scroll,
    Drag,
    Swipe,
    Wait,
    Speak,
    DeviceControl,
    BrowserAction,
    FileAction,
    SystemAction,
    MultiStepGoal,
    Unknown,
}

/// A single parameter extracted from the input.
#[derive(Debug, Clone, PartialEq)]
pub struct IntentParameter {
    pub key: String,
    pub value: String,
}

impl IntentParameter {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// A parsed intent with type, target, parameters, and confidence.
#[derive(Debug, Clone, PartialEq)]
pub struct Intent {
    pub intent_type: IntentType,
    pub target: Option<String>,
    pub parameters: Vec<IntentParameter>,
    pub confidence: IntentConfidence,
    pub original_text: String,
}

impl Intent {
    pub fn new(intent_type: IntentType, original_text: impl Into<String>) -> Self {
        Self {
            intent_type,
            target: None,
            parameters: Vec::new(),
            confidence: IntentConfidence::low(),
            original_text: original_text.into(),
        }
    }

    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn with_parameter(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.parameters.push(IntentParameter::new(key, value));
        self
    }

    pub fn with_confidence(mut self, confidence: IntentConfidence) -> Self {
        self.confidence = confidence;
        self
    }
}

/// Confidence score for a parsed intent.
#[derive(Debug, Clone, PartialEq)]
pub struct IntentConfidence {
    pub score: f32,
    pub reasoning: Option<String>,
}

impl IntentConfidence {
    pub fn new(score: f32, reasoning: impl Into<String>) -> Self {
        Self {
            score: score.clamp(0.0, 1.0),
            reasoning: Some(reasoning.into()),
        }
    }

    pub fn certain() -> Self {
        Self {
            score: 1.0,
            reasoning: Some("exact match".into()),
        }
    }

    pub fn high() -> Self {
        Self {
            score: 0.85,
            reasoning: Some("strong keyword match".into()),
        }
    }

    pub fn medium() -> Self {
        Self {
            score: 0.65,
            reasoning: Some("partial keyword match".into()),
        }
    }

    pub fn low() -> Self {
        Self {
            score: 0.35,
            reasoning: Some("weak or fallback match".into()),
        }
    }

    pub fn minimal() -> Self {
        Self {
            score: 0.1,
            reasoning: Some("unrecognized input".into()),
        }
    }
}

/// Context information for a parsed intent.
#[derive(Debug, Clone, PartialEq)]
pub struct IntentContext {
    pub source: String,
    pub timestamp: i64,
    pub application_state: Option<String>,
}

impl IntentContext {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            application_state: None,
        }
    }

    pub fn with_timestamp(mut self, ts: i64) -> Self {
        self.timestamp = ts;
        self
    }

    pub fn with_application_state(mut self, state: impl Into<String>) -> Self {
        self.application_state = Some(state.into());
        self
    }
}

/// The result of parsing a natural language input.
#[derive(Debug, Clone, PartialEq)]
pub enum IntentParseResult {
    Single(Intent),
    Multi(Vec<Intent>),
    Unknown(String),
}

impl IntentParseResult {
    pub fn intents(&self) -> Vec<&Intent> {
        match self {
            IntentParseResult::Single(i) => vec![i],
            IntentParseResult::Multi(v) => v.iter().collect(),
            IntentParseResult::Unknown(_) => vec![],
        }
    }

    pub fn is_unknown(&self) -> bool {
        matches!(self, IntentParseResult::Unknown(_))
    }

    pub fn confidence(&self) -> f32 {
        match self {
            IntentParseResult::Single(i) => i.confidence.score,
            IntentParseResult::Multi(v) => {
                if v.is_empty() {
                    0.0
                } else {
                    v.iter().map(|i| i.confidence.score).sum::<f32>() / v.len() as f32
                }
            }
            IntentParseResult::Unknown(_) => 0.0,
        }
    }
}

// ── Synonym / Keyword maps ──

struct IntentPatterns {
    open_app: &'static [&'static str],
    close_app: &'static [&'static str],
    search: &'static [&'static str],
    navigate: &'static [&'static str],
    click: &'static [&'static str],
    r#type: &'static [&'static str],
    scroll: &'static [&'static str],
    drag: &'static [&'static str],
    swipe: &'static [&'static str],
    wait: &'static [&'static str],
    speak: &'static [&'static str],
    device_control: &'static [&'static str],
    browser: &'static [&'static str],
    file: &'static [&'static str],
    system: &'static [&'static str],
}

const PATTERNS: IntentPatterns = IntentPatterns {
    open_app: &["open ", "launch ", "start ", "run ", "load "],
    close_app: &["close ", "quit ", "exit ", "terminate ", "kill "],
    search: &["search ", "find ", "look for ", "look up ", "query "],
    navigate: &["go to ", "navigate ", "open url", "goto "],
    click: &["click ", "tap ", "press ", "select ", "choose "],
    r#type: &["type ", "enter ", "input ", "write "],
    scroll: &["scroll ", "scroll up", "scroll down", "scroll to"],
    drag: &["drag "],
    swipe: &["swipe "],
    wait: &["wait ", "pause ", "sleep ", "delay "],
    speak: &["speak ", "say ", "tell ", "announce "],
    device_control: &[
        "brightness",
        "dim ",
        "dimmer",
        "volume",
        "sound",
        "mute",
        "wifi",
        "wi-fi",
        "bluetooth",
        "dnd",
        "do not disturb",
        "lock ",
        "power save",
        "power saving",
    ],
    browser: &[
        "browser",
        "google",
        "search on ",
        "open in browser",
        "bing ",
        "youtube",
    ],
    file: &[
        "open file",
        "save file",
        "create file",
        "delete file",
        "rename file",
        "copy file",
        "move file",
        "new file",
    ],
    system: &[
        "shutdown",
        "restart",
        "reboot",
        "sleep",
        "hibernate",
        "log off",
    ],
};

// ── DeviceControl subtype specification ──

#[derive(Debug, Clone, PartialEq)]
enum DeviceControlSubtype {
    SetBrightness(Option<u32>),
    SetVolume(Option<u32>),
    ToggleWiFi(Option<bool>),
    ToggleBluetooth(Option<bool>),
    ToggleDND(Option<bool>),
    LockScreen,
    PowerSave(Option<bool>),
}

fn classify_device_control(text: &str) -> Option<(DeviceControlSubtype, f32)> {
    let lower = text.to_lowercase();

    if lower.contains("brightness") || lower.contains("dim") || lower.contains("dimmer") {
        if lower.contains("max") || lower.contains("full") {
            return Some((DeviceControlSubtype::SetBrightness(Some(100)), 0.95));
        }
        if lower.contains("min") || lower.contains("lowest") {
            return Some((DeviceControlSubtype::SetBrightness(Some(0)), 0.95));
        }
        let value = extract_number(text, "brightness").or_else(|| extract_number(text, "to"));
        return Some((
            DeviceControlSubtype::SetBrightness(value),
            if value.is_some() { 0.95 } else { 0.8 },
        ));
    }

    if lower.contains("volume") || lower.contains("sound") {
        if lower.contains("up") || lower.contains("increase") || lower.contains("higher") {
            let value = extract_number(text, "to").unwrap_or(70);
            return Some((DeviceControlSubtype::SetVolume(Some(value.min(100))), 0.9));
        }
        if lower.contains("down") || lower.contains("decrease") || lower.contains("lower") {
            let value = extract_number(text, "to").unwrap_or(30);
            return Some((DeviceControlSubtype::SetVolume(Some(value.min(100))), 0.9));
        }
        let value = extract_number(text, "volume").or_else(|| extract_number(text, "to"));
        return Some((
            DeviceControlSubtype::SetVolume(value),
            if value.is_some() { 0.95 } else { 0.85 },
        ));
    }

    if lower.contains("mute") {
        return Some((DeviceControlSubtype::SetVolume(Some(0)), 0.95));
    }

    if lower.contains("wifi") || lower.contains("wi-fi") {
        let enable = !lower.contains("off") && !lower.contains("disable");
        return Some((DeviceControlSubtype::ToggleWiFi(Some(enable)), 0.9));
    }

    if lower.contains("bluetooth") {
        let enable = !lower.contains("off") && !lower.contains("disable");
        return Some((DeviceControlSubtype::ToggleBluetooth(Some(enable)), 0.9));
    }

    if lower.contains("dnd") || lower.contains("do not disturb") {
        let enable = !lower.contains("off") && !lower.contains("disable");
        return Some((DeviceControlSubtype::ToggleDND(Some(enable)), 0.9));
    }

    if lower.contains("lock") {
        return Some((DeviceControlSubtype::LockScreen, 0.9));
    }

    if lower.contains("power save") || lower.contains("power saving") {
        let enable = !lower.contains("off") && !lower.contains("disable");
        return Some((DeviceControlSubtype::PowerSave(Some(enable)), 0.85));
    }

    None
}

// ── Parser ──

/// Rule-based natural language intent parser.
///
/// Converts natural language text into structured `Intent` values without AI.
/// Supports 16 intent types, synonym normalization, parameter extraction,
/// confidence scoring, and multi-step parsing.
pub struct IntentParser {
    _private: (),
}

impl IntentParser {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Parse a single natural language input into an intent.
    pub fn parse(&self, text: &str) -> IntentParseResult {
        let normalized = self.normalize(text);
        if normalized.is_empty() {
            return IntentParseResult::Unknown("empty input".into());
        }

        // Check for multi-step (contains "and" or "then" between actions)
        if self.is_multi_step(&normalized) {
            let parts = self.split_multi_step(&normalized);
            let mut intents: Vec<Intent> = Vec::new();
            for part in parts {
                if part.is_empty() {
                    continue;
                }
                match self.parse_single(&part) {
                    IntentParseResult::Single(i) => intents.push(i),
                    IntentParseResult::Multi(mut v) => intents.append(&mut v),
                    IntentParseResult::Unknown(_) => {}
                }
            }
            if intents.is_empty() {
                return IntentParseResult::Unknown(format!(
                    "could not parse any step in: {}",
                    text
                ));
            }
            if intents.len() == 1 {
                // Only one actual intent despite connectors
                return IntentParseResult::Single(intents.into_iter().next().unwrap());
            }
            let combined_text = intents
                .iter()
                .map(|i| i.original_text.clone())
                .collect::<Vec<_>>()
                .join(" ");
            let mut multi = Intent::new(IntentType::MultiStepGoal, &combined_text);
            multi.confidence = IntentConfidence::new(
                intents.iter().map(|i| i.confidence.score).sum::<f32>() / intents.len() as f32,
                "multi-step goal",
            );
            multi.parameters = intents.iter().flat_map(|i| i.parameters.clone()).collect();
            return IntentParseResult::Multi(intents);
        }

        self.parse_single(&normalized)
    }

    /// Parse a single normalized input (no multi-step splitting).
    fn parse_single(&self, text: &str) -> IntentParseResult {
        // Try device control first (has specific sub-type matching)
        if let Some((subtype, conf)) = classify_device_control(text) {
            let mut intent = Intent::new(IntentType::DeviceControl, text);
            intent.confidence = IntentConfidence::new(conf, "device control keyword match");
            match subtype {
                DeviceControlSubtype::SetBrightness(v) => {
                    intent.target = Some("brightness".into());
                    if let Some(val) = v {
                        intent
                            .parameters
                            .push(IntentParameter::new("value", val.to_string()));
                    }
                }
                DeviceControlSubtype::SetVolume(v) => {
                    intent.target = Some("volume".into());
                    if let Some(val) = v {
                        intent
                            .parameters
                            .push(IntentParameter::new("value", val.to_string()));
                    }
                }
                DeviceControlSubtype::ToggleWiFi(v) => {
                    intent.target = Some("wifi".into());
                    if let Some(val) = v {
                        intent
                            .parameters
                            .push(IntentParameter::new("enabled", val.to_string()));
                    }
                }
                DeviceControlSubtype::ToggleBluetooth(v) => {
                    intent.target = Some("bluetooth".into());
                    if let Some(val) = v {
                        intent
                            .parameters
                            .push(IntentParameter::new("enabled", val.to_string()));
                    }
                }
                DeviceControlSubtype::ToggleDND(v) => {
                    intent.target = Some("dnd".into());
                    if let Some(val) = v {
                        intent
                            .parameters
                            .push(IntentParameter::new("enabled", val.to_string()));
                    }
                }
                DeviceControlSubtype::LockScreen => {
                    intent.target = Some("lock".into());
                }
                DeviceControlSubtype::PowerSave(v) => {
                    intent.target = Some("power_save".into());
                    if let Some(val) = v {
                        intent
                            .parameters
                            .push(IntentParameter::new("enabled", val.to_string()));
                    }
                }
            }
            return IntentParseResult::Single(intent);
        }

        // Check each intent type in priority order
        type IntentCheck = (fn(&str) -> Option<Intent>, &'static str);
        let checks: Vec<IntentCheck> = vec![
            (try_file_action, "file action"),
            (try_open_app, "open application"),
            (try_close_app, "close application"),
            (try_browser_action, "browser action"),
            (try_search, "search"),
            (try_navigate, "navigate"),
            (try_click, "click"),
            (try_type, "type"),
            (try_scroll, "scroll"),
            (try_drag, "drag"),
            (try_swipe, "swipe"),
            (try_wait, "wait"),
            (try_speak, "speak"),
            (try_system_action, "system action"),
        ];

        for (check_fn, _) in checks {
            if let Some(intent) = check_fn(text) {
                return IntentParseResult::Single(intent);
            }
        }

        IntentParseResult::Unknown(format!("unrecognized input: {}", text))
    }

    /// Parse multiple inputs, returning a list of results.
    pub fn parse_many(&self, texts: &[&str]) -> Vec<IntentParseResult> {
        texts.iter().map(|t| self.parse(t)).collect()
    }

    /// Normalize input text for consistent matching.
    pub fn normalize(&self, text: &str) -> String {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        let collapsed: String = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
        collapsed.to_lowercase()
    }

    /// Estimate confidence for a given text against all intent types.
    pub fn confidence(&self, text: &str) -> HashMap<String, f32> {
        let normalized = self.normalize(text);
        let mut scores: HashMap<String, f32> = HashMap::new();

        if normalized.is_empty() {
            scores.insert("unknown".into(), 1.0);
            return scores;
        }

        if let Some((_, conf)) = classify_device_control(&normalized) {
            scores.insert("device_control".into(), conf);
        }

        let patterns: Vec<(&str, &[&str])> = vec![
            ("open_application", PATTERNS.open_app),
            ("close_application", PATTERNS.close_app),
            ("search", PATTERNS.search),
            ("navigate", PATTERNS.navigate),
            ("click", PATTERNS.click),
            ("type", PATTERNS.r#type),
            ("scroll", PATTERNS.scroll),
            ("drag", PATTERNS.drag),
            ("swipe", PATTERNS.swipe),
            ("wait", PATTERNS.wait),
            ("speak", PATTERNS.speak),
            ("browser_action", PATTERNS.browser),
            ("file_action", PATTERNS.file),
            ("system_action", PATTERNS.system),
        ];

        for (name, keywords) in &patterns {
            let score = keywords.iter().fold(0.0f32, |acc, kw| {
                if normalized.contains(kw.trim_end()) {
                    (acc + 1.0).min(1.0)
                } else {
                    acc
                }
            });
            if score > 0.0 {
                let conf = if normalized.starts_with(keywords[0].trim_end()) {
                    0.9
                } else {
                    0.6
                };
                scores.insert(name.to_string(), conf);
            }
        }

        // Multi-step detection
        if self.is_multi_step(&normalized) {
            scores.insert("multi_step_goal".into(), 0.85);
        }

        if scores.is_empty() {
            scores.insert("unknown".into(), 1.0);
        }

        scores
    }

    /// Return a list of supported intent type descriptions.
    pub fn supported_intents(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            (
                "open_application",
                "Open, launch, start, or run an application",
            ),
            (
                "close_application",
                "Close, quit, exit, or terminate an application",
            ),
            ("search", "Search, find, look for, or query information"),
            ("navigate", "Go to a URL, navigate, or open a web address"),
            ("click", "Click, tap, press, select, or choose an element"),
            ("type", "Type, enter, input, or write text"),
            ("scroll", "Scroll up, down, or to a position"),
            ("drag", "Drag an element from one position to another"),
            ("swipe", "Swipe in a direction"),
            ("wait", "Wait, pause, sleep, or delay for a duration"),
            ("speak", "Speak, say, tell, or announce text"),
            (
                "device_control",
                "Control device settings: brightness, volume, wifi, bluetooth, DND, lock, power",
            ),
            (
                "browser_action",
                "Browser-related actions: search on Google, open in browser",
            ),
            (
                "file_action",
                "File operations: open, save, create, delete, rename files",
            ),
            (
                "system_action",
                "System operations: shutdown, restart, sleep, hibernate",
            ),
            (
                "multi_step_goal",
                "Compound goals with multiple steps connected by 'and' or 'then'",
            ),
        ]
    }

    // ── Internal helpers ──

    fn is_multi_step(&self, text: &str) -> bool {
        // Check for action connectors between two recognizable intents
        let connectors = &[" and ", " then ", ", and ", "; "];
        connectors
            .iter()
            .any(|c| text.contains(c) && self.count_actions(text) > 1)
    }

    fn count_actions(&self, text: &str) -> usize {
        let mut count = 0usize;
        let lower = text.to_lowercase();

        // Count recognizable action prefixes
        let prefixes: &[&[&str]] = &[
            PATTERNS.open_app,
            PATTERNS.close_app,
            PATTERNS.search,
            PATTERNS.navigate,
            PATTERNS.click,
            PATTERNS.r#type,
            PATTERNS.scroll,
            PATTERNS.drag,
            PATTERNS.swipe,
            PATTERNS.wait,
            PATTERNS.speak,
            PATTERNS.device_control,
            PATTERNS.browser,
            PATTERNS.file,
            PATTERNS.system,
        ];

        for group in prefixes {
            for kw in *group {
                if lower.contains(kw.trim_end()) {
                    count += 1;
                    break;
                }
            }
        }

        count
    }

    fn split_multi_step(&self, text: &str) -> Vec<String> {
        let mut parts = Vec::new();

        // Split on connectors with lookahead for action words
        let mut current = String::new();
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut i = 0;

        let is_connector =
            |w: &str| matches!(w, "and" | "then" | "&" | "subsequently" | "afterwards");

        while i < words.len() {
            if is_connector(words[i]) && !current.is_empty() {
                if i + 1 < words.len() {
                    let next = words[i + 1];
                    let is_new_action = PATTERNS
                        .open_app
                        .iter()
                        .any(|p| next.starts_with(p.trim_end()))
                        || PATTERNS
                            .close_app
                            .iter()
                            .any(|p| next.starts_with(p.trim_end()))
                        || PATTERNS
                            .search
                            .iter()
                            .any(|p| next.starts_with(p.trim_end()))
                        || PATTERNS
                            .click
                            .iter()
                            .any(|p| next.starts_with(p.trim_end()))
                        || PATTERNS
                            .r#type
                            .iter()
                            .any(|p| next.starts_with(p.trim_end()))
                        || PATTERNS
                            .scroll
                            .iter()
                            .any(|p| next.starts_with(p.trim_end()));
                    if is_new_action {
                        parts.push(current.trim().to_string());
                        current = String::new();
                        i += 1;
                        continue;
                    }
                }
                current.push(' ');
                current.push_str(words[i]);
            } else {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(words[i]);
            }
            i += 1;
        }

        if !current.trim().is_empty() {
            parts.push(current.trim().to_string());
        }

        // Also split on "; " and ", and "
        let mut refined = Vec::new();
        for part in &parts {
            let sub_parts: Vec<&str> = part.split("; ").collect();
            for sub in sub_parts {
                let sub_sub: Vec<&str> = sub.split(", and ").collect();
                for s in sub_sub {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        refined.push(trimmed.to_string());
                    }
                }
            }
        }

        if !refined.is_empty() {
            refined
        } else {
            parts
        }
    }
}

impl Default for IntentParser {
    fn default() -> Self {
        Self::new()
    }
}

// ── Intent matching functions ──

fn try_open_app(text: &str) -> Option<Intent> {
    let lower = text.to_lowercase();
    for prefix in PATTERNS.open_app {
        if lower.starts_with(prefix.trim_end()) {
            let app = extract_action_target(text, prefix).unwrap_or("application");
            let mut intent = Intent::new(IntentType::OpenApplication, text);
            intent.target = Some(app.to_string());
            intent
                .parameters
                .push(IntentParameter::new("application", app));
            intent.confidence = IntentConfidence::new(0.9, "open app keyword match");
            return Some(intent);
        }
    }
    None
}

fn try_close_app(text: &str) -> Option<Intent> {
    let lower = text.to_lowercase();
    for prefix in PATTERNS.close_app {
        if lower.starts_with(prefix.trim_end()) || lower.contains(prefix.trim_end()) {
            let app = extract_action_target(text, prefix).unwrap_or("application");
            let mut intent = Intent::new(IntentType::CloseApplication, text);
            intent.target = Some(app.to_string());
            intent
                .parameters
                .push(IntentParameter::new("application", app));
            intent.confidence = IntentConfidence::new(0.9, "close app keyword match");
            return Some(intent);
        }
    }
    None
}

fn try_search(text: &str) -> Option<Intent> {
    let lower = text.to_lowercase();
    for prefix in PATTERNS.search {
        if lower.starts_with(prefix.trim_end()) || lower.contains(prefix.trim_end()) {
            let query = extract_quoted(text)
                .or_else(|| extract_action_target(text, prefix))
                .unwrap_or("search query");
            let mut intent = Intent::new(IntentType::Search, text);
            intent.target = Some(query.to_string());
            intent.parameters.push(IntentParameter::new("query", query));
            intent.confidence = IntentConfidence::new(0.9, "search keyword match");
            return Some(intent);
        }
    }
    None
}

fn try_navigate(text: &str) -> Option<Intent> {
    let lower = text.to_lowercase();
    for prefix in PATTERNS.navigate {
        if lower.starts_with(prefix.trim_end()) || lower.contains(prefix.trim_end()) {
            let dest = extract_action_target(text, prefix).unwrap_or("destination");
            let mut intent = Intent::new(IntentType::Navigate, text);
            intent.target = Some(dest.to_string());
            // Detect if destination looks like a URL
            if dest.contains('.') && !dest.contains(' ') {
                intent.parameters.push(IntentParameter::new("url", dest));
            } else {
                intent
                    .parameters
                    .push(IntentParameter::new("destination", dest));
            }
            intent.confidence = IntentConfidence::new(0.85, "navigate keyword match");
            return Some(intent);
        }
    }
    None
}

fn try_click(text: &str) -> Option<Intent> {
    let lower = text.to_lowercase();
    for prefix in PATTERNS.click {
        if lower.starts_with(prefix.trim_end()) || lower.contains(prefix.trim_end()) {
            let target = extract_quoted(text)
                .or_else(|| extract_action_target(text, prefix))
                .unwrap_or("element");
            let mut intent = Intent::new(IntentType::Click, text);
            intent.target = Some(target.to_string());
            intent
                .parameters
                .push(IntentParameter::new("target", target));
            intent.confidence = IntentConfidence::new(0.85, "click keyword match");
            return Some(intent);
        }
    }
    None
}

fn try_type(text: &str) -> Option<Intent> {
    let lower = text.to_lowercase();
    for prefix in PATTERNS.r#type {
        if lower.starts_with(prefix.trim_end()) || lower.contains(prefix.trim_end()) {
            let content = extract_quoted(text)
                .or_else(|| extract_action_target(text, prefix))
                .unwrap_or("text");
            let mut intent = Intent::new(IntentType::Type, text);
            intent.target = Some(content.to_string());
            intent
                .parameters
                .push(IntentParameter::new("text", content));
            intent.confidence = IntentConfidence::new(0.85, "type keyword match");
            return Some(intent);
        }
    }
    None
}

fn try_scroll(text: &str) -> Option<Intent> {
    let lower = text.to_lowercase();
    for prefix in PATTERNS.scroll {
        if lower.contains(prefix.trim_end()) {
            let direction = if lower.contains("up") {
                "up"
            } else if lower.contains("down") {
                "down"
            } else if lower.contains("to") {
                let target = extract_action_target(text, "scroll to")
                    .or_else(|| extract_action_target(text, "scroll"));
                target.unwrap_or("down")
            } else {
                "down"
            };
            let mut intent = Intent::new(IntentType::Scroll, text);
            intent
                .parameters
                .push(IntentParameter::new("direction", direction));
            if let Some(target) = extract_action_target(text, "scroll to")
                .or_else(|| extract_action_target(text, "to"))
            {
                intent.target = Some(target.to_string());
                intent
                    .parameters
                    .push(IntentParameter::new("target", target));
            }
            intent.confidence = IntentConfidence::new(0.8, "scroll keyword match");
            return Some(intent);
        }
    }
    None
}

fn try_drag(text: &str) -> Option<Intent> {
    let lower = text.to_lowercase();
    if lower.contains("drag") {
        let mut intent = Intent::new(IntentType::Drag, text);
        intent.confidence = IntentConfidence::new(0.75, "drag keyword match");
        // Handle "from X to Y" pattern
        if let Some(from_full) = extract_after(text, "from") {
            if let Some(to_pos) = from_full.to_lowercase().find(" to ") {
                let from_val = from_full[..to_pos].trim();
                let to_val = from_full[to_pos + 4..]
                    .trim()
                    .trim_end_matches(['.', ',', '!']);
                intent
                    .parameters
                    .push(IntentParameter::new("from", from_val));
                if !to_val.is_empty() {
                    intent.parameters.push(IntentParameter::new("to", to_val));
                }
            } else {
                intent
                    .parameters
                    .push(IntentParameter::new("from", from_full));
            }
        } else if let Some(target) = extract_action_target(text, "drag ") {
            intent.target = Some(target.to_string());
            intent
                .parameters
                .push(IntentParameter::new("target", target));
        }
        Some(intent)
    } else {
        None
    }
}

fn try_swipe(text: &str) -> Option<Intent> {
    let lower = text.to_lowercase();
    if lower.contains("swipe") || lower.contains("slide") {
        let direction = if lower.contains("left") {
            "left"
        } else if lower.contains("right") {
            "right"
        } else if lower.contains("up") {
            "up"
        } else if lower.contains("down") {
            "down"
        } else {
            "right"
        };
        let mut intent = Intent::new(IntentType::Swipe, text);
        intent
            .parameters
            .push(IntentParameter::new("direction", direction));
        intent.confidence = IntentConfidence::new(0.8, "swipe keyword match");
        Some(intent)
    } else {
        None
    }
}

fn try_wait(text: &str) -> Option<Intent> {
    let lower = text.to_lowercase();
    for prefix in PATTERNS.wait {
        if lower.starts_with(prefix.trim_end()) || lower.contains(prefix.trim_end()) {
            let duration = extract_number(text, "for")
                .or_else(|| extract_number(text, "wait"))
                .or_else(|| extract_number(text, "seconds"))
                .or_else(|| extract_number(text, "sec"))
                .unwrap_or(1);
            let unit = if lower.contains("ms") || lower.contains("millisecond") {
                "ms"
            } else if lower.contains("min") || lower.contains("minute") {
                "minutes"
            } else {
                "seconds"
            };
            let mut intent = Intent::new(IntentType::Wait, text);
            intent
                .parameters
                .push(IntentParameter::new("duration", duration.to_string()));
            intent.parameters.push(IntentParameter::new("unit", unit));
            intent.target = Some(format!("{}{}", duration, unit));
            intent.confidence = IntentConfidence::new(0.85, "wait keyword match");
            return Some(intent);
        }
    }
    None
}

fn try_speak(text: &str) -> Option<Intent> {
    let lower = text.to_lowercase();
    for prefix in PATTERNS.speak {
        if lower.starts_with(prefix.trim_end()) || lower.contains(prefix.trim_end()) {
            let msg = extract_quoted(text)
                .or_else(|| extract_action_target(text, prefix))
                .unwrap_or("message");
            let mut intent = Intent::new(IntentType::Speak, text);
            intent.target = Some(msg.to_string());
            intent.parameters.push(IntentParameter::new("text", msg));
            intent.confidence = IntentConfidence::new(0.85, "speak keyword match");
            return Some(intent);
        }
    }
    None
}

fn try_browser_action(text: &str) -> Option<Intent> {
    let lower = text.to_lowercase();
    // "search X on Y" pattern
    if lower.contains("search") && (lower.contains("on ") || lower.contains("using ")) {
        let query = extract_quoted(text)
            .or_else(|| extract_action_target(text, "search"))
            .unwrap_or("query");
        let engine = if lower.contains("google") {
            "google"
        } else if lower.contains("bing") {
            "bing"
        } else if lower.contains("youtube") {
            "youtube"
        } else if lower.contains("duckduckgo") {
            "duckduckgo"
        } else {
            "browser"
        };
        let mut intent = Intent::new(IntentType::BrowserAction, text);
        intent.target = Some(query.to_string());
        intent.parameters.push(IntentParameter::new("query", query));
        intent
            .parameters
            .push(IntentParameter::new("engine", engine));
        intent.confidence = IntentConfidence::new(0.85, "browser search keyword match");
        return Some(intent);
    }

    for prefix in PATTERNS.browser {
        if lower.contains(prefix.trim_end()) {
            let query = extract_quoted(text)
                .or_else(|| extract_action_target(text, prefix))
                .unwrap_or("query");
            let mut intent = Intent::new(IntentType::BrowserAction, text);
            intent.target = Some(query.to_string());
            intent.parameters.push(IntentParameter::new("query", query));
            intent.confidence = IntentConfidence::new(0.75, "browser keyword match");
            return Some(intent);
        }
    }
    None
}

fn try_file_action(text: &str) -> Option<Intent> {
    let lower = text.to_lowercase();
    for prefix in PATTERNS.file {
        if lower.contains(prefix.trim_end()) {
            let operation = if prefix.starts_with("open") {
                "open"
            } else if prefix.starts_with("save")
                || prefix.starts_with("create")
                || prefix.starts_with("new")
            {
                "create"
            } else if prefix.starts_with("delete") || prefix.starts_with("remove") {
                "delete"
            } else if prefix.starts_with("rename") {
                "rename"
            } else if prefix.starts_with("copy") {
                "copy"
            } else if prefix.starts_with("move") {
                "move"
            } else {
                "open"
            };
            let path = extract_quoted(text)
                .or_else(|| extract_action_target(text, prefix))
                .unwrap_or("file");
            let mut intent = Intent::new(IntentType::FileAction, text);
            intent.target = Some(path.to_string());
            intent
                .parameters
                .push(IntentParameter::new("operation", operation));
            intent.parameters.push(IntentParameter::new("path", path));
            intent.confidence = IntentConfidence::new(0.8, "file action keyword match");
            return Some(intent);
        }
    }
    None
}

fn try_system_action(text: &str) -> Option<Intent> {
    let lower = text.to_lowercase();
    for prefix in PATTERNS.system {
        if lower.contains(prefix.trim_end()) {
            let action = if lower.contains("shutdown") || lower.contains("power off") {
                "shutdown"
            } else if lower.contains("restart") || lower.contains("reboot") {
                "restart"
            } else if lower.contains("sleep") {
                "sleep"
            } else if lower.contains("hibernate") {
                "hibernate"
            } else if lower.contains("log off")
                || lower.contains("sign out")
                || lower.contains("logout")
            {
                "logoff"
            } else {
                "shutdown"
            };
            let mut intent = Intent::new(IntentType::SystemAction, text);
            intent.target = Some(action.to_string());
            intent
                .parameters
                .push(IntentParameter::new("action", action));
            intent.confidence = IntentConfidence::new(0.85, "system action keyword match");
            return Some(intent);
        }
    }
    None
}

// ── Extraction helpers ──

fn extract_action_target<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let lower = text.to_lowercase();
    let prefix_lower = prefix.trim().to_lowercase();
    if let Some(pos) = lower.find(&prefix_lower) {
        let after = text[pos + prefix_lower.len()..].trim();
        if after.is_empty() {
            return None;
        }
        let end = after.find(['!', '?', ';']).unwrap_or(after.len());
        let result = after[..end].trim();
        let result = result.trim_end_matches(['.', ',']);
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    } else if let Some(pos) = lower.find(prefix_lower.trim_end()) {
        let after = text[pos + prefix_lower.trim_end().len()..].trim();
        if after.is_empty() {
            return None;
        }
        let end = after.find(['!', '?', ';']).unwrap_or(after.len());
        let result = after[..end].trim();
        let result = result.trim_end_matches(['.', ',']);
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    } else {
        None
    }
}

fn extract_after<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let lower = text.to_lowercase();
    let kw = keyword.to_lowercase();
    if let Some(pos) = lower.find(&kw) {
        let after = text[pos + kw.len()..].trim();
        if after.is_empty() {
            return None;
        }
        let end = after.find(['!', '?', ';']).unwrap_or(after.len());
        let result = after[..end].trim();
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    } else {
        None
    }
}

fn extract_quoted(text: &str) -> Option<&str> {
    if let Some(start) = text.find('\'') {
        let remaining = &text[start + 1..];
        if let Some(end) = remaining.find('\'') {
            return Some(&remaining[..end]);
        }
    }
    if let Some(start) = text.find('"') {
        let remaining = &text[start + 1..];
        if let Some(end) = remaining.find('"') {
            return Some(&remaining[..end]);
        }
    }
    None
}

fn extract_number(text: &str, after_key: &str) -> Option<u32> {
    let lower = text.to_lowercase();
    if let Some(pos) = lower.find(after_key) {
        let remaining = &lower[pos + after_key.len()..];
        for word in remaining.split_whitespace() {
            let cleaned: String = word.chars().filter(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = cleaned.parse::<u32>() {
                return Some(n);
            }
        }
    }
    // Fallback: find any number in the text
    for word in lower.split_whitespace() {
        let cleaned: String = word.chars().filter(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = cleaned.parse::<u32>() {
            if n <= 1000 {
                return Some(n);
            }
        }
    }
    None
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    fn parser() -> IntentParser {
        IntentParser::new()
    }

    // ── Intent type tests ──

    #[test]
    fn test_open_application() {
        let p = parser();
        let result = p.parse("open chrome");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::OpenApplication);
                assert_eq!(intent.target.as_deref(), Some("chrome"));
                assert!(intent.confidence.score >= 0.8);
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_open_application_synonyms() {
        let p = parser();
        for text in &["launch firefox", "start terminal", "run notepad"] {
            match p.parse(text) {
                IntentParseResult::Single(intent) => {
                    assert_eq!(
                        intent.intent_type,
                        IntentType::OpenApplication,
                        "failed for: {}",
                        text
                    );
                }
                _ => panic!("expected Single intent for: {}", text),
            }
        }
    }

    #[test]
    fn test_close_application() {
        let p = parser();
        let result = p.parse("close chrome");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::CloseApplication);
                assert_eq!(intent.target.as_deref(), Some("chrome"));
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_close_application_synonyms() {
        let p = parser();
        for text in &["quit vscode", "exit terminal", "kill notepad"] {
            match p.parse(text) {
                IntentParseResult::Single(intent) => {
                    assert_eq!(
                        intent.intent_type,
                        IntentType::CloseApplication,
                        "failed for: {}",
                        text
                    );
                }
                _ => panic!("expected Single intent for: {}", text),
            }
        }
    }

    #[test]
    fn test_search_intent() {
        let p = parser();
        let result = p.parse("search rust programming");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::Search);
                assert_eq!(intent.target.as_deref(), Some("rust programming"));
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_search_synonyms() {
        let p = parser();
        for text in &["find recipes", "look for hotels", "look up weather"] {
            match p.parse(text) {
                IntentParseResult::Single(intent) => {
                    assert_eq!(
                        intent.intent_type,
                        IntentType::Search,
                        "failed for: {}",
                        text
                    );
                }
                _ => panic!("expected Single intent for: {}", text),
            }
        }
    }

    #[test]
    fn test_navigate_intent() {
        let p = parser();
        let result = p.parse("go to github.com");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::Navigate);
                assert_eq!(intent.target.as_deref(), Some("github.com"));
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_click_intent() {
        let p = parser();
        let result = p.parse("click submit");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::Click);
                assert_eq!(intent.target.as_deref(), Some("submit"));
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_click_synonyms() {
        let p = parser();
        for text in &["tap button", "press enter", "select option"] {
            match p.parse(text) {
                IntentParseResult::Single(intent) => {
                    assert_eq!(
                        intent.intent_type,
                        IntentType::Click,
                        "failed for: {}",
                        text
                    );
                }
                _ => panic!("expected Single intent for: {}", text),
            }
        }
    }

    #[test]
    fn test_type_intent() {
        let p = parser();
        let result = p.parse("type hello world");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::Type);
                assert_eq!(intent.target.as_deref(), Some("hello world"));
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_type_synonyms() {
        let p = parser();
        for text in &["enter password", "input text", "write code"] {
            match p.parse(text) {
                IntentParseResult::Single(intent) => {
                    assert_eq!(intent.intent_type, IntentType::Type, "failed for: {}", text);
                }
                _ => panic!("expected Single intent for: {}", text),
            }
        }
    }

    #[test]
    fn test_scroll_intent() {
        let p = parser();
        let result = p.parse("scroll down");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::Scroll);
                let dir = intent.parameters.iter().find(|p| p.key == "direction");
                assert!(dir.is_some());
                assert_eq!(dir.unwrap().value, "down");
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_drag_intent() {
        let p = parser();
        let result = p.parse("drag from left to right");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::Drag);
                let from = intent.parameters.iter().find(|p| p.key == "from");
                assert!(from.is_some());
                assert_eq!(from.unwrap().value, "left");
                let to = intent.parameters.iter().find(|p| p.key == "to");
                assert!(to.is_some());
                assert_eq!(to.unwrap().value, "right");
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_swipe_intent() {
        let p = parser();
        let result = p.parse("swipe left");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::Swipe);
                let dir = intent.parameters.iter().find(|p| p.key == "direction");
                assert!(dir.is_some());
                assert_eq!(dir.unwrap().value, "left");
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_wait_intent() {
        let p = parser();
        let result = p.parse("wait for 5 seconds");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::Wait);
                let dur = intent.parameters.iter().find(|p| p.key == "duration");
                assert!(dur.is_some());
                assert_eq!(dur.unwrap().value, "5");
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_wait_synonyms() {
        let p = parser();
        for text in &["pause 2 seconds", "sleep 3"] {
            match p.parse(text) {
                IntentParseResult::Single(intent) => {
                    assert_eq!(intent.intent_type, IntentType::Wait, "failed for: {}", text);
                }
                _ => panic!("expected Single intent for: {}", text),
            }
        }
    }

    #[test]
    fn test_speak_intent() {
        let p = parser();
        let result = p.parse("speak hello world");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::Speak);
                assert_eq!(intent.target.as_deref(), Some("hello world"));
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_speak_synonyms() {
        let p = parser();
        for text in &["say hello", "tell time", "announce reminder"] {
            match p.parse(text) {
                IntentParseResult::Single(intent) => {
                    assert_eq!(
                        intent.intent_type,
                        IntentType::Speak,
                        "failed for: {}",
                        text
                    );
                }
                _ => panic!("expected Single intent for: {}", text),
            }
        }
    }

    // ── DeviceControl tests ──

    #[test]
    fn test_device_control_brightness() {
        let p = parser();
        let result = p.parse("set brightness to 75");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::DeviceControl);
                assert_eq!(intent.target.as_deref(), Some("brightness"));
                let val = intent.parameters.iter().find(|p| p.key == "value");
                assert!(val.is_some());
                assert_eq!(val.unwrap().value, "75");
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_device_control_dim() {
        let p = parser();
        let result = p.parse("dim the screen");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::DeviceControl);
                assert_eq!(intent.target.as_deref(), Some("brightness"));
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_device_control_volume() {
        let p = parser();
        let result = p.parse("set volume to 50");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::DeviceControl);
                assert_eq!(intent.target.as_deref(), Some("volume"));
                let val = intent.parameters.iter().find(|p| p.key == "value");
                assert!(val.is_some());
                assert_eq!(val.unwrap().value, "50");
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_device_control_mute() {
        let p = parser();
        let result = p.parse("mute device");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::DeviceControl);
                assert_eq!(intent.target.as_deref(), Some("volume"));
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_device_control_wifi() {
        let p = parser();
        let result = p.parse("enable wifi");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::DeviceControl);
                assert_eq!(intent.target.as_deref(), Some("wifi"));
                let enabled = intent.parameters.iter().find(|p| p.key == "enabled");
                assert!(enabled.is_some());
                assert_eq!(enabled.unwrap().value, "true");
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_device_control_bluetooth_off() {
        let p = parser();
        let result = p.parse("turn off bluetooth");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::DeviceControl);
                assert_eq!(intent.target.as_deref(), Some("bluetooth"));
                let enabled = intent.parameters.iter().find(|p| p.key == "enabled");
                assert!(enabled.is_some());
                assert_eq!(enabled.unwrap().value, "false");
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_device_control_dnd() {
        let p = parser();
        let result = p.parse("enable do not disturb");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::DeviceControl);
                assert_eq!(intent.target.as_deref(), Some("dnd"));
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_device_control_lock() {
        let p = parser();
        let result = p.parse("lock device");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::DeviceControl);
                assert_eq!(intent.target.as_deref(), Some("lock"));
            }
            _ => panic!("expected Single intent"),
        }
    }

    // ── BrowserAction tests ──

    #[test]
    fn test_browser_action_search_on() {
        let p = parser();
        let result = p.parse("search rust on google");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::BrowserAction);
                let engine = intent.parameters.iter().find(|p| p.key == "engine");
                assert!(engine.is_some());
                assert_eq!(engine.unwrap().value, "google");
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_browser_action_youtube() {
        let p = parser();
        let result = p.parse("search tutorials on youtube");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::BrowserAction);
            }
            _ => panic!("expected Single intent"),
        }
    }

    // ── FileAction tests ──

    #[test]
    fn test_file_action_open() {
        let p = parser();
        let result = p.parse("open file report.pdf");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::FileAction);
                let op = intent.parameters.iter().find(|p| p.key == "operation");
                assert!(op.is_some());
                assert_eq!(op.unwrap().value, "open");
            }
            _ => panic!("expected Single intent"),
        }
    }

    // ── SystemAction tests ──

    #[test]
    fn test_system_action_shutdown() {
        let p = parser();
        let result = p.parse("shutdown computer");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::SystemAction);
                let action = intent.parameters.iter().find(|p| p.key == "action");
                assert!(action.is_some());
                assert_eq!(action.unwrap().value, "shutdown");
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_system_action_restart() {
        let p = parser();
        let result = p.parse("restart system");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::SystemAction);
                let action = intent.parameters.iter().find(|p| p.key == "action");
                assert!(action.is_some());
                assert_eq!(action.unwrap().value, "restart");
            }
            _ => panic!("expected Single intent"),
        }
    }

    // ── Multi-step tests ──

    #[test]
    fn test_multi_step_with_and() {
        let p = parser();
        let result = p.parse("open chrome and search rust");
        match result {
            IntentParseResult::Multi(intents) => {
                assert_eq!(intents.len(), 2);
                assert_eq!(intents[0].intent_type, IntentType::OpenApplication);
                assert_eq!(intents[1].intent_type, IntentType::Search);
            }
            IntentParseResult::Single(i) => {
                // If the parser consolidated, it should be MultiStepGoal
                assert_eq!(
                    i.intent_type,
                    IntentType::MultiStepGoal,
                    "expected MultiStepGoal, got {:?}",
                    i.intent_type
                );
            }
            _ => panic!("expected Multi or Single(MultiStepGoal)"),
        }
    }

    #[test]
    fn test_multi_step_with_then() {
        let p = parser();
        let result = p.parse("open settings then click network");
        match result {
            IntentParseResult::Multi(intents) => {
                assert!(intents.len() >= 2);
                assert_eq!(intents[0].intent_type, IntentType::OpenApplication);
                assert_eq!(intents[1].intent_type, IntentType::Click);
            }
            IntentParseResult::Single(i) => {
                assert_eq!(i.intent_type, IntentType::MultiStepGoal);
            }
            _ => panic!("expected multi-step result"),
        }
    }

    // ── Unknown / edge case tests ──

    #[test]
    fn test_unknown_intent() {
        let p = parser();
        let result = p.parse("purple elephant dancing");
        match result {
            IntentParseResult::Unknown(_) => {} // expected
            _ => panic!("expected Unknown for gibberish"),
        }
    }

    #[test]
    fn test_empty_input() {
        let p = parser();
        let result = p.parse("");
        assert!(result.is_unknown());
    }

    #[test]
    fn test_whitespace_input() {
        let p = parser();
        let result = p.parse("   ");
        assert!(result.is_unknown());
    }

    #[test]
    fn test_malformed_input() {
        let p = parser();
        let result = p.parse("open");
        match result {
            IntentParseResult::Single(intent) => {
                // "open" alone should still resolve as OpenApplication with default target
                assert_eq!(intent.intent_type, IntentType::OpenApplication);
                assert!(intent.confidence.score >= 0.8);
            }
            _ => panic!("expected Single intent for 'open'"),
        }
    }

    // ── Confidence tests ──

    #[test]
    fn test_confidence_certain() {
        let c = IntentConfidence::certain();
        assert!((c.score - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_confidence_scores_differ() {
        let c1 = IntentConfidence::high();
        let c2 = IntentConfidence::medium();
        let c3 = IntentConfidence::low();
        assert!(c1.score > c2.score);
        assert!(c2.score > c3.score);
    }

    #[test]
    fn test_confidence_best_for_exact_open() {
        let p = parser();
        let scores = p.confidence("open chrome");
        let open_score = scores.get("open_application").copied().unwrap_or(0.0);
        assert!(
            open_score > 0.0,
            "expected non-zero open_application confidence"
        );
    }

    // ── Parameter extraction tests ──

    #[test]
    fn test_parameter_extraction_click_quoted() {
        let p = parser();
        let result = p.parse("click 'submit button'");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::Click);
                let target = intent.parameters.iter().find(|p| p.key == "target");
                assert!(target.is_some());
                assert_eq!(target.unwrap().value, "submit button");
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_parameter_extraction_type_quoted() {
        let p = parser();
        let result = p.parse("type 'hello world'");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::Type);
                let text = intent.parameters.iter().find(|p| p.key == "text");
                assert!(text.is_some());
                assert_eq!(text.unwrap().value, "hello world");
            }
            _ => panic!("expected Single intent"),
        }
    }

    #[test]
    fn test_parameter_brightness_value() {
        let p = parser();
        let result = p.parse("set brightness to 75");
        match result {
            IntentParseResult::Single(intent) => {
                assert_eq!(intent.intent_type, IntentType::DeviceControl);
                let val = intent.parameters.iter().find(|p| p.key == "value");
                assert!(val.is_some());
                assert_eq!(val.unwrap().value, "75");
            }
            _ => panic!("expected Single intent"),
        }
    }

    // ── Unicode tests ──

    #[test]
    fn test_unicode_input() {
        let p = parser();
        let result = p.parse("ouvrir chrome");
        // French "ouvrir" is not in our English patterns — should be unknown
        assert!(result.is_unknown());
    }

    #[test]
    fn test_unicode_mixed() {
        let p = parser();
        let result = p.parse("open chrome 打开浏览器");
        match result {
            IntentParseResult::Single(intent) => {
                // "open" prefix should still match
                assert_eq!(intent.intent_type, IntentType::OpenApplication);
            }
            _ => panic!("expected Single intent"),
        }
    }

    // ── parse_many tests ──

    #[test]
    fn test_parse_many_multiple() {
        let p = parser();
        let results = p.parse_many(&["open chrome", "close notepad", "garbage"]);
        assert_eq!(results.len(), 3);
        match &results[0] {
            IntentParseResult::Single(i) => assert_eq!(i.intent_type, IntentType::OpenApplication),
            _ => panic!("expected Single"),
        }
        match &results[1] {
            IntentParseResult::Single(i) => assert_eq!(i.intent_type, IntentType::CloseApplication),
            _ => panic!("expected Single"),
        }
        assert!(results[2].is_unknown());
    }

    // ── normalize tests ──

    #[test]
    fn test_normalize_lowercase() {
        let p = parser();
        assert_eq!(p.normalize("OPEN CHROME"), "open chrome");
    }

    #[test]
    fn test_normalize_collapse_whitespace() {
        let p = parser();
        assert_eq!(p.normalize("open    chrome"), "open chrome");
    }

    #[test]
    fn test_normalize_empty() {
        let p = parser();
        assert_eq!(p.normalize(""), "");
        assert_eq!(p.normalize("  "), "");
    }

    // ── supported_intents test ──

    #[test]
    fn test_supported_intents_count() {
        let p = parser();
        let intents = p.supported_intents();
        // 16 intent types (excluding Unknown)
        assert_eq!(intents.len(), 16);
    }

    // ── Intent builder tests ──

    #[test]
    fn test_intent_builder_pattern() {
        let intent = Intent::new(IntentType::Search, "find rust")
            .with_target("rust")
            .with_parameter("query", "rust")
            .with_confidence(IntentConfidence::high());
        assert_eq!(intent.target.as_deref(), Some("rust"));
        assert_eq!(intent.parameters.len(), 1);
        assert!((intent.confidence.score - 0.85).abs() < 0.01);
    }

    // ── IntentContext tests ──

    #[test]
    fn test_intent_context_builder() {
        let ctx = IntentContext::new("voice")
            .with_timestamp(1000)
            .with_application_state("chrome");
        assert_eq!(ctx.source, "voice");
        assert_eq!(ctx.timestamp, 1000);
        assert_eq!(ctx.application_state.as_deref(), Some("chrome"));
    }

    // ── IntentParseResult helpers ──

    #[test]
    fn test_parse_result_intents_single() {
        let intent = Intent::new(IntentType::OpenApplication, "open chrome").with_target("chrome");
        let result = IntentParseResult::Single(intent);
        assert_eq!(result.intents().len(), 1);
        assert!(!result.is_unknown());
        assert!(result.confidence() > 0.0);
    }

    #[test]
    fn test_parse_result_intents_multi() {
        let i1 = Intent::new(IntentType::OpenApplication, "open chrome");
        let i2 = Intent::new(IntentType::Search, "search rust");
        let result = IntentParseResult::Multi(vec![i1, i2]);
        assert_eq!(result.intents().len(), 2);
        assert!(!result.is_unknown());
    }

    #[test]
    fn test_parse_result_intents_unknown() {
        let result = IntentParseResult::Unknown("garbage".into());
        assert!(result.intents().is_empty());
        assert!(result.is_unknown());
        assert!((result.confidence() - 0.0).abs() < 0.001);
    }

    // ── IntentConfidence clamping test ──

    #[test]
    fn test_confidence_clamping() {
        let c = IntentConfidence::new(2.0, "too high");
        assert!((c.score - 1.0).abs() < 0.001);
        let c = IntentConfidence::new(-1.0, "too low");
        assert!((c.score - 0.0).abs() < 0.001);
    }
}
