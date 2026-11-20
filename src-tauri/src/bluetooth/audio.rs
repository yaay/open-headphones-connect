//! One-click connect / disconnect of the headset's audio.
//!
//! Windows exposes a paired Bluetooth headset as audio endpoints even while
//! it is disconnected. Each endpoint sits on a kernel-streaming filter that
//! accepts a "one-shot reconnect" (or disconnect) request — the same call the
//! Bluetooth quick-settings flyout makes.

use std::collections::HashSet;
use std::ffi::c_void;

use windows::core::{Result, GUID, PCWSTR, PWSTR};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    eAll, IDeviceTopology, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    DEVICE_STATE, DEVICE_STATEMASK_ALL,
};
use windows::Win32::Media::KernelStreaming::{IKsControl, KSIDENTIFIER, KSPROPERTY_TYPE_GET};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL, STGM_READ};

const KSPROPSETID_BT_AUDIO: GUID = GUID::from_u128(0x7FA06C40_B8F6_4C7E_8556_E8C33A12E54D);
const KSPROPERTY_ONESHOT_RECONNECT: u32 = 0;
const KSPROPERTY_ONESHOT_DISCONNECT: u32 = 1;

const PKEY_DEVICE_CONTAINER_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x8C7ED206_3F8A_4827_B3AB_AE9E1FAEFC6C),
    pid: 2,
};
const PKEY_DEVICE_INTERFACE_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x026E516E_B814_414B_83CD_856D6FEF4822),
    pid: 2,
};

const VT_LPWSTR: u16 = 31;
const VT_CLSID: u16 = 72;

/// Identifies which audio endpoints belong to the headset.
pub struct Target<'a> {
    pub container_id: Option<GUID>,
    pub name: &'a str,
    pub address_hex: String,
}

pub fn connect(target: &Target) -> std::result::Result<(), String> {
    request(target, KSPROPERTY_ONESHOT_RECONNECT)
}

pub fn disconnect(target: &Target) -> std::result::Result<(), String> {
    request(target, KSPROPERTY_ONESHOT_DISCONNECT)
}

fn request(target: &Target, property: u32) -> std::result::Result<(), String> {
    let sent = unsafe { request_all(target, property) }.map_err(|e| e.message())?;
    if sent == 0 {
        Err(format!("Windows has no audio device for {}. Pair it again in Bluetooth settings.", target.name))
    } else {
        Ok(())
    }
}

/// The audio filters Windows associates with the headset, without touching
/// them. Used by `--probe`.
pub fn list_filters(target: &Target) -> std::result::Result<Vec<String>, String> {
    unsafe { controls(target) }
        .map(|found| found.into_iter().map(|(id, _)| id).collect())
        .map_err(|e| e.message())
}

/// One control per kernel-streaming filter that belongs to the headset.
unsafe fn controls(target: &Target) -> Result<Vec<(String, IKsControl)>> {
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let endpoints = enumerator.EnumAudioEndpoints(eAll, DEVICE_STATE(DEVICE_STATEMASK_ALL))?;
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for index in 0..endpoints.GetCount()? {
        let Ok(endpoint) = endpoints.Item(index) else { continue };
        let Some(filter_id) = connected_filter_id(&endpoint) else { continue };
        if !belongs_to(target, &endpoint, &filter_id) {
            continue;
        }
        // Playback and microphone endpoints of one profile share a filter.
        if !seen.insert(filter_id.to_ascii_lowercase()) {
            continue;
        }
        let wide: Vec<u16> = filter_id.encode_utf16().chain([0]).collect();
        let Ok(filter) = enumerator.GetDevice(PCWSTR(wide.as_ptr())) else { continue };
        let Ok(control) = filter.Activate::<IKsControl>(CLSCTX_ALL, None) else { continue };
        found.push((filter_id, control));
    }
    Ok(found)
}

unsafe fn request_all(target: &Target, property: u32) -> Result<usize> {
    let mut sent = 0;
    for (filter_id, control) in controls(target)? {
        let mut request: KSIDENTIFIER = std::mem::zeroed();
        request.Anonymous.Anonymous.Set = KSPROPSETID_BT_AUDIO;
        request.Anonymous.Anonymous.Id = property;
        request.Anonymous.Anonymous.Flags = KSPROPERTY_TYPE_GET;
        let mut returned = 0u32;
        let result = control.KsProperty(
            &request,
            std::mem::size_of::<KSIDENTIFIER>() as u32,
            std::ptr::null_mut(),
            0,
            &mut returned,
        );
        match result {
            Ok(()) => sent += 1,
            Err(e) => crate::log!("Bluetooth audio request rejected by {filter_id}: {}", e.message()),
        }
    }
    Ok(sent)
}

/// The device id of the kernel-streaming filter behind an endpoint.
unsafe fn connected_filter_id(endpoint: &IMMDevice) -> Option<String> {
    let topology = endpoint.Activate::<IDeviceTopology>(CLSCTX_ALL, None).ok()?;
    let connector = topology.GetConnector(0).ok()?;
    let id = connector.GetDeviceIdConnectedTo().ok()?;
    take_string(id)
}

unsafe fn belongs_to(target: &Target, endpoint: &IMMDevice, filter_id: &str) -> bool {
    if filter_id.to_ascii_lowercase().contains(&target.address_hex) {
        return true;
    }
    let Ok(store) = endpoint.OpenPropertyStore(STGM_READ) else { return false };
    if let (Some(wanted), Ok(value)) = (target.container_id, store.GetValue(&PKEY_DEVICE_CONTAINER_ID)) {
        if guid_of(&value) == Some(wanted) {
            return true;
        }
    }
    match store.GetValue(&PKEY_DEVICE_INTERFACE_FRIENDLY_NAME) {
        Ok(value) => string_of(&value).is_some_and(|name| name.contains(target.name)),
        Err(_) => false,
    }
}

unsafe fn take_string(value: PWSTR) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let text = value.to_string().ok();
    CoTaskMemFree(Some(value.0 as *const c_void));
    text
}

// PROPVARIANT is a fixed ABI layout: a 16-bit type tag, three reserved words,
// then the value at offset 8.
unsafe fn variant_parts(value: &PROPVARIANT) -> (u16, *const c_void) {
    let base = value as *const PROPVARIANT as *const u8;
    let tag = *(base as *const u16);
    let pointer = *(base.add(8) as *const *const c_void);
    (tag, pointer)
}

unsafe fn guid_of(value: &PROPVARIANT) -> Option<GUID> {
    let (tag, pointer) = variant_parts(value);
    (tag == VT_CLSID && !pointer.is_null()).then(|| *(pointer as *const GUID))
}

unsafe fn string_of(value: &PROPVARIANT) -> Option<String> {
    let (tag, pointer) = variant_parts(value);
    if tag != VT_LPWSTR || pointer.is_null() {
        return None;
    }
    PCWSTR(pointer as *const u16).to_string().ok()
}
