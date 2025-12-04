use std::time::Duration;

use anyhow::Result;
use spd3303x_control::instrument::{Channel, Spd3303x, TimerState};
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

    // 设置型示例：先软复位，避免受之前状态影响。
    inst.soft_reset().await?;
    let channel = Channel::Ch1;

    let steps = [
        (1_u8, 3.3_f64, 1.0_f64, 2.0_f64),
        (2_u8, 5.0_f64, 1.0_f64, 2.0_f64),
        (3_u8, 9.0_f64, 1.0_f64, 2.0_f64),
    ];

    for (group, volts, amps, seconds) in steps {
        inst.timer_set(channel, group, volts, amps, seconds).await?;
    }

    println!("{} timer groups programmed:", channel.label());
    for group in 1_u8..=3_u8 {
        let entry = inst.timer_query(channel, group).await?;
        println!(
            "  Group {} -> {:.3} V / {:.3} A / {:.3} s",
            entry.group, entry.voltage_v, entry.current_a, entry.duration_s
        );
    }

    inst.timer_state(channel, TimerState::On).await?;
    println!("{} timer state: ON", channel.label());

    // 结束前再次软复位，关闭定时等功能，恢复到默认安全状态。
    inst.soft_reset().await?;

    inst.close().await?;
    Ok(())
}
