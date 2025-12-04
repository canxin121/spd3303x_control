use std::time::Duration;

use anyhow::Result;
use spd3303x_control::instrument::{Channel, OutputState, Spd3303x, TrackMode};
use tokio::time::{sleep, timeout};

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

    // 设置型示例：先软复位，避免受之前状态影响。
    inst.soft_reset().await?;

    inst.set_track_mode(TrackMode::Series).await?;
    println!("Tracking mode set to SERIES");

    inst.set_voltage(Channel::Ch1, 5.0).await?;
    inst.set_voltage(Channel::Ch2, 5.0).await?;
    inst.set_current(Channel::Ch1, 1.0).await?;
    inst.set_current(Channel::Ch2, 1.0).await?;

    inst.set_output(Channel::Ch1, OutputState::On).await?;
    inst.set_output(Channel::Ch2, OutputState::On).await?;
    println!("CH1/CH2 outputs enabled at 5 V / 1 A");

    sleep(Duration::from_secs(3)).await;

    // Some firmware variants may not support total MEAS queries like `MEAS:VOLT?`
    // without an explicit channel. Measure CH1/CH2 separately and combine them
    // to avoid empty or unsupported responses.
    let v_ch1 = inst.measure_voltage(Some(Channel::Ch1)).await?;
    let v_ch2 = inst.measure_voltage(Some(Channel::Ch2)).await?;
    let v_total = v_ch1 + v_ch2;

    let i_ch1 = inst.measure_current(Some(Channel::Ch1)).await?;
    // In series tracking, CH1/CH2 share the same current; use CH1 as reference.
    let i_total = i_ch1;

    let p_total = v_total * i_total;
    println!(
        "Total measured -> {:.3} V / {:.3} A / {:.3} W",
        v_total, i_total, p_total
    );

    inst.set_output(Channel::Ch1, OutputState::Off).await?;
    inst.set_output(Channel::Ch2, OutputState::Off).await?;
    println!("CH1/CH2 outputs disabled");

    // 结束前再次软复位，恢复到默认安全状态。
    inst.soft_reset().await?;

    inst.close().await?;
    Ok(())
}
