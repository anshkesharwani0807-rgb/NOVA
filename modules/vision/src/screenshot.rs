use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiElementType {
    Button,
    TextBlock,
    Form,
    Dialog,
    NavigationBar,
    ErrorDialog,
    PermissionDialog,
    InputField,
    Checkbox,
    RadioButton,
    Dropdown,
    Icon,
    Image,
    Link,
    List,
    Card,
    Tab,
    Slider,
    Toggle,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiElement {
    pub element_type: UiElementType,
    pub confidence: f64,
    pub bounding_box: BoundingRect,
    pub text: Option<String>,
    pub enabled: Option<bool>,
    pub focused: Option<bool>,
    pub selected: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotAnalysis {
    pub elements: Vec<UiElement>,
    pub text_blocks: Vec<UiElement>,
    pub buttons: Vec<UiElement>,
    pub dialogs: Vec<UiElement>,
    pub navigation_bars: Vec<UiElement>,
    pub forms: Vec<UiElement>,
    pub error_dialogs: Vec<UiElement>,
    pub permission_dialogs: Vec<UiElement>,
    pub has_text_inputs: bool,
    pub has_scrollable_content: bool,
    pub estimated_complexity: f64,
    pub duration_ms: u64,
}

impl ScreenshotAnalysis {
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    pub fn has_errors(&self) -> bool {
        !self.error_dialogs.is_empty()
    }

    pub fn has_permission_request(&self) -> bool {
        !self.permission_dialogs.is_empty()
    }

    pub fn summary(&self) -> String {
        let mut parts = vec![];
        if !self.buttons.is_empty() {
            parts.push(format!("{} buttons", self.buttons.len()));
        }
        if !self.text_blocks.is_empty() {
            parts.push(format!("{} text blocks", self.text_blocks.len()));
        }
        if !self.dialogs.is_empty() {
            parts.push(format!("{} dialogs", self.dialogs.len()));
        }
        if !self.navigation_bars.is_empty() {
            parts.push(format!("{} nav bars", self.navigation_bars.len()));
        }
        if !self.forms.is_empty() {
            parts.push(format!("{} forms", self.forms.len()));
        }
        if self.has_errors() {
            parts.push("has errors".to_string());
        }
        if self.has_permission_request() {
            parts.push("has permission request".to_string());
        }
        if parts.is_empty() {
            format!("{} elements detected", self.elements.len())
        } else {
            format!("{} elements: {}", self.elements.len(), parts.join(", "))
        }
    }
}

#[async_trait::async_trait]
pub trait ScreenshotAnalyzer: Send + Sync {
    async fn analyze_screenshot(&self, bytes: &[u8]) -> nova_kernel::Result<ScreenshotAnalysis>;
}

pub struct MockScreenshotAnalyzer;

impl MockScreenshotAnalyzer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockScreenshotAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ScreenshotAnalyzer for MockScreenshotAnalyzer {
    async fn analyze_screenshot(&self, _bytes: &[u8]) -> nova_kernel::Result<ScreenshotAnalysis> {
        Ok(ScreenshotAnalysis {
            elements: vec![
                UiElement {
                    element_type: UiElementType::NavigationBar,
                    confidence: 0.95,
                    bounding_box: BoundingRect {
                        x: 0.0,
                        y: 0.0,
                        w: 1080.0,
                        h: 80.0,
                    },
                    text: Some("Mock App".to_string()),
                    enabled: Some(true),
                    focused: Some(false),
                    selected: Some(false),
                },
                UiElement {
                    element_type: UiElementType::Button,
                    confidence: 0.92,
                    bounding_box: BoundingRect {
                        x: 100.0,
                        y: 400.0,
                        w: 200.0,
                        h: 60.0,
                    },
                    text: Some("Submit".to_string()),
                    enabled: Some(true),
                    focused: Some(false),
                    selected: Some(false),
                },
                UiElement {
                    element_type: UiElementType::TextBlock,
                    confidence: 0.88,
                    bounding_box: BoundingRect {
                        x: 50.0,
                        y: 200.0,
                        w: 500.0,
                        h: 100.0,
                    },
                    text: Some("Welcome to the app! Please sign in to continue.".to_string()),
                    enabled: Some(true),
                    focused: Some(false),
                    selected: Some(false),
                },
                UiElement {
                    element_type: UiElementType::InputField,
                    confidence: 0.85,
                    bounding_box: BoundingRect {
                        x: 100.0,
                        y: 320.0,
                        w: 400.0,
                        h: 50.0,
                    },
                    text: Some("username@example.com".to_string()),
                    enabled: Some(true),
                    focused: Some(true),
                    selected: Some(false),
                },
            ],
            text_blocks: vec![UiElement {
                element_type: UiElementType::TextBlock,
                confidence: 0.88,
                bounding_box: BoundingRect {
                    x: 50.0,
                    y: 200.0,
                    w: 500.0,
                    h: 100.0,
                },
                text: Some("Welcome to the app! Please sign in to continue.".to_string()),
                enabled: Some(true),
                focused: Some(false),
                selected: Some(false),
            }],
            buttons: vec![UiElement {
                element_type: UiElementType::Button,
                confidence: 0.92,
                bounding_box: BoundingRect {
                    x: 100.0,
                    y: 400.0,
                    w: 200.0,
                    h: 60.0,
                },
                text: Some("Submit".to_string()),
                enabled: Some(true),
                focused: Some(false),
                selected: Some(false),
            }],
            dialogs: vec![],
            navigation_bars: vec![UiElement {
                element_type: UiElementType::NavigationBar,
                confidence: 0.95,
                bounding_box: BoundingRect {
                    x: 0.0,
                    y: 0.0,
                    w: 1080.0,
                    h: 80.0,
                },
                text: Some("Mock App".to_string()),
                enabled: Some(true),
                focused: Some(false),
                selected: Some(false),
            }],
            forms: vec![],
            error_dialogs: vec![],
            permission_dialogs: vec![],
            has_text_inputs: true,
            has_scrollable_content: false,
            estimated_complexity: 0.3,
            duration_ms: 45,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_screenshot_analysis() {
        let analyzer = MockScreenshotAnalyzer::new();
        let result = analyzer.analyze_screenshot(b"mock data").await.unwrap();
        assert_eq!(result.element_count(), 4);
        assert!(!result.buttons.is_empty());
        assert!(!result.text_blocks.is_empty());
        assert!(!result.navigation_bars.is_empty());
        assert!(result.has_text_inputs);
        assert!(!result.summary().is_empty());
    }

    #[tokio::test]
    async fn test_screenshot_summary() {
        let analyzer = MockScreenshotAnalyzer::new();
        let result = analyzer.analyze_screenshot(b"mock data").await.unwrap();
        let summary = result.summary();
        assert!(summary.contains("buttons"));
        assert!(summary.contains("text"));
    }

    #[test]
    fn test_empty_screenshot() {
        let analysis = ScreenshotAnalysis {
            elements: vec![],
            text_blocks: vec![],
            buttons: vec![],
            dialogs: vec![],
            navigation_bars: vec![],
            forms: vec![],
            error_dialogs: vec![],
            permission_dialogs: vec![],
            has_text_inputs: false,
            has_scrollable_content: false,
            estimated_complexity: 0.0,
            duration_ms: 0,
        };
        assert!(analysis.is_empty());
    }
}
