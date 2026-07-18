pub fn create() -> crate::ScreenResult<std::sync::Arc<dyn crate::OCREngine>> {
    #[cfg(target_os = "windows")]
    {
        Ok(std::sync::Arc::new(WindowsOCREngine::new()?))
    }
    #[cfg(target_os = "android")]
    {
        Ok(std::sync::Arc::new(AndroidOCREngine::new()?))
    }
    #[cfg(not(any(target_os = "windows", target_os = "android")))]
    {
        Err(crate::ScreenError::UnsupportedPlatform)
    }
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use crate::{CapturedFrame, OCRRegion, OCRResult, Rect, ScreenError, ScreenResult};
    use async_trait::async_trait;
    use windows::Foundation::Rect as WinRTRect;
    use windows::Graphics::Imaging::{BitmapBufferAccessMode, BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Win32::System::WinRT::IMemoryBufferByteAccess;
    use windows::core::Interface;

    pub struct WindowsOCREngine {
        engine: OcrEngine,
    }

    impl WindowsOCREngine {
        pub fn new() -> ScreenResult<Self> {
            let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(|e| {
                ScreenError::OCRFailed(format!("Windows OCR unavailable: {e}"))
            })?;
            Ok(Self { engine })
        }

        fn extract_pixels(
            frame: &CapturedFrame,
            region: Option<Rect>,
        ) -> ScreenResult<(Vec<u8>, u32, u32)> {
            let src_stride = frame.width as usize * 4;

            if frame.format != crate::PixelFormat::BGRA8 {
                return Err(ScreenError::OCRFailed(
                    "OCR requires BGRA8 pixel format".into(),
                ));
            }

            let (out_data, out_w, out_h) = if let Some(r) = region {
                let x = r.x.max(0) as usize;
                let y = r.y.max(0) as usize;
                let w = (r.width as usize).min(frame.width as usize - x);
                let h = (r.height as usize).min(frame.height as usize - y);
                let dst_stride = w * 4;
                let mut buf = vec![0u8; dst_stride * h];
                for row in 0..h {
                    let src_off = (y + row) * src_stride + x * 4;
                    let dst_off = row * dst_stride;
                    buf[dst_off..dst_off + dst_stride]
                        .copy_from_slice(&frame.data[src_off..src_off + dst_stride]);
                }
                (buf, w as u32, h as u32)
            } else {
                (frame.data.clone(), frame.width, frame.height)
            };

            Ok((out_data, out_w, out_h))
        }

        fn data_to_software_bitmap(
            data: &[u8],
            width: u32,
            height: u32,
        ) -> ScreenResult<SoftwareBitmap> {
            let bitmap = SoftwareBitmap::Create(
                BitmapPixelFormat::Bgra8,
                width as i32,
                height as i32,
            )
            .map_err(|e| ScreenError::OCRFailed(format!("SoftwareBitmap create: {e}")))?;

            let buffer = bitmap
                .LockBuffer(BitmapBufferAccessMode::ReadWrite)
                .map_err(|e| ScreenError::OCRFailed(format!("LockBuffer: {e}")))?;

            let reference = buffer
                .CreateReference()
                .map_err(|e| ScreenError::OCRFailed(format!("CreateReference: {e}")))?;

            let byte_access: IMemoryBufferByteAccess = reference.cast()?;

            let mut buffer_ptr: *mut u8 = std::ptr::null_mut();
            let mut capacity: u32 = 0;
            unsafe {
                byte_access
                    .GetBuffer(&mut buffer_ptr, &mut capacity)
                    .map_err(|e| ScreenError::OCRFailed(format!("GetBuffer: {e}")))?;
                let len = (width as usize * height as usize) * 4;
                std::slice::from_raw_parts_mut(buffer_ptr, len).copy_from_slice(data);
            }

            Ok(bitmap)
        }

        fn winrt_rect_to_rect(rect: WinRTRect, bounds: &Rect) -> Rect {
            Rect {
                x: (rect.X + bounds.x as f32).max(0.0) as i32,
                y: (rect.Y + bounds.y as f32).max(0.0) as i32,
                width: rect.Width.max(0.0) as u32,
                height: rect.Height.max(0.0) as u32,
            }
        }

        pub async fn recognize_impl(
            &self,
            frame: &CapturedFrame,
            region: Option<Rect>,
        ) -> ScreenResult<OCRResult> {
            let (data, width, height) = Self::extract_pixels(frame, region)?;
            let bitmap = Self::data_to_software_bitmap(&data, width, height)?;
            let ocr_result = self
                .engine
                .RecognizeAsync(&bitmap)
                .map_err(|e| ScreenError::OCRFailed(format!("RecognizeAsync: {e}")))?
                .get()
                .map_err(|e| ScreenError::OCRFailed(format!("Recognize failed: {e}")))?;

            let text = ocr_result
                .Text()
                .map(|s| s.to_string())
                .unwrap_or_default();

            let language = "en".to_string();

            let bounds = region.unwrap_or(Rect {
                x: 0,
                y: 0,
                width,
                height,
            });

            let mut regions: Vec<OCRRegion> = Vec::new();

            if let Ok(lines) = ocr_result.Lines() {
                for line in lines {
                    if let Ok(words) = line.Words() {
                        for word in words {
                            let word_text = word
                                .Text()
                                .map(|s| s.to_string())
                                .unwrap_or_default();

                            if let Ok(winrt_rect) = word.BoundingRect() {
                                regions.push(OCRRegion {
                                    text: word_text,
                                    confidence: 0.8,
                                    bounds: Self::winrt_rect_to_rect(winrt_rect, &bounds),
                                });
                            }
                        }
                    }
                }
            }

            Ok(OCRResult {
                text,
                confidence: 0.8,
                language,
                regions,
            })
        }
    }

    #[async_trait]
    impl crate::OCREngine for WindowsOCREngine {
        fn id(&self) -> &str {
            "windows-ocr"
        }

        async fn recognize(&self, frame: &CapturedFrame) -> ScreenResult<OCRResult> {
            self.recognize_impl(frame, None).await
        }

        async fn recognize_region(
            &self,
            frame: &CapturedFrame,
            region: Rect,
        ) -> ScreenResult<OCRResult> {
            self.recognize_impl(frame, Some(region)).await
        }

        fn supported_languages(&self) -> Vec<String> {
            vec![
                "en".to_string(),
                "es".to_string(),
                "fr".to_string(),
                "de".to_string(),
                "zh".to_string(),
                "ja".to_string(),
                "ko".to_string(),
            ]
        }
    }
}

#[cfg(target_os = "windows")]
pub use windows_impl::WindowsOCREngine;

// ---------------------------------------------------------------------------
// Android OCR — ML Kit Text Recognition via JNI
// ---------------------------------------------------------------------------
//
// Fully self-contained Rust-side implementation:
//   1. Converts BGRA8 frame data → RGBA8 (for Android Bitmap.Config.ARGB_8888)
//   2. Creates an android.graphics.Bitmap via JNI
//   3. Creates com.google.mlkit.vision.common.InputImage from the Bitmap
//   4. Calls TextRecognizer.process() → Tasks.await() to get Text result
//   5. Extracts lines/elements with bounding boxes
//
// No Kotlin-side JNI entry points required — ML Kit classes are obtained
// via static JNI calls from Rust.

#[cfg(target_os = "android")]
use jni::objects::{GlobalRef, JObject, JValue};
#[cfg(target_os = "android")]
use jni::JNIEnv;
#[cfg(target_os = "android")]
use std::sync::OnceLock;

#[cfg(target_os = "android")]
static RECOGNIZER: OnceLock<GlobalRef> = OnceLock::new();

/// Convert a Java String (java.lang.String) JObject to a Rust String.
#[cfg(target_os = "android")]
fn obj_to_string(env: &JNIEnv, obj: &JObject) -> String {
    if obj.is_null() {
        return String::new();
    }
    let js = unsafe { jni::objects::JString::from_raw(obj.as_raw()) };
    env.get_string(&js)
        .map(|s| s.into())
        .unwrap_or_default()
}

/// Get or lazily initialise the ML Kit TextRecognizer singleton.
#[cfg(target_os = "android")]
fn get_recognizer(env: &JNIEnv) -> crate::ScreenResult<&'static GlobalRef> {
    if let Some(r) = RECOGNIZER.get() {
        return Ok(r);
    }

    let recognizer = env
        .call_static_method(
            "com/google/mlkit/vision/text/TextRecognition",
            "getClient",
            "()Lcom/google/mlkit/vision/text/TextRecognizer;",
            &[],
        )
        .map_err(|e| {
            crate::ScreenError::OCRFailed(format!("TextRecognition.getClient failed: {e}"))
        })?
        .l()?;

    let global = env
        .new_global_ref(&recognizer)
        .map_err(|e| crate::ScreenError::OCRFailed(format!("GlobalRef failed: {e}")))?;

    RECOGNIZER
        .set(global)
        .map_err(|_| crate::ScreenError::OCRFailed("Recognizer already initialised".into()))?;

    Ok(RECOGNIZER.get().unwrap())
}

#[cfg(target_os = "android")]
pub struct AndroidOCREngine {
    java_vm: std::sync::Arc<jni::JavaVM>,
}

#[cfg(target_os = "android")]
impl AndroidOCREngine {
    pub fn new() -> crate::ScreenResult<Self> {
        let java_vm = unsafe {
            let vm_ptr = jni::sys::JNI_GetCreatedJavaVMs().map_err(|_| {
                crate::ScreenError::PlatformError(
                    "No Java VM — Android runtime not started".into(),
                )
            })?;
            jni::JavaVM::from_raw(vm_ptr.0 as *mut jni::sys::JavaVM).map_err(|_| {
                crate::ScreenError::PlatformError("Failed to wrap JavaVM handle".into())
            })?
        };

        Ok(Self {
            java_vm: std::sync::Arc::new(java_vm),
        })
    }

    fn get_env(&self) -> crate::ScreenResult<JNIEnv> {
        match self.java_vm.get_env() {
            Ok(env) => Ok(env),
            Err(_) => self
                .java_vm
                .attach_current_thread_as_daemon()
                .map_err(|_| {
                    crate::ScreenError::PlatformError("JNI thread attach failed".into())
                }),
        }
    }

    fn recognize_impl(
        &self,
        frame: &CapturedFrame,
        region: Option<Rect>,
    ) -> crate::ScreenResult<OCRResult> {
        if frame.format != crate::PixelFormat::BGRA8 {
            return Err(crate::ScreenError::OCRFailed(
                "Android OCR requires BGRA8 pixel format".into(),
            ));
        }

        let env = self.get_env()?;

        // --- 1. Extract and convert pixels ---------------------------------
        let src_stride = frame.width as usize * 4;
        let (crop_data, crop_w, crop_h) = if let Some(r) = region {
            let x = r.x.max(0) as usize;
            let y = r.y.max(0) as usize;
            let w = (r.width as usize).min(frame.width as usize - x).max(1);
            let h = (r.height as usize).min(frame.height as usize - y).max(1);
            let dst_stride = w * 4;
            let mut buf = vec![0u8; dst_stride * h];
            for row in 0..h {
                let src_off = (y + row) * src_stride + x * 4;
                let dst_off = row * dst_stride;
                buf[dst_off..dst_off + dst_stride]
                    .copy_from_slice(&frame.data[src_off..src_off + dst_stride]);
            }
            (buf, w as u32, h as u32)
        } else {
            (frame.data.clone(), frame.width, frame.height)
        };

        // BGRA8 → RGBA8: swap B↔R so Android ARGB_8888 interprets correctly
        let mut rgba = crop_data;
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }

        // --- 2. Create android.graphics.Bitmap -----------------------------
        let config = env
            .get_static_field(
                &env.find_class("android/graphics/Bitmap$Config")?,
                "ARGB_8888",
                "Landroid/graphics/Bitmap$Config;",
            )
            .map_err(|e| crate::ScreenError::OCRFailed(format!("Bitmap.Config field: {e}")))?
            .l()?;

        let bitmap = env
            .call_static_method(
                "android/graphics/Bitmap",
                "createBitmap",
                "(IILandroid/graphics/Bitmap$Config;)Landroid/graphics/Bitmap;",
                &[
                    JValue::Int(crop_w as jint),
                    JValue::Int(crop_h as jint),
                    JValue::Object(&config),
                ],
            )
            .map_err(|e| crate::ScreenError::OCRFailed(format!("createBitmap: {e}")))?
            .l()?;

        let buffer = unsafe {
            env.new_direct_byte_buffer(
                rgba.as_mut_ptr() as *mut std::ffi::c_void,
                rgba.len() as jni::sys::jlong,
            )
        }
        .map_err(|e| {
            crate::ScreenError::OCRFailed(format!("newDirectByteBuffer: {e}"))
        })?;

        env.call_method(
            &bitmap,
            "copyPixelsFromBuffer",
            "(Ljava/nio/ByteBuffer;)V",
            &[JValue::Object(&buffer)],
        )
        .map_err(|e| crate::ScreenError::OCRFailed(format!("copyPixelsFromBuffer: {e}")))?;

        // --- 3. Create InputImage from Bitmap ------------------------------
        let input_image = env
            .call_static_method(
                "com/google/mlkit/vision/common/InputImage",
                "fromBitmap",
                "(Landroid/graphics/Bitmap;I)Lcom/google/mlkit/vision/common/InputImage;",
                &[JValue::Object(&bitmap), JValue::Int(0)],
            )
            .map_err(|e| crate::ScreenError::OCRFailed(format!("InputImage.fromBitmap: {e}")))?
            .l()?;

        // --- 4. Get TextRecognizer & process --------------------------------
        let recognizer = get_recognizer(&env)?;

        let task = env
            .call_method(
                recognizer.as_obj(),
                "process",
                "(Lcom/google/mlkit/vision/common/InputImage;)Lcom/google/android/gms/tasks/Task;",
                &[JValue::Object(&input_image)],
            )
            .map_err(|e| crate::ScreenError::OCRFailed(format!("recognizer.process: {e}")))?
            .l()?;

        // --- 5. Block on result via Tasks.await() ---------------------------
        let text_result = env
            .call_static_method(
                "com/google/android/gms/tasks/Tasks",
                "await",
                "(Lcom/google/android/gms/tasks/Task;)Ljava/lang/Object;",
                &[JValue::Object(&task)],
            );

        let text_obj = match text_result {
            Ok(val) => val.l().map_err(|_| {
                crate::ScreenError::OCRFailed("Tasks.await returned null".into())
            })?,
            Err(e) => {
                env.exception_clear().ok();
                return Err(crate::ScreenError::OCRFailed(format!(
                    "OCR processing failed: {e}"
                )));
            }
        };

        // --- 6. Extract text blocks / lines / elements ----------------------
        let full_text = obj_to_string(
            &env,
            &env.call_method(&text_obj, "getText", "()Ljava/lang/String;", &[])
                .ok()
                .and_then(|v| v.l().ok())
                .unwrap_or_else(JObject::null),
        );

        let list = env
            .call_method(&text_obj, "getTextBlocks", "()Ljava/util/List;", &[])
            .ok()
            .and_then(|v| v.l().ok())
            .unwrap_or_else(JObject::null);

        let mut regions: Vec<crate::OCRRegion> = Vec::new();
        let mut total_confidence: f32 = 0.0;
        let mut region_count: u32 = 0;

        if !list.is_null() {
            let list_cls = env.find_class("java/util/List").ok();
            let size = list_cls
                .as_ref()
                .and_then(|_| {
                    env.call_method(&list, "size", "()I", &[])
                        .ok()
                        .and_then(|v| v.i().ok())
                })
                .unwrap_or(0);

            for block_idx in 0..size {
                let block = env
                    .call_method(&list, "get", "(I)Ljava/lang/Object;", &[JValue::Int(block_idx)])
                    .ok()
                    .and_then(|v| v.l().ok())
                    .unwrap_or_else(JObject::null);
                if block.is_null() {
                    continue;
                }

                let lines_list = env
                    .call_method(&block, "getLines", "()Ljava/util/List;", &[])
                    .ok()
                    .and_then(|v| v.l().ok())
                    .unwrap_or_else(JObject::null);
                if lines_list.is_null() {
                    continue;
                }

                let line_count = env
                    .call_method(&lines_list, "size", "()I", &[])
                    .ok()
                    .and_then(|v| v.i().ok())
                    .unwrap_or(0);

                for line_idx in 0..line_count {
                    let line = env
                        .call_method(
                            &lines_list,
                            "get",
                            "(I)Ljava/lang/Object;",
                            &[JValue::Int(line_idx)],
                        )
                        .ok()
                        .and_then(|v| v.l().ok())
                        .unwrap_or_else(JObject::null);
                    if line.is_null() {
                        continue;
                    }

                    let elements_list = env
                        .call_method(&line, "getElements", "()Ljava/util/List;", &[])
                        .ok()
                        .and_then(|v| v.l().ok())
                        .unwrap_or_else(JObject::null);
                    if elements_list.is_null() {
                        continue;
                    }

                    let elem_count: i32 = env
                        .call_method(&elements_list, "size", "()I", &[])
                        .ok()
                        .and_then(|v| v.i().ok())
                        .unwrap_or(0);

                    for elem_idx in 0..elem_count {
                        let element = env
                            .call_method(
                                &elements_list,
                                "get",
                                "(I)Ljava/lang/Object;",
                                &[JValue::Int(elem_idx)],
                            )
                            .ok()
                            .and_then(|v| v.l().ok())
                            .unwrap_or_else(JObject::null);
                        if element.is_null() {
                            continue;
                        }

                        let elem_text = obj_to_string(
                            &env,
                            &env.call_method(&element, "getText", "()Ljava/lang/String;", &[])
                                .ok()
                                .and_then(|v| v.l().ok())
                                .unwrap_or_else(JObject::null),
                        );

                        let bounds_obj = env
                            .call_method(&element, "getBoundingBox", "()Landroid/graphics/Rect;", &[])
                            .ok()
                            .and_then(|v| v.l().ok())
                            .unwrap_or_else(JObject::null);

                        let (l, t, r, b) = if !bounds_obj.is_null() {
                            let left = env
                                .get_field(&bounds_obj, "left", "I")
                                .ok()
                                .and_then(|v| v.i().ok())
                                .unwrap_or(0);
                            let top = env
                                .get_field(&bounds_obj, "top", "I")
                                .ok()
                                .and_then(|v| v.i().ok())
                                .unwrap_or(0);
                            let right = env
                                .get_field(&bounds_obj, "right", "I")
                                .ok()
                                .and_then(|v| v.i().ok())
                                .unwrap_or(0);
                            let bottom = env
                                .get_field(&bounds_obj, "bottom", "I")
                                .ok()
                                .and_then(|v| v.i().ok())
                                .unwrap_or(0);
                            (left, top, right, bottom)
                        } else {
                            (0, 0, 0, 0)
                        };

                        let elem_rect = crate::Rect {
                            x: l,
                            y: t,
                            width: (r - l).max(0) as u32,
                            height: (b - t).max(0) as u32,
                        };

                        // ML Kit on-device doesn't expose per-element confidence.
                        // Use 0.9 as a reasonable default.
                        total_confidence += 0.9;
                        region_count += 1;

                        regions.push(crate::OCRRegion {
                            text: elem_text,
                            confidence: 0.9,
                            bounds: elem_rect,
                        });
                    }
                }
            }
        }

        let avg_confidence = if region_count > 0 {
            total_confidence / region_count as f32
        } else {
            0.0
        };

        Ok(crate::OCRResult {
            text: full_text,
            confidence: avg_confidence,
            language: "en".to_string(),
            regions,
        })
    }
}

#[cfg(target_os = "android")]
use async_trait::async_trait;

#[cfg(target_os = "android")]
#[async_trait]
impl crate::OCREngine for AndroidOCREngine {
    fn id(&self) -> &str {
        "android-mlkit"
    }

    async fn recognize(&self, frame: &CapturedFrame) -> crate::ScreenResult<OCRResult> {
        self.recognize_impl(frame, None)
    }

    async fn recognize_region(
        &self,
        frame: &CapturedFrame,
        region: Rect,
    ) -> crate::ScreenResult<OCRResult> {
        self.recognize_impl(frame, Some(region))
    }

    fn supported_languages(&self) -> Vec<String> {
        vec![
            "en".to_string(),
            "es".to_string(),
            "fr".to_string(),
            "de".to_string(),
            "it".to_string(),
            "pt".to_string(),
            "ru".to_string(),
        ]
    }
}
