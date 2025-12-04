use std::time::Duration;

use anyhow::Result;
use spd3303x_control::instrument::{Channel, OutputState, Spd3303x};
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

    // 将电源恢复到一个已知、安全的默认状态，避免受之前设置影响。
    inst.soft_reset().await?;

    inst.set_voltage(Channel::Ch1, 10.0).await?;
    inst.set_current(Channel::Ch1, 1.0).await?;
    inst.set_output(Channel::Ch1, OutputState::On).await?;

    sleep(Duration::from_secs(1)).await;

    inst.set_output(Channel::Ch1, OutputState::Off).await?;

    // 示例为“设置型”：结束前再做一次软复位，恢复到默认安全状态。
    inst.soft_reset().await?;
    inst.close().await?;

    Ok(())
}
