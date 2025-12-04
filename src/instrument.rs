use anyhow::{anyhow, Context, Result};
use clap::ValueEnum;
use std::time::Duration;
use tokio_vxi11::DeviceClient;
use tracing::debug;

const MAX_READ: u32 = 4096;

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum Channel {
    #[value(name = "CH1")]
    Ch1,
    #[value(name = "CH2")]
    Ch2,
    #[value(name = "CH3")]
    Ch3,
}

impl Channel {
    fn as_scpi(self) -> &'static str {
        match self {
            Channel::Ch1 => "CH1",
            Channel::Ch2 => "CH2",
            Channel::Ch3 => "CH3",
        }
    }

    pub fn label(self) -> &'static str {
        self.as_scpi()
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputState {
    On,
    Off,
}

impl OutputState {
    fn as_str(self) -> &'static str {
        match self {
            OutputState::On => "ON",
            OutputState::Off => "OFF",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TrackMode {
    Independent,
    Series,
    Parallel,
}

impl TrackMode {
    fn as_value(self) -> u8 {
        match self {
            TrackMode::Independent => 0,
            TrackMode::Series => 1,
            TrackMode::Parallel => 2,
        }
    }

    fn from_value(value: u8) -> Result<Self> {
        match value {
            0 => Ok(TrackMode::Independent),
            1 => Ok(TrackMode::Series),
            2 => Ok(TrackMode::Parallel),
            other => Err(anyhow!("unknown track mode value {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TimerState {
    On,
    Off,
}

impl TimerState {
    fn as_str(self) -> &'static str {
        match self {
            TimerState::On => "ON",
            TimerState::Off => "OFF",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DhcpState {
    On,
    Off,
}

impl DhcpState {
    fn as_str(self) -> &'static str {
        match self {
            DhcpState::On => "ON",
            DhcpState::Off => "OFF",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum RegulationMode {
    ConstantVoltage,
    ConstantCurrent,
}

#[derive(Debug, Clone)]
pub struct SystemStatus {
    /// Raw status word as returned by `SYSTem:STATus?` (after hex decoding).
    pub raw: u32,
    pub ch1_regulation_mode: RegulationMode,
    pub ch2_regulation_mode: RegulationMode,
    pub track_mode: Option<TrackMode>,
    pub ch1_output_on: bool,
    pub ch2_output_on: bool,
    pub timer1_on: bool,
    pub timer2_on: bool,
    pub ch1_waveform_display: bool,
    pub ch2_waveform_display: bool,
    pub parallel_mode: bool,
}

impl SystemStatus {
    fn from_word(word: u32) -> Self {
        let ch1_regulation_mode = if word & (1 << 0) == 0 {
            RegulationMode::ConstantVoltage
        } else {
            RegulationMode::ConstantCurrent
        };
        let ch2_regulation_mode = if word & (1 << 1) == 0 {
            RegulationMode::ConstantVoltage
        } else {
            RegulationMode::ConstantCurrent
        };
        // Bits 2–3 encode the track mode according to the manual:
        // 01: Independent, 11: Series, 10: Parallel. Other values are treated
        // as "unknown" and mapped to None.
        let track_bits = ((word >> 2) & 0b11) as u8;
        let track_mode = match track_bits {
            0b01 => Some(TrackMode::Independent),
            0b11 => Some(TrackMode::Series),
            0b10 => Some(TrackMode::Parallel),
            _ => None,
        };

        let ch1_output_on = (word & (1 << 4)) != 0;
        let ch2_output_on = (word & (1 << 5)) != 0;
        let timer1_on = (word & (1 << 6)) != 0;
        let timer2_on = (word & (1 << 7)) != 0;
        let ch1_waveform_display = (word & (1 << 8)) != 0;
        let ch2_waveform_display = (word & (1 << 9)) != 0;
        let parallel_mode = (word & (1 << 10)) != 0;

        SystemStatus {
            raw: word,
            ch1_regulation_mode,
            ch2_regulation_mode,
            track_mode,
            ch1_output_on,
            ch2_output_on,
            timer1_on,
            timer2_on,
            ch1_waveform_display,
            ch2_waveform_display,
            parallel_mode,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChannelStatus {
    pub set_voltage_v: f64,
    pub set_current_a: f64,
    pub measured_voltage_v: f64,
    pub measured_current_a: f64,
    pub measured_power_w: f64,
}

#[derive(Debug, Clone)]
pub struct TimerEntry {
    pub group: u8,
    pub voltage_v: f64,
    pub current_a: f64,
    pub duration_s: f64,
}

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub ip: String,
    pub mask: String,
    pub gateway: String,
    pub dhcp: bool,
}

pub struct Spd3303x {
    inner: DeviceClient,
}

impl Spd3303x {
    /// Perform a "soft reset" to bring the instrument into a known, safe state
    /// without relying on any vendor-specific reset command.
    ///
    /// This method:
    /// - turns OFF outputs on CH1/CH2/CH3
    /// - sets track mode to Independent
    /// - disables timers on CH1/CH2
    /// - disables waveform display on CH1/CH2
    /// - resets CH1/CH2 set voltage/current to 0 V / 0 A
    pub async fn soft_reset(&mut self) -> Result<()> {
        debug!("soft_reset: turning all outputs OFF");
        self.set_output(Channel::Ch1, OutputState::Off).await?;
        self.set_output(Channel::Ch2, OutputState::Off).await?;
        self.set_output(Channel::Ch3, OutputState::Off).await?;

        debug!("soft_reset: setting track mode to Independent");
        self.set_track_mode(TrackMode::Independent).await?;

        debug!("soft_reset: disabling timers on CH1/CH2");
        self.timer_state(Channel::Ch1, TimerState::Off).await?;
        self.timer_state(Channel::Ch2, TimerState::Off).await?;

        debug!("soft_reset: disabling waveform display on CH1/CH2");
        self.set_wave_display(Channel::Ch1, OutputState::Off).await?;
        self.set_wave_display(Channel::Ch2, OutputState::Off).await?;

        debug!("soft_reset: resetting CH1/CH2 setpoints to 0 V / 0 A");
        self.set_voltage(Channel::Ch1, 0.0).await?;
        self.set_current(Channel::Ch1, 0.0).await?;
        self.set_voltage(Channel::Ch2, 0.0).await?;
        self.set_current(Channel::Ch2, 0.0).await?;

        debug!("soft_reset: complete");
        Ok(())
    }

    pub async fn connect(host: &str, resource: &str) -> Result<Self> {
        let inner = DeviceClient::connect(host, resource).await?;
        Ok(Self { inner })
    }

    pub async fn connect_with_timeout(
        host: &str,
        resource: &str,
        timeout: Duration,
    ) -> Result<Self> {
        let inner = DeviceClient::connect_with_timeout(host, resource, timeout).await?;
        Ok(Self { inner })
    }

    pub async fn close(&mut self) -> Result<()> {
        self.inner.close().await?;
        Ok(())
    }

    pub async fn idn(&mut self) -> Result<String> {
        self.query("*IDN?\n").await
    }

    pub async fn save_state(&mut self, slot: u8) -> Result<()> {
        ensure_slot(slot)?;
        self.write(&format!("*SAV {}\n", slot)).await
    }

    pub async fn recall_state(&mut self, slot: u8) -> Result<()> {
        ensure_slot(slot)?;
        self.write(&format!("*RCL {}\n", slot)).await
    }

    pub async fn select_channel(&mut self, channel: Channel) -> Result<()> {
        self.write(&format!("INST {}\n", channel.as_scpi())).await
    }

    pub async fn query_selected_channel(&mut self) -> Result<Channel> {
        let resp = self.query("INST?\n").await?;
        parse_channel(resp.trim())
    }

    pub async fn set_voltage(&mut self, channel: Channel, volts: f64) -> Result<()> {
        guard_programmable(channel)?;
        self.write(&format!("{}:VOLT {:.6}\n", channel.as_scpi(), volts))
            .await
    }

    pub async fn query_voltage(&mut self, channel: Channel) -> Result<f64> {
        guard_programmable(channel)?;
        let resp = self
            .query(&format!("{}:VOLT?\n", channel.as_scpi()))
            .await?;
        parse_f64(&resp)
    }

    pub async fn set_current(&mut self, channel: Channel, amps: f64) -> Result<()> {
        guard_programmable(channel)?;
        self.write(&format!("{}:CURR {:.6}\n", channel.as_scpi(), amps))
            .await
    }

    pub async fn query_current(&mut self, channel: Channel) -> Result<f64> {
        guard_programmable(channel)?;
        let resp = self
            .query(&format!("{}:CURR?\n", channel.as_scpi()))
            .await?;
        parse_f64(&resp)
    }

    pub async fn set_output(&mut self, channel: Channel, state: OutputState) -> Result<()> {
        self.write(&format!("OUTPut {},{}\n", channel.as_scpi(), state.as_str()))
            .await
    }

    pub async fn query_output(&mut self, channel: Channel) -> Result<bool> {
        match channel {
            Channel::Ch1 | Channel::Ch2 => {
                // For CH1/CH2, use the documented `SYSTem:STATus?` status word
                // and decode the corresponding output bits instead of relying
                // on the undocumented `OUTPut?` query form.
                let status = self.system_status().await?;
                let on = match channel {
                    Channel::Ch1 => status.ch1_output_on,
                    Channel::Ch2 => status.ch2_output_on,
                    Channel::Ch3 => unreachable!(),
                };
                Ok(on)
            }
            Channel::Ch3 => Err(anyhow!(
                "CH3 does not support querying output state via SYST:STATus?; \
                 only on/off control (`OUTPut CH3,ON/OFF`) is available"
            )),
        }
    }

    pub async fn set_track_mode(&mut self, mode: TrackMode) -> Result<()> {
        self.write(&format!("OUTP:TRACK {}\n", mode.as_value())).await
    }

    pub async fn query_track_mode(&mut self) -> Result<TrackMode> {
        let resp = self.query("OUTP:TRACK?\n").await?;
        let value = resp.trim().parse::<u8>()?;
        TrackMode::from_value(value)
    }

    pub async fn set_wave_display(&mut self, channel: Channel, state: OutputState) -> Result<()> {
        guard_programmable(channel)?;
        self.write(&format!("OUTP:WAVE {},{}\n", channel.as_scpi(), state.as_str()))
            .await
    }

    pub async fn measure_voltage(&mut self, channel: Option<Channel>) -> Result<f64> {
        if let Some(ch) = channel {
            guard_programmable(ch)?;
        }
        let suffix = match channel { Some(ch) => format!(" {}", ch.as_scpi()), None => String::new() };
        let resp = self.query(&format!("MEAS:VOLT?{}\n", suffix)).await?;
        parse_f64(&resp)
    }

    pub async fn measure_current(&mut self, channel: Option<Channel>) -> Result<f64> {
        if let Some(ch) = channel {
            guard_programmable(ch)?;
        }
        let suffix = match channel { Some(ch) => format!(" {}", ch.as_scpi()), None => String::new() };
        let resp = self.query(&format!("MEAS:CURR?{}\n", suffix)).await?;
        parse_f64(&resp)
    }

    pub async fn measure_power(&mut self, channel: Option<Channel>) -> Result<f64> {
        if let Some(ch) = channel {
            guard_programmable(ch)?;
        }
        let suffix = match channel { Some(ch) => format!(" {}", ch.as_scpi()), None => String::new() };
        // According to the SPD3303X/3303X-E manual, the SCPI command is
        // `MEASure: POWEr? [{CH1|CH2}]`. Use the full mnemonic `POWEr`
        // here, as some firmware revisions appear not to respond to the
        // abbreviated `POW?` form.
        let resp = self.query(&format!("MEAS:POWEr?{}\n", suffix)).await?;
        parse_f64(&resp)
    }

    pub async fn channel_status(&mut self, channel: Channel) -> Result<ChannelStatus> {
        Ok(ChannelStatus {
            set_voltage_v: self.query_voltage(channel).await?,
            set_current_a: self.query_current(channel).await?,
            measured_voltage_v: self.measure_voltage(Some(channel)).await?,
            measured_current_a: self.measure_current(Some(channel)).await?,
            measured_power_w: self.measure_power(Some(channel)).await?,
        })
    }

    pub async fn timer_set(
        &mut self,
        channel: Channel,
        group: u8,
        voltage: f64,
        current: f64,
        seconds: f64,
    ) -> Result<()> {
        guard_programmable(channel)?;
        ensure_group(group)?;
        self.write(&format!(
            "TIMER:SET {},{},{:.6},{:.6},{:.6}\n",
            channel.as_scpi(), group, voltage, current, seconds
        ))
        .await
    }

    pub async fn timer_query(&mut self, channel: Channel, group: u8) -> Result<TimerEntry> {
        guard_programmable(channel)?;
        ensure_group(group)?;
        let resp = self
            .query(&format!("TIMER:SET? {},{}\n", channel.as_scpi(), group))
            .await?;
        parse_timer_response(group, &resp)
    }

    pub async fn timer_state(&mut self, channel: Channel, state: TimerState) -> Result<()> {
        guard_programmable(channel)?;
        self.write(&format!("TIMER {},{}\n", channel.as_scpi(), state.as_str()))
            .await
    }

    pub async fn system_error(&mut self) -> Result<String> {
        self.query("SYST:ERR?\n").await
    }

    pub async fn system_version(&mut self) -> Result<String> {
        self.query("SYST:VERS?\n").await
    }

    pub async fn system_status(&mut self) -> Result<SystemStatus> {
        let resp = self.query("SYST:STAT?\n").await?;
        let trimmed = resp.trim_start_matches("0x").trim();
        let word = u32::from_str_radix(trimmed, 16)?;
        Ok(SystemStatus::from_word(word))
    }

    pub async fn set_ip(&mut self, ip: &str) -> Result<()> {
        self.write(&format!("IPaddr {}\n", ip)).await
    }

    pub async fn set_mask(&mut self, mask: &str) -> Result<()> {
        self.write(&format!("MASKaddr {}\n", mask)).await
    }

    pub async fn set_gateway(&mut self, gateway: &str) -> Result<()> {
        self.write(&format!("GATEaddr {}\n", gateway)).await
    }

    pub async fn query_ip(&mut self) -> Result<String> {
        self.query("IPaddr?\n").await
    }

    pub async fn query_mask(&mut self) -> Result<String> {
        self.query("MASKaddr?\n").await
    }

    pub async fn query_gateway(&mut self) -> Result<String> {
        self.query("GATEaddr?\n").await
    }

    pub async fn set_dhcp(&mut self, state: DhcpState) -> Result<()> {
        self.write(&format!("DHCP {}\n", state.as_str())).await
    }

    pub async fn query_dhcp(&mut self) -> Result<DhcpState> {
        let resp = self.query("DHCP?\n").await?;
        if resp.to_uppercase().contains("ON") {
            Ok(DhcpState::On)
        } else {
            Ok(DhcpState::Off)
        }
    }

    pub async fn network_config(&mut self) -> Result<NetworkConfig> {
        Ok(NetworkConfig {
            ip: self.query_ip().await?.trim().to_string(),
            mask: self.query_mask().await?.trim().to_string(),
            gateway: self.query_gateway().await?.trim().to_string(),
            dhcp: matches!(self.query_dhcp().await?, DhcpState::On),
        })
    }

    async fn write(&mut self, command: &str) -> Result<()> {
        debug!("SCPI write  -> {}", command.trim_end_matches('\n'));
        self.inner
            .write(command.as_bytes())
            .await
            .with_context(|| format!("failed to send {command:?}"))?;
        Ok(())
    }

    async fn query(&mut self, command: &str) -> Result<String> {
        debug!("SCPI query  -> {}", command.trim_end_matches('\n'));
        self.write(command).await?;
        let resp = self.inner.read(MAX_READ).await?;
        let raw = String::from_utf8(resp)?;
        let trimmed = raw.trim_matches(char::from(0)).trim().to_string();

        debug!("SCPI result <- {}", trimmed);

        if trimmed.is_empty() {
            return Err(anyhow!("empty response from device for command {command:?}"));
        }

        Ok(trimmed)
    }
}

fn ensure_slot(slot: u8) -> Result<()> {
    if (1..=5).contains(&slot) {
        Ok(())
    } else {
        Err(anyhow!("slot must be 1..=5"))
    }
}

fn ensure_group(group: u8) -> Result<()> {
    if (1..=5).contains(&group) {
        Ok(())
    } else {
        Err(anyhow!("timer group must be 1..=5"))
    }
}

fn guard_programmable(channel: Channel) -> Result<()> {
    if matches!(channel, Channel::Ch1 | Channel::Ch2) {
        Ok(())
    } else {
        Err(anyhow!("channel {} does not support this command", channel.as_scpi()))
    }
}

fn parse_channel(value: &str) -> Result<Channel> {
    match value.trim().to_uppercase().as_str() {
        "CH1" => Ok(Channel::Ch1),
        "CH2" => Ok(Channel::Ch2),
        "CH3" => Ok(Channel::Ch3),
        other => Err(anyhow!("unknown channel {other}")),
    }
}

fn parse_f64(input: &str) -> Result<f64> {
    input
        .trim()
        .parse::<f64>()
        .map_err(|e| anyhow!("failed to parse float from {input:?}: {e}"))
}

fn parse_on_off(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("ON") || value.trim() == "1"
}

fn parse_timer_response(group: u8, resp: &str) -> Result<TimerEntry> {
    let mut parts = resp.trim().split(',');
    let voltage = parts
        .next()
        .ok_or_else(|| anyhow!("missing voltage in timer response"))?
        .parse::<f64>()?;
    let current = parts
        .next()
        .ok_or_else(|| anyhow!("missing current in timer response"))?
        .parse::<f64>()?;
    let duration = parts
        .next()
        .ok_or_else(|| anyhow!("missing duration in timer response"))?
        .parse::<f64>()?;
    Ok(TimerEntry {
        group,
        voltage_v: voltage,
        current_a: current,
        duration_s: duration,
    })
}
