# Brightctl

BrightCtl is an automated laptops panel brightness adjustment based on room ambiance for MS Windows.

It calculates the enrgy intensity from the camera snapshot, and accordingly sets the proper panel brightness

## Build

```bash
cargo build --release
```

## Running
```bash
brightctl.exe
```

## Command line parameters

Usage: brightctl.exe [OPTIONS]

### Options:
| Short | Long | Description |
|-|-|-|
| -r | --repeat <SECONDS> | Turns this command into daemon that adjust brightness every duration specified in seconds |
| -t | --tolerence <PERCENTAGE> | the tolerence percentage for the service to consider a change  in brightness and avoid flactuations [default: 10] |
|    | --min <PERCENTAGE>       | Minimum brightness allowed (0-100) [default: 0] |
|    | --max <PERCENTAGE>       | Maximum brightness allowed (0-100) [default: 100] |
| -v | --verbose <ERRORLEVEL>   | level of file logging (0=Off.. 5=Trace) [default: Error] |
| -h | --help                   | Print help |
| -V | --version                | Print version |

