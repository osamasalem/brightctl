//TODO: Revisite the logging and error handling later
//TODO: Add command line switches
//TODO: Split code to multiple files

use core::slice;
use std::cell::OnceCell;
use std::error::Error;
use std::fs::File;
use std::ops::Deref;
use std::ptr::{self};
use std::str::FromStr;
use std::thread::sleep;
use std::time::{Duration, Instant};

use clap::Parser;
use log::{LevelFilter, error, info, trace, warn};
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
use simplelog::{
    ColorChoice, CombinedLogger, Config, SharedLogger, TermLogger, TerminalMode, WriteLogger,
};
use wmi::{WMIConnection, WMIResult};

const MAXIMUM_BRIGHTNESS: u8 = 100;
const MINIMUM_BRIGHTNESS: u8 = 20;

const REFRESH_DELAY_IN_SECS: u64 = 20;
const TOLERANCE_THRESHOLD: u8 = 5;
const WMISETBRIGHTNESS: &str = "WmiSetBrightness";

struct Handle<T, F>
where
    F: for<'a> Fn(&'a mut T),
{
    handle: T,
    drop_fn: F,
}

impl<T, F> Handle<T, F>
where
    F: for<'a> Fn(&'a mut T),
{
    fn new(handle: T, drop_fn: F) -> Self {
        Self { handle, drop_fn }
    }
}

impl<T, F> Drop for Handle<T, F>
where
    F: for<'a> Fn(&'a mut T),
{
    fn drop(&mut self) {
        (self.drop_fn)(&mut self.handle)
    }
}

impl<T, F> Deref for Handle<T, F>
where
    F: for<'a> Fn(&'a mut T),
{
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

fn capture_from_camera() -> Result<(u32, u32, Vec<u8>), Box<dyn Error>> {
    unsafe {
        let _mf = {
            MFStartup(MF_VERSION, MFSTARTUP_LITE)
                .inspect_err(|err| error!("MF: Failed to startup:{err}"))?;
            Handle::new((), |_| {
                let _ = MFShutdown().inspect_err(|err| warn!("MF: Failed to shutdown:{err}"));
            })
        };

        let mut attr = None;

        MFCreateAttributes(&mut attr, 1)
            .inspect_err(|err| error!("MF: Failed to create attributes: {err}"))?;

        let attr = attr
            .ok_or("Cannot get attributes")
            .inspect_err(|err| error!("MF: Failed to get attributes:{err}"))?;

        attr.SetGUID(
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
        )
        .inspect_err(|err| error!("MF: Setting GUID attributes:{err}"))?;

        let mut devices = ptr::null_mut();
        let mut count = 0;
        MFEnumDeviceSources(&attr, &raw mut devices, &mut count)
            .inspect_err(|err| error!("MF: Failed to enum devices: {err}"))?;

        let res = slice::from_raw_parts_mut(devices, count as usize);

        let media_source = res[0]
            .as_ref()
            .ok_or("Cannot get media source")?
            .ActivateObject::<IMFMediaSource>()
            .inspect_err(|err| error!("MF: failed to activate object: {err}"))?;

        for src in res {
            src.take();
        }

        CoTaskMemFree(Some(devices.cast()));

        let reader =
            MFCreateSourceReaderFromMediaSource(&media_source, &attr).inspect_err(|err| {
                error!("MF: Failed to create source reader from media source: {err}")
            })?;

        drop(media_source);
        let media_type =
            MFCreateMediaType().inspect_err(|err| error!("MF: Create Media type: {err}"))?;

        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .inspect_err(|err| error!("MF: Failed to set major type guid: {err}"))?;

        media_type
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB24)
            .inspect_err(|err| error!("MF: Failed to set subtype :{err}"))?;

        reader
            .SetCurrentMediaType(
                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                None,
                &media_type,
            )
            .inspect_err(|err| error!("MF: Set current media type: {err}"))?;

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
                .inspect_err(|err| error!("MF: Failed to read sample: {err}"))?;

            if sample.is_some() {
                break;
            }
        }
        trace!("Sample found");

        let sample = sample
            .ok_or("Cannot get valid sample")
            .inspect_err(|err| error!("{err}"))?;

        let buffer = sample
            .ConvertToContiguousBuffer()
            .inspect_err(|err| error!("MF: Converting to buffer failed: {err}"))?;

        let mut data = ptr::null_mut();
        let mut maxlen = 0;
        let mut curlen = 0;
        buffer.Lock(&raw mut data, Some(&mut maxlen), Some(&mut curlen))?;
        let media_type = reader
            .GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32)
            .inspect_err(|err| error!("MF: Failed to get current Media type:{err}"))?;

        let size = media_type
            .GetUINT64(&MF_MT_FRAME_SIZE)
            .inspect_err(|err| error!("Failed to get dimensions:{err}"))?;
        let height = size as u32;
        let width = (size >> 32/* bits */) as u32;
        info!("Camera image dimension {width}px x {height}px");
        let slice = slice::from_raw_parts(data, curlen as usize);
        let slice = slice.to_vec();
        Ok((width, height, slice))
    }
}

fn get_camera_brightness() -> Result<u8, Box<dyn Error>> {
    let (width, height, slice) = capture_from_camera()?;
    let image = image::DynamicImage::ImageRgb8(
        RgbImage::from_raw(width, height, slice)
            .ok_or("Cannot get RGB image from raw data")
            .inspect_err(|err| error!("{err}"))?,
    );
    let image = image::DynamicImage::ImageLuma8(image.to_luma8());
    let max = width as f32 * height as f32;
    let energy = image
        .as_flat_samples_u8()
        .ok_or("Cannot get flat symbols")
        .inspect_err(|err| error!("{err}"))?
        .as_slice()
        .iter()
        .map(|&px| px as f32 / u8::MAX as f32)
        .sum::<f32>();

    info!("Camera image intensities {energy} / {max}");
    Ok((energy / max * 100.0/* % */) as u8)
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
struct WmiMonitorBrightness {
    InstanceName: String,
    CurrentBrightness: u8,
}

#[allow(dead_code)]
#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct WmiMonitorBrightnessMethods {
    __PATH: String,
    InstanceName: String,
}

struct Monitor {
    wmi: WMIConnection,
}

impl Monitor {
    fn new() -> WMIResult<Self> {
        trace!("Creating new WMI Connection...");
        Ok(Self {
            wmi: WMIConnection::with_namespace_path(r#"ROOT/WMI"#)
                .inspect_err(|err| error!("Cannot Initialize WMI Connection: {err}"))?,
        })
    }

    fn set_brightness(&mut self, lvl: u8) -> Result<(), Box<dyn Error>> {
        let lvl = lvl.clamp(0, 100);

        info!("Set Brightness {lvl}");
        let monitor = self
            .wmi
            .get::<WmiMonitorBrightnessMethods>()
            .inspect_err(|err| error!("Cannot get monitor: {err}"))?;

        info!("Monitor found:{}", monitor.InstanceName);

        self.wmi
            .exec_instance_method::<WmiMonitorBrightnessMethods, ()>(
                &monitor.__PATH,
                WMISETBRIGHTNESS,
                WmiSetBrightness {
                    Brightness: lvl,
                    Timeout: 0,
                },
            )
            .inspect_err(|err| error!("Cannot set monitor brightness: {err}"))?;
        Ok(())
    }

    fn get_brightness(&self) -> Result<u8, Box<dyn Error>> {
        let monitor = self
            .wmi
            .get::<WmiMonitorBrightness>()
            .inspect_err(|err| error!("Cannot get monitor: {err}"))?;

        info!("Monitor found:{}", monitor.InstanceName);

        Ok(monitor.CurrentBrightness)
    }
}

fn adjust_brightness_cycle(cli: &CliParams) -> Result<(), Box<dyn Error>> {
    trace!("Starting adjusting brightness");
    let _end = Handle::new(Instant::now(), |v| {
        trace!(
            "Finished adjusting brightness in {}ms",
            v.elapsed().as_millis()
        )
    });

    let mut monitor = Monitor::new()?;

    let curr_ratio = monitor.get_brightness()?;

    let ratio = get_camera_brightness()?;
    info!("Current Brightness: {curr_ratio}%");
    trace!("Raw Brightness: {ratio}%");

    let ratio = normalize(ratio, cli.min, cli.max);
    trace!("Normalized Brightness: {ratio}%");

    if curr_ratio.abs_diff(ratio) > cli.tolerence {
        monitor.set_brightness(ratio)?;
    } else {
        warn!("Change aborted");
    }

    Ok(())
}

fn normalize(n: u8, min: u8, max: u8) -> u8 {
    let n = n as f32;
    let min = min as f32;
    let max = max as f32;

    let factor = (max - min) / 100.0;
    let n = n * factor + min;

    (n as u8).clamp(0, 100)
}

#[derive(Parser)]
#[command(version, about)]
struct CliParams {
    #[arg(
        short,
        long,
        value_name = "SECONDS",
        help = "Turns this command into daemon that adjust brightness every duration specified in seconds"
    )]
    repeat: Option<u64>,

    #[arg(
        short,
        long,
        value_name = "PERCENTAGE",
        default_value = "10",
        help = "the tolerence percentage for the service to consider a change in brightness and avoid flactuations"
    )]
    tolerence: u8,

    #[arg(
        long,
        value_name = "PERCENTAGE",
        default_value = "0",
        help = "Minimum brightness allowed (0-100)"
    )]
    min: u8,

    #[arg(
        long,
        value_name = "PERCENTAGE",
        default_value = "100",
        help = "Maximum brightness allowed (0-100)"
    )]
    max: u8,

    #[arg(
        short,
        long,
        value_name = "ERRORLEVEL",
        default_value = "Error",
        help = "level of file logging (0=Off.. 5=Trace)"
    )]
    verbose: String,
}

fn main() {
    let cli = CliParams::parse();

    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![TermLogger::new(
        LevelFilter::Trace,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )];

    if let Ok(file) = File::create("brightctrl.log") {
        loggers.push(WriteLogger::new(
            LevelFilter::from_str(&cli.verbose).unwrap_or(LevelFilter::Error),
            Config::default(),
            file,
        ));
    }

    let _ = CombinedLogger::init(loggers);

    if adjust_brightness_cycle(&cli).is_err() {
        error!("The first cycle failed, Aborting");
        return;
    }

    if let Some(r) = cli.repeat {
        loop {
            trace!("Sleeping for {REFRESH_DELAY_IN_SECS}s..");
            sleep(Duration::from_secs(r));

            let _ = adjust_brightness_cycle(&cli);
        }
    }
}
