use cpal::traits::{DeviceTrait, HostTrait};

pub struct CpalDeviceInfo {
    pub index: String,
    pub name: String,
    pub is_default: bool,
    pub device: cpal::Device,
}

fn device_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "Unknown".into())
}

pub fn list_input_devices() -> Result<Vec<CpalDeviceInfo>, Box<dyn std::error::Error>> {
    let host = crate::audio_toolkit::get_cpal_host();
    let default_name = host.default_input_device().map(|d| device_name(&d));

    let mut out = Vec::<CpalDeviceInfo>::new();

    for (index, device) in host.input_devices()?.enumerate() {
        let name = device_name(&device);

        let is_default = Some(name.clone()) == default_name;

        out.push(CpalDeviceInfo {
            index: index.to_string(),
            name,
            is_default,
            device,
        });
    }

    Ok(out)
}

pub fn list_output_devices() -> Result<Vec<CpalDeviceInfo>, Box<dyn std::error::Error>> {
    let host = crate::audio_toolkit::get_cpal_host();
    let default_name = host.default_output_device().map(|d| device_name(&d));

    let mut out = Vec::<CpalDeviceInfo>::new();

    for (index, device) in host.output_devices()?.enumerate() {
        let name = device_name(&device);

        let is_default = Some(name.clone()) == default_name;

        out.push(CpalDeviceInfo {
            index: index.to_string(),
            name,
            is_default,
            device,
        });
    }

    Ok(out)
}

/// The enumerated endpoint whose name is the default handle's, or the handle
/// itself when no enumerated device carries that name (a backend that only
/// exposes its virtual default).
fn concrete_default<I>(handle: cpal::Device, devices: Option<I>) -> CpalDeviceInfo
where
    I: Iterator<Item = cpal::Device>,
{
    let name = device_name(&handle);
    let found = devices.and_then(|devices| {
        devices
            .enumerate()
            .find(|(_, device)| device_name(device) == name)
    });
    match found {
        Some((index, device)) => CpalDeviceInfo {
            index: index.to_string(),
            name,
            is_default: true,
            device,
        },
        None => CpalDeviceInfo {
            index: "default".to_string(),
            name,
            is_default: true,
            device: handle,
        },
    }
}

/// The system default input as the concrete enumerated endpoint, never
/// cpal's virtual "default" handle.
///
/// cpal 0.18 resolves `host.default_input_device()` on Windows to a virtual
/// handle that follows the system default: it activates through
/// `ActivateAudioInterfaceAsync` and installs an `IMMNotificationClient`
/// whose callbacks initialise COM (single-threaded) on whatever thread
/// Windows delivers them from. After the first such callback (a wireless
/// microphone dropping out was enough) every later activation in the process
/// failed with RPC_E_CHANGED_MODE, "Cannot change thread mode after it is
/// set", and the microphone could not be reopened until restart. The concrete
/// endpoint activates through the plain `IMMDevice::Activate` and registers
/// nothing. The trade-off: a changed system default is picked up by the next
/// open (idle close, stream rebuild), not by a running stream.
pub fn default_input_endpoint() -> Option<CpalDeviceInfo> {
    let host = crate::audio_toolkit::get_cpal_host();
    let handle = host.default_input_device()?;
    Some(concrete_default(handle, host.input_devices().ok()))
}

/// The system default output as the concrete enumerated endpoint; see
/// [`default_input_endpoint`] for why the virtual handle is avoided.
pub fn default_output_endpoint() -> Option<CpalDeviceInfo> {
    let host = crate::audio_toolkit::get_cpal_host();
    let handle = host.default_output_device()?;
    Some(concrete_default(handle, host.output_devices().ok()))
}
