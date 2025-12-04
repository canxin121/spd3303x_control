use std::time::Duration;

use anyhow::Result;
use spd3303x_control::instrument::{Channel, Spd3303x};
use tokio::time::timeout;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let args: Vec<String> = std::env::args().collect();
    let host = args.get(1).map(String::as_str).unwrap_or("192.168.0.232");
    let resource = args.get(2).map(String::as_str).unwrap_or("inst0");

    let mut inst = match timeout(Duration::from_secs(5), Spd3303x::connect(host, resource)).await {
        Ok(Ok(client)) => client,
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            eprintln!("连接 SPD3303X 超时（5 秒），请检查设备电源和网络连接。");
            return Ok(());
        }
    };

    let idn = inst.idn().await?;
    println!("IDN: {}", idn.trim());

    let version = inst.system_version().await?;
    println!("Firmware version: {}", version.trim());

    let status = inst.system_status().await?;
    println!("System status: 0x{:04X}", status.raw);

    // CH1 / CH2: fully programmable, support SCPI queries for V/I/P.
    for channel in [Channel::Ch1, Channel::Ch2] {
        let status = inst.channel_status(channel).await?;
        println!("{}:", channel.label());
        println!(
            "  Set      : {:.3} V / {:.3} A",
            status.set_voltage_v, status.set_current_a
        );
        println!(
            "  Measured : {:.3} V / {:.3} A / {:.3} W",
            status.measured_voltage_v,
            status.measured_current_a,
            status.measured_power_w
        );
        let output_on = inst.query_output(channel).await?;
        println!("  Output   : {}", if output_on { "ON" } else { "OFF" });
    }

    // CH3: on SPD3303X/3303X-E, CH3 is a fixed output
    // channel (2.5/3.3/5 V via DIP switch) and does not
    // support remote V/I/P queries. Only the output state
    // can be controlled.
    println!("CH3:");
    println!("  Set      : fixed (2.5/3.3/5 V via DIP switch)");
    println!("  Measured : N/A (no SCPI measurement for CH3)");
    match inst.query_output(Channel::Ch3).await {
        Ok(on) => println!("  Output   : {}", if on { "ON" } else { "OFF" }),
        Err(e) => println!("  Output   : unknown ({})", e),
    }
    // 读取型示例：最后做一次软复位，保证用完后设备处于安全状态。
    inst.soft_reset().await?;
    inst.close().await?;
    Ok(())
}
