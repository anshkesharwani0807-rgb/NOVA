use crate::{PixelFormat, ScreenCaptureConfig};
use async_trait::async_trait;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

pub struct WindowsScreenCapture {
    config: Option<ScreenCaptureConfig>,
    capturing: bool,
}

impl WindowsScreenCapture {
    pub fn new() -> crate::ScreenResult<Self> {
        Ok(Self { config: None, capturing: false })
    }

    fn do_capture(&self, config: &ScreenCaptureConfig) -> crate::ScreenResult<crate::CapturedFrame> {
        unsafe {
            let sm_x = GetSystemMetrics(SM_CXSCREEN);
            let sm_y = GetSystemMetrics(SM_CYSCREEN);

            let hdc_screen = GetDC(None);
            if hdc_screen.0.is_null() {
                return Err(crate::ScreenError::CaptureFailed("GetDC failed".into()));
            }

            let hdc_mem = CreateCompatibleDC(hdc_screen);
            if hdc_mem.0.is_null() {
                let _ = ReleaseDC(None, hdc_screen);
                return Err(crate::ScreenError::CaptureFailed("CreateCompatibleDC failed".into()));
            }

            let h_bitmap = CreateCompatibleBitmap(hdc_screen, sm_x, sm_y);
            if h_bitmap.0.is_null() {
                let _ = DeleteDC(hdc_mem);
                let _ = ReleaseDC(None, hdc_screen);
                return Err(crate::ScreenError::CaptureFailed("CreateCompatibleBitmap failed".into()));
            }

            let old_bitmap = SelectObject(hdc_mem, h_bitmap);

            BitBlt(hdc_mem, 0, 0, sm_x, sm_y, hdc_screen, 0, 0, SRCCOPY | CAPTUREBLT)?;

            let mut bmp_info: BITMAP = std::mem::zeroed();
            GetObjectW(h_bitmap, std::mem::size_of::<BITMAP>() as i32, Some(&mut bmp_info as *mut _ as *mut std::ffi::c_void));

            if bmp_info.bmWidthBytes == 0 || bmp_info.bmHeight == 0 {
                let _ = SelectObject(hdc_mem, old_bitmap);
                let _ = DeleteObject(h_bitmap);
                let _ = DeleteDC(hdc_mem);
                let _ = ReleaseDC(None, hdc_screen);
                return Err(crate::ScreenError::CaptureFailed("GetObjectW returned invalid bitmap info".into()));
            }

            let src_stride = bmp_info.bmWidthBytes as usize;
            let total_size = src_stride * bmp_info.bmHeight as usize;
            let mut full_data: Vec<u8> = vec![0u8; total_size];
            GetBitmapBits(h_bitmap, total_size as i32, full_data.as_mut_ptr() as *mut std::ffi::c_void);

            let _ = SelectObject(hdc_mem, old_bitmap);
            let _ = DeleteObject(h_bitmap);
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(None, hdc_screen);

            let (out_w, out_h, region) = if let Some(r) = config.region {
                let rw = r.width.min((sm_x as u32).saturating_sub(r.x as u32));
                let rh = r.height.min((sm_y as u32).saturating_sub(r.y as u32));
                (rw.max(1), rh.max(1), Some(r))
            } else {
                (sm_x as u32, sm_y as u32, None)
            };

            let cropped = if let Some(r) = config.region {
                let rx = r.x.max(0) as usize;
                let ry = r.y.max(0) as usize;
                let rw = out_w as usize;
                let rh = out_h as usize;
                let dst_stride = rw * 4;
                let mut buf = vec![0u8; dst_stride * rh];
                for y in 0..rh {
                    let src_off = (ry + y) * src_stride + rx * 4;
                    let dst_off = y * dst_stride;
                    buf[dst_off..dst_off + dst_stride].copy_from_slice(&full_data[src_off..src_off + dst_stride]);
                }
                buf
            } else {
                full_data
            };

            let (final_data, final_w, final_h) = if let Some(factor) = config.downscale_factor {
                if factor > 1.0 {
                    let scale = 1.0 / factor;
                    let dw = (out_w as f32 * scale) as u32;
                    let dh = (out_h as f32 * scale) as u32;
                    let dstride = dw as usize * 4;
                    let scurve = out_w as usize * 4;
                    let mut scaled = vec![0u8; dstride * dh as usize];
                    for y in 0..dh {
                        let sy = (y as f32 / scale) as usize;
                        for x in 0..dw {
                            let sx = (x as f32 / scale) as usize;
                            let si = sy * scurve + sx * 4;
                            let di = y as usize * dstride + x as usize * 4;
                            scaled[di..di + 4].copy_from_slice(&cropped[si..si + 4]);
                        }
                    }
                    (scaled, dw, dh)
                } else {
                    (cropped, out_w, out_h)
                }
            } else {
                (cropped, out_w, out_h)
            };

            Ok(crate::CapturedFrame {
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
}

#[async_trait]
impl crate::ScreenCapture for WindowsScreenCapture {
    fn id(&self) -> &str {
        "windows-gdi"
    }

    async fn start_capture(&mut self, config: ScreenCaptureConfig) -> crate::ScreenResult<()> {
        if self.capturing {
            return Err(crate::ScreenError::AlreadyCapturing);
        }
        self.config = Some(config);
        self.capturing = true;
        Ok(())
    }

    async fn stop_capture(&mut self) -> crate::ScreenResult<()> {
        self.capturing = false;
        Ok(())
    }

    async fn capture_frame(&mut self) -> crate::ScreenResult<crate::CapturedFrame> {
        if !self.capturing {
            return Err(crate::ScreenError::NotCapturing);
        }
        let config = self.config.as_ref().ok_or(crate::ScreenError::NotInitialized)?;
        self.do_capture(config)
    }

    async fn start_stream(&mut self, _tx: tokio::sync::mpsc::Sender<crate::CapturedFrame>) -> crate::ScreenResult<()> {
        Ok(())
    }

    fn is_capturing(&self) -> bool {
        self.capturing
    }
}
