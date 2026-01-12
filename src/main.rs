use std::{
    fs::File,
    str::FromStr,
    thread::sleep,
    time::{Duration, Instant},
};

use clap::Parser;
use log::{LevelFilter, error, info, trace, warn};
use simplelog::{
    ColorChoice, CombinedLogger, ConfigBuilder, SharedLogger, TermLogger, TerminalMode,
    WriteLogger, format_description,
};

use crate::{
    camera::get_camera_brightness,
    cli::CliParams,
    monitor::Monitor,
    utils::{Handle, normalize},
};

mod camera;
mod cli;
mod monitor;
mod utils;

fn adjust_brightness_cycle(cli: &CliParams) -> Result<(), String> {
    trace!("Starting adjusting brightness");
    let _end = Handle::new(Instant::now(), |v| {
        trace!(
            "Finished adjusting brightness in {}ms",
            v.elapsed().as_millis()
        )
    });

    let mut monitor =
        Monitor::new().map_err(|err| format!("Error instentiating monitor because:\n{err}"))?;

    let curr_ratio = monitor
        .get_brightness()
        .map_err(|err| format!("Error getting monitor brightness because:\n{err}"))?;

    let ratio = get_camera_brightness()
        .map_err(|err| format!("Error getting camera brightness because:\n{err}"))?;

    info!("Current Brightness: {curr_ratio}%");
    trace!("Raw Brightness: {ratio}%");

    let ratio = normalize(ratio, cli.min, cli.max);
    trace!("Normalized Brightness: {ratio}%");

    if curr_ratio.abs_diff(ratio) > cli.tolerence {
        monitor
            .set_brightness(ratio)
            .map_err(|err| format!("Error setting monitor brightness because:\n{err}"))?;
    } else {
        warn!("Change aborted");
    }

    Ok(())
}

fn main() {
    let cli = CliParams::parse();

    let config = ConfigBuilder::new()
        .set_time_format_custom(format_description!("[hour]:[minute]:[second].[subsecond]"))
        .build();

    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![TermLogger::new(
        LevelFilter::Trace,
        config.clone(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )];

    if let Ok(file) = File::open("brightctrl.log") {
        loggers.push(WriteLogger::new(
            LevelFilter::from_str(&cli.verbose).unwrap_or(LevelFilter::Error),
            config,
            file,
        ));
    }

    let _ = CombinedLogger::init(loggers);

    if let Err(err) = adjust_brightness_cycle(&cli) {
        error!("The first cycle failed, Aborting because:\n{err}");

        return;
    }

    if let Some(delay) = cli.repeat {
        let delay = delay.clamp(5, 60 * 60);

        loop {
            trace!("Sleeping for {delay}s..");
            sleep(Duration::from_secs(delay));

            let _ = adjust_brightness_cycle(&cli)
                .inspect_err(|err| warn!("Error adjusting brightness because:\n{err}"));
        }
    }
}
