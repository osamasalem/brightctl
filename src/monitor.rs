use log::{info, trace};
use serde::{Deserialize, Serialize};
use wmi::WMIConnection;

const WMISETBRIGHTNESS: &str = "WmiSetBrightness";

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

pub struct Monitor {
    wmi: WMIConnection,
}

impl Monitor {
    pub fn new() -> Result<Self, String> {
        trace!("Creating new WMI Connection...");
        Ok(Self {
            wmi: WMIConnection::with_namespace_path(r#"ROOT/WMI"#)
                .map_err(|err| format!("Cannot Initialize WMI Connection because:\n{err}"))?,
        })
    }

    pub fn set_brightness(&mut self, lvl: u8) -> Result<(), String> {
        let lvl = lvl.clamp(0, 100);

        info!("Set Brightness {lvl}");
        let monitor = self
            .wmi
            .get::<WmiMonitorBrightnessMethods>()
            .map_err(|err| format!("Cannot get monitor: {err}"))?;

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
            .map_err(|err| format!("Cannot set monitor brightness: {err}"))?;
        Ok(())
    }

    pub fn get_brightness(&self) -> Result<u8, String> {
        let monitor = self
            .wmi
            .get::<WmiMonitorBrightness>()
            .map_err(|err| format!("Cannot get monitor: {err}"))?;

        info!("Monitor found:{}", monitor.InstanceName);

        Ok(monitor.CurrentBrightness)
    }
}
