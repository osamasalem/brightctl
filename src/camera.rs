use std::{ptr, slice};

use image::RgbImage;
use log::{info, trace, warn};
use windows::Win32::{
    Media::MediaFoundation::{
        IMFMediaSource, IMFSample, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
        MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE,
        MF_MT_SUBTYPE, MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION, MFCreateAttributes,
        MFCreateMediaType, MFCreateSourceReaderFromMediaSource, MFEnumDeviceSources,
        MFMediaType_Video, MFSTARTUP_LITE, MFShutdown, MFStartup, MFVideoFormat_RGB24,
    },
    System::Com::CoTaskMemFree,
};

use crate::utils::Handle;

pub fn capture_from_camera() -> Result<(u32, u32, Vec<u8>), String> {
    unsafe {
        let _mf = {
            MFStartup(MF_VERSION, MFSTARTUP_LITE)
                .map_err(|err| format!("MF: Failed to startup because:\n{err}"))?;
            Handle::new((), |_| {
                let _ =
                    MFShutdown().inspect_err(|err| warn!("MF: Failed to shutdown because:\n{err}"));
            })
        };

        let mut attr = None;

        MFCreateAttributes(&mut attr, 1)
            .map_err(|err| format!("MF: Failed to create attributes because:\n{err}"))?;

        let attr = attr
            .ok_or("Cannot get attributes")
            .map_err(|err| format!("MF: Failed to get attributes because:\n{err}"))?;

        attr.SetGUID(
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
        )
        .map_err(|err| format!("MF: Setting GUID attributes because:\n{err}"))?;

        let mut devices = ptr::null_mut();
        let mut count = 0;
        MFEnumDeviceSources(&attr, &raw mut devices, &mut count)
            .map_err(|err| format!("MF: Failed to enum devices because:\n{err}"))?;

        let res = slice::from_raw_parts_mut(devices, count as usize);

        let media_source = res[0]
            .as_ref()
            .ok_or("Cannot get media source")?
            .ActivateObject::<IMFMediaSource>()
            .map_err(|err| format!("MF: failed to activate object because:\n{err}"))?;

        for src in res {
            src.take();
        }

        CoTaskMemFree(Some(devices.cast()));

        let reader = MFCreateSourceReaderFromMediaSource(&media_source, &attr).map_err(|err| {
            format!("MF: Failed to create source reader from media source because:\n{err}")
        })?;

        drop(media_source);
        let media_type =
            MFCreateMediaType().map_err(|err| format!("MF: Create Media type because:\n{err}"))?;

        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(|err| format!("MF: Failed to set major type guid because:\n{err}"))?;

        media_type
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB24)
            .map_err(|err| format!("MF: Failed to set subtype because:\n{err}"))?;

        reader
            .SetCurrentMediaType(
                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                None,
                &media_type,
            )
            .map_err(|err| format!("MF: Set current media type because:\n{err}"))?;

        drop(media_type);
        let mut sample: Option<IMFSample> = None;

        for round in 0..10 {
            trace!("Attempting for {round} time(s)");
            let mut stream_index = 0;
            let mut flags = 0;
            reader
                .ReadSample(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    0,
                    Some(&mut stream_index),
                    Some(&mut flags),
                    None,
                    Some(&mut sample),
                )
                .map_err(|err| format!("MF: Failed to read sample because:\n{err}"))?;

            if sample.is_some() {
                break;
            }
        }
        trace!("Sample found");

        let sample = sample.ok_or("Cannot get valid sample")?;

        let buffer = sample
            .ConvertToContiguousBuffer()
            .map_err(|err| format!("MF: Converting to buffer failed because:\n{err}"))?;

        let mut data = ptr::null_mut();
        let mut maxlen = 0;
        let mut curlen = 0;
        buffer
            .Lock(&raw mut data, Some(&mut maxlen), Some(&mut curlen))
            .map_err(|err| format!("MF: Locking buffer failed because:\n{err}"))?;

        let media_type = reader
            .GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32)
            .map_err(|err| format!("MF: Failed to get current Media type because:\n{err}"))?;

        let size = media_type
            .GetUINT64(&MF_MT_FRAME_SIZE)
            .map_err(|err| format!("MF: Failed to get dimensions because:\n{err}"))?;

        let height = size as u32;
        let width = (size >> 32/* bits */) as u32;
        info!("Camera image dimension {width}px x {height}px");
        let slice = slice::from_raw_parts(data, curlen as usize);
        let slice = slice.to_vec();
        Ok((width, height, slice))
    }
}

pub fn get_camera_brightness() -> Result<u8, String> {
    let (width, height, slice) = capture_from_camera()
        .map_err(|err| format!("Error capture from camera because:\n{err}"))?;
    let image = image::DynamicImage::ImageRgb8(
        RgbImage::from_raw(width, height, slice).ok_or("Cannot get RGB image from raw data")?,
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

    info!("Camera image intensities {energy} / {max}");
    Ok((energy / max * 100.0/* % */) as u8)
}
