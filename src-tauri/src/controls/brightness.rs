//! Built-in laptop panel brightness through WMI (ROOT\WMI). Works without admin rights.
//! External monitors would need DDC/CI instead; out of scope (single laptop screen).
use windows::core::{w, Error, Result, BSTR};
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::System::Com::*;
use windows::Win32::System::Variant::{VariantClear, VARENUM, VARIANT, VT_BSTR, VT_I4, VT_UI1};
use windows::Win32::System::Wmi::*;

// windows 0.62 exposes the raw Win32 VARIANT union, so set/read the tag + value by hand.
fn var_i4(x: i32) -> VARIANT {
    let mut v = VARIANT::default();
    unsafe {
        (*v.Anonymous.Anonymous).vt = VT_I4;
        (*v.Anonymous.Anonymous).Anonymous.lVal = x;
    }
    v
}
fn var_u8(x: u8) -> VARIANT {
    let mut v = VARIANT::default();
    unsafe {
        (*v.Anonymous.Anonymous).vt = VT_UI1;
        (*v.Anonymous.Anonymous).Anonymous.bVal = x;
    }
    v
}
fn vt(v: &VARIANT) -> VARENUM {
    unsafe { v.Anonymous.Anonymous.vt }
}

pub struct Brightness {
    svc: IWbemServices,
}

impl Brightness {
    pub fn new() -> Result<Self> {
        unsafe {
            let loc: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER)?;
            let svc = loc.ConnectServer(&BSTR::from("ROOT\\WMI"), &BSTR::new(), &BSTR::new(), &BSTR::new(), 0, &BSTR::new(), None)?;
            CoSetProxyBlanket(
                &svc,
                10, // RPC_C_AUTHN_WINNT
                0,  // RPC_C_AUTHZ_NONE
                None,
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
            )?;
            Ok(Brightness { svc })
        }
    }

    fn first(&self, query: &str) -> Result<IWbemClassObject> {
        unsafe {
            let en = self.svc.ExecQuery(
                &BSTR::from("WQL"),
                &BSTR::from(query),
                WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                None,
            )?;
            let mut objs = [None];
            let mut n = 0u32;
            en.Next(WBEM_INFINITE, &mut objs, &mut n).ok()?;
            objs[0].take().ok_or_else(|| Error::from(E_FAIL))
        }
    }

    pub fn get(&self) -> Result<u8> {
        unsafe {
            let o = self.first("SELECT CurrentBrightness FROM WmiMonitorBrightness")?;
            let mut v = VARIANT::default();
            o.Get(w!("CurrentBrightness"), 0, &mut v, None, None)?;
            let out = if vt(&v) == VT_UI1 { Ok(v.Anonymous.Anonymous.Anonymous.bVal) } else { Err(Error::from(E_FAIL)) };
            let _ = VariantClear(&mut v);
            out
        }
    }

    pub fn set(&self, pct: u8) -> Result<()> {
        unsafe {
            let inst = self.first("SELECT * FROM WmiMonitorBrightnessMethods")?;
            let mut path = VARIANT::default();
            inst.Get(w!("__PATH"), 0, &mut path, None, None)?;
            if vt(&path) != VT_BSTR {
                return Err(Error::from(E_FAIL));
            }
            let path_b: BSTR = (*path.Anonymous.Anonymous.Anonymous.bstrVal).clone();
            let _ = VariantClear(&mut path);
            let path = path_b;

            let mut class = None;
            self.svc.GetObject(&BSTR::from("WmiMonitorBrightnessMethods"), WBEM_GENERIC_FLAG_TYPE(0), None, Some(&mut class), None)?;
            let class = class.ok_or_else(|| Error::from(E_FAIL))?;
            let mut in_sig = None;
            class.GetMethod(w!("WmiSetBrightness"), 0, &mut in_sig, std::ptr::null_mut())?;
            let params = in_sig.ok_or_else(|| Error::from(E_FAIL))?.SpawnInstance(0)?;
            // WMI carries uint32 as VT_I4 and uint8 as VT_UI1.
            params.Put(w!("Timeout"), 0, &var_i4(0), 0)?;
            params.Put(w!("Brightness"), 0, &var_u8(pct.min(100)), 0)?;
            self.svc.ExecMethod(&path, &BSTR::from("WmiSetBrightness"), WBEM_GENERIC_FLAG_TYPE(0), None, &params, None, None)?;
            Ok(())
        }
    }
}
