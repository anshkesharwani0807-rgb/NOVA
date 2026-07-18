use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelFormat {
    RGBA8,
    BGRA8,
    RGB24,
    BGR24,
    NV12,
    YUY2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenCaptureConfig {
    pub target_fps: u32,
    pub region: Option<ScreenRegion>,
    pub include_cursor: bool,
    pub downscale_factor: Option<f32>,
}

impl Default for ScreenCaptureConfig {
    fn default() -> Self {
        Self {
            target_fps: 30,
            region: None,
            include_cursor: true,
            downscale_factor: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ScreenRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedFrame {
    pub frame_id: String,
    pub timestamp: i64,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub data: Vec<u8>,
    pub region: Option<ScreenRegion>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn center(&self) -> Point {
        Point {
            x: self.x + self.width as i32 / 2,
            y: self.y + self.height as i32 / 2,
        }
    }

    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.x
            && point.x < self.x + self.width as i32
            && point.y >= self.y
            && point.y < self.y + self.height as i32
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UIElementType {
    Button,
    TextBlock,
    Edit,
    ComboBox,
    List,
    Tree,
    Tab,
    Menu,
    Toolbar,
    StatusBar,
    ScrollBar,
    Slider,
    CheckBox,
    RadioButton,
    Link,
    Image,
    Document,
    Pane,
    Window,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIElementRef {
    pub element_id: String,
    pub element_type: UIElementType,
    pub bounds: Rect,
    pub text: Option<String>,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIElement {
    pub element_id: String,
    pub element_type: UIElementType,
    pub bounds: Rect,
    pub name: Option<String>,
    pub text: Option<String>,
    pub automation_id: Option<String>,
    pub class_name: Option<String>,
    pub children: Vec<UIElement>,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UITree {
    pub root: UIElement,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundingQuery {
    pub query: String,
    pub context: Option<String>,
    pub max_results: usize,
    pub confidence_threshold: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundingResult {
    pub element: UIElementRef,
    pub confidence: f32,
    pub match_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCRResult {
    pub text: String,
    pub confidence: f32,
    pub language: String,
    pub regions: Vec<OCRRegion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCRRegion {
    pub text: String,
    pub confidence: f32,
    pub bounds: Rect,
}
