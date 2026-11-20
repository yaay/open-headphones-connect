//! Windows Bluetooth access: finding the paired headset, the RFCOMM control
//! link, and switching the audio connection on and off.

pub mod audio;
pub mod link;
pub mod probe;

use windows::core::{Interface, Result, GUID, HSTRING};
use windows::Devices::Bluetooth::Rfcomm::RfcommServiceId;
use windows::Devices::Bluetooth::{
    BluetoothAdapter, BluetoothCacheMode, BluetoothConnectionStatus, BluetoothDevice,
};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Devices::Radios::RadioState;
use windows_collections::IIterable;
use windows::Foundation::IReference;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

use crate::protocol::{v1, v2, Generation};

const CONTAINER_ID_PROPERTY: &str = "System.Devices.Aep.ContainerId";

/// Every thread that touches WinRT or COM calls this first.
pub fn init_thread() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

pub fn radio_is_on() -> bool {
    let check = || -> Result<bool> {
        let adapter = BluetoothAdapter::GetDefaultAsync()?.get()?;
        let radio = adapter.GetRadioAsync()?.get()?;
        Ok(radio.State()? == RadioState::On)
    };
    check().unwrap_or(false)
}

pub fn service_uuid(generation: Generation) -> GUID {
    GUID::from_u128(match generation {
        Generation::V1 => v1::SERVICE_UUID,
        Generation::V2 => v2::SERVICE_UUID,
    })
}

pub struct SonyDevice {
    pub device: BluetoothDevice,
    pub name: String,
    pub address: u64,
    pub generation: Generation,
    /// Groups the Bluetooth device with its Windows audio endpoints.
    pub container_id: Option<GUID>,
}

impl SonyDevice {
    pub fn is_connected(&self) -> bool {
        self.device
            .ConnectionStatus()
            .map(|s| s == BluetoothConnectionStatus::Connected)
            .unwrap_or(false)
    }

    pub fn address_hex(&self) -> String {
        format!("{:012x}", self.address)
    }
}

fn has_cached_service(device: &BluetoothDevice, generation: Generation) -> bool {
    let check = || -> Result<bool> {
        let id = RfcommServiceId::FromUuid(service_uuid(generation))?;
        let result = device
            .GetRfcommServicesForIdWithCacheModeAsync(&id, BluetoothCacheMode::Cached)?
            .get()?;
        Ok(result.Services()?.Size()? > 0)
    };
    check().unwrap_or(false)
}

/// Used only when Windows has no cached service record for the device.
fn generation_from_name(name: &str) -> Option<Generation> {
    let upper = name.to_ascii_uppercase();
    let sony = ["WH-", "WF-", "MDR-", "LINKBUDS", "ULT WEAR", "INZONE"]
        .iter()
        .any(|prefix| upper.starts_with(prefix) || upper.starts_with(&format!("LE_{prefix}")));
    if !sony {
        return None;
    }
    let first_gen = ["XM3", "WH-1000XM4", "WH-1000XM2", "MDR-1000X", "H900N", "H910N", "XB900N", "CH710N"]
        .iter()
        .any(|model| upper.contains(model));
    Some(if first_gen { Generation::V1 } else { Generation::V2 })
}

pub fn has_wind_mode(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["WH-1000XM3", "WH-1000XM2", "MDR-1000X"].iter().any(|m| upper.contains(m))
}

pub fn is_earbuds(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.contains("WF-") || upper.contains("LINKBUDS")
}

fn container_id(info: &DeviceInformation) -> Option<GUID> {
    let value = info.Properties().ok()?.Lookup(&HSTRING::from(CONTAINER_ID_PROPERTY)).ok()?;
    value.cast::<IReference<GUID>>().ok()?.Value().ok()
}

/// Looks through the paired Bluetooth devices for a Sony headset. A device
/// that is connected right now wins over one that is merely paired.
pub fn find_sony_device() -> Option<SonyDevice> {
    let search = || -> Result<Vec<SonyDevice>> {
        let selector = BluetoothDevice::GetDeviceSelectorFromPairingState(true)?;
        let extra: IIterable<HSTRING> = vec![HSTRING::from(CONTAINER_ID_PROPERTY)].into();
        let paired =
            DeviceInformation::FindAllAsyncAqsFilterAndAdditionalProperties(&selector, &extra)?
                .get()?;
        let mut found = Vec::new();
        for info in &paired {
            let Ok(device) = BluetoothDevice::FromIdAsync(&info.Id()?).and_then(|op| op.get())
            else {
                continue;
            };
            let name = device.Name().map(|n| n.to_string()).unwrap_or_default();
            let generation = if has_cached_service(&device, Generation::V2) {
                Some(Generation::V2)
            } else if has_cached_service(&device, Generation::V1) {
                Some(Generation::V1)
            } else {
                generation_from_name(&name)
            };
            let Some(generation) = generation else { continue };
            found.push(SonyDevice {
                address: device.BluetoothAddress().unwrap_or(0),
                container_id: container_id(&info),
                name: if name.trim().is_empty() { "Sony headphones".into() } else { name },
                generation,
                device,
            });
        }
        Ok(found)
    };

    let mut found = match search() {
        Ok(found) => found,
        Err(e) => {
            crate::log!("Bluetooth device search failed: {}", e.message());
            return None;
        }
    };
    let pick = found.iter().position(|d| d.is_connected()).unwrap_or(0);
    (!found.is_empty()).then(|| found.swap_remove(pick))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_generation_only_for_sony_names() {
        assert_eq!(generation_from_name("WH-1000XM3"), Some(Generation::V1));
        assert_eq!(generation_from_name("LE_WH-1000XM3"), Some(Generation::V1));
        assert_eq!(generation_from_name("WH-1000XM4"), Some(Generation::V1));
        assert_eq!(generation_from_name("WF-1000XM4"), Some(Generation::V2));
        assert_eq!(generation_from_name("WH-1000XM5"), Some(Generation::V2));
        assert_eq!(generation_from_name("LinkBuds S"), Some(Generation::V2));
        assert_eq!(generation_from_name("AirPods Pro"), None);
        assert_eq!(generation_from_name("Logitech MX Keys"), None);
    }

    #[test]
    fn knows_which_models_have_a_wind_mode() {
        assert!(has_wind_mode("WH-1000XM3"));
        assert!(!has_wind_mode("WH-1000XM4"));
        assert!(is_earbuds("WF-1000XM3"));
        assert!(!is_earbuds("WH-1000XM3"));
    }
}
