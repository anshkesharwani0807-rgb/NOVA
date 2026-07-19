use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::intention_parser::{Intent, IntentType};
use crate::planner::Goal;

/// Category assigned to a goal definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GoalCategory {
    Application,
    Browser,
    Device,
    System,
    File,
    Navigation,
    Communication,
    Search,
    Automation,
    Custom,
}

/// Capability required by a goal definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GoalCapability {
    ScreenRequired,
    InputRequired,
    NetworkRequired,
    MemoryRequired,
    AIRequired,
    DeviceRequired,
}

/// Template for converting resolved parameters into a Planner `Goal`.
#[derive(Debug, Clone)]
pub struct GoalTemplate {
    /// Template string with `{key}` placeholders, e.g. "open {application}".
    pub description_template: String,
    /// Parameter keys to extract from the resolved intent and inject as context.
    pub context_keys: Vec<String>,
}

impl GoalTemplate {
    pub fn new(description_template: impl Into<String>) -> Self {
        Self {
            description_template: description_template.into(),
            context_keys: Vec::new(),
        }
    }

    pub fn with_context_keys(mut self, keys: Vec<&str>) -> Self {
        self.context_keys = keys.iter().map(|k| k.to_string()).collect();
        self
    }

    /// Fill the template with parameters to produce a `Goal`.
    pub fn fill(&self, params: &HashMap<String, String>) -> Goal {
        let mut description = self.description_template.clone();
        for (key, value) in params {
            description = description.replace(&format!("{{{}}}", key), value);
        }
        let mut context = HashMap::new();
        for key in &self.context_keys {
            if let Some(value) = params.get(key) {
                context.insert(key.clone(), value.clone());
            }
        }
        let mut goal = Goal::new(description);
        for (key, value) in context {
            goal = goal.with_context(key, value);
        }
        goal
    }
}

/// Metadata attached to a goal definition.
#[derive(Debug, Clone)]
pub struct GoalMetadata {
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub notes: Option<String>,
}

impl GoalMetadata {
    pub fn new() -> Self {
        Self {
            author: None,
            tags: Vec::new(),
            notes: None,
        }
    }

    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }
}

impl Default for GoalMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// A complete goal definition stored in the registry.
#[derive(Debug, Clone)]
pub struct GoalDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: GoalCategory,
    pub supported_intents: Vec<IntentType>,
    pub required_parameters: Vec<String>,
    pub optional_parameters: Vec<String>,
    pub required_capabilities: Vec<GoalCapability>,
    pub planner_template: GoalTemplate,
    pub metadata: GoalMetadata,
    pub version: String,
    pub enabled: bool,
    pub aliases: Vec<String>,
    pub synonyms: Vec<String>,
}

impl GoalDefinition {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        category: GoalCategory,
        supported_intents: Vec<IntentType>,
        template: GoalTemplate,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            category,
            supported_intents,
            required_parameters: Vec::new(),
            optional_parameters: Vec::new(),
            required_capabilities: Vec::new(),
            planner_template: template,
            metadata: GoalMetadata::new(),
            version: "1.0.0".into(),
            enabled: true,
            aliases: Vec::new(),
            synonyms: Vec::new(),
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn with_required_parameters(mut self, params: Vec<&str>) -> Self {
        self.required_parameters = params.iter().map(|p| p.to_string()).collect();
        self
    }

    pub fn with_optional_parameters(mut self, params: Vec<&str>) -> Self {
        self.optional_parameters = params.iter().map(|p| p.to_string()).collect();
        self
    }

    pub fn with_capabilities(mut self, caps: Vec<GoalCapability>) -> Self {
        self.required_capabilities = caps;
        self
    }

    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.aliases.push(alias.into());
        self
    }

    pub fn with_synonym(mut self, synonym: impl Into<String>) -> Self {
        self.synonyms.push(synonym.into());
        self
    }

    pub fn with_metadata(mut self, meta: GoalMetadata) -> Self {
        self.metadata = meta;
        self
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Check whether all required parameters are present in the given map.
    pub fn has_required_parameters(&self, params: &HashMap<String, String>) -> bool {
        self.required_parameters
            .iter()
            .all(|k| params.contains_key(k))
    }
}

/// A match result produced during goal resolution.
#[derive(Debug, Clone)]
pub struct GoalMatch {
    pub definition: GoalDefinition,
    pub confidence: f32,
    pub matched_parameters: HashMap<String, String>,
}

/// The outcome of resolving an intent against the registry.
#[derive(Debug, Clone)]
pub enum GoalResolutionResult {
    Exact(GoalMatch),
    Synonym(GoalMatch),
    Alias(GoalMatch),
    Partial(GoalMatch),
    NoMatch(String),
}

impl GoalResolutionResult {
    pub fn into_match(self) -> Option<GoalMatch> {
        match self {
            GoalResolutionResult::Exact(m)
            | GoalResolutionResult::Synonym(m)
            | GoalResolutionResult::Alias(m)
            | GoalResolutionResult::Partial(m) => Some(m),
            GoalResolutionResult::NoMatch(_) => None,
        }
    }

    pub fn confidence(&self) -> f32 {
        match self {
            GoalResolutionResult::Exact(m) => m.confidence,
            GoalResolutionResult::Synonym(m) => m.confidence,
            GoalResolutionResult::Alias(m) => m.confidence,
            GoalResolutionResult::Partial(m) => m.confidence,
            GoalResolutionResult::NoMatch(_) => 0.0,
        }
    }
}

/// Configuration for the goal registry.
#[derive(Debug, Clone)]
pub struct GoalRegistryConfig {
    pub enable_custom_goals: bool,
    pub enable_aliases: bool,
    pub enable_synonyms: bool,
    pub minimum_confidence: f32,
    pub max_registered_goals: usize,
}

impl Default for GoalRegistryConfig {
    fn default() -> Self {
        Self {
            enable_custom_goals: true,
            enable_aliases: true,
            enable_synonyms: true,
            minimum_confidence: 0.3,
            max_registered_goals: 500,
        }
    }
}

/// Statistics about the registry state.
#[derive(Debug, Clone, Default)]
pub struct GoalRegistryStatistics {
    pub total_goals: usize,
    pub enabled_goals: usize,
    pub disabled_goals: usize,
    pub total_aliases: usize,
    pub total_synonyms: usize,
    pub categories: HashMap<String, usize>,
    pub custom_goals: usize,
    pub builtin_goals: usize,
}

/// Resolves an `Intent` into a `GoalDefinition` using the registry.
pub trait GoalResolver: Send + Sync {
    fn resolve(&self, intent: &Intent, registry: &GoalRegistry) -> GoalResolutionResult;
}

/// Default resolver implementation using confidence-based matching.
pub struct DefaultGoalResolver;

impl GoalResolver for DefaultGoalResolver {
    fn resolve(&self, intent: &Intent, registry: &GoalRegistry) -> GoalResolutionResult {
        registry.resolve_intent(intent)
    }
}

// ── Registry ──

struct GoalRegistryInner {
    goals: HashMap<String, GoalDefinition>,
    aliases: HashMap<String, String>,
    config: GoalRegistryConfig,
}

impl GoalRegistryInner {
    fn new(config: GoalRegistryConfig) -> Self {
        Self {
            goals: HashMap::new(),
            aliases: HashMap::new(),
            config,
        }
    }

    fn register(&mut self, def: GoalDefinition) -> Result<(), String> {
        if self.goals.len() >= self.config.max_registered_goals {
            return Err(format!(
                "registry full (max {})",
                self.config.max_registered_goals
            ));
        }
        if self.goals.contains_key(&def.id) {
            return Err(format!("goal '{}' already registered", def.id));
        }
        for alias in &def.aliases {
            if self.aliases.contains_key(alias) {
                return Err(format!("alias '{}' already registered", alias));
            }
        }
        for alias in &def.aliases {
            self.aliases.insert(alias.clone(), def.id.clone());
        }
        self.goals.insert(def.id.clone(), def);
        Ok(())
    }

    fn unregister(&mut self, id: &str) -> Result<GoalDefinition, String> {
        let def = self
            .goals
            .remove(id)
            .ok_or_else(|| format!("goal '{}' not found", id))?;
        // Clean up aliases that pointed to this goal.
        self.aliases.retain(|_, v| v != id);
        Ok(def)
    }

    fn resolve_by_intent(&self, intent: &Intent) -> GoalResolutionResult {
        let candidates: Vec<&GoalDefinition> = self
            .goals
            .values()
            .filter(|g| g.enabled)
            .filter(|g| g.supported_intents.contains(&intent.intent_type))
            .collect();

        let mut scored: Vec<(&GoalDefinition, f32, HashMap<String, String>, &str)> = Vec::new();

        for def in &candidates {
            let params = build_parameters(def, intent);

            // Boost score if target appears in goal name or description
            let name_boost = intent.target.as_ref().map_or(0.0, |target| {
                let t = target.to_lowercase();
                let n = def.name.to_lowercase();
                let d = def.description.to_lowercase();
                if n == t || n.starts_with(&t) || n.contains(&t) {
                    0.04
                } else if d.contains(&t) {
                    0.02
                } else {
                    0.0
                }
            });

            // Boost if the intent target maps to a required parameter key
            let param_boost = intent.target.as_ref().map_or(0.0, |_| {
                let target_key = guess_param_key(def);
                if target_key != "target" && def.required_parameters.contains(&target_key) {
                    0.01
                } else {
                    0.0
                }
            });

            // Exact match: intent type matches AND all required params satisfied
            if def.has_required_parameters(&params) {
                scored.push((def, 0.95 + name_boost + param_boost, params, "exact"));
                continue;
            }

            // Alias match: check intent target against registered aliases
            if self.config.enable_aliases {
                if let Some(target) = &intent.target {
                    let target_lower = target.to_lowercase();
                    if let Some(goal_id) = self.aliases.get(&target_lower) {
                        if goal_id == &def.id {
                            scored.push((def, 0.9, params, "alias"));
                            continue;
                        }
                    }
                    // Also check if any alias is contained in the target
                    let alias_match = def.aliases.iter().any(|a| {
                        target_lower.contains(&a.to_lowercase())
                            || a.to_lowercase().contains(&target_lower)
                    });
                    if alias_match {
                        scored.push((def, 0.85, params, "alias"));
                        continue;
                    }
                }
            }

            // Synonym match
            if self.config.enable_synonyms {
                if let Some(target) = &intent.target {
                    let target_lower = target.to_lowercase();
                    let matched_synonym = def.synonyms.iter().any(|s| {
                        let s_lower = s.to_lowercase();
                        target_lower == s_lower || target_lower.contains(&s_lower)
                    });
                    if matched_synonym {
                        scored.push((def, 0.8, params, "synonym"));
                        continue;
                    }
                }
            }

            // Partial match by name or description
            if let Some(target) = &intent.target {
                let target_lower = target.to_lowercase();
                let score = partial_match_score(&def.name, &target_lower)
                    .max(partial_match_score(&def.description, &target_lower));
                if score >= self.config.minimum_confidence {
                    scored.push((def, score, params, "partial"));
                    continue;
                }
            }

            // Lowest confidence: intent type match only
            scored.push((def, 0.4, params, "fallback"));
        }

        // Sort by descending confidence
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Pick best. "exact" > "alias" > "synonym" > "partial" > "fallback" when tied.
        if let Some((best, score, params, kind)) = scored.into_iter().next() {
            match kind {
                "exact" => GoalResolutionResult::Exact(GoalMatch {
                    definition: best.clone(),
                    confidence: score,
                    matched_parameters: params,
                }),
                "alias" => GoalResolutionResult::Alias(GoalMatch {
                    definition: best.clone(),
                    confidence: score,
                    matched_parameters: params,
                }),
                "synonym" => GoalResolutionResult::Synonym(GoalMatch {
                    definition: best.clone(),
                    confidence: score,
                    matched_parameters: params,
                }),
                _ => GoalResolutionResult::Partial(GoalMatch {
                    definition: best.clone(),
                    confidence: score,
                    matched_parameters: params,
                }),
            }
        } else {
            GoalResolutionResult::NoMatch(format!(
                "no goal found for intent type {:?} with target '{:?}'",
                intent.intent_type, intent.target
            ))
        }
    }

    fn contains(&self, id: &str) -> bool {
        self.goals.contains_key(id)
    }

    fn list(&self) -> Vec<GoalDefinition> {
        self.goals.values().cloned().collect()
    }

    fn statistics(&self) -> GoalRegistryStatistics {
        let mut enabled = 0usize;
        let mut disabled = 0usize;
        let mut categories: HashMap<String, usize> = HashMap::new();
        let mut total_aliases = 0usize;
        let mut total_synonyms = 0usize;
        let mut custom = 0usize;
        let mut builtin = 0usize;

        for def in self.goals.values() {
            if def.enabled {
                enabled += 1;
            } else {
                disabled += 1;
            }
            let cat = format!("{:?}", def.category);
            *categories.entry(cat).or_insert(0) += 1;
            total_aliases += def.aliases.len();
            total_synonyms += def.synonyms.len();
            if def.category == GoalCategory::Custom {
                custom += 1;
            } else {
                builtin += 1;
            }
        }

        GoalRegistryStatistics {
            total_goals: self.goals.len(),
            enabled_goals: enabled,
            disabled_goals: disabled,
            total_aliases,
            total_synonyms,
            categories,
            custom_goals: custom,
            builtin_goals: builtin,
        }
    }

    fn clear(&mut self) {
        self.goals.clear();
        self.aliases.clear();
    }
}

/// Thread-safe registry that maps parsed `Intent` values to `GoalDefinition`s
/// and produces Planner `Goal` values.
pub struct GoalRegistry {
    inner: Arc<parking_lot::RwLock<GoalRegistryInner>>,
}

impl GoalRegistry {
    pub fn new(config: GoalRegistryConfig) -> Self {
        let inner = GoalRegistryInner::new(config);
        Self {
            inner: Arc::new(parking_lot::RwLock::new(inner)),
        }
    }

    /// Create a registry with default config and built-in goals pre-registered.
    pub fn with_builtins() -> Self {
        let reg = Self::new(GoalRegistryConfig::default());
        reg.register_builtins();
        reg
    }

    /// Register a goal definition.
    pub fn register(&self, def: GoalDefinition) -> Result<(), String> {
        self.inner.write().register(def)
    }

    /// Unregister a goal by ID.
    pub fn unregister(&self, id: &str) -> Result<GoalDefinition, String> {
        self.inner.write().unregister(id)
    }

    /// Resolve an Intent into a GoalDefinition using the best matching strategy.
    pub fn resolve(&self, intent: &Intent) -> GoalResolutionResult {
        self.inner.read().resolve_by_intent(intent)
    }

    /// Resolve via the trait interface.
    fn resolve_intent(&self, intent: &Intent) -> GoalResolutionResult {
        self.resolve(intent)
    }

    /// Check if a goal ID is registered.
    pub fn contains(&self, id: &str) -> bool {
        self.inner.read().contains(id)
    }

    /// List all registered goal definitions.
    pub fn list(&self) -> Vec<GoalDefinition> {
        self.inner.read().list()
    }

    /// List goals filtered by category.
    pub fn list_by_category(&self, category: GoalCategory) -> Vec<GoalDefinition> {
        self.inner
            .read()
            .list()
            .into_iter()
            .filter(|g| g.category == category)
            .collect()
    }

    /// Get a goal by ID.
    pub fn get(&self, id: &str) -> Option<GoalDefinition> {
        self.inner.read().goals.get(id).cloned()
    }

    /// Get registry statistics.
    pub fn statistics(&self) -> GoalRegistryStatistics {
        self.inner.read().statistics()
    }

    /// Clear all registered goals.
    pub fn clear(&self) {
        self.inner.write().clear();
    }

    /// Get a reference to the inner config.
    pub fn config(&self) -> GoalRegistryConfig {
        self.inner.read().config.clone()
    }

    /// Update the config.
    pub fn set_config(&self, config: GoalRegistryConfig) {
        self.inner.write().config = config;
    }

    /// Register all built-in goals.
    fn register_builtins(&self) {
        let builtins = builtin_goals();
        for def in builtins {
            let _ = self.inner.write().register(def);
        }
    }
}

impl Clone for GoalRegistry {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

// ── Built-in goal definitions ──

fn builtin_goals() -> Vec<GoalDefinition> {
    vec![
        GoalDefinition::new(
            "builtin.open_app",
            "Open Application",
            GoalCategory::Application,
            vec![IntentType::OpenApplication],
            GoalTemplate::new("open {application}").with_context_keys(vec!["application"]),
        )
        .with_description("Open or launch a desktop or mobile application")
        .with_required_parameters(vec!["application"])
        .with_capabilities(vec![GoalCapability::ScreenRequired])
        .with_alias("launch")
        .with_alias("start app")
        .with_synonym("run")
        .with_synonym("execute")
        .with_version("1.0.0"),
        GoalDefinition::new(
            "builtin.close_app",
            "Close Application",
            GoalCategory::Application,
            vec![IntentType::CloseApplication],
            GoalTemplate::new("close {application}").with_context_keys(vec!["application"]),
        )
        .with_description("Close or quit a running application")
        .with_required_parameters(vec!["application"])
        .with_capabilities(vec![GoalCapability::ScreenRequired])
        .with_alias("quit")
        .with_alias("exit app")
        .with_synonym("terminate")
        .with_synonym("kill"),
        GoalDefinition::new(
            "builtin.search_web",
            "Search Web",
            GoalCategory::Search,
            vec![IntentType::Search, IntentType::BrowserAction],
            GoalTemplate::new("search {query} on the web").with_context_keys(vec!["query"]),
        )
        .with_description("Search the internet using the default search engine")
        .with_required_parameters(vec!["query"])
        .with_capabilities(vec![GoalCapability::NetworkRequired])
        .with_alias("google")
        .with_alias("web search")
        .with_synonym("internet search"),
        GoalDefinition::new(
            "builtin.search_local",
            "Search Local",
            GoalCategory::Search,
            vec![IntentType::Search],
            GoalTemplate::new("search local files for {query}").with_context_keys(vec!["query"]),
        )
        .with_description("Search local files, documents, and memories")
        .with_required_parameters(vec!["query"])
        .with_capabilities(vec![GoalCapability::MemoryRequired])
        .with_alias("local search")
        .with_alias("find file"),
        GoalDefinition::new(
            "builtin.click_element",
            "Click Element",
            GoalCategory::Automation,
            vec![IntentType::Click],
            GoalTemplate::new("click {target}").with_context_keys(vec!["target"]),
        )
        .with_description("Click or tap a screen element by text or query")
        .with_required_parameters(vec!["target"])
        .with_capabilities(vec![
            GoalCapability::ScreenRequired,
            GoalCapability::InputRequired,
        ])
        .with_alias("tap")
        .with_alias("press")
        .with_synonym("select")
        .with_synonym("choose"),
        GoalDefinition::new(
            "builtin.type_text",
            "Type Text",
            GoalCategory::Automation,
            vec![IntentType::Type],
            GoalTemplate::new("type {text}").with_context_keys(vec!["text"]),
        )
        .with_description("Type or enter text into the active input field")
        .with_required_parameters(vec!["text"])
        .with_capabilities(vec![
            GoalCapability::ScreenRequired,
            GoalCapability::InputRequired,
        ])
        .with_alias("enter text")
        .with_alias("input")
        .with_synonym("write"),
        GoalDefinition::new(
            "builtin.scroll_page",
            "Scroll Page",
            GoalCategory::Automation,
            vec![IntentType::Scroll],
            GoalTemplate::new("scroll {direction}").with_context_keys(vec!["direction"]),
        )
        .with_description("Scroll the active page or view in a direction")
        .with_required_parameters(vec!["direction"])
        .with_capabilities(vec![
            GoalCapability::ScreenRequired,
            GoalCapability::InputRequired,
        ])
        .with_alias("scroll down")
        .with_alias("scroll up")
        .with_synonym("pan"),
        GoalDefinition::new(
            "builtin.drag_element",
            "Drag Element",
            GoalCategory::Automation,
            vec![IntentType::Drag],
            GoalTemplate::new("drag from {from} to {to}").with_context_keys(vec!["from", "to"]),
        )
        .with_description("Drag a UI element from one position to another")
        .with_optional_parameters(vec!["from", "to"])
        .with_capabilities(vec![
            GoalCapability::ScreenRequired,
            GoalCapability::InputRequired,
        ])
        .with_alias("drag and drop"),
        GoalDefinition::new(
            "builtin.swipe_element",
            "Swipe Element",
            GoalCategory::Automation,
            vec![IntentType::Swipe],
            GoalTemplate::new("swipe {direction}").with_context_keys(vec!["direction"]),
        )
        .with_description("Swipe in a direction across the screen")
        .with_required_parameters(vec!["direction"])
        .with_capabilities(vec![
            GoalCapability::ScreenRequired,
            GoalCapability::InputRequired,
        ])
        .with_alias("slide"),
        GoalDefinition::new(
            "builtin.change_brightness",
            "Change Brightness",
            GoalCategory::Device,
            vec![IntentType::DeviceControl],
            GoalTemplate::new("set brightness to {value}").with_context_keys(vec!["value"]),
        )
        .with_description("Adjust the screen brightness level")
        .with_optional_parameters(vec!["value"])
        .with_capabilities(vec![GoalCapability::DeviceRequired])
        .with_alias("brightness")
        .with_alias("dim screen")
        .with_synonym("screen brightness"),
        GoalDefinition::new(
            "builtin.change_volume",
            "Change Volume",
            GoalCategory::Device,
            vec![IntentType::DeviceControl],
            GoalTemplate::new("set volume to {value}").with_context_keys(vec!["value"]),
        )
        .with_description("Adjust the system volume level")
        .with_optional_parameters(vec!["value"])
        .with_capabilities(vec![GoalCapability::DeviceRequired])
        .with_alias("volume")
        .with_alias("sound")
        .with_synonym("audio"),
        GoalDefinition::new(
            "builtin.open_settings",
            "Open Settings",
            GoalCategory::Application,
            vec![IntentType::OpenApplication],
            GoalTemplate::new("open settings"),
        )
        .with_description("Open the system settings or preferences application")
        .with_capabilities(vec![GoalCapability::ScreenRequired])
        .with_alias("settings")
        .with_alias("preferences")
        .with_synonym("system settings"),
        GoalDefinition::new(
            "builtin.lock_device",
            "Lock Device",
            GoalCategory::Device,
            vec![IntentType::DeviceControl],
            GoalTemplate::new("lock device"),
        )
        .with_description("Lock the device screen")
        .with_capabilities(vec![GoalCapability::DeviceRequired])
        .with_alias("lock screen")
        .with_alias("lock"),
        GoalDefinition::new(
            "builtin.shutdown",
            "Shutdown",
            GoalCategory::System,
            vec![IntentType::SystemAction],
            GoalTemplate::new("shutdown system"),
        )
        .with_description("Shut down the computer or device")
        .with_capabilities(vec![GoalCapability::DeviceRequired])
        .with_alias("power off")
        .with_alias("shut down"),
        GoalDefinition::new(
            "builtin.restart",
            "Restart",
            GoalCategory::System,
            vec![IntentType::SystemAction],
            GoalTemplate::new("restart system"),
        )
        .with_description("Restart or reboot the computer or device")
        .with_capabilities(vec![GoalCapability::DeviceRequired])
        .with_alias("reboot")
        .with_alias("reload"),
        GoalDefinition::new(
            "builtin.sleep",
            "Sleep",
            GoalCategory::System,
            vec![IntentType::SystemAction],
            GoalTemplate::new("put system to sleep"),
        )
        .with_description("Put the computer or device into sleep mode")
        .with_capabilities(vec![GoalCapability::DeviceRequired])
        .with_alias("suspend")
        .with_alias("standby"),
        GoalDefinition::new(
            "builtin.browser_search",
            "Browser Search",
            GoalCategory::Browser,
            vec![IntentType::BrowserAction, IntentType::Search],
            GoalTemplate::new("search {query} on {engine}")
                .with_context_keys(vec!["query", "engine"]),
        )
        .with_description("Search a specific search engine or website from the browser")
        .with_required_parameters(vec!["query"])
        .with_optional_parameters(vec!["engine"])
        .with_capabilities(vec![GoalCapability::NetworkRequired])
        .with_alias("online search")
        .with_alias("internet search"),
        GoalDefinition::new(
            "builtin.open_url",
            "Open URL",
            GoalCategory::Navigation,
            vec![IntentType::Navigate],
            GoalTemplate::new("go to {url}").with_context_keys(vec!["url"]),
        )
        .with_description("Open a specific URL or web address in the browser")
        .with_required_parameters(vec!["url"])
        .with_capabilities(vec![
            GoalCapability::NetworkRequired,
            GoalCapability::ScreenRequired,
        ])
        .with_alias("navigate")
        .with_alias("go to")
        .with_synonym("visit"),
    ]
}

// ── Helpers ──

fn guess_param_key(def: &GoalDefinition) -> String {
    if def.required_parameters.contains(&"application".to_string())
        || def.optional_parameters.contains(&"application".to_string())
    {
        "application".to_string()
    } else if def.required_parameters.contains(&"query".to_string())
        || def.optional_parameters.contains(&"query".to_string())
    {
        "query".to_string()
    } else if def.required_parameters.contains(&"target".to_string())
        || def.optional_parameters.contains(&"target".to_string())
    {
        "target".to_string()
    } else if def.required_parameters.contains(&"text".to_string())
        || def.optional_parameters.contains(&"text".to_string())
    {
        "text".to_string()
    } else if def.required_parameters.contains(&"url".to_string())
        || def.optional_parameters.contains(&"url".to_string())
    {
        "url".to_string()
    } else if def.required_parameters.contains(&"direction".to_string())
        || def.optional_parameters.contains(&"direction".to_string())
    {
        "direction".to_string()
    } else if def.required_parameters.contains(&"value".to_string())
        || def.optional_parameters.contains(&"value".to_string())
    {
        "value".to_string()
    } else {
        def.required_parameters
            .first()
            .cloned()
            .unwrap_or_else(|| "target".to_string())
    }
}

fn build_parameters(def: &GoalDefinition, intent: &Intent) -> HashMap<String, String> {
    let mut params = HashMap::new();
    if let Some(target) = &intent.target {
        params.insert(guess_param_key(def), target.clone());
    }
    // Copy all intent parameters
    for p in &intent.parameters {
        if !params.contains_key(&p.key) {
            params.insert(p.key.clone(), p.value.clone());
        }
    }
    params
}

fn partial_match_score(name: &str, target: &str) -> f32 {
    let name_lower = name.to_lowercase();
    let target_lower = target.to_lowercase();

    if name_lower == target_lower {
        return 1.0;
    }
    if name_lower.contains(&target_lower) {
        return 0.8;
    }
    if target_lower.contains(&name_lower) {
        return 0.7;
    }
    // Token overlap
    let name_tokens: HashSet<&str> = name_lower.split_whitespace().collect();
    let target_tokens: HashSet<&str> = target_lower.split_whitespace().collect();
    let intersection: HashSet<&&str> = name_tokens.intersection(&target_tokens).collect();
    if name_tokens.is_empty() || target_tokens.is_empty() {
        return 0.0;
    }
    let score = intersection.len() as f32 / name_tokens.len().max(target_tokens.len()) as f32;
    score.max(0.3) // Floor for any overlap
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intention_parser::{Intent, IntentConfidence, IntentType};

    fn test_registry() -> GoalRegistry {
        GoalRegistry::with_builtins()
    }

    fn make_intent(intent_type: IntentType, target: &str, params: Vec<(&str, &str)>) -> Intent {
        let mut intent = Intent::new(intent_type, target)
            .with_target(target)
            .with_confidence(IntentConfidence::high());
        for (k, v) in params {
            intent = intent.with_parameter(k, v);
        }
        intent
    }

    // ── Built-in registration ──

    #[test]
    fn test_builtin_goals_registered() {
        let reg = test_registry();
        let goals = reg.list();
        // 18 built-in goals
        assert_eq!(
            goals.len(),
            18,
            "expected 18 built-in goals, got {}",
            goals.len()
        );
    }

    #[test]
    fn test_builtin_goals_have_unique_ids() {
        let reg = test_registry();
        let goals = reg.list();
        let ids: HashSet<&str> = goals.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(ids.len(), goals.len());
    }

    #[test]
    fn test_contains_builtin() {
        let reg = test_registry();
        assert!(reg.contains("builtin.open_app"));
        assert!(reg.contains("builtin.close_app"));
        assert!(reg.contains("builtin.search_web"));
        assert!(reg.contains("builtin.lock_device"));
    }

    // ── Custom registration ──

    #[test]
    fn test_register_custom_goal() {
        let reg = GoalRegistry::new(GoalRegistryConfig::default());
        let def = GoalDefinition::new(
            "custom.test",
            "Test Goal",
            GoalCategory::Custom,
            vec![IntentType::Speak],
            GoalTemplate::new("say {text}"),
        );
        assert!(reg.register(def).is_ok());
        assert!(reg.contains("custom.test"));
    }

    #[test]
    fn test_register_duplicate_id_fails() {
        let reg = GoalRegistry::new(GoalRegistryConfig::default());
        let def = GoalDefinition::new(
            "dup",
            "First",
            GoalCategory::Custom,
            vec![IntentType::Click],
            GoalTemplate::new("click {target}"),
        );
        assert!(reg.register(def).is_ok());
        let def2 = GoalDefinition::new(
            "dup",
            "Second",
            GoalCategory::Custom,
            vec![IntentType::Click],
            GoalTemplate::new("click {target}"),
        );
        assert!(reg.register(def2).is_err());
    }

    // ── Unregister ──

    #[test]
    fn test_unregister_goal() {
        let reg = GoalRegistry::new(GoalRegistryConfig::default());
        let def = GoalDefinition::new(
            "custom.g1",
            "Test",
            GoalCategory::Custom,
            vec![IntentType::Speak],
            GoalTemplate::new("say {text}"),
        );
        reg.register(def).unwrap();
        assert!(reg.contains("custom.g1"));
        let removed = reg.unregister("custom.g1");
        assert!(removed.is_ok());
        assert!(!reg.contains("custom.g1"));
    }

    #[test]
    fn test_unregister_nonexistent_fails() {
        let reg = test_registry();
        assert!(reg.unregister("nonexistent").is_err());
    }

    // ── Duplicate aliases ──

    #[test]
    fn test_duplicate_aliases_rejected() {
        let reg = GoalRegistry::new(GoalRegistryConfig::default());
        let def1 = GoalDefinition::new(
            "g1",
            "Goal One",
            GoalCategory::Custom,
            vec![IntentType::Click],
            GoalTemplate::new("click {target}"),
        )
        .with_alias("shared_alias");
        assert!(reg.register(def1).is_ok());

        let def2 = GoalDefinition::new(
            "g2",
            "Goal Two",
            GoalCategory::Custom,
            vec![IntentType::Type],
            GoalTemplate::new("type {text}"),
        )
        .with_alias("shared_alias");
        assert!(reg.register(def2).is_err());
    }

    // ── Exact match ──

    #[test]
    fn test_exact_match_by_name() {
        let reg = test_registry();
        let intent = make_intent(IntentType::OpenApplication, "chrome", vec![]);
        let result = reg.resolve(&intent);
        match &result {
            GoalResolutionResult::Exact(m) => {
                assert_eq!(m.definition.id, "builtin.open_app");
                assert!(m.confidence > 0.9);
            }
            other => panic!(
                "expected Exact match, got {:?}",
                std::mem::discriminant(other)
            ),
        }
    }

    #[test]
    fn test_exact_match_case_insensitive() {
        let reg = test_registry();
        let intent = make_intent(IntentType::OpenApplication, "CHROME", vec![]);
        let result = reg.resolve(&intent);
        assert!(matches!(result, GoalResolutionResult::Exact(_)));
    }

    // ── Synonym match ──

    #[test]
    fn test_synonym_match() {
        let reg = test_registry();
        // "run" is a synonym for OpenApplication
        let intent = make_intent(IntentType::OpenApplication, "run chrome", vec![]);
        let result = reg.resolve(&intent);
        match &result {
            GoalResolutionResult::Synonym(m) => {
                assert_eq!(m.definition.id, "builtin.open_app");
                assert!(m.confidence > 0.8);
            }
            GoalResolutionResult::Exact(_) => {} // also acceptable
            other => panic!(
                "expected Synonym or Exact, got {:?}",
                std::mem::discriminant(other)
            ),
        }
    }

    // ── Alias match ──

    #[test]
    fn test_alias_match() {
        let reg = test_registry();
        // "launch" is an alias for OpenApplication
        let intent = make_intent(IntentType::OpenApplication, "launch", vec![]);
        let result = reg.resolve(&intent);
        match &result {
            GoalResolutionResult::Alias(m) => {
                assert_eq!(m.definition.id, "builtin.open_app");
                assert!(m.confidence > 0.85);
            }
            GoalResolutionResult::Exact(_) => {} // also acceptable
            other => panic!(
                "expected Alias or Exact, got {:?}",
                std::mem::discriminant(other)
            ),
        }
    }

    #[test]
    fn test_alias_settings() {
        let reg = test_registry();
        // "settings" is an alias for OpenSettings
        let intent = make_intent(IntentType::OpenApplication, "settings", vec![]);
        let result = reg.resolve(&intent);
        assert!(
            matches!(&result, GoalResolutionResult::Alias(m) if m.definition.id == "builtin.open_settings")
                || matches!(&result, GoalResolutionResult::Exact(m) if m.definition.id == "builtin.open_settings")
        );
    }

    // ── Partial match ──

    #[test]
    fn test_partial_match_name_contains() {
        let reg = test_registry();
        let intent = make_intent(IntentType::DeviceControl, "bright", vec![]);
        let result = reg.resolve(&intent);
        match &result {
            GoalResolutionResult::Partial(m) => {
                assert_eq!(m.definition.id, "builtin.change_brightness");
                assert!(m.confidence >= 0.3);
            }
            GoalResolutionResult::Exact(m) => {
                assert_eq!(m.definition.id, "builtin.change_brightness");
            }
            other => panic!(
                "expected Partial or Exact, got {:?}",
                std::mem::discriminant(other)
            ),
        }
    }

    // ── Confidence ordering ──

    #[test]
    fn test_exact_over_synonym() {
        let reg = test_registry();
        let intent = make_intent(IntentType::OpenApplication, "chrome", vec![]);
        let result = reg.resolve(&intent);
        match &result {
            GoalResolutionResult::Exact(m) => {
                assert!(m.confidence > 0.9, "exact match confidence should be high");
            }
            other => panic!("expected Exact, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn test_synonym_over_partial() {
        let reg = test_registry();
        let intent = make_intent(IntentType::OpenApplication, "run chrome", vec![]);
        let result = reg.resolve(&intent);
        // Should be at least Synonym (0.85), not Partial (0.5)
        assert!(
            result.confidence() >= 0.8,
            "expected confidence >= 0.8, got {}",
            result.confidence()
        );
    }

    // ── Disabled goals ──

    #[test]
    fn test_disabled_goal_not_matched() {
        let reg = GoalRegistry::new(GoalRegistryConfig::default());
        let def = GoalDefinition::new(
            "custom.disabled",
            "Disabled Goal",
            GoalCategory::Custom,
            vec![IntentType::Speak],
            GoalTemplate::new("say {text}"),
        )
        .disabled();
        reg.register(def).unwrap();

        let intent = make_intent(IntentType::Speak, "hello", vec![]);
        let result = reg.resolve(&intent);
        assert!(matches!(result, GoalResolutionResult::NoMatch(_)));
    }

    // ── Missing parameters ──

    #[test]
    fn test_has_required_parameters() {
        let def = GoalDefinition::new(
            "test",
            "Test",
            GoalCategory::Custom,
            vec![IntentType::Click],
            GoalTemplate::new("click {target}"),
        )
        .with_required_parameters(vec!["target"]);

        let mut params = HashMap::new();
        assert!(!def.has_required_parameters(&params));
        params.insert("target".into(), "button".into());
        assert!(def.has_required_parameters(&params));
    }

    // ── Capability validation ──

    #[test]
    fn test_goal_definition_capabilities() {
        let def = GoalDefinition::new(
            "test",
            "Test",
            GoalCategory::Automation,
            vec![IntentType::Click],
            GoalTemplate::new("click {target}"),
        )
        .with_capabilities(vec![
            GoalCapability::ScreenRequired,
            GoalCapability::InputRequired,
        ]);
        assert_eq!(def.required_capabilities.len(), 2);
        assert!(def
            .required_capabilities
            .contains(&GoalCapability::ScreenRequired));
    }

    // ── Statistics ──

    #[test]
    fn test_statistics_builtins() {
        let reg = test_registry();
        let stats = reg.statistics();
        assert_eq!(stats.total_goals, 18);
        assert_eq!(stats.enabled_goals, 18);
        assert_eq!(stats.disabled_goals, 0);
        assert!(stats.total_aliases > 0);
        assert!(stats.total_synonyms > 0);
        assert!(stats.builtin_goals > 0);
        assert_eq!(stats.custom_goals, 0);
    }

    #[test]
    fn test_statistics_custom_goals() {
        let reg = GoalRegistry::new(GoalRegistryConfig::default());
        let def = GoalDefinition::new(
            "custom.c1",
            "Custom One",
            GoalCategory::Custom,
            vec![IntentType::Speak],
            GoalTemplate::new("say {text}"),
        );
        reg.register(def).unwrap();
        let stats = reg.statistics();
        assert_eq!(stats.total_goals, 1);
        assert_eq!(stats.custom_goals, 1);
    }

    #[test]
    fn test_statistics_disabled() {
        let reg = GoalRegistry::new(GoalRegistryConfig::default());
        let d1 = GoalDefinition::new(
            "g1",
            "G1",
            GoalCategory::Custom,
            vec![IntentType::Click],
            GoalTemplate::new("click"),
        );
        let d2 = GoalDefinition::new(
            "g2",
            "G2",
            GoalCategory::Custom,
            vec![IntentType::Type],
            GoalTemplate::new("type"),
        )
        .disabled();
        reg.register(d1).unwrap();
        reg.register(d2).unwrap();
        let stats = reg.statistics();
        assert_eq!(stats.enabled_goals, 1);
        assert_eq!(stats.disabled_goals, 1);
    }

    // ── Clear ──

    #[test]
    fn test_clear_registry() {
        let reg = GoalRegistry::with_builtins();
        assert!(!reg.list().is_empty());
        reg.clear();
        assert!(reg.list().is_empty());
        assert_eq!(reg.statistics().total_goals, 0);
    }

    // ── Goal resolution result ──

    #[test]
    fn test_resolve_no_match() {
        let reg = GoalRegistry::new(GoalRegistryConfig::default());
        let intent = make_intent(IntentType::Speak, "hello", vec![]);
        let result = reg.resolve(&intent);
        assert!(matches!(result, GoalResolutionResult::NoMatch(_)));
        assert!(result.into_match().is_none());
    }

    #[test]
    fn test_resolve_no_match_confidence_zero() {
        let reg = GoalRegistry::new(GoalRegistryConfig::default());
        let intent = make_intent(IntentType::Speak, "hello", vec![]);
        let result = reg.resolve(&intent);
        assert!((result.confidence() - 0.0).abs() < 0.001);
    }

    // ── Goal template ──

    #[test]
    fn test_goal_template_fill() {
        let template =
            GoalTemplate::new("open {application}").with_context_keys(vec!["application"]);
        let mut params = HashMap::new();
        params.insert("application".into(), "chrome".into());
        let goal = template.fill(&params);
        assert_eq!(goal.description, "open chrome");
        assert_eq!(goal.context.get("application").unwrap(), "chrome");
    }

    #[test]
    fn test_goal_template_fill_multiple() {
        let template = GoalTemplate::new("search {query} on {engine}")
            .with_context_keys(vec!["query", "engine"]);
        let mut params = HashMap::new();
        params.insert("query".into(), "rust".into());
        params.insert("engine".into(), "google".into());
        let goal = template.fill(&params);
        assert_eq!(goal.description, "search rust on google");
    }

    #[test]
    fn test_goal_template_fill_missing_param() {
        let template =
            GoalTemplate::new("open {application}").with_context_keys(vec!["application"]);
        let params = HashMap::new();
        // Missing parameter: template retains {application} placeholder
        let goal = template.fill(&params);
        assert_eq!(goal.description, "open {application}");
    }

    // ── Goal definition builder ──

    #[test]
    fn test_goal_definition_builder() {
        let def = GoalDefinition::new(
            "test.builder",
            "Builder Test",
            GoalCategory::Communication,
            vec![IntentType::Speak],
            GoalTemplate::new("say {text}"),
        )
        .with_description("A test goal")
        .with_required_parameters(vec!["text"])
        .with_capabilities(vec![GoalCapability::AIRequired])
        .with_alias("test_alias")
        .with_synonym("syn")
        .with_version("2.0.0")
        .with_metadata(GoalMetadata::new().with_author("test").with_tag("test_tag"));

        assert_eq!(def.category, GoalCategory::Communication);
        assert_eq!(def.version, "2.0.0");
        assert_eq!(def.aliases.len(), 1);
        assert_eq!(def.synonyms.len(), 1);
        assert_eq!(def.metadata.author.unwrap(), "test");
        assert!(def.metadata.tags.contains(&"test_tag".to_string()));
    }

    // ── List by category ──

    #[test]
    fn test_list_by_category() {
        let reg = test_registry();
        let device_goals = reg.list_by_category(GoalCategory::Device);
        assert!(device_goals.len() >= 3); // brightness, volume, lock
        let all_device = device_goals
            .iter()
            .all(|g| g.category == GoalCategory::Device);
        assert!(all_device);
    }

    // ── Get by ID ──

    #[test]
    fn test_get_goal_by_id() {
        let reg = test_registry();
        let def = reg.get("builtin.open_app");
        assert!(def.is_some());
        assert_eq!(def.unwrap().name, "Open Application");
    }

    #[test]
    fn test_get_nonexistent() {
        let reg = test_registry();
        assert!(reg.get("nonexistent").is_none());
    }

    // ── Config ──

    #[test]
    fn test_config_set_get() {
        let reg = GoalRegistry::new(GoalRegistryConfig::default());
        let mut config = reg.config();
        assert!((config.minimum_confidence - 0.3).abs() < 0.001);
        config.minimum_confidence = 0.5;
        reg.set_config(config);
        let updated = reg.config();
        assert!((updated.minimum_confidence - 0.5).abs() < 0.001);
    }

    // ── Clone ──

    #[test]
    fn test_registry_clone_shares_state() {
        let reg = GoalRegistry::new(GoalRegistryConfig::default());
        let reg2 = reg.clone();
        let def = GoalDefinition::new(
            "shared",
            "Shared",
            GoalCategory::Custom,
            vec![IntentType::Speak],
            GoalTemplate::new("say {text}"),
        );
        reg.register(def).unwrap();
        assert!(reg2.contains("shared"));
    }

    // ── Thread safety (basic concurrent access) ──

    #[test]
    fn test_concurrent_access() {
        let reg = Arc::new(GoalRegistry::new(GoalRegistryConfig::default()));
        let mut handles = Vec::new();

        for i in 0..10 {
            let r = reg.clone();
            handles.push(std::thread::spawn(move || {
                let def = GoalDefinition::new(
                    format!("conc.g{}", i),
                    format!("Concurrent {}", i),
                    GoalCategory::Custom,
                    vec![IntentType::Click],
                    GoalTemplate::new("click"),
                );
                r.register(def).ok();
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(reg.list().len(), 10);
    }

    // ── DefaultGoalResolver ──

    #[test]
    fn test_default_resolver() {
        let reg = test_registry();
        let resolver = DefaultGoalResolver;
        let intent = make_intent(IntentType::OpenApplication, "chrome", vec![]);
        let result = resolver.resolve(&intent, &reg);
        match &result {
            GoalResolutionResult::Exact(m) => {
                assert_eq!(m.definition.id, "builtin.open_app");
            }
            other => panic!("expected Exact, got {:?}", std::mem::discriminant(other)),
        }
    }

    // ── Edge cases ──

    #[test]
    fn test_resolve_with_no_target() {
        let reg = test_registry();
        // Intent with unknown target but valid intent type
        let mut intent = make_intent(IntentType::Click, "unknown_target_xyz", vec![]);
        intent = intent.with_confidence(IntentConfidence::low());
        let result = reg.resolve(&intent);
        // Should still match ClickElement via intent type fallback
        match &result {
            GoalResolutionResult::Partial(m) => {
                assert_eq!(m.definition.id, "builtin.click_element");
            }
            GoalResolutionResult::Exact(_) => {}
            GoalResolutionResult::NoMatch(msg) => {
                panic!("expected match, got NoMatch: {}", msg);
            }
            _ => {}
        }
    }

    #[test]
    fn test_goal_metadata_builder() {
        let meta = GoalMetadata::new()
            .with_author("tester")
            .with_tag("important")
            .with_tag("urgent")
            .with_notes("this is a note");
        assert_eq!(meta.author.unwrap(), "tester");
        assert_eq!(meta.tags.len(), 2);
        assert_eq!(meta.notes.unwrap(), "this is a note");
    }

    #[test]
    fn test_supported_intents_field() {
        let def = GoalDefinition::new(
            "test.si",
            "SI Test",
            GoalCategory::Custom,
            vec![IntentType::Search, IntentType::BrowserAction],
            GoalTemplate::new("search {query}"),
        );
        assert!(def.supported_intents.contains(&IntentType::Search));
        assert!(def.supported_intents.contains(&IntentType::BrowserAction));
        assert!(!def.supported_intents.contains(&IntentType::Click));
    }

    #[test]
    fn test_empty_registry_stats() {
        let reg = GoalRegistry::new(GoalRegistryConfig::default());
        let stats = reg.statistics();
        assert_eq!(stats.total_goals, 0);
        assert_eq!(stats.enabled_goals, 0);
        assert_eq!(stats.disabled_goals, 0);
    }

    #[test]
    fn test_resolve_with_synonym_disabled() {
        let config = GoalRegistryConfig {
            enable_synonyms: false,
            ..Default::default()
        };
        let reg = GoalRegistry::new(config);
        // Register a goal with a synonym
        let def = GoalDefinition::new(
            "test.audio",
            "Audio Control",
            GoalCategory::Device,
            vec![IntentType::DeviceControl],
            GoalTemplate::new("set volume to {value}"),
        )
        .with_synonym("sound");
        reg.register(def).unwrap();

        // Attempt synonym match — should fail since synonyms are disabled
        let intent = make_intent(IntentType::DeviceControl, "sound", vec![]);
        let result = reg.resolve(&intent);
        // Should not be a Synonym match
        assert!(!matches!(result, GoalResolutionResult::Synonym(_)));
    }

    #[test]
    fn test_resolve_with_aliases_disabled() {
        let config = GoalRegistryConfig {
            enable_aliases: false,
            ..Default::default()
        };
        let reg = GoalRegistry::new(config);
        // Register a goal with an alias
        let def = GoalDefinition::new(
            "test.myalias",
            "My App",
            GoalCategory::Application,
            vec![IntentType::OpenApplication],
            GoalTemplate::new("open {application}"),
        )
        .with_alias("myapp");
        reg.register(def).unwrap();

        let intent = make_intent(IntentType::OpenApplication, "myapp", vec![]);
        let result = reg.resolve(&intent);
        // Should not be an Alias match
        assert!(!matches!(result, GoalResolutionResult::Alias(_)));
    }

    #[test]
    fn test_intent_type_device_control_resolves() {
        let reg = test_registry();
        let intent = make_intent(
            IntentType::DeviceControl,
            "brightness",
            vec![("value", "75")],
        );
        let result = reg.resolve(&intent);
        match &result {
            GoalResolutionResult::Partial(m) | GoalResolutionResult::Exact(m) => {
                assert_eq!(m.definition.id, "builtin.change_brightness");
            }
            other => panic!(
                "expected match for device control, got {:?}",
                std::mem::discriminant(other)
            ),
        }
    }

    #[test]
    fn test_intent_type_system_action_resolves() {
        let reg = test_registry();
        let intent = make_intent(IntentType::SystemAction, "shutdown", vec![]);
        let result = reg.resolve(&intent);
        match &result {
            GoalResolutionResult::Partial(m) | GoalResolutionResult::Exact(m) => {
                assert_eq!(m.definition.id, "builtin.shutdown");
            }
            GoalResolutionResult::NoMatch(msg) => {
                panic!("no match: {}", msg);
            }
            other => panic!("expected match, got {:?}", std::mem::discriminant(other)),
        }
    }

    // ── GoalFill from resolved result ──

    #[test]
    fn test_resolve_and_fill_goal() {
        let reg = test_registry();
        let intent = make_intent(IntentType::OpenApplication, "notepad", vec![]);
        let result = reg.resolve(&intent);
        let gm = result.into_match().expect("expected match");
        let goal = gm.definition.planner_template.fill(&gm.matched_parameters);
        assert_eq!(goal.description, "open notepad");
    }

    #[test]
    fn test_resolve_search_and_fill() {
        let reg = test_registry();
        let intent = make_intent(
            IntentType::Search,
            "rust programming",
            vec![("query", "rust programming")],
        );
        let result = reg.resolve(&intent);
        let gm = result.into_match().expect("expected match");
        let goal = gm.definition.planner_template.fill(&gm.matched_parameters);
        assert!(goal.description.contains("rust"));
    }
}
