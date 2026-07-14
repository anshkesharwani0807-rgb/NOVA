use async_trait::async_trait;
use nova_ai::tool::{Tool, ToolSpec};
use nova_kernel::{NovaError, Result};
use parking_lot::RwLock;
use std::sync::Arc;
use uuid::Uuid;

use crate::engine::VisionEngine;
use crate::events::{VisionEvent, VisionEventPayload};
use crate::permission::{VisionCapability, VisionPermissionManager};

struct ToolContext {
    engine: Arc<VisionEngine>,
    permissions: Arc<VisionPermissionManager>,
    audit: Arc<RwLock<Vec<VisionEvent>>>,
}

impl ToolContext {
    async fn check_permission(&self, cap: &VisionCapability) -> Result<()> {
        if !self.permissions.is_granted(cap) {
            return Err(NovaError::new(
                nova_kernel::ErrorCategory::ConsentRequired,
                "ERR_VISION_PERMISSION_DENIED",
                &format!("Permission denied for '{}'", cap.name()),
            ));
        }
        Ok(())
    }

    fn log(&self, payload: VisionEventPayload) {
        let event = VisionEvent::new(Uuid::new_v4(), payload);
        nova_kernel::log_activity(
            "vision",
            event.action_name(),
            &event.description(),
            Some(event.correlation_id),
        );
        self.audit.write().push(event);
    }
}

macro_rules! vision_tool {
    ($name:ident, $spec_name:expr, $desc:expr, $params:expr, $cap:ident) => {
        pub struct $name {
            ctx: ToolContext,
        }
        impl $name {
            pub fn new(
                engine: Arc<VisionEngine>,
                permissions: Arc<VisionPermissionManager>,
                audit: Arc<RwLock<Vec<VisionEvent>>>,
            ) -> Self {
                Self {
                    ctx: ToolContext {
                        engine,
                        permissions,
                        audit,
                    },
                }
            }
        }
        #[async_trait]
        impl Tool for $name {
            fn spec(&self) -> ToolSpec {
                ToolSpec::new($spec_name, $desc, $params)
            }
            async fn invoke(&self, arguments: &str) -> Result<String> {
                self.ctx.check_permission(&VisionCapability::$cap).await?;
                let start = std::time::Instant::now();
                let result = self.execute(arguments).await;
                let duration_ms = start.elapsed().as_millis() as u64;
                let success = result.is_ok();
                self.ctx.log(VisionEventPayload::VisionToolInvoked {
                    tool: $spec_name.to_string(),
                    duration_ms,
                    success,
                });
                result
            }
        }
    };
}

vision_tool!(
    DescribeImageTool,
    "describe_image",
    "Generate a natural language description of an image",
    r#"{"type":"object","properties":{"image_path":{"type":"string"},"image_data":{"type":"string","description":"Base64-encoded image data"}},"required":[]}"#,
    GalleryRead
);

impl DescribeImageTool {
    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
        let bytes = get_image_bytes(&args, &self.ctx.engine).await?;
        let result = self.ctx.engine.caption_image(&bytes, None).await?;
        Ok(serde_json::json!({
            "caption": result.caption,
            "confidence": result.confidence,
            "duration_ms": result.duration_ms,
        })
        .to_string())
    }
}

vision_tool!(
    ExtractTextTool,
    "extract_text",
    "Extract text from an image using OCR",
    r#"{"type":"object","properties":{"image_path":{"type":"string"},"image_data":{"type":"string"}},"required":[]}"#,
    GalleryRead
);

impl ExtractTextTool {
    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
        let bytes = get_image_bytes(&args, &self.ctx.engine).await?;
        let result = self.ctx.engine.ocr_image(&bytes, None).await?;
        Ok(serde_json::json!({
            "text": result.text,
            "confidence": result.confidence,
            "language": result.language,
            "blocks": result.blocks.len(),
            "duration_ms": result.duration_ms,
        })
        .to_string())
    }
}

vision_tool!(
    FindObjectsTool,
    "find_objects",
    "Detect objects in an image",
    r#"{"type":"object","properties":{"image_path":{"type":"string"},"image_data":{"type":"string"}},"required":[]}"#,
    GalleryRead
);

impl FindObjectsTool {
    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
        let bytes = get_image_bytes(&args, &self.ctx.engine).await?;
        let result = self.ctx.engine.detect_objects(&bytes).await?;
        let objects: Vec<serde_json::Value> = result
            .objects
            .into_iter()
            .map(|o| {
                serde_json::json!({
                    "label": o.label,
                    "confidence": o.confidence,
                    "bounding_box": {
                        "x": o.bounding_box.x,
                        "y": o.bounding_box.y,
                        "w": o.bounding_box.w,
                        "h": o.bounding_box.h,
                    },
                })
            })
            .collect();
        Ok(serde_json::json!({"objects": objects}).to_string())
    }
}

vision_tool!(
    SearchImagesTool,
    "search_images",
    "Search indexed images by text, tags, or similarity",
    r#"{"type":"object","properties":{"query":{"type":"string","description":"Search text"},"max_results":{"type":"integer","default":10}},"required":["query"]}"#,
    VisualSearch
);

impl SearchImagesTool {
    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let max = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;
        Ok(serde_json::json!({
            "query": query,
            "max_results": max,
            "note": "Visual search requires indexed images. Use index_image tool first."
        })
        .to_string())
    }
}

vision_tool!(
    AnalyzePhotoTool,
    "analyze_photo",
    "Perform full analysis of an image (objects, scene, quality, colors, tags, faces)",
    r#"{"type":"object","properties":{"image_path":{"type":"string"},"image_data":{"type":"string"}},"required":[]}"#,
    GalleryRead
);

impl AnalyzePhotoTool {
    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
        let bytes = get_image_bytes(&args, &self.ctx.engine).await?;
        let analysis = self.ctx.engine.analyze(&bytes).await?;
        let json = serde_json::json!({
            "duration_ms": analysis.duration_ms,
            "caption": analysis.caption.map(|c| serde_json::json!({
                "text": c.caption, "confidence": c.confidence
            })),
            "ocr": analysis.ocr.map(|o| serde_json::json!({
                "text": o.text, "confidence": o.confidence
            })),
            "objects": analysis.objects.map(|o| o.objects.iter().map(|obj| serde_json::json!({
                "label": obj.label, "confidence": obj.confidence
            })).collect::<Vec<_>>()),
            "scene": analysis.scene.map(|s| s.scenes.iter().map(|sc| serde_json::json!({
                "label": sc.label.as_str(), "confidence": sc.confidence
            })).collect::<Vec<_>>()),
            "quality": analysis.quality.map(|q| serde_json::json!({
                "blur_score": q.blur_score,
                "is_blurry": q.is_blurry,
                "aesthetics": q.aesthetics,
            })),
            "colors": analysis.colors.map(|c| serde_json::json!({
                "dominant": c.dominant_colors.len(),
                "colorfulness": c.colorfulness,
            })),
            "tags": analysis.tags.map(|t| t.tags.iter().map(|tag| serde_json::json!({
                "tag": tag.tag, "confidence": tag.confidence, "category": tag.category.as_str()
            })).collect::<Vec<_>>()),
            "faces": analysis.faces.map(|f| f.faces.len()),
        });
        Ok(json.to_string())
    }
}

vision_tool!(
    GenerateCaptionTool,
    "generate_caption",
    "Generate a caption for an image (alias for describe_image)",
    r#"{"type":"object","properties":{"image_path":{"type":"string"},"image_data":{"type":"string"}},"required":[]}"#,
    GalleryRead
);

impl GenerateCaptionTool {
    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
        let bytes = get_image_bytes(&args, &self.ctx.engine).await?;
        let result = self.ctx.engine.caption_image(&bytes, None).await?;
        Ok(serde_json::json!({
            "caption": result.caption,
            "confidence": result.confidence,
            "duration_ms": result.duration_ms,
        })
        .to_string())
    }
}

async fn get_image_bytes(args: &serde_json::Value, engine: &VisionEngine) -> Result<Vec<u8>> {
    if let Some(path) = args.get("image_path").and_then(|v| v.as_str()) {
        if !path.is_empty() {
            let loaded = engine.loader.load_from_path(path).await?;
            return Ok(loaded.data);
        }
    }
    if let Some(b64) = args.get("image_data").and_then(|v| v.as_str()) {
        use base64::Engine;
        return base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| {
                NovaError::new(
                    nova_kernel::ErrorCategory::Internal,
                    "ERR_VISION_BAD_BASE64",
                    &format!("Failed to decode base64 image data: {e}"),
                )
            });
    }
    Err(NovaError::new(
        nova_kernel::ErrorCategory::Internal,
        "ERR_VISION_NO_IMAGE",
        "Provide either image_path or image_data",
    ))
}

pub struct VisionToolkit {
    pub tools: Vec<Arc<dyn Tool>>,
}

impl VisionToolkit {
    pub fn new(
        engine: Arc<VisionEngine>,
        permissions: Arc<VisionPermissionManager>,
        audit: Arc<RwLock<Vec<VisionEvent>>>,
    ) -> Self {
        let tools: Vec<Arc<dyn Tool>> = vec![
            Arc::new(DescribeImageTool::new(
                engine.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(ExtractTextTool::new(
                engine.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(FindObjectsTool::new(
                engine.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(SearchImagesTool::new(
                engine.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(AnalyzePhotoTool::new(
                engine.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(GenerateCaptionTool::new(engine, permissions, audit)),
        ];
        Self { tools }
    }

    pub fn count(&self) -> usize {
        self.tools.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::mock::MockVisionProvider;

    fn setup() -> (
        Arc<VisionEngine>,
        Arc<VisionPermissionManager>,
        Arc<RwLock<Vec<VisionEvent>>>,
    ) {
        let provider =
            Arc::new(MockVisionProvider::new()) as Arc<dyn crate::providers::VisionProvider>;
        let engine = Arc::new(VisionEngine::new(provider));
        let perms = Arc::new(VisionPermissionManager::new());
        let audit = Arc::new(RwLock::new(Vec::new()));
        (engine, perms, audit)
    }

    #[tokio::test]
    async fn test_describe_permission_denied() {
        let (e, p, a) = setup();
        let tool = DescribeImageTool::new(e, p, a);
        let result = tool.invoke(r#"{"image_data":""}"#).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_extract_text_permission_denied() {
        let (e, p, a) = setup();
        let tool = ExtractTextTool::new(e, p, a);
        let result = tool.invoke(r#"{"image_data":""}"#).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_toolkit_count() {
        let (e, p, a) = setup();
        let tk = VisionToolkit::new(e, p, a);
        assert_eq!(tk.count(), 6);
    }

    #[tokio::test]
    async fn test_granted_describe() {
        let (e, p, a) = setup();
        p.grant(&VisionCapability::GalleryRead);
        let tool = DescribeImageTool::new(e, p, a);
        let result = tool.invoke(r#"{"image_data":"aGVsbG8="}"#).await;
        assert!(result.is_ok());
        let val: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(val.get("caption").is_some());
    }
}
