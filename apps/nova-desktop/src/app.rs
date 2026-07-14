use nova_kernel::NovaConfig;
use nova_memory::{MemoryCategory, MemoryEngine, MemoryRecord, Query};
use nova_search::UniversalSearch;

#[derive(Default, PartialEq)]
pub enum AppTab {
    #[default]
    Search,
    Memory,
    Voice,
    Activity,
    Health,
    Settings,
}

pub struct NovaDesktopApp {
    // Kernel modules
    pub kernel: Option<std::sync::Arc<nova_kernel::Kernel>>,
    pub memory: Option<std::sync::Arc<MemoryEngine>>,
    pub search: Option<std::sync::Arc<UniversalSearch>>,

    // UI state
    pub active_tab: AppTab,
    pub status_message: String,
    pub is_initialized: bool,

    // Search panel state
    pub search_query: String,
    pub search_results: Vec<nova_search::SearchResult>,
    pub search_mode: SearchMode,

    // Memory panel state
    pub memory_records: Vec<MemoryRecord>,
    pub memory_detail: Option<MemoryRecord>,
    pub new_memory_text: String,
    pub memory_filter: String,

    // Settings panel state
    pub config_json: String,
    pub config_edit: String,

    // Activity panel state
    pub activity_trail: Vec<String>,
    pub egress_log: Vec<String>,

    // Voice panel state
    pub voice_status: String,
    pub wake_word: String,

    // Health panel
    pub health_report: String,
}

#[derive(PartialEq)]
pub enum SearchMode {
    Text,
    NaturalLanguage,
}

impl Default for NovaDesktopApp {
    fn default() -> Self {
        Self {
            kernel: None,
            memory: None,
            search: None,
            active_tab: AppTab::default(),
            status_message: String::new(),
            is_initialized: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_mode: SearchMode::Text,
            memory_records: Vec::new(),
            memory_detail: None,
            new_memory_text: String::new(),
            memory_filter: String::new(),
            config_json: String::new(),
            config_edit: String::new(),
            activity_trail: Vec::new(),
            egress_log: Vec::new(),
            voice_status: "Idle".to_string(),
            wake_word: "NOVA".to_string(),
            health_report: String::new(),
        }
    }
}

impl NovaDesktopApp {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn initialize(&mut self) {
        let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("crate is nested two levels under the project root");
        let base = project_root.join(".nova-runtime");
        let config_dir = base.join("config");
        let log_dir = base.join("logs");
        let _ = std::fs::create_dir_all(&config_dir);
        let _ = std::fs::create_dir_all(&log_dir);

        match nova_kernel::Kernel::bootstrap(&config_dir, &log_dir) {
            Ok(k) => {
                let memory = std::sync::Arc::new(MemoryEngine::new(k.clone()));
                let search = std::sync::Arc::new(UniversalSearch::new(k.clone()));
                let _ = k.registry.register(memory.clone());
                let _ = k.registry.register(search.clone());

                self.kernel = Some(k);
                self.memory = Some(memory);
                self.search = Some(search);
                self.is_initialized = true;
                self.status_message = "NOVA initialized successfully".to_string();
                self.refresh_all();
            }
            Err(e) => {
                self.status_message = format!("Failed to initialize NOVA: {}", e);
            }
        }
    }

    pub fn refresh_all(&mut self) {
        self.refresh_config();
        self.refresh_activity_trail();
        self.refresh_egress_log();
        self.refresh_memory_list();
        self.refresh_health();
    }

    pub fn refresh_config(&mut self) {
        let cfg = nova_kernel::get_config();
        self.config_json = serde_json::to_string_pretty(&cfg).unwrap_or_default();
        self.config_edit = self.config_json.clone();
    }

    pub fn refresh_activity_trail(&mut self) {
        let entries = nova_kernel::get_recent_activity();
        self.activity_trail = entries
            .iter()
            .map(|e| format!("[{}] {}: {}", e.timestamp, e.module, e.action))
            .collect();
    }

    pub fn refresh_egress_log(&mut self) {
        let entries = nova_kernel::get_recent_egress();
        self.egress_log = entries
            .iter()
            .map(|e| {
                format!(
                    "[{}] -> {}: consent={}",
                    e.timestamp, e.destination, e.consent_granted
                )
            })
            .collect();
    }

    pub fn refresh_memory_list(&mut self) {
        if let Some(m) = &self.memory {
            let mut q = Query::new();
            if !self.memory_filter.is_empty() {
                q.text = Some(self.memory_filter.clone());
            }
            self.memory_records = m.find(&q).unwrap_or_default();
        }
    }

    pub fn refresh_health(&mut self) {
        if let Some(k) = &self.kernel {
            let report = k.registry.list();
            self.health_report =
                serde_json::to_string_pretty(&report).unwrap_or_else(|_| "[]".to_string());
        }
    }

    pub fn do_search(&mut self) {
        if self.search_query.is_empty() {
            return;
        }
        if let Some(s) = &self.search {
            let results = match self.search_mode {
                SearchMode::Text => s.search_text(&self.search_query, Some(50)),
                SearchMode::NaturalLanguage => s.search_nl(&self.search_query, Some(50)),
            };
            self.search_results = results.unwrap_or_default();
        }
    }

    pub fn save_config(&mut self) {
        if let Ok(cfg) = serde_json::from_str::<NovaConfig>(&self.config_edit) {
            let _ = nova_kernel::update_config(cfg);
            self.status_message = "Config updated".to_string();
            self.refresh_config();
        } else {
            self.status_message = "Invalid config JSON".to_string();
        }
    }

    pub fn add_memory(&mut self) {
        if self.new_memory_text.is_empty() {
            return;
        }
        if let Some(m) = &self.memory {
            let rec = MemoryRecord::new(MemoryCategory::Knowledge, &self.new_memory_text, "");
            let _ = m.insert(&rec);
            self.new_memory_text.clear();
            self.refresh_memory_list();
            self.status_message = "Memory added".to_string();
        }
    }
}
