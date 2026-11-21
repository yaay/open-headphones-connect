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

use ohc_core::models::{generation_from_name, is_earbuds};
use ohc_core::platform;

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

    fn audio_target(&self) -> audio::Target<'_> {
        audio::Target {
            container_id: self.container_id,
            name: &self.name,
            address_hex: self.address_hex(),
        }
    }
}

/// Windows Bluetooth, as the shared session sees it.
pub struct Windows;

impl platform::Bluetooth for Windows {
    fn init_thread(&self) {
        init_thread();
    }

    fn radio_is_on(&self) -> bool {
        radio_is_on()
    }

    fn find_headset(&self) -> Option<Box<dyn platform::Headset>> {
        find_sony_device().map(|device| Box::new(device) as Box<dyn platform::Headset>)
    }
}

impl platform::Headset for SonyDevice {
    fn name(&self) -> &str {
        &self.name
    }

    fn is_connected(&self) -> bool {
        SonyDevice::is_connected(self)
    }

    fn open(&self) -> std::result::Result<(Box<dyn platform::Link>, Generation), String> {
        link::Link::open(self)
            .map(|(link, generation)| (Box::new(link) as Box<dyn platform::Link>, generation))
    }

    fn connect_audio(&self) -> std::result::Result<(), String> {
        audio::connect(&self.audio_target())
    }

    fn disconnect_audio(&self) -> std::result::Result<(), String> {
        audio::disconnect(&self.audio_target())
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
                // Windows has no cached service record for the device.
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
