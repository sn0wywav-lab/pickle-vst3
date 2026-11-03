//! Audio device discovery.

use cpal::traits::{DeviceTrait, HostTrait};

#[derive(Debug, Clone, Default)]
pub struct DeviceInfo {
    pub name: String,
    pub is_default: bool,
    pub max_channels: u16,
    /// Common sample rates the device advertises.
    pub sample_rates: Vec<u32>,
}

const COMMON_RATES: [u32; 8] = [8_000, 16_000, 22_050, 32_000, 44_100, 48_000, 96_000, 192_000];

fn describe(dev: &cpal::Device, input: bool, default_name: &Option<String>) -> Option<DeviceInfo> {
    let name = dev.to_string();
    let ranges: Vec<cpal::SupportedStreamConfigRange> = if input {
        dev.supported_input_configs().ok()?.collect()
    } else {
        dev.supported_output_configs().ok()?.collect()
    };
    if ranges.is_empty() {
        return None;
    }
    let max_channels = ranges.iter().map(|r| r.channels()).max().unwrap_or(0);
    let mut sample_rates: Vec<u32> = COMMON_RATES
        .iter()
        .copied()
        .filter(|&sr| ranges.iter().any(|r| r.min_sample_rate() <= sr && sr <= r.max_sample_rate()))
        .collect();
    sample_rates.dedup();
    Some(DeviceInfo {
        is_default: default_name.as_deref() == Some(name.as_str()),
        name,
        max_channels,
        sample_rates,
    })
}

pub fn input_devices() -> Vec<DeviceInfo> {
    let host = cpal::default_host();
    let default = host.default_input_device().map(|d| d.to_string());
    host.input_devices()
        .map(|it| it.filter_map(|d| describe(&d, true, &default)).collect())
        .unwrap_or_default()
}

pub fn output_devices() -> Vec<DeviceInfo> {
    let host = cpal::default_host();
    let default = host.default_output_device().map(|d| d.to_string());
    host.output_devices()
        .map(|it| it.filter_map(|d| describe(&d, false, &default)).collect())
        .unwrap_or_default()
}

pub(crate) fn find_input(name: Option<&str>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if let Some(name) = name {
        if let Ok(mut it) = host.input_devices() {
            if let Some(d) = it.find(|d| d.to_string() == name) {
                return Some(d);
            }
        }
    }
    host.default_input_device()
}

pub(crate) fn find_output(name: Option<&str>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if let Some(name) = name {
        if let Ok(mut it) = host.output_devices() {
            if let Some(d) = it.find(|d| d.to_string() == name) {
                return Some(d);
            }
        }
    }
    host.default_output_device()
}

/// Choose a stream config: requested rate if supported, else 48 kHz, else
/// the device default. Prefers stereo, f32.
pub(crate) fn pick_config(
    dev: &cpal::Device,
    input: bool,
    wanted_rate: u32,
) -> Option<cpal::SupportedStreamConfig> {
    let ranges: Vec<cpal::SupportedStreamConfigRange> = if input {
        dev.supported_input_configs().ok()?.collect()
    } else {
        dev.supported_output_configs().ok()?.collect()
    };
    let rate_pref = [if wanted_rate == 0 { 48_000 } else { wanted_rate }, 48_000];
    for &rate in &rate_pref {
        let mut candidates: Vec<&cpal::SupportedStreamConfigRange> = ranges
            .iter()
            .filter(|r| r.min_sample_rate() <= rate && rate <= r.max_sample_rate())
            .collect();
        // Prefer exactly 2 channels, then f32, then fewer channels.
        candidates.sort_by_key(|r| {
            (
                (r.channels() != 2) as u8,
                (r.sample_format() != cpal::SampleFormat::F32) as u8,
                r.channels(),
            )
        });
        if let Some(r) = candidates.first() {
            return Some((**r).with_sample_rate(rate));
        }
    }
    if input {
        dev.default_input_config().ok()
    } else {
        dev.default_output_config().ok()
    }
}
