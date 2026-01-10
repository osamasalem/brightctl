use core::slice;
use std::error::Error;
use std::ptr::{self};
use std::thread::sleep;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use ::windows::Win32::Media::MediaFoundation::MFShutdown;
use ::windows::Win32::Media::MediaFoundation::{
    IMFMediaSource, IMFSample, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE,
    MF_MT_SUBTYPE, MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION, MFCreateAttributes,
    MFCreateMediaType, MFCreateSourceReaderFromMediaSource, MFEnumDeviceSources, MFMediaType_Video,
    MFSTARTUP_LITE, MFStartup, MFVideoFormat_RGB24,
};
use ::windows::Win32::System::Com::CoTaskMemFree;
use image::RgbImage;
use wmi::WMIConnection;

const CORRECTION_FACTOR: f32 = 1.5;
const REFRESH_DELAY_IN_SECS: u64 = 60;
const TOLERANCE_THRESHOLD: u8 = 10;

struct MFLib;

impl MFLib {
    fn init() -> ::windows::core::Result<Self> {
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_LITE).map(|_| Self) }
    }

    fn capture(&self) -> Result<(u32, u32, Vec<u8>), Box<dyn Error>> {
        unsafe {
            let mut attr = None;

            MFCreateAttributes(&mut attr, 1)?;

            let attr = attr.ok_or("Cannot get attributes")?;
            attr.SetGUID(
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            )?;

            let mut devices = ptr::null_mut();
            let mut count = 0;
            MFEnumDeviceSources(&attr, &raw mut devices, &mut count)?;

            let res = slice::from_raw_parts_mut(devices, count as usize);

            let media_source = res[0]
                .as_ref()
                .ok_or("Cannot get media source")?
                .ActivateObject::<IMFMediaSource>()?;

            for src in res {
                src.take();
            }

            CoTaskMemFree(Some(devices.cast()));

            let reader = MFCreateSourceReaderFromMediaSource(&media_source, &attr)?;
            drop(media_source);
            let media_type = MFCreateMediaType()?;
            media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            media_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB24)?;
            reader.SetCurrentMediaType(
                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                None,
                &media_type,
            )?;
            drop(media_type);
            let mut sample: Option<IMFSample> = None;

            for _ in 0..10 {
                let mut stream_index = 0;
                let mut flags = 0;
                reader.ReadSample(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    0,
                    Some(&mut stream_index),
                    Some(&mut flags),
                    None,
                    Some(&mut sample),
                )?;

                if sample.is_some() {
                    break;
                }
            }
            let sample = sample.ok_or("Cannot get valid sample")?;
            let buffer = sample.ConvertToContiguousBuffer()?;
            let mut data = ptr::null_mut();
            let mut maxlen = 0;
            let mut curlen = 0;
            buffer.Lock(&raw mut data, Some(&mut maxlen), Some(&mut curlen))?;
            let media_type =
                reader.GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32)?;

            let size = media_type.GetUINT64(&MF_MT_FRAME_SIZE)?;
            let height = size as u32;
            let width = (size >> 32/* bits */) as u32;
            let slice = slice::from_raw_parts(data, curlen as usize);
            let slice = slice.to_vec();
            Ok((width, height, slice))
        }
    }
    fn camera_brightness(&self) -> Result<u8, Box<dyn Error>> {
        let (width, height, slice) = self.capture()?;
        let image = image::DynamicImage::ImageRgb8(
            RgbImage::from_raw(width, height, slice).ok_or("Cannot read raw image data")?,
        );
        let image = image::DynamicImage::ImageLuma8(image.to_luma8());
        let max = width as f32 * height as f32;
        let energy = image
            .as_flat_samples_u8()
            .ok_or("Cannot get flat symbols")?
            .as_slice()
            .iter()
            .map(|&px| px as f32 / u8::MAX as f32)
            .sum::<f32>();
        Ok((energy / max * 100.0/* % */) as u8)
    }
}

impl Drop for MFLib {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
        }
    }
}

#[allow(non_snake_case)]
#[derive(Serialize)]
struct WmiSetBrightness {
    Brightness: u8,
    Timeout: u32,
}

#[allow(dead_code)]
#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct WmiMonitorBrightnessMethods {
    __PATH: String,
    InstanceName: String,
}

fn set_brightness(lvl: u8) -> Result<(), Box<dyn Error>> {
    let lvl = lvl.clamp(0, 100);
    let wmi = WMIConnection::with_namespace_path(r#"ROOT\WMI"#)?;

    let monitor: Vec<WmiMonitorBrightnessMethods> = wmi.query()?;

    let monitor = monitor.first().ok_or("Cannot find monitor")?;

    wmi.exec_instance_method::<WmiMonitorBrightnessMethods, ()>(
        &monitor.__PATH,
        "WmiSetBrightness",
        WmiSetBrightness {
            Brightness: lvl,
            Timeout: 0,
        },
    )?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let lib = MFLib::init()?;
    let mut last_ratio: Option<u8> = None;
    loop {
        let ratio = lib.camera_brightness()?;

        let ratio = ((ratio as f32) * CORRECTION_FACTOR) as u8;
        println!("{}", ratio);
        if let Some(ref mut last_ratio) = last_ratio {
            if last_ratio.abs_diff(ratio) > TOLERANCE_THRESHOLD {
                set_brightness(ratio)?;
                *last_ratio = ratio;
            }
        } else {
            set_brightness(ratio)?;
            last_ratio = Some(ratio);
        }
        sleep(Duration::from_secs(REFRESH_DELAY_IN_SECS));
    }
}
