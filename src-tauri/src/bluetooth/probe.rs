//! `open-headphones-connect.exe --probe`: prints what Windows knows about each paired Bluetooth
//! device and whether the Sony control service can be reached. Read-only.

use windows::core::{Result, GUID, HSTRING};
use windows::Devices::Bluetooth::Rfcomm::{RfcommDeviceServicesResult, RfcommServiceId};
use windows::Devices::Bluetooth::{BluetoothCacheMode, BluetoothConnectionStatus, BluetoothDevice};
use windows::Devices::Enumeration::DeviceInformation;

use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::audio;
use super::link::{Incoming, Link};
use super::{find_sony_device, generation_from_name, init_thread, is_earbuds, radio_is_on, service_uuid};
use crate::protocol::framing::{pack, TYPE_ACK, TYPE_DATA_MDR, TYPE_DATA_MDR_NO2};
use crate::protocol::{v1, v2, BatterySlot, Event, Generation};

fn describe(result: Result<RfcommDeviceServicesResult>) -> String {
    let result = match result {
        Ok(result) => result,
        Err(e) => return format!("failed: {}", e.message()),
    };
    let status = result.Error().map(|e| format!("{e:?}")).unwrap_or_else(|e| e.message());
    let mut lines = vec![format!("status {status}")];
    if let Ok(services) = result.Services() {
        for service in &services {
            let uuid = service.ServiceId().and_then(|id| id.Uuid()).unwrap_or(GUID::zeroed());
            let name = service.ConnectionServiceName().unwrap_or_default();
            lines.push(format!("      {uuid:?}  {name}"));
        }
        if services.Size().unwrap_or(0) == 0 {
            lines.push("      (no services)".into());
        }
    }
    lines.join("\n")
}

fn query(device: &BluetoothDevice, uuid: GUID, mode: BluetoothCacheMode) -> String {
    describe(
        RfcommServiceId::FromUuid(uuid)
            .and_then(|id| device.GetRfcommServicesForIdWithCacheModeAsync(&id, mode))
            .and_then(|op| op.get()),
    )
}

/// `write_test` additionally re-sends the noise mode the headset is already
/// in, to check that it accepts settings. That is the only write.
pub fn run(write_test: bool) {
    init_thread();
    println!("Bluetooth radio on: {}", radio_is_on());

    let list = || -> Result<()> {
        let selector: HSTRING = BluetoothDevice::GetDeviceSelectorFromPairingState(true)?;
        let paired = DeviceInformation::FindAllAsyncAqsFilter(&selector)?.get()?;
        println!("Paired devices: {}", paired.Size()?);
        for info in &paired {
            let Ok(device) = BluetoothDevice::FromIdAsync(&info.Id()?).and_then(|op| op.get())
            else {
                println!("- {} (could not be opened)", info.Name().unwrap_or_default());
                continue;
            };
            let name = device.Name().map(|n| n.to_string()).unwrap_or_default();
            let connected = device.ConnectionStatus().ok() == Some(BluetoothConnectionStatus::Connected);
            println!(
                "- {name}  [{:012X}]  connected={connected}  name-guess={:?}",
                device.BluetoothAddress().unwrap_or(0),
                generation_from_name(&name)
            );
            if generation_from_name(&name).is_none() {
                continue;
            }
            for generation in [Generation::V1, Generation::V2] {
                let uuid = service_uuid(generation);
                println!("    {generation:?} control service, cached:   {}", query(&device, uuid, BluetoothCacheMode::Cached));
                println!("    {generation:?} control service, uncached: {}", query(&device, uuid, BluetoothCacheMode::Uncached));
            }
            println!(
                "    all serial services, uncached: {}",
                describe(
                    device
                        .GetRfcommServicesWithCacheModeAsync(BluetoothCacheMode::Uncached)
                        .and_then(|op| op.get())
                )
            );
        }
        Ok(())
    };
    if let Err(e) = list() {
        println!("Probe failed: {}", e.message());
    }

    println!();
    match find_sony_device() {
        Some(device) if device.is_connected() => {
            let target = audio::Target {
                container_id: device.container_id,
                name: &device.name,
                address_hex: device.address_hex(),
            };
            println!("Container id: {:?}", device.container_id);
            match audio::list_filters(&target) {
                Ok(filters) => {
                    println!("Audio filters for connect / disconnect: {}", filters.len());
                    for filter in filters {
                        println!("    {filter}");
                    }
                }
                Err(e) => println!("Audio filters could not be listed: {e}"),
            }
            println!();
            if let Err(e) = talk(&device.name, Link::open(&device), write_test) {
                println!("Control link: {e}");
            }
        }
        Some(device) => println!("{} is paired but not connected; skipping the control link.", device.name),
        None => println!("No Sony headset found among the paired devices."),
    }
}

/// Sends every read-only query and prints the raw replies next to how they
/// were understood. Nothing here changes a setting on the headset.
fn talk(
    name: &str,
    opened: std::result::Result<(Link, Generation), String>,
    write_test: bool,
) -> std::result::Result<(), String> {
    let (link, generation) = opened?;
    println!("Control link to {name} open, speaking {generation:?}");
    let (tx, rx) = mpsc::channel();
    link.spawn_reader(move |incoming| {
        let _ = tx.send(incoming);
    });

    let mut noise = None;
    // The noise-control message as last received, which a second-generation
    // headset's next change starts from.
    let mut noise_raw: Option<Vec<u8>> = None;
    const T1: u8 = TYPE_DATA_MDR;
    const T2: u8 = TYPE_DATA_MDR_NO2;
    let mut queries: Vec<(&str, u8, Vec<u8>)> = match generation {
        Generation::V1 => vec![
            ("protocol info", T1, v1::get_protocol_info()),
            ("functions", T1, v1::get_support_functions()),
            ("battery", T1, v1::get_battery(if is_earbuds(name) { BatterySlot::Left } else { BatterySlot::Single })),
            ("model", T1, v1::get_model_name()),
            ("series and colour", T1, v1::get_series_and_colour()),
            ("noise control layout", T1, v1::get_noise_capability(v1::NoiseKind::Both)),
            ("noise control", T1, v1::get_noise(v1::NoiseKind::Both)),
            ("equalizer", T1, v1::get_eq()),
            ("firmware", T1, v1::get_firmware_version()),
            ("codec", T1, v1::get_codec()),
            ("volume", T1, v1::get_volume()),
            ("playback", T1, v1::get_playback()),
            ("DSEE upscaling", T1, v1::get_upscaling()),
            ("auto power-off choices", T1, v1::get_auto_power_off_options()),
            ("auto power-off", T1, v1::get_auto_power_off()),
            ("general setting 1: what it is", T1, v1::get_general_setting_info(0xD1)),
            ("general setting 2: what it is", T1, v1::get_general_setting_info(0xD2)),
            ("general setting 2: value", T1, v1::get_general_setting(0xD2)),
            ("voice guidance", T2, v1::get_voice_guidance()),
        ],
        // Every variant is asked for; a headset answers the ones it has.
        Generation::V2 => vec![
            ("protocol info", T1, v2::get_protocol_info()),
            ("functions", T1, v2::get_support_functions()),
            ("functions, second table", T2, v2::get_support_functions()),
            ("model", T1, v2::get_model_name()),
            ("firmware", T1, v2::get_firmware_version()),
            ("series and colour", T1, v2::get_series_and_colour()),
            ("codec", T1, v2::get_codec()),
            ("battery", T1, v2::get_battery(0x00)),
            ("battery, left and right", T1, v2::get_battery(0x01)),
            ("battery, case", T1, v2::get_battery(0x02)),
            ("battery with warning level", T1, v2::get_battery(0x08)),
            ("battery with warning level, left and right", T1, v2::get_battery(0x09)),
            ("battery with warning level, case", T1, v2::get_battery(0x0A)),
            ("noise control (XM5 kind)", T1, v2::get_noise(0x17)),
            ("noise control (adaptive kind)", T1, v2::get_noise(0x19)),
            ("noise control (ambient only)", T1, v2::get_noise(0x22)),
            ("equalizer layout", T1, v2::get_eq_layout(0x00)),
            ("equalizer bands", T1, v2::get_eq_frequencies()),
            ("equalizer", T1, v2::get_eq()),
            ("volume", T1, v2::get_volume()),
            ("playback", T1, v2::get_playback()),
            ("DSEE upscaling", T1, v2::get_upscaling()),
            ("auto power-off", T1, v2::get_auto_power_off(0x04)),
            ("auto power-off by wearing", T1, v2::get_auto_power_off(0x05)),
            ("Speak-to-Chat", T1, v2::get_speak_to_chat(0x0C)),
            ("pause when taken off", T1, v2::get_pause_when_removed()),
            ("general setting 1: what it is", T1, v2::get_general_setting_info(0xD1)),
            ("general setting 1: value", T1, v2::get_general_setting(0xD1)),
            ("general setting 2: what it is", T1, v2::get_general_setting_info(0xD2)),
            ("general setting 2: value", T1, v2::get_general_setting(0xD2)),
            ("voice guidance", T2, v2::get_voice_guidance(0x01)),
        ],
    };

    if write_test {
        // Filled in below with whatever mode the headset reports.
        queries.push(("set noise control to its current value", T1, Vec::new()));
    }

    let mut seq = 0u8;
    for (label, kind, mut payload) in queries {
        if payload.is_empty() {
            let Some(current) = noise else {
                println!("> {label}: skipped, the current mode is unknown");
                continue;
            };
            payload = match generation {
                Generation::V1 => v1::set_noise(v1::NoiseKind::Both, current, true),
                Generation::V2 => {
                    let layout = noise_raw.as_ref().and_then(|raw| v2::NoiseLayout::of(raw[1]));
                    let Some(layout) = layout else {
                        println!("> {label}: skipped, the noise-control variant is unknown");
                        continue;
                    };
                    layout.write(noise_raw.as_deref(), current)
                }
            };
        }
        println!("> {label}: {payload:02x?}");
        link.send(&pack(kind, seq, &payload))?;
        let mut deadline = Instant::now() + Duration::from_millis(1500);
        let (mut acked, mut answered) = (false, false);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(Incoming::Frame(frame)) if frame.is_ack() => {
                    seq = frame.seq;
                    acked = true;
                }
                Ok(Incoming::Frame(frame)) => {
                    link.send(&pack(TYPE_ACK, 1u8.wrapping_sub(frame.seq), &[]))?;
                    let events = match generation {
                        Generation::V1 => v1::parse(frame.kind, &frame.payload),
                        Generation::V2 => v2::parse(frame.kind, &frame.payload),
                    };
                    println!("  < type {:#04x}  {:02x?}", frame.kind, frame.payload);
                    println!("    understood as {events:?}");
                    for event in &events {
                        match event {
                            Event::Noise(state) => noise = Some(*state),
                            Event::NoiseRaw(raw) => noise_raw = Some(raw.clone()),
                            _ => {}
                        }
                    }
                    answered = true;
                    // Leave a short window for anything else the headset adds.
                    deadline = Instant::now() + Duration::from_millis(250);
                }
                Ok(Incoming::Closed(why)) => {
                    return Err(format!("closed by the headset: {}", why.unwrap_or_else(|| "no reason given".into())));
                }
                Err(_) => break,
            }
        }
        if !acked {
            println!("  (not acknowledged)");
        } else if !answered {
            println!("  (acknowledged, no reply)");
        }
    }
    Ok(())
}
