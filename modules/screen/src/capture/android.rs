//! Android screen capture using MediaProjection API via JNI.
//!
//! ## Kotlin integration required
//! 1. `NovaCore.nativeSetApplicationContext(context)` — call once at app start.
//! 2. `NovaCore.nativeSetMediaProjection(projection)` — call after the user
//!    grants screen capture permission via ActivityResultLauncher.
//!    `projection` is the `MediaProjection` from `MediaProjectionManager`.
//!
//! Frame capture uses `ImageReader` with RGBA_8888 format; pixels are
//! converted to BGRA8 (nova canonical) by swapping R↔B.  YUV_420_888 images
//! (some devices) use a CPU colour-space transform.

use crate::{CapturedFrame, PixelFormat, ScreenCaptureConfig, ScreenError, ScreenResult};
use async_trait::async_trait;
use jni::objects::{GlobalRef, JArray, JObject, JValue};
use jni::JNIEnv;
use parking_lot::Mutex;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// MediaProjection global — set from Kotlin via JNI
// ---------------------------------------------------------------------------

static MEDIA_PROJECTION: std::sync::OnceLock<GlobalRef> = std::sync::OnceLock::new();

pub fn set_media_projection(env: &JNIEnv, obj: &JObject) {
    let global = env
        .new_global_ref(obj)
        .expect("AndroidScreenCapture: failed to create GlobalRef for MediaProjection");
    let _ = MEDIA_PROJECTION.set(global);
}

pub fn get_media_projection() -> Option<&'static GlobalRef> {
    MEDIA_PROJECTION.get()
}

pub fn has_media_projection() -> bool {
    MEDIA_PROJECTION.get().is_some()
}

// ---------------------------------------------------------------------------
// Helper: create a local ref from a GlobalRef so lifetimes align with JNIEnv
// ---------------------------------------------------------------------------

fn local_ref<'local>(env: &JNIEnv<'local>, global: &GlobalRef) -> ScreenResult<JObject<'local>> {
    unsafe { env.new_local_ref(global.as_obj()) }
        .map_err(|_| ScreenError::PlatformError("new_local_ref failed".into()))
}

// ---------------------------------------------------------------------------
// Display metrics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct DisplayMetrics {
    width: u32,
    height: u32,
    density_dpi: u32,
}

// ---------------------------------------------------------------------------
// AndroidScreenCapture
// ---------------------------------------------------------------------------

pub struct AndroidScreenCapture {
    java_vm: Arc<jni::JavaVM>,
    image_reader: Mutex<Option<GlobalRef>>,
    virtual_display: Mutex<Option<GlobalRef>>,
    config: Mutex<Option<ScreenCaptureConfig>>,
    capturing: Mutex<bool>,
    display_metrics: Mutex<Option<DisplayMetrics>>,
}

impl AndroidScreenCapture {
    pub fn new() -> ScreenResult<Self> {
        let java_vm = unsafe {
            let vm_ptr = jni::sys::JNI_GetCreatedJavaVMs().map_err(|_| {
                ScreenError::PlatformError("No Java VM — Android runtime not started".into())
            })?;
            jni::JavaVM::from_raw(vm_ptr.0 as *mut jni::sys::JavaVM)
                .map_err(|_| ScreenError::PlatformError("Failed to wrap JavaVM handle".into()))?
        };

        Ok(Self {
            java_vm: Arc::new(java_vm),
            image_reader: Mutex::new(None),
            virtual_display: Mutex::new(None),
            config: Mutex::new(None),
            capturing: Mutex::new(false),
            display_metrics: Mutex::new(None),
        })
    }

    fn get_env(&self) -> ScreenResult<JNIEnv> {
        match self.java_vm.get_env() {
            Ok(env) => Ok(env),
            Err(_) => self
                .java_vm
                .attach_current_thread_as_daemon()
                .map_err(|_| ScreenError::PlatformError("JNI thread attach failed".into())),
        }
    }

    // ------------------------------------------------------------------
    // DisplayMetrics via WindowManager
    // ------------------------------------------------------------------

    fn query_display_metrics(&self, env: &JNIEnv) -> ScreenResult<DisplayMetrics> {
        let dm = env.new_object("android/util/DisplayMetrics", "()V", &[])?;

        let ctx = crate::jni_bridge::get_application_context().ok_or_else(|| {
            ScreenError::PlatformError(
                "Application Context not set — Kotlin must call nativeSetApplicationContext".into(),
            )
        })?;
        let ctx_local = local_ref(env, ctx)?;

        let wm_svc = env.new_string("window")?;
        let wm = env
            .call_method(
                &ctx_local,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&wm_svc)],
            )?
            .l()?;

        let display = env
            .call_method(&wm, "getDefaultDisplay", "()Landroid/view/Display;", &[])?
            .l()?;

        env.call_method(
            &display,
            "getRealMetrics",
            "(Landroid/util/DisplayMetrics;)V",
            &[JValue::Object(&dm)],
        )?;

        let width = env.get_field(&dm, "widthPixels", "I")?.i()? as u32;
        let height = env.get_field(&dm, "heightPixels", "I")?.i()? as u32;
        let dpi = env.get_field(&dm, "densityDpi", "I")?.i()? as u32;

        Ok(DisplayMetrics {
            width,
            height,
            density_dpi: dpi,
        })
    }

    // ------------------------------------------------------------------
    // ImageReader + VirtualDisplay creation
    // ------------------------------------------------------------------

    fn create_image_reader(
        &self,
        env: &JNIEnv,
        width: u32,
        height: u32,
    ) -> ScreenResult<GlobalRef> {
        let reader = env
            .call_static_method(
                "android/media/ImageReader",
                "newInstance",
                "(III)Landroid/media/ImageReader;",
                &[
                    JValue::Int(width as jint),
                    JValue::Int(height as jint),
                    JValue::Int(3), // PixelFormat.RGBA_8888
                    JValue::Int(2), // maxImages
                ],
            )?
            .l()?;

        env.new_global_ref(&reader)
            .map_err(|_| ScreenError::PlatformError("GlobalRef for ImageReader failed".into()))
    }

    fn get_surface_from_reader<'local>(
        &self,
        env: &JNIEnv<'local>,
        ir: &JObject<'local>,
    ) -> ScreenResult<JObject<'local>> {
        env.call_method(ir, "getSurface", "()Landroid/view/Surface;", &[])?
            .l()
    }

    fn create_virtual_display<'local>(
        &self,
        env: &JNIEnv<'local>,
        mp: &JObject<'local>,
        surface: &JObject<'local>,
        width: u32,
        height: u32,
        dpi: u32,
    ) -> ScreenResult<GlobalRef> {
        let name = env.new_string("NOVA_ScreenCapture")?;

        let vd = env.call_method(
            mp,
            "createVirtualDisplay",
            "(Ljava/lang/String;IIIILandroid/view/Surface;Landroid/hardware/display/VirtualDisplay$Callback;Landroid/os/Handler;)Landroid/hardware/display/VirtualDisplay;",
            &[
                JValue::Object(&name),
                JValue::Int(width as jint),
                JValue::Int(height as jint),
                JValue::Int(dpi as jint),
                JValue::Int(1 | 32), // PUBLIC | AUTO_MIRROR
                JValue::Object(surface),
                JValue::Object(&JObject::null()),
                JValue::Object(&JObject::null()),
            ],
        )?
        .l()?;

        env.new_global_ref(&vd)
            .map_err(|_| ScreenError::PlatformError("GlobalRef for VirtualDisplay failed".into()))
    }

    // ------------------------------------------------------------------
    // Frame acquisition
    // ------------------------------------------------------------------

    fn acquire_latest_frame(&self, env: &JNIEnv) -> ScreenResult<CapturedFrame> {
        let ir_guard = self.image_reader.lock();
        let ir = ir_guard.as_ref().ok_or(ScreenError::NotInitialized)?;
        let ir_local = local_ref(env, ir)?;

        let image = env
            .call_method(
                &ir_local,
                "acquireLatestImage",
                "()Landroid/media/Image;",
                &[],
            )?
            .l()?;

        if image.is_null() {
            return Err(ScreenError::CaptureFailed(
                "No frame available from ImageReader".into(),
            ));
        }

        let width = env.call_method(&image, "getWidth", "()I", &[])?.i()? as u32;
        let height = env.call_method(&image, "getHeight", "()I", &[])?.i()? as u32;
        let fmt = env.call_method(&image, "getFormat", "()I", &[])?.i()?;

        let pixel_data = self.image_to_bgra8(env, &image, width, height, fmt)?;

        env.call_method(&image, "close", "()V", &[])?;

        self.build_frame(width, height, pixel_data)
    }

    fn image_to_bgra8(
        &self,
        env: &JNIEnv,
        image: &JObject,
        width: u32,
        height: u32,
        fmt: i32,
    ) -> ScreenResult<Vec<u8>> {
        match fmt {
            3 => rgba8888_to_bgra8(env, image, width, height),
            35 => yuv420_to_bgra8(env, image, width, height),
            other => Err(ScreenError::CaptureFailed(format!(
                "Unsupported Image pixel format: 0x{other:x} (expected 3=RGBA_8888)"
            ))),
        }
    }

    // ------------------------------------------------------------------
    // Post-processing: region crop + downscale
    // ------------------------------------------------------------------

    fn build_frame(
        &self,
        width: u32,
        height: u32,
        mut pixel_data: Vec<u8>,
    ) -> ScreenResult<CapturedFrame> {
        let dm = self.display_metrics.lock();
        let cfg = self.config.lock();

        let full_w = dm.as_ref().map(|m| m.width).unwrap_or(width);

        let (final_data, final_w, final_h, region) =
            if let Some(r) = cfg.as_ref().and_then(|c| c.region) {
                let rw = r.width.min(full_w.saturating_sub(r.x as u32)).max(1);
                let rh = r.height.min(full_w.saturating_sub(r.y as u32)).max(1);
                let mut cropped = vec![0u8; rw as usize * rh as usize * 4];
                let src_stride = width as usize * 4;
                let dst_stride = rw as usize * 4;
                for y in 0..rh as usize {
                    let sy = r.y as usize + y;
                    let sx = r.x as usize;
                    let src_off = sy * src_stride + sx * 4;
                    let dst_off = y * dst_stride;
                    cropped[dst_off..dst_off + dst_stride]
                        .copy_from_slice(&pixel_data[src_off..src_off + dst_stride]);
                }
                (cropped, rw, rh, Some(r))
            } else {
                (pixel_data, width, height, None)
            };

        let (final_data, final_w, final_h) =
            if let Some(factor) = cfg.as_ref().and_then(|c| c.downscale_factor) {
                if factor > 1.0 {
                    let scale = 1.0 / factor;
                    let dw = (final_w as f32 * scale) as u32;
                    let dh = (final_h as f32 * scale) as u32;
                    let dstride = dw as usize * 4;
                    let sstride = final_w as usize * 4;
                    let mut scaled = vec![0u8; dstride * dh as usize];
                    for y in 0..dh {
                        let sy = (y as f32 / scale) as usize;
                        for x in 0..dw {
                            let sx = (x as f32 / scale) as usize;
                            let si = sy * sstride + sx * 4;
                            let di = y as usize * dstride + x as usize * 4;
                            scaled[di..di + 4].copy_from_slice(&final_data[si..si + 4]);
                        }
                    }
                    (scaled, dw, dh)
                } else {
                    (final_data, final_w, final_h)
                }
            } else {
                (final_data, final_w, final_h)
            };

        Ok(CapturedFrame {
            frame_id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            width: final_w,
            height: final_h,
            format: PixelFormat::BGRA8,
            data: final_data,
            region,
        })
    }
}

// ===========================================================================
// Pixel-format conversion functions (free functions)
// ===========================================================================

fn rgba8888_to_bgra8(
    env: &JNIEnv,
    image: &JObject,
    width: u32,
    height: u32,
) -> ScreenResult<Vec<u8>> {
    let planes_arr = env
        .call_method(image, "getPlanes", "()[Landroid/media/Image$Plane;", &[])?
        .l()?;
    let planes = unsafe { JArray::<JObject>::from_raw(planes_arr.as_raw()) };
    let plane0 = unsafe { env.get_object_array_element(&planes, 0)? };

    let buf = env
        .call_method(&plane0, "getBuffer", "()Ljava/nio/ByteBuffer;", &[])?
        .l()?;
    let row_stride = env.call_method(&plane0, "getRowStride", "()I", &[])?.i()? as usize;

    let row_bytes = width as usize * 4;
    let src_size = height as usize * row_stride;
    let buf_ptr = unsafe { env.get_direct_buffer_address(&buf)? };
    let src = unsafe { std::slice::from_raw_parts(buf_ptr as *const u8, src_size) };

    let mut dst = vec![0u8; row_bytes * height as usize];
    for y in 0..height as usize {
        let src_off = y * row_stride;
        let dst_off = y * row_bytes;
        for x in 0..width as usize {
            let si = src_off + x * 4;
            let di = dst_off + x * 4;
            dst[di] = src[si + 2]; // B
            dst[di + 1] = src[si + 1]; // G
            dst[di + 2] = src[si]; // R
            dst[di + 3] = src[si + 3]; // A
        }
    }

    Ok(dst)
}

fn yuv420_to_bgra8(
    env: &JNIEnv,
    image: &JObject,
    width: u32,
    height: u32,
) -> ScreenResult<Vec<u8>> {
    let planes_arr = env
        .call_method(image, "getPlanes", "()[Landroid/media/Image$Plane;", &[])?
        .l()?;
    let planes = unsafe { JArray::<JObject>::from_raw(planes_arr.as_raw()) };

    let read_plane = |env: &JNIEnv, idx: i32| -> ScreenResult<(Vec<u8>, usize, usize)> {
        let p = unsafe { env.get_object_array_element(&planes, idx)? };
        let buf = env
            .call_method(&p, "getBuffer", "()Ljava/nio/ByteBuffer;", &[])?
            .l()?;
        let rs = env.call_method(&p, "getRowStride", "()I", &[])?.i()? as usize;
        let ps = env.call_method(&p, "getPixelStride", "()I", &[])?.i()? as usize;
        let cap = unsafe { env.get_direct_buffer_capacity(&buf)? } as usize;
        let ptr = unsafe { env.get_direct_buffer_address(&buf)? as *const u8 };
        let data = if !ptr.is_null() && cap > 0 {
            unsafe { std::slice::from_raw_parts(ptr, cap) }.to_vec()
        } else {
            vec![]
        };
        Ok((data, rs, ps))
    };

    let (y_data, y_rs, _y_ps) = read_plane(env, 0)?;
    let (u_data, u_rs, u_ps) = read_plane(env, 1)?;
    let (v_data, v_rs, v_ps) = read_plane(env, 2)?;

    let mut dst = vec![0u8; width as usize * height as usize * 4];

    for y in 0..height as usize {
        for x in 0..width as usize {
            let yi = y * y_rs + x;
            let y_val = *y_data.get(yi).unwrap_or(&0) as f32;

            let ux = x / 2;
            let uy = y / 2;
            let ui = uy * u_rs + ux * u_ps;
            let u_val = *u_data.get(ui).unwrap_or(&128) as f32 - 128.0;

            let vx = x / 2;
            let vy = y / 2;
            let vi = vy * v_rs + vx * v_ps;
            let v_val = *v_data.get(vi).unwrap_or(&128) as f32 - 128.0;

            let r = (y_val + 1.402 * v_val).clamp(0.0, 255.0) as u8;
            let g = (y_val - 0.344 * u_val - 0.714 * v_val).clamp(0.0, 255.0) as u8;
            let b = (y_val + 1.772 * u_val).clamp(0.0, 255.0) as u8;

            let off = y * width as usize * 4 + x * 4;
            dst[off] = b;
            dst[off + 1] = g;
            dst[off + 2] = r;
            dst[off + 3] = 255;
        }
    }

    Ok(dst)
}

// ===========================================================================
// Trait implementation
// ===========================================================================

#[async_trait]
impl super::ScreenCapture for AndroidScreenCapture {
    fn id(&self) -> &str {
        "android-mediaprojection"
    }

    async fn start_capture(&mut self, config: ScreenCaptureConfig) -> ScreenResult<()> {
        let mut cap = self.capturing.lock();
        if *cap {
            return Err(ScreenError::AlreadyCapturing);
        }

        let mp = MEDIA_PROJECTION.get().ok_or(ScreenError::PlatformError(
            "MediaProjection not set — Kotlin must call nativeSetMediaProjection first".into(),
        ))?;

        let env = self.get_env()?;
        let metrics = self.query_display_metrics(&env)?;

        // VirtualDisplay covers the full screen; region crop is in build_frame.
        let vd_w = metrics.width;
        let vd_h = metrics.height;

        let mp_local = local_ref(&env, mp)?;
        let ir = self.create_image_reader(&env, vd_w, vd_h)?;
        let ir_local = local_ref(&env, &ir)?;
        let surface = self.get_surface_from_reader(&env, &ir_local)?;
        let vd = self.create_virtual_display(
            &env,
            &mp_local,
            &surface,
            vd_w,
            vd_h,
            metrics.density_dpi,
        )?;

        *self.image_reader.lock() = Some(ir);
        *self.virtual_display.lock() = Some(vd);
        *self.config.lock() = Some(config);
        *self.display_metrics.lock() = Some(metrics);
        *cap = true;

        Ok(())
    }

    async fn stop_capture(&mut self) -> ScreenResult<()> {
        let env = self.get_env()?;

        if let Some(vd) = self.virtual_display.lock().take() {
            if let Ok(vd_local) = local_ref(&env, &vd) {
                env.call_method(&vd_local, "release", "()V", &[]).ok();
            }
        }

        if let Some(ir) = self.image_reader.lock().take() {
            if let Ok(ir_local) = local_ref(&env, &ir) {
                env.call_method(&ir_local, "close", "()V", &[]).ok();
            }
        }

        *self.capturing.lock() = false;
        *self.config.lock() = None;
        *self.display_metrics.lock() = None;

        Ok(())
    }

    async fn capture_frame(&mut self) -> ScreenResult<CapturedFrame> {
        if !*self.capturing.lock() {
            return Err(ScreenError::NotCapturing);
        }
        let env = self.get_env()?;
        self.acquire_latest_frame(&env)
    }

    async fn start_stream(
        &mut self,
        _tx: tokio::sync::mpsc::Sender<CapturedFrame>,
    ) -> ScreenResult<()> {
        Ok(())
    }

    fn is_capturing(&self) -> bool {
        *self.capturing.lock()
    }
}
