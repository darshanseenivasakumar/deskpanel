// THROWAWAY: can an unpackaged exe read/toggle radios? Usage: radios [toggle <WiFi|Bluetooth> <seconds-off>]
use windows::Devices::Radios::*;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

fn main() -> windows::core::Result<()> {
    unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };
    let access = Radio::RequestAccessAsync()?.join()?;
    println!("RequestAccess = {access:?}");
    let radios = Radio::GetRadiosAsync()?.join()?;
    for r in &radios {
        println!("radio name={:?} kind={:?} state={:?}", r.Name()?.to_string(), r.Kind()?, r.State()?);
    }
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 4 && args[1] == "toggle" {
        let want = if args[2].eq_ignore_ascii_case("wifi") { RadioKind::WiFi } else { RadioKind::Bluetooth };
        let secs: u64 = args[3].parse().unwrap_or(3);
        for r in &radios {
            if r.Kind()? != want {
                continue;
            }
            let before = r.State()?;
            let target = if before == RadioState::On { RadioState::Off } else { RadioState::On };
            let res = r.SetStateAsync(target)?.join()?;
            std::thread::sleep(std::time::Duration::from_millis(800));
            println!("toggle {want:?}: {before:?} -> requested {target:?} -> result {res:?} -> now {:?}", r.State()?);
            std::thread::sleep(std::time::Duration::from_secs(secs));
            let res2 = r.SetStateAsync(before)?.join()?;
            std::thread::sleep(std::time::Duration::from_millis(800));
            println!("restore {want:?}: result {res2:?} -> now {:?}", r.State()?);
        }
    }
    Ok(())
}
