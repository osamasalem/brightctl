use clap::Parser;

#[derive(Parser)]
#[command(version, about)]
pub struct CliParams {
    #[arg(
        short,
        long,
        value_name = "SECONDS",
        help = "Turns this command into daemon that adjust brightness every duration specified in seconds"
    )]
    pub repeat: Option<u64>,

    #[arg(
        short,
        long,
        value_name = "PERCENTAGE",
        default_value = "10",
        help = "the tolerence percentage for the service to consider a change in brightness and avoid flactuations"
    )]
    pub tolerence: u8,

    #[arg(
        long,
        value_name = "PERCENTAGE",
        default_value = "0",
        help = "Minimum brightness allowed (0-100)"
    )]
    pub min: u8,

    #[arg(
        long,
        value_name = "PERCENTAGE",
        default_value = "100",
        help = "Maximum brightness allowed (0-100)"
    )]
    pub max: u8,

    #[arg(
        short,
        long,
        value_name = "ERRORLEVEL",
        default_value = "Error",
        help = "level of file logging (0=Off.. 5=Trace)"
    )]
    pub verbose: String,
}
