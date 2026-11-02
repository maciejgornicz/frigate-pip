//! H.264/HEVC decoder via Media Foundation: hardware (D3D11/DXVA) or software.

use crate::rtp::Codec;
use std::mem::ManuallyDrop;
use std::sync::OnceLock;
use windows::core::w;
use windows::core::{Interface, Result, GUID};
use windows::Win32::Foundation::{E_FAIL, E_NOTIMPL};
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::CoTaskMemFree;

pub enum Frame {
    /// Frame in GPU memory (decoder texture and array index).
    Gpu { tex: ID3D11Texture2D, index: u32, width: u32, height: u32, _sample: IMFSample },
    /// NV12 frame in system RAM.
    Cpu { data: Vec<u8>, stride: usize, width: u32, height: u32, alloc_height: u32 },
}

pub struct Decoder {
    mft: IMFTransform,
    pub hw: bool,
    provides_samples: bool,
    out_bytes: u32,
    frame: (u32, u32),
    crop: (u32, u32),
    ts: i64,
    _manager: Option<IMFDXGIDeviceManager>,
}

pub(crate) fn mf_startup() -> bool {
    static READY: OnceLock<bool> = OnceLock::new();
    *READY.get_or_init(|| unsafe {
        use windows::Win32::System::LibraryLoader::{LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
        // mfplat.dll is delay-loaded (see build.rs) and missing on Windows "N" editions
        // without the Media Feature Pack: nothing may call it unless it can be loaded.
        // FRIGATEPIP_NO_MF=1 simulates such a system.
        let present = std::env::var_os("FRIGATEPIP_NO_MF").is_none() && LoadLibraryExW(w!("mfplat.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32).is_ok();
        if !present {
            crate::log!("Media Foundation is not available (Windows N without the Media Feature Pack?) - no RTSP video or AAC audio");
            return false;
        }
        MFStartup(MF_VERSION, MFSTARTUP_LITE).is_ok()
    })
}

unsafe fn find_decoder(subtype: GUID) -> Result<IMFTransform> {
    let input = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: subtype };
    let base = MFT_ENUM_FLAG_SYNCMFT.0 | MFT_ENUM_FLAG_LOCALMFT.0 | MFT_ENUM_FLAG_SORTANDFILTER.0;
    for flags in [base, base | MFT_ENUM_FLAG_UNTRUSTED_STOREMFT.0] {
        let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;
        if MFTEnumEx(MFT_CATEGORY_VIDEO_DECODER, MFT_ENUM_FLAG(flags), Some(&input), None, &mut list, &mut count).is_err() {
            continue;
        }
        let mut found = None;
        if !list.is_null() {
            let items = std::slice::from_raw_parts_mut(list, count as usize);
            for item in items.iter_mut() {
                if let (None, Some(act)) = (&found, item.as_ref()) {
                    found = act.ActivateObject::<IMFTransform>().ok();
                }
                drop(item.take());
            }
            CoTaskMemFree(Some(list as *const _));
        }
        if let Some(t) = found {
            return Ok(t);
        }
    }
    Err(E_FAIL.into())
}

impl Decoder {
    /// `device`: D3D11 device for hardware decoding (`None` = software).
    pub fn new(codec: Codec, device: Option<&ID3D11Device>) -> Result<Decoder> {
        if !mf_startup() {
            return Err(E_NOTIMPL.into());
        }
        let subtype = match codec {
            Codec::H264 => MFVideoFormat_H264,
            Codec::H265 => MFVideoFormat_HEVC,
        };
        unsafe {
            let mft = find_decoder(subtype)?;
            let mut hw = false;
            let mut manager = None;
            // Low-latency mode: a frame is output right after it is input,
            // without waiting for the DPB to fill up.
            if let Ok(api) = mft.cast::<ICodecAPI>() {
                let on = windows::core::VARIANT::from(1u32);
                if api.SetValue(&CODECAPI_AVLowLatencyMode, &on).is_err() {
                    let _ = api.SetValue(&CODECAPI_AVLowLatencyMode, &windows::core::VARIANT::from(true));
                }
            }
            if let Ok(attrs) = mft.GetAttributes() {
                let _ = attrs.SetUINT32(&MF_LOW_LATENCY, 1);
                if let Some(dev) = device {
                    if attrs.GetUINT32(&MF_SA_D3D11_AWARE).unwrap_or(0) != 0 {
                        let mut token = 0u32;
                        let mut mgr: Option<IMFDXGIDeviceManager> = None;
                        MFCreateDXGIDeviceManager(&mut token, &mut mgr)?;
                        if let Some(mgr) = mgr {
                            mgr.ResetDevice(dev, token)?;
                            if mft.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, mgr.as_raw() as usize).is_ok() {
                                hw = true;
                                manager = Some(mgr);
                            }
                        }
                    }
                }
            }
            let it = MFCreateMediaType()?;
            it.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            it.SetGUID(&MF_MT_SUBTYPE, &subtype)?;
            it.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            if codec == Codec::H265 {
                // The HEVC decoder offers no output types without a frame size; the real
                // size arrives from the SPS as MF_E_TRANSFORM_STREAM_CHANGE.
                it.SetUINT64(&MF_MT_FRAME_SIZE, (1920u64 << 32) | 1080)?;
            }
            mft.SetInputType(0, &it, 0)?;
            let mut d = Decoder { mft, hw, provides_samples: false, out_bytes: 0, frame: (0, 0), crop: (0, 0), ts: 0, _manager: manager };
            d.set_output_type()?;
            d.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            d.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
            Ok(d)
        }
    }

    unsafe fn set_output_type(&mut self) -> Result<()> {
        let mut chosen = None;
        let mut i = 0;
        while let Ok(t) = self.mft.GetOutputAvailableType(0, i) {
            let st = t.GetGUID(&MF_MT_SUBTYPE).unwrap_or_default();
            if st == MFVideoFormat_NV12 {
                chosen = Some(t);
                break;
            }
            if self.hw && chosen.is_none() && st == MFVideoFormat_P010 {
                chosen = Some(t);
            }
            i += 1;
        }
        let t = chosen.ok_or_else(|| windows::core::Error::from(E_FAIL))?;
        self.mft.SetOutputType(0, &t, 0)?;
        if let Ok(v) = t.GetUINT64(&MF_MT_FRAME_SIZE) {
            self.frame = ((v >> 32) as u32, v as u32);
        }
        self.crop = self.frame;
        let mut area = [0u8; 16];
        if t.GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, &mut area, None).is_ok() {
            let cx = i32::from_le_bytes([area[8], area[9], area[10], area[11]]);
            let cy = i32::from_le_bytes([area[12], area[13], area[14], area[15]]);
            if cx > 0 && cy > 0 {
                self.crop = (cx as u32, cy as u32);
            }
        }
        let info = self.mft.GetOutputStreamInfo(0)?;
        let provides = MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0 as u32;
        self.provides_samples = info.dwFlags & provides != 0;
        self.out_bytes = info.cbSize;
        Ok(())
    }

    /// Decodes one Annex B unit; passes finished frames to `emit`.
    pub fn decode(&mut self, data: &[u8], emit: &mut dyn FnMut(Frame)) -> Result<()> {
        unsafe {
            let buf = MFCreateMemoryBuffer(data.len() as u32)?;
            let mut p = std::ptr::null_mut();
            buf.Lock(&mut p, None, None)?;
            std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
            buf.Unlock()?;
            buf.SetCurrentLength(data.len() as u32)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buf)?;
            sample.SetSampleTime(self.ts)?;
            sample.SetSampleDuration(400_000)?;
            self.ts += 400_000;
            match self.mft.ProcessInput(0, &sample, 0) {
                Err(e) if e.code() == MF_E_NOTACCEPTING => {
                    self.drain(emit)?;
                    self.mft.ProcessInput(0, &sample, 0)?;
                }
                r => r?,
            }
            self.drain(emit)
        }
    }

    unsafe fn drain(&mut self, emit: &mut dyn FnMut(Frame)) -> Result<()> {
        loop {
            let provided = if self.provides_samples {
                None
            } else {
                let size = if self.out_bytes > 0 { self.out_bytes } else { (self.frame.0 * self.frame.1 * 3 / 2).max(1 << 20) };
                let s = MFCreateSample()?;
                s.AddBuffer(&MFCreateMemoryBuffer(size)?)?;
                Some(s)
            };
            let mut out =
                [MFT_OUTPUT_DATA_BUFFER { dwStreamID: 0, pSample: ManuallyDrop::new(provided), dwStatus: 0, pEvents: ManuallyDrop::new(None) }];
            let mut status = 0u32;
            let r = self.mft.ProcessOutput(0, &mut out, &mut status);
            let sample = ManuallyDrop::take(&mut out[0].pSample);
            drop(ManuallyDrop::take(&mut out[0].pEvents));
            match r {
                Ok(()) => {
                    if let Some(s) = sample {
                        if let Some(f) = self.to_frame(s)? {
                            emit(f);
                        }
                    }
                }
                Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(()),
                Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => self.set_output_type()?,
                Err(e) => return Err(e),
            }
        }
    }

    unsafe fn to_frame(&self, s: IMFSample) -> Result<Option<Frame>> {
        let (width, height) = self.crop;
        if width == 0 || height == 0 {
            return Ok(None);
        }
        let buf = s.GetBufferByIndex(0)?;
        if let Ok(dxgi) = buf.cast::<IMFDXGIBuffer>() {
            let mut raw = std::ptr::null_mut();
            dxgi.GetResource(&ID3D11Texture2D::IID, &mut raw)?;
            let tex = ID3D11Texture2D::from_raw(raw);
            let index = dxgi.GetSubresourceIndex()?;
            return Ok(Some(Frame::Gpu { tex, index, width, height, _sample: s }));
        }
        let buf = s.ConvertToContiguousBuffer()?;
        let alloc_height = self.frame.1.max(height);
        if let Ok(b2) = buf.cast::<IMF2DBuffer>() {
            let mut p = std::ptr::null_mut();
            let mut pitch = 0i32;
            b2.Lock2D(&mut p, &mut pitch)?;
            let stride = pitch.unsigned_abs() as usize;
            let len = stride * alloc_height as usize * 3 / 2;
            let data = std::slice::from_raw_parts(p, len).to_vec();
            b2.Unlock2D()?;
            return Ok(Some(Frame::Cpu { data, stride, width, height, alloc_height }));
        }
        let mut p = std::ptr::null_mut();
        let mut cur = 0u32;
        buf.Lock(&mut p, None, Some(&mut cur))?;
        let data = std::slice::from_raw_parts(p, cur as usize).to_vec();
        buf.Unlock()?;
        let stride = self.frame.0.max(width) as usize;
        if data.len() < stride * alloc_height as usize * 3 / 2 {
            return Ok(None);
        }
        Ok(Some(Frame::Cpu { data, stride, width, height, alloc_height }))
    }
}

/// NV12 (BT.709, 16–235 range) → BGRA.
pub fn nv12_to_bgra(src: &[u8], stride: usize, width: usize, height: usize, alloc_height: usize, dst: &mut Vec<u8>) {
    dst.resize(width * height * 4, 0);
    let uv_base = stride * alloc_height;
    for y in 0..height {
        let yrow = &src[y * stride..y * stride + width];
        let uvrow = &src[uv_base + (y / 2) * stride..uv_base + (y / 2) * stride + width.div_ceil(2) * 2];
        let out = &mut dst[y * width * 4..(y + 1) * width * 4];
        for x in 0..width {
            let c = (yrow[x] as i32 - 16) * 298;
            let d = uvrow[x & !1] as i32 - 128;
            let e = uvrow[(x & !1) + 1] as i32 - 128;
            let r = (c + 459 * e + 128) >> 8;
            let g = (c - 55 * d - 136 * e + 128) >> 8;
            let b = (c + 541 * d + 128) >> 8;
            out[x * 4] = b.clamp(0, 255) as u8;
            out[x * 4 + 1] = g.clamp(0, 255) as u8;
            out[x * 4 + 2] = r.clamp(0, 255) as u8;
            out[x * 4 + 3] = 255;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nv12_conversion_known_colors() {
        // 2x2: Y = 16 (black) and 235 (white), neutral UV.
        let src = vec![16, 235, 235, 16, 128, 128];
        let mut dst = Vec::new();
        nv12_to_bgra(&src, 2, 2, 2, 2, &mut dst);
        assert_eq!(&dst[0..4], &[0, 0, 0, 255]);
        assert_eq!(&dst[4..8], &[255, 255, 255, 255]);
        // BT.709 red: Y=63, U=102, V=240
        let red = vec![63, 63, 63, 63, 102, 240];
        nv12_to_bgra(&red, 2, 2, 2, 2, &mut dst);
        assert!(dst[2] > 240 && dst[1] < 15 && dst[0] < 15, "{:?}", &dst[0..4]);
    }

    #[test]
    #[ignore = "requires Media Foundation"]
    fn finds_h264_and_hevc_decoders() {
        assert!(Decoder::new(Codec::H264, None).is_ok());
        assert!(Decoder::new(Codec::H265, None).is_ok());
    }
}
