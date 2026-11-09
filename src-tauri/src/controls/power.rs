//! Windows 11 power mode (Settings > System > Power > Power mode) = the "power overlay scheme".
//! These powrprof.dll exports are undocumented but stable since Windows 10 1709. They are not in
//! the windows crate metadata nor in the SDK import library, so they are resolved at runtime with
//! GetProcAddress (exports verified on this machine).
use std::sync::OnceLock;

use windows::core::{s, w, GUID};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

type GetFn = unsafe extern "system" fn(*mut GUID) -> u32;
type SetFn = unsafe extern "system" fn(*const GUID) -> u32;

fn api() -> Option<(GetFn, SetFn)> {
    static API: OnceLock<Option<(GetFn, SetFn)>> = OnceLock::new();
    *API.get_or_init(|| unsafe {
        let lib = LoadLibraryW(w!("powrprof.dll")).ok()?;
        let get = GetProcAddress(lib, s!("PowerGetEffectiveOverlayScheme"))?;
        let set = GetProcAddress(lib, s!("PowerSetActiveOverlayScheme"))?;
        Some((std::mem::transmute::<_, GetFn>(get), std::mem::transmute::<_, SetFn>(set)))
    })
}

pub const EFFICIENCY: GUID = GUID::from_u128(0x961cc777_2547_4f9d_8174_7d86181b8a7a);
pub const BALANCED: GUID = GUID::zeroed();
pub const PERFORMANCE: GUID = GUID::from_u128(0xded574b5_45a0_4f42_8737_46345c09c238);

pub fn get() -> String {
    let Some((get, _)) = api() else { return "unknown".into() };
    let mut g = GUID::zeroed();
    if unsafe { get(&mut g) } != 0 {
        return "unknown".into();
    }
    match g {
        x if x == EFFICIENCY => "efficiency",
        x if x == BALANCED => "balanced",
        x if x == PERFORMANCE => "performance",
        _ => "custom",
    }
    .into()
}

pub fn set(mode: &str) -> Result<(), String> {
    let (_, set) = api().ok_or("power mode API unavailable")?;
    let g = match mode {
        "efficiency" => EFFICIENCY,
        "balanced" => BALANCED,
        "performance" => PERFORMANCE,
        _ => return Err(format!("unknown power mode {mode}")),
    };
    let rc = unsafe { set(&g) };
    if rc == 0 { Ok(()) } else { Err(format!("PowerSetActiveOverlayScheme rc={rc}")) }
}
